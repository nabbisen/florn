//! The headless runner (RFC 005 rule 31; external design §4): a front end like the others,
//! driving the Host through the control interface and nothing else. The binary `florn` is the
//! `florn` crate, whose `main` calls [`run`] with the process's arguments.
//!
//! ```text
//! florn run <scenario.toml|package.florn> [--module <file>]... [--modules <dir>] [--seed N]
//!           [--out <file>] [--trace] [--payloads] [--keep-records]
//! florn replay <run.florn-run> <scenario.toml|package.florn> [--module <file>]... [--modules <dir>]
//! florn admit <module.wasm>
//! florn probe [--instances N] [--memory BYTES]
//! florn help
//! ```
//!
//! Messages are English from the built-in table; `run` prints the chain, `replay` the
//! comparison, `admit` the manifest and the admitted hash, `probe` the capacity. Exit codes: 0
//! done, 1 refused or failed, 2 a replay diverged, 3 the run ended with a run fault (the
//! `ended` line says which).
#![forbid(unsafe_code)]

pub mod english;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use florn_control::{Answer, Command, Envelope, ExportKind, Message, Query, SeedChoice, Unit};
use florn_domain_network::NetworkDomain;
use florn_engine_wasmtime::{Limits, WasmtimeEngine};
use florn_host::Domains;
use florn_host::{Controller, Options};
use florn_platform_native::NativePlatform;

struct Args {
    command: String,
    positional: Vec<String>,
    modules: Vec<PathBuf>,
    module_dirs: Vec<PathBuf>,
    seed: Option<u64>,
    out: Option<PathBuf>,
    trace: bool,
    payloads: bool,
    keep_records: bool,
    instances: Option<u64>,
    memory: Option<u64>,
    /// `run --checkpoint-at TICK --checkpoint-out FILE`: one checkpoint file at the boundary
    /// before the tick (RFC 009 rule 6).
    checkpoint_at: Option<u64>,
    checkpoint_out: Option<PathBuf>,
    /// `resume --to TICK`: stop there instead of at the scenario's end.
    to: Option<u64>,
}

fn usage() -> &'static str {
    "usage:\n  florn run <scenario.toml|package.florn> [--module <file>]... [--modules <dir>] [--seed N] [--out <file>] [--trace] [--payloads] [--checkpoint-at TICK --checkpoint-out <file>]\n  florn resume <run.florn-checkpoint> <scenario.toml|package.florn> [--module <file>]... [--modules <dir>] [--to TICK] [--out <file>] [--trace] [--payloads]\n  florn replay <run.florn-run> <scenario.toml|package.florn> [--module <file>]... [--modules <dir>]\n  florn admit <module.wasm>\n  florn probe [--instances N] [--memory BYTES]\n  florn help\nexit codes: 0 done; 1 refused or failed; 2 the replay diverged; 3 the run ended with a run fault"
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args {
        command: argv.first().cloned().unwrap_or_default(),
        positional: Vec::new(),
        modules: Vec::new(),
        module_dirs: Vec::new(),
        seed: None,
        out: None,
        trace: false,
        payloads: false,
        keep_records: false,
        instances: None,
        memory: None,
        checkpoint_at: None,
        checkpoint_out: None,
        to: None,
    };
    let mut i = 1;
    let value = |i: &mut usize, name: &str| -> Result<String, String> {
        *i += 1;
        argv.get(*i)
            .cloned()
            .ok_or_else(|| format!("{name} needs a value"))
    };
    while i < argv.len() {
        match argv[i].as_str() {
            "--module" => a.modules.push(PathBuf::from(value(&mut i, "--module")?)),
            "--modules" => a
                .module_dirs
                .push(PathBuf::from(value(&mut i, "--modules")?)),
            "--seed" => {
                a.seed = Some(
                    value(&mut i, "--seed")?
                        .parse()
                        .map_err(|_| "--seed needs an unsigned 64-bit integer".to_string())?,
                )
            }
            "--out" => a.out = Some(PathBuf::from(value(&mut i, "--out")?)),
            "--trace" => a.trace = true,
            "--payloads" => a.payloads = true,
            "--keep-records" => a.keep_records = true,
            "--instances" => {
                a.instances = Some(
                    value(&mut i, "--instances")?
                        .parse()
                        .map_err(|_| "--instances needs an integer".to_string())?,
                )
            }
            "--memory" => {
                a.memory = Some(
                    value(&mut i, "--memory")?
                        .parse()
                        .map_err(|_| "--memory needs an integer of bytes".to_string())?,
                )
            }
            "--checkpoint-at" => {
                a.checkpoint_at = Some(
                    value(&mut i, "--checkpoint-at")?
                        .parse()
                        .map_err(|_| "--checkpoint-at needs a tick".to_string())?,
                )
            }
            "--checkpoint-out" => {
                a.checkpoint_out = Some(PathBuf::from(value(&mut i, "--checkpoint-out")?))
            }
            "--to" => {
                a.to = Some(
                    value(&mut i, "--to")?
                        .parse()
                        .map_err(|_| "--to needs a tick".to_string())?,
                )
            }
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s => a.positional.push(s.to_string()),
        }
        i += 1;
    }
    Ok(a)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Print the notifications of a reply on standard error; the answer, if any, is returned. A
