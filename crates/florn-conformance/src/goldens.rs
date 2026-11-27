//! The goldens of the circuit across engines (RFC 013 rules 19 to 21; AUTO-004): for each circuit
//! scenario the kit records, natively on x86-64, a results file `scenarios/results/<name>.results`,
//! and the table of `bit_identity` in `scenarios/results/bit-identity.table`. Every other Host and
//! engine is compared to them by the comparison of rule 20 as a second run: natively on AArch64 by
//! `tests/circuit_results.rs`, in the browsers by `ci/browser/run.js` through the Host's own
//! `compare::golden`. The cross-engine table the figures print is made here.

use std::path::{Path, PathBuf};

use florn_domain_circuit::bits;
use florn_host::compare::golden::Golden;

use crate::{host, shipped};

/// The circuit scenarios that have a results file (RFC 012 rule 27, as far as they are shipped): a
/// scenario of a later unit joins the list with its own file.
pub const CIRCUIT: &[&str] = &["rc-step", "motor-loop"];

/// The name of the bit-identity table's file.
pub const BITS_FILE: &str = "bit-identity.table";

/// `scenarios/results`.
pub fn directory() -> PathBuf {
    shipped::directory().join("results")
}

/// The file of a circuit scenario's results.
pub fn results_file(name: &str) -> PathBuf {
    directory().join(format!("{name}.results"))
}

/// A shipped scenario by name.
fn shipped_scenario(name: &str) -> crate::scenario::Scenario {
    shipped::all()
        .into_iter()
        .map(|s| s.scenario)
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("{name} is not a shipped scenario"))
}

/// Run a circuit scenario over the shipped modules under the native Host, to its end.
pub fn run(name: &str) -> host::Host {
    host::run(&shipped_scenario(name))
}

/// The results text of a run that kept its records and payloads (the kit's runs do).
pub fn text_of(run: &host::Host) -> String {
    let rows = run.rows().expect("the run kept its records and payloads");
    florn_host::compare::results::write(&rows)
}

/// The results of a circuit scenario run natively on this machine.
pub fn results_of(name: &str) -> String {
    text_of(&run(name))
}

/// Write every results file and the bit-identity table into `dir`.
pub fn write(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for name in CIRCUIT {
        let path = dir.join(format!("{name}.results"));
        std::fs::write(&path, results_of(name)).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let path = dir.join(BITS_FILE);
    std::fs::write(&path, bits::render(&bits::table()))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// How this machine's run of a scenario stands against its recorded file.
pub struct Standing {
    pub name: String,
    pub golden: Golden,
}

/// Every circuit scenario run here and compared with its file.
pub fn standings(dir: &Path) -> Vec<Standing> {
    CIRCUIT
        .iter()
        .map(|name| {
            let run = run(name);
            let file = std::fs::read_to_string(dir.join(format!("{name}.results")))
                .unwrap_or_else(|e| panic!("{name}.results: {e} (write them with `scenarios`)"));
            let golden = run
                .against_results(&file)
                .unwrap_or_else(|m| Golden::Unreadable(format!("the run lacks {m:?}")));
            Standing {
                name: name.to_string(),
                golden,
            }
        })
        .collect()
}

/// `bit_identity` here against the recorded table: the number of lines that agree, or what
/// differs.
pub fn bit_identity(dir: &Path) -> Result<usize, Vec<String>> {
    let file = std::fs::read_to_string(dir.join(BITS_FILE))
        .map_err(|e| vec![format!("{BITS_FILE}: {e} (write it with `scenarios`)")])?;
    bits::compare(&file, &bits::table())
}

/// The machine's name in the table: the architecture and the operating system.
pub fn engine_name() -> String {
    format!(
        "native {} ({})",
        std::env::consts::ARCH,
        std::env::consts::OS
    )
}

/// One engine's column of the cross-engine table, as Markdown: a row per circuit scenario with the
/// outcome and the figures, and the row of `bit_identity`.
pub fn table(engine: &str, standings: &[Standing], bits: &Result<usize, Vec<String>>) -> String {
    let mut t = format!(
        "| {engine} | Outcome | Records | Differing | Other work |\n|---|---|---:|---:|---:|\n"
    );
    for s in standings {
        t.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            s.name,
            s.golden.word(),
            s.golden.compared(),
            s.golden.differing(),
            s.golden.consumption_differing()
        ));
    }
    match bits {
        Ok(n) => t.push_str(&format!("| `bit_identity` | matched | {n} | 0 | — |\n")),
        Err(e) => t.push_str(&format!(
            "| `bit_identity` | diverged | — | {} | — |\n",
            e.len()
        )),
    }
    t
}

/// This machine's part of the cross-engine table: every circuit scenario and `bit_identity` against
/// the recorded files, as Markdown, and whether every row agrees. The browsers' rows are printed
/// by `ci/browser/run.js` in the same form.
pub fn report(dir: &Path) -> (String, bool) {
    let standings = standings(dir);
    let bits = bit_identity(dir);
    let good = bits.is_ok() && standings.iter().all(|s| s.golden.agrees());
    let mut text = table(&engine_name(), &standings, &bits);
    if let Err(lines) = &bits {
        for l in lines {
            text.push_str(&format!("\n`bit_identity`: {l}\n"));
        }
    }
    for s in standings.iter().filter(|s| !s.golden.agrees()) {
        text.push_str(&format!("\n`{}`: {}\n", s.name, s.golden.line()));
    }
    (text, good)
}
