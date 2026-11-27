//! `florn-conformance`: the kit's command.
//!
//! ```text
//! florn-conformance scenarios [--out <dir>]   write the shipped scenarios and their digests
//! florn-conformance figures [--quick]         run the benchmarks and print the figures
//! florn-conformance cross-engine              this machine's results against the recorded files, as a table
//! florn-conformance results <record> <scenario> [--modules <dir>] [--out <file>]
//!                                             the results file of a run record with its trace and payloads
//! ```
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("scenarios") => {
            let out = match args.iter().position(|a| a == "--out") {
                Some(i) => match args.get(i + 1) {
                    Some(d) => PathBuf::from(d),
                    None => return usage(),
                },
                None => florn_conformance::shipped::directory(),
            };
            match florn_conformance::shipped::write(&out) {
                Ok(d) => {
                    for (name, r) in &d.runs {
                        println!(
                            "{name}: seed {} chain {} records {} ended {}",
                            r.seed, r.chain, r.records, r.ended
                        );
                    }
                    println!("written to {}", out.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("figures") => {
            let quick = args.iter().any(|a| a == "--quick");
            let f = florn_conformance::bench::figures(quick);
            print!("{}", f.markdown());
            let (table, _) =
                florn_conformance::goldens::report(&florn_conformance::goldens::directory());
            print!("\n### Results across engines (RFC 013 rules 19 to 21)\n\n{table}");
            ExitCode::SUCCESS
        }
        Some("cross-engine") => {
            let (table, good) =
                florn_conformance::goldens::report(&florn_conformance::goldens::directory());
            print!("{table}");
            if good {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Some("results") => results(&args[1..]),
        _ => usage(),
    }
}

/// `results <record> <scenario> [--modules <dir>] [--out <file>]`: the text on standard output, or
/// in the file.
fn results(args: &[String]) -> ExitCode {
    let (mut positional, mut dir, mut out) = (Vec::new(), None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--modules" => dir = it.next().map(PathBuf::from),
            "--out" => out = it.next().map(PathBuf::from),
            _ => positional.push(PathBuf::from(a)),
        }
    }
    let [record, scenario] = positional.as_slice() else {
        return usage();
    };
    let read =
        |p: &PathBuf| std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()));
    let run = || -> Result<String, String> {
        let mut modules = Vec::new();
        if let Some(d) = &dir {
            let mut files: Vec<PathBuf> = std::fs::read_dir(d)
                .map_err(|e| format!("cannot read {}: {e}", d.display()))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "wasm"))
                .collect();
            files.sort();
            for f in files {
                modules.push(read(&f)?);
            }
        }
        florn_conformance::results::from_record(
            florn_conformance::host::domains(),
            &read(record)?,
            &read(scenario)?,
            &modules,
        )
    };
    match run() {
        Ok(text) => match out {
            Some(path) => match std::fs::write(&path, text) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("cannot write {}: {e}", path.display());
                    ExitCode::from(1)
                }
            },
            None => {
                print!("{text}");
                ExitCode::SUCCESS
            }
        },
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  florn-conformance scenarios [--out <dir>]\n  florn-conformance figures [--quick]\n  florn-conformance cross-engine\n  florn-conformance results <record> <scenario> [--modules <dir>] [--out <file>]"
    );
    ExitCode::from(1)
}