/// command that was applied answers; one that was refused only notifies. An error notification
/// with an answer (an actor's fault during an advance) is not a refusal.
fn report(replies: Vec<Message>) -> (Option<Answer>, bool) {
    let mut answer = None;
    for m in replies {
        match m {
            Message::Notification(n) => eprintln!("{}", english::line(&n)),
            Message::Answer(a) => answer = Some(a),
            _ => {}
        }
    }
    let error = answer.is_none();
    (answer, error)
}

fn send(c: &mut Controller<WasmtimeEngine>, m: Message) -> (Option<Answer>, bool) {
    report(c.handle(Envelope::new(m)))
}

fn controller(args: &Args) -> Result<Controller<WasmtimeEngine>, String> {
    let engine = WasmtimeEngine::new(Limits::default()).map_err(|e| e.to_string())?;
    let platform = NativePlatform::with_capacity(
        args.instances.unwrap_or(NativePlatform::DEFAULT_INSTANCES),
        args.memory.unwrap_or(NativePlatform::DEFAULT_MEMORY_BYTES),
    );
    Ok(Controller::new(
        engine,
        florn_host::run::default_admission(),
        Box::new(platform),
        Domains::core().with(NetworkDomain::new),
        Options {
            keep_records: args.trace || args.keep_records,
            keep_payloads: args.payloads,
            ..Options::default()
        },
    ))
}

/// Load the scenario and the modules named on the command line or found in the directories.
fn load(c: &mut Controller<WasmtimeEngine>, scenario: &Path, args: &Args) -> Result<(), String> {
    let (_, error) = send(
        c,
        Message::Command(Command::LoadScenario {
            bytes: read(scenario)?,
        }),
    );
    if error {
        return Err("the scenario was not loaded".into());
    }
    let mut files: Vec<PathBuf> = args.modules.clone();
    for dir in &args.module_dirs {
        let entries =
            std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|x| x == "wasm") {
                files.push(p);
            }
        }
    }
    files.sort();
    let mut loaded: Vec<(PathBuf, String)> = Vec::new();
    for f in files {
        let bytes = read(&f)?;
        loaded.push((f.clone(), florn_host::run::sha256_hex(&bytes)));
        let (_, error) = send(c, Message::Command(Command::LoadModule { bytes }));
        if error {
            return Err(format!("the module {} was not loaded", f.display()));
        }
    }
    if let (Some(Answer::Scenario(Some(s))), _) = send(c, Message::Query(Query::Scenario))
        && !s.missing_modules.is_empty()
    {
        for h in &s.missing_modules {
            eprintln!("missing module: {h}");
        }
        let referenced: Vec<&str> = s.modules.iter().map(|m| m.source_sha256.as_str()).collect();
        for (f, h) in &loaded {
            if !referenced.contains(&h.as_str()) {
                eprintln!(
                    "module {} hashes to {h}, which the scenario does not reference",
                    f.display()
                );
            }
        }
        return Err(
            "the scenario references modules that were not supplied (--module, --modules)".into(),
        );
    }
    Ok(())
}

fn run_to_end(c: &mut Controller<WasmtimeEngine>) -> Result<(), String> {
    let (_, error) = send(
        c,
        Message::Command(Command::Advance {
            unit: Unit::ToTick(u64::MAX),
        }),
    );
    if error {
        return Err("the run did not complete".into());
    }
    Ok(())
}

