//! The shipped scenarios (`scenarios/` at the workspace root; RFC 005 rule 42; AUTO-003): each a
//! file the headless runner takes, with its digest recorded in `scenarios/digests.toml` by
//! `florn-conformance scenarios`. The kit's test runs every one under the native Host and
//! compares; the browser Host compares the same file (AC-001).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::guests;
use crate::scenario::{Scenario, actor};

/// A shipped scenario and what the kit expects of it.
pub struct Shipped {
    pub scenario: Scenario,
    /// Whether the run ends at `end` (else with the run fault of RFC 001 rule 13).
    pub ends_normally: bool,
}

fn echo_pair() -> Scenario {
    Scenario::new("echo-pair", 10_000)
        .seed(42)
        .actor(
            actor(1, "pinger", 1)
                .with("period", "7")
                .with("payload", "16"),
        )
        .actor(actor(2, "echo", 1))
        .connect(&[(1, 0), (2, 0)])
}

/// Two origins race for one instant at the echo, 10,000 times (AC-004).
fn race(tie: &'static str, reversed: bool) -> Scenario {
    let name = format!("race-{tie}{}", if reversed { "-reversed" } else { "" });
    let mut actors = vec![
        actor(1, "pinger", 1)
            .with("period", "7")
            .with("payload", "8"),
        actor(2, "pinger", 1)
            .with("period", "7")
            .with("payload", "8"),
        actor(3, "echo", 2),
    ];
    if reversed {
        actors.reverse();
    }
    let mut s = Scenario::new(&name, 7 * 10_000 + 1).tie(tie).seed(99);
    for a in actors {
        s = s.actor(a);
    }
    s.connect(&[(1, 0), (3, 0)]).connect(&[(2, 0), (3, 1)])
}

fn hostile(name: &str, guest: &'static str) -> Scenario {
    Scenario::new(name, 1_000)
        .actor(actor(1, guest, 1))
        .actor(actor(2, "echo", 0))
}

/// Every shipped scenario, in the order of the digests file.
pub fn all() -> Vec<Shipped> {
    let normal = |scenario: Scenario| Shipped {
        scenario,
        ends_normally: true,
    };
    vec![
        normal(echo_pair()),
        normal(race("by-identity", false)),
        normal(race("by-identity", true)),
        normal(race("by-seed", false)),
        normal(race("by-seed", true)),
        // The benign neighbour alone: the reference for the hostile scenarios (AC-002).
        normal(Scenario::new("neighbour", 1_000).actor(actor(2, "echo", 0))),
        normal(hostile("hostile-loop", "hostile_loop")),
        normal(hostile("hostile-recurse", "hostile_recurse")),
        normal(hostile("hostile-grow", "hostile_grow")),
        normal(hostile("hostile-flood", "hostile_flood")),
        normal(hostile("hostile-bad-pointer", "hostile_bad_pointer")),
        // Two echoing pingers, one of them sending at init: a chain without delay that ends the
        // run with the fault of RFC 001 rule 13.
        Shipped {
            scenario: Scenario::new("zero-delay", 100)
                .actor(
                    actor(1, "pinger", 1)
                        .with("period", "0")
                        .with("at-init", "true")
                        .with("echo", "true"),
                )
                .actor(
                    actor(2, "pinger", 1)
                        .with("period", "0")
                        .with("echo", "true"),
                )
                .connect(&[(1, 0), (2, 0)]),
            ends_normally: false,
        },
        // The heap guest alone, 10,000 timer activations: the export and restore reference (AC-006).
        normal(
            Scenario::new("heap", 10_001)
                .limit("memory", "\"4MiB\"")
                .actor(actor(1, "heap", 0)),
        ),
        // The Host-path benchmarks: one message per tick to the benchmark guest, which replies.
        normal(
            Scenario::new("bench-protocol", 10_000)
                .actor(actor(1, "pinger", 1).with("payload", "1500"))
                .actor(actor(2, "bench_protocol", 1))
                .connect(&[(1, 0), (2, 0)]),
        ),
        normal(
            Scenario::new("bench-compute", 2_000)
                .actor(actor(1, "pinger", 1).with("payload", "8"))
                .actor(actor(2, "bench_compute", 1))
                .connect(&[(1, 0), (2, 0)]),
        ),
    ]
}