fn print_state(c: &mut Controller<WasmtimeEngine>) -> Option<florn_control::message::RunStateInfo> {
    match send(c, Message::Query(Query::RunState)) {
        (Some(Answer::RunState(r)), _) => {
            println!("state = {}", r.state.as_str());
            if let Some(seed) = r.seed {
                println!("seed = {seed}");
            }
            println!("handled = {}", r.handled);
            println!("records = {}", r.records);
            if let Some(chain) = &r.chain {
                println!("chain = {chain}");
            }
            if let Some(e) = &r.ended {
                println!("ended = {e}");
            }
            Some(r)
        }
        _ => None,
    }
}

fn cmd_run(args: &Args) -> Result<ExitCode, String> {
    let scenario = PathBuf::from(args.positional.first().ok_or(usage())?);
    if args.checkpoint_at.is_some() != args.checkpoint_out.is_some() {
        return Err("--checkpoint-at and --checkpoint-out go together".into());
    }
    let mut c = controller(args)?;
    load(&mut c, &scenario, args)?;
    let seed = args.seed.map_or(SeedChoice::Choose, SeedChoice::Given);
    let (_, error) = send(&mut c, Message::Command(Command::NewRun { seed }));
    if error {
        return Err("the run was not started".into());
    }
    if let (Some(tick), Some(out)) = (args.checkpoint_at, &args.checkpoint_out) {
        // To the boundary before the tick, a checkpoint there, its file (RFC 009 rule 6).
        let (_, error) = send(
            &mut c,
            Message::Command(Command::Advance {
                unit: Unit::ToTick(tick),
            }),
        );
        if error {
            return Err("the run did not reach the checkpoint's tick".into());
        }
        let (_, error) = send(&mut c, Message::Command(Command::Checkpoint));
        if error {
            return Err("the checkpoint was not taken".into());
        }
        let index = match send(&mut c, Message::Query(Query::Checkpoints)) {
            (Some(Answer::Checkpoints(list)), _) if !list.is_empty() => list.len() as u32 - 1,
            _ => return Err("the checkpoint was not kept".into()),
        };
        let (answer, error) = send(
            &mut c,
            Message::Command(Command::Export {
                kind: ExportKind::Checkpoint { index },
                confirmed: true,
            }),
        );
        match answer {
            Some(Answer::Exported { bytes, .. }) if !error => {
                std::fs::write(out, bytes)
                    .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
                println!("checkpoint = {}", out.display());
            }
            _ => return Err("the checkpoint was not written".into()),
        }
    }
    run_to_end(&mut c)?;
    finish(&mut c, args, &scenario)
}

/// A run from a checkpoint file (RFC 009 rule 6; `resume`): the scenario and its modules as for
/// `run`, then to `--to` or to the end, with the same lines and exit codes.
fn cmd_resume(args: &Args) -> Result<ExitCode, String> {
    let checkpoint = PathBuf::from(args.positional.first().ok_or(usage())?);
    let scenario = PathBuf::from(args.positional.get(1).ok_or(usage())?);
    let mut c = controller(args)?;
    load(&mut c, &scenario, args)?;
    let (_, error) = send(
        &mut c,
        Message::Command(Command::LoadCheckpoint {
            bytes: read(&checkpoint)?,
        }),
    );
    if error {
        return Err("the checkpoint was not restored".into());
    }
    let (_, error) = send(
        &mut c,
        Message::Command(Command::Advance {
            unit: Unit::ToTick(args.to.unwrap_or(u64::MAX)),
        }),
    );
    if error {
        return Err("the run did not complete".into());
    }
    finish(&mut c, args, &scenario)
}

/// The state lines, the run record and the exit code, once a run has advanced as far as asked.
fn finish(
    c: &mut Controller<WasmtimeEngine>,
    args: &Args,
    scenario: &Path,
) -> Result<ExitCode, String> {
    let state = print_state(c).ok_or("no run state")?;
    let out = args.out.clone().unwrap_or_else(|| {
        let stem = scenario
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "run".into());
        PathBuf::from(format!("{stem}.florn-run"))
    });
    let (answer, error) = send(
        c,
        Message::Command(Command::Export {
            kind: ExportKind::RunRecord {
                trace: args.trace,
                payloads: args.payloads,
            },
            confirmed: true,
        }),
    );
    match answer {
        Some(Answer::Exported { bytes, .. }) if !error => {
            std::fs::write(&out, bytes)
                .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
            println!("run-record = {}", out.display());
        }
        _ => return Err("the run record was not written".into()),
    }
    // A run fault (a limit of RFC 001 rule 13, or a Host fault) is its own exit code, so that a
    // script can tell it from a refusal; the `ended` line names the reason.
    Ok(
        if state.ended.as_deref() != Some("end-reached") && args.to.is_none() {
            ExitCode::from(3)
        } else if state.reproducible {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        },
    )
}

fn cmd_replay(args: &Args) -> Result<ExitCode, String> {
    let record = PathBuf::from(args.positional.first().ok_or(usage())?);
    let scenario = PathBuf::from(args.positional.get(1).ok_or(usage())?);
    let mut c = controller(args)?;
    load(&mut c, &scenario, args)?;
    let (_, error) = send(
        &mut c,
        Message::Command(Command::Replay {
            record: read(&record)?,
        }),
    );
    if error {
        return Err("the replay was not started".into());
    }
    let replies = c.handle(Envelope::new(Message::Command(Command::Advance {
        unit: Unit::ToTick(u64::MAX),
    })));
    let mut outcome: Option<&str> = None;
    for m in &replies {
        if let Message::Notification(n) = m {
            match n.code.as_str() {
                "replay.matched" => outcome = Some("matched"),
                "replay.diverged" => outcome = Some("diverged"),
                _ => {}
            }
        }
    }
    report(replies);
    print_state(&mut c);
    match outcome {
        Some("matched") => {
            println!("replay = matched");
            Ok(ExitCode::SUCCESS)
        }
        Some(_) => {
            println!("replay = diverged");
            Ok(ExitCode::from(2))
        }
        None => Err("the replay reported no comparison".into()),
    }
}

fn cmd_admit(args: &Args) -> Result<ExitCode, String> {
    let module = PathBuf::from(args.positional.first().ok_or(usage())?);
    let mut c = controller(args)?;
    let (answer, error) = send(
        &mut c,
        Message::Command(Command::Admit {
            bytes: read(&module)?,
        }),
    );
    match answer {
        Some(Answer::Admitted {
            module: m,
            admitted_sha256,
        }) if !error => {
            println!("module = {}", module.display());
            println!("source = {}", m.source_sha256);
            println!("admitted = {admitted_sha256}");
            println!("contract = {}", m.contract);
            println!("kind = {}", m.kind);
            if let Some(d) = &m.description {
                println!("description = {d}");
            }
            for (kind, ty, min, max) in &m.port_kinds {
                println!("port-kind {kind}: type {ty}, {min} to {max}");
            }
            for p in &m.params {
                let mut line = format!("param {}: {}", p.name, p.ty);
                if let Some(u) = &p.unit {
                    line.push_str(&format!(" in {u}"));
                }
                if p.required {
                    line.push_str(", required");
                }
                if let Some(d) = &p.default {
                    line.push_str(&format!(", default {d}"));
                }
                println!("{line}");
            }
            for c in &m.channels {
                println!("channel {}: {}", c.channel, c.name);
            }
            for (limit, need) in &m.needs {
                println!("needs {limit} = {need}");
            }
            Ok(ExitCode::SUCCESS)
        }
        _ => Ok(ExitCode::from(1)),
    }
}

fn cmd_probe(args: &Args) -> Result<ExitCode, String> {
    let mut c = controller(args)?;
    match send(&mut c, Message::Query(Query::Capacity)) {
        (Some(Answer::Capacity(cap)), _) => {
            println!("instances = {}", cap.instances);
            println!("memory = {}", cap.memory_bytes);
            if let Some(s) = cap.swap_cost_us_per_kb {
                println!("swap-cost-us-per-kb = {s}");
            }
        }
        _ => return Err("no capacity".into()),
    }
    if let (Some(Answer::Support(s)), _) = send(&mut c, Message::Query(Query::Support)) {
        println!("engine = {} {}", s.engine, s.engine_version);
        println!("arch = {}", s.arch);
        println!("florn = {}", s.florn_version);
        println!("control = {}", s.control_version);
    }
    Ok(ExitCode::SUCCESS)
}

/// The runner over the process's arguments (without the program name); the exit code.
pub fn run(argv: Vec<String>) -> ExitCode {
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{}", usage());
            return ExitCode::from(1);
        }
    };
    let result = match args.command.as_str() {
        "run" => cmd_run(&args),
        "resume" => cmd_resume(&args),
        "replay" => cmd_replay(&args),
        "admit" => cmd_admit(&args),
        "probe" => cmd_probe(&args),
        "--version" | "version" => {
            println!("florn {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(usage().to_string()),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