/// The `scenarios/` directory.
pub fn directory() -> PathBuf {
    guests::workspace().join("scenarios")
}

/// The header every shipped file carries.
pub fn header(name: &str) -> String {
    format!(
        "# {name}: a scenario of the conformance kit, written by `florn-conformance scenarios`.\n# Its digest is recorded in digests.toml; the readme says how to run it.\n\n"
    )
}

/// What one run of a shipped scenario produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    pub seed: u64,
    pub chain: String,
    pub records: u64,
    pub ended: String,
}

/// The recorded digests: by scenario name, plus the module and admitted hashes by module name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Digests {
    pub arch: String,
    pub runs: BTreeMap<String, Recorded>,
    pub modules: BTreeMap<String, String>,
    pub admitted: BTreeMap<String, String>,
}

impl Digests {
    pub fn to_toml(&self) -> String {
        let mut t = String::from(
            "# Recorded digests of the shipped scenarios (AUTO-003), written by\n# `cargo run --release -p florn-conformance -- scenarios`. The kit's test runs every\n# scenario and compares; continuous integration does so on three architectures.\n\n",
        );
        t.push_str(&format!("recorded-on = \"{}\"\n", self.arch));
        t.push_str("\n[modules]\n");
        for (k, v) in &self.modules {
            t.push_str(&format!("{k} = \"{v}\"\n"));
        }
        t.push_str("\n[admitted]\n");
        for (k, v) in &self.admitted {
            t.push_str(&format!("{k} = \"{v}\"\n"));
        }
        for (name, r) in &self.runs {
            t.push_str(&format!(
                "\n[runs.{name:?}]\nseed = {}\nchain = \"{}\"\nrecords = {}\nended = \"{}\"\n",
                r.seed, r.chain, r.records, r.ended
            ));
        }
        t
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("digests.toml: {e}"))?;
        let strings = |key: &str| -> BTreeMap<String, String> {
            table
                .get(key)
                .and_then(|v| v.as_table())
                .map(|t| {
                    t.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut runs = BTreeMap::new();
        if let Some(t) = table.get("runs").and_then(|v| v.as_table()) {
            for (name, r) in t {
                let get = |k: &str| r.get(k).ok_or_else(|| format!("{name}: no {k}"));
                runs.insert(
                    name.clone(),
                    Recorded {
                        seed: get("seed")?.as_integer().unwrap_or(0) as u64,
                        chain: get("chain")?.as_str().unwrap_or("").to_string(),
                        records: get("records")?.as_integer().unwrap_or(0) as u64,
                        ended: get("ended")?.as_str().unwrap_or("").to_string(),
                    },
                );
            }
        }
        Ok(Self {
            arch: table
                .get("recorded-on")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            runs,
            modules: strings("modules"),
            admitted: strings("admitted"),
        })
    }

    pub fn read(dir: &Path) -> Result<Self, String> {
        let path = dir.join("digests.toml");
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text)
    }
}

/// Run every shipped scenario under the native Host and record what it produced, with the
/// admitted hash of every SDK guest under the kit's limits (RFC 003 §10, transformed bytes).
pub fn record() -> Digests {
    let mut d = Digests {
        arch: std::env::consts::ARCH.to_string(),
        modules: guests::hashes(),
        ..Default::default()
    };
    for name in guests::SDK {
        if let Ok(a) = florn_admission::Admission::default().admit(
            name,
            &guests::bytes(name),
            &crate::host::kit_limits(),
        ) {
            d.admitted.insert(name.to_string(), a.admitted_hex());
        }
    }
    for s in all() {
        let run = crate::host::run(&s.scenario);
        d.runs.insert(
            s.scenario.name.clone(),
            Recorded {
                seed: s.scenario.seed,
                chain: run.chain_hex(),
                records: run.kernel().trace.records(),
                ended: crate::host::ended(&run).as_str().to_string(),
            },
        );
    }
    d
}

/// Write every scenario file and the digests into `dir`.
pub fn write(dir: &Path) -> Result<Digests, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let hashes = guests::hashes();
    for s in all() {
        let path = dir.join(format!("{}.toml", s.scenario.name));
        let text = header(&s.scenario.name) + &s.scenario.text(&hashes);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let d = record();
    let path = dir.join("digests.toml");
    std::fs::write(&path, d.to_toml()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(d)
}
