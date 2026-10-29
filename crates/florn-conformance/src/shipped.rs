//! The shipped scenarios (`scenarios/` at the workspace root; RFC 005 rule 42; AUTO-003): each a
//! file the headless runner takes, over the shipped modules of `scenarios/modules`, with its
//! digest recorded in `scenarios/digests.toml` by `florn-conformance scenarios`. The kit's test
//! runs every one under the native Host and compares; the browser Host compares the same file
//! (AC-001).
//!
//! The modules are shipped as bytes because a build of the same guest source with the same
//! toolchain differs between host architectures (measured in CI: every module built on an
//! AArch64 host differs from the x86-64 build; two x86-64 hosts agree). One digest across
//! machines needs one set of bytes; the fresh build is compared with the shipped one by
//! [`fresh_report`], which the figures print.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::guests;
use crate::scenario::{Scenario, actor, builtin, router};

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

/// Two routers over one link (RFC 008 §12): `a1` sends a frame every `period` ticks through
/// its interface, `a2` echoes it back when `echo` is on.
pub fn net_pair(name: &str, end: u64, period: u64, payload: u64, echo: bool) -> Scenario {
    Scenario::new(name, end)
        .domain("network")
        .actor(
            router(1, "net_echo", 1)
                .with("period", period.to_string())
                .with("payload", payload.to_string())
                .with("echo", "false"),
        )
        .actor(router(2, "net_echo", 1).with("echo", if echo { "true" } else { "false" }))
}

/// Distance-vector routers with two interfaces each, `period` ticks between advertisements.
fn dv(id: u32, period: u64, infinity: u64) -> crate::scenario::Actor {
    router(id, "distance_vector", 2)
        .with("period", format!("\"{period}ns\""))
        .with("infinity", infinity.to_string())
}

/// An OSPF router with `hello` ticks between hellos and `dead` ticks before a silent
/// neighbour is down, in `area`.
fn ospf(id: u32, interfaces: u32, hello: u64, dead: u64, area: u64) -> crate::scenario::Actor {
    let mut a = router(id, "ospf", interfaces)
        .with("hello", format!("\"{hello}ns\""))
        .with("dead", format!("\"{dead}ns\""));
    if area != 0 {
        a = a.with("area", area.to_string());
    }
    a
}

/// The learning lab of NET-010 (RFC 008 rule 21): four OSPF routers in a ring with a costly
/// chord r1-r3, hellos every 50 ticks, a neighbour dead after 200; the link r2-r3 breaks at
/// 1500 and is repaired at 3000, so that the adjacencies form, the database floods, the
/// routes converge, reroute the long way or over the chord, and converge back.
pub fn ospf_lab() -> Scenario {
    Scenario::new("ospf-lab", 4_500)
        .domain("network")
        .actor(ospf(1, 3, 50, 200, 0))
        .actor(ospf(2, 2, 50, 200, 0))
        .actor(ospf(3, 3, 50, 200, 0))
        .actor(ospf(4, 2, 50, 200, 0))
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(2, 1), (3, 0)], &[("delay", "\"2ns\"")])
        .change(1_500, &[("enabled", "false")])
        .change(3_000, &[("enabled", "true")])
        .link("network/link", &[(3, 1), (4, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(4, 1), (1, 1)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(1, 2), (3, 2)], &[("delay", "\"2ns\"")])
        .per_port((1, 2), &[("cost", "5")])
        .per_port((3, 2), &[("cost", "5")])
}

/// A scenario with a feature outside the subset (checklist C21): r2 is in area 1, so it logs
/// its refusal and stays idle while the run goes on; r1 keeps saying hello.
pub fn ospf_outside() -> Scenario {
    Scenario::new("ospf-outside", 600)
        .domain("network")
        .actor(ospf(1, 1, 50, 200, 0))
        .actor(ospf(2, 1, 50, 200, 1))
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")])
}

/// The triangle of RFC 008 §12 (NET-012): three routers, the link r1-r2 down at 1000 and up at
/// 2000, so that the routes converge, reroute through r3 and converge back.
pub fn dv_triangle(period: u64) -> Scenario {
    Scenario::new("dv-triangle", 3_000)
        .domain("network")
        .actor(dv(1, period, 16))
        .actor(dv(2, period, 16))
        .actor(dv(3, period, 16))
        .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")])
        .change(1_000, &[("enabled", "false")])
        .change(2_000, &[("enabled", "true")])
        .link("network/link", &[(2, 1), (3, 0)], &[("delay", "\"2ns\"")])
        .link("network/link", &[(3, 1), (1, 1)], &[("delay", "\"2ns\"")])
}

fn hostile(name: &str, guest: &'static str) -> Scenario {
    Scenario::new(name, 1_000)
        .actor(actor(1, guest, 1))
        .actor(actor(2, "echo", 0))
}

impl Shipped {
    /// The sender's `count`: how many frames `a1` sends in all.
    fn with_count(mut self, count: u64) -> Self {
        let a = &mut self.scenario.actors[0];
        a.config.push(("count", count.to_string()));
        self
    }
}

/// Every shipped scenario, in the order of the digests file.
pub fn all() -> Vec<Shipped> {
    let mut list = definitions();
    for s in &mut list {
        s.scenario.shipped = true;
    }
    list
}

/// The scenarios over a fresh build of the guests.
fn definitions() -> Vec<Shipped> {
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
        // The matrix product needs more than the default budget of one activation.
        normal(
            Scenario::new("bench-compute", 2_000)
                .limit("budget", "100000000")
                .actor(actor(1, "pinger", 1).with("payload", "8"))
                .actor(actor(2, "bench_compute", 1))
                .connect(&[(1, 0), (2, 0)]),
        ),
        // The network domain (RFC 008 §12). Delay and serialisation: a 1,500-byte frame over a
        // 20 ns link at 1000 Gbit/s arrives 32 ticks after it was sent, and the echo 32 later.
        normal(net_pair("net-delay", 1_000, 50, 1500, true).link(
            "network/link",
            &[(1, 0), (2, 0)],
            &[("delay", "\"20ns\""), ("bandwidth", "\"1000Gbit/s\"")],
        )),
        // Loss: 10,000 frames at loss 0.1, drawn from the link's own stream.
        normal(net_pair("net-loss", 10_100, 1, 64, false).link(
            "network/link",
            &[(1, 0), (2, 0)],
            &[("loss", "0.1")],
        ))
        .with_count(10_000),
        // Jitter: every arrival in [delay, delay + jitter].
        normal(net_pair("net-jitter", 2_000, 10, 64, false).link(
            "network/link",
            &[(1, 0), (2, 0)],
            &[("delay", "\"5ns\""), ("jitter", "\"7ns\"")],
        )),
        // Link down at 300 and up at 600: `on-link` at both ends, frames in flight arrive,
        // sends while down are dropped.
        normal(
            net_pair("net-link-down", 1_000, 10, 64, true)
                .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"3ns\"")])
                .per_port((1, 0), &[("cost", "7")])
                .change(300, &[("enabled", "false")])
                .change(600, &[("enabled", "true")]),
        ),
        // A lan of three: a frame reaches the two others, each with its own loss draw.
        normal(
            net_pair("net-lan", 500, 10, 64, false)
                .actor(router(3, "net_echo", 1).with("echo", "false"))
                .link(
                    "network/lan",
                    &[(1, 0), (2, 0), (3, 0)],
                    &[("delay", "\"2ns\""), ("loss", "0.2")],
                ),
        ),
        // The ladder's second rung (RFC 008 rule 20): the triangle, and a chain of four whose
        // far ends are beyond `infinity = 3`.
        normal(dv_triangle(100)),
        normal(
            Scenario::new("dv-chain", 1_500)
                .domain("network")
                .actor(dv(1, 100, 3))
                .actor(dv(2, 100, 3))
                .actor(dv(3, 100, 3))
                .actor(dv(4, 100, 3))
                .link("network/link", &[(1, 0), (2, 0)], &[("delay", "\"2ns\"")])
                .link("network/link", &[(2, 1), (3, 0)], &[("delay", "\"2ns\"")])
                .link("network/link", &[(3, 1), (4, 0)], &[("delay", "\"2ns\"")]),
        ),
        // The OSPF subset (RFC 008 rule 21): the learning lab, and a router outside the subset.
        normal(ospf_lab()),
        normal(ospf_outside()),
        // The blackhole built-in drops every frame and reports the count.
        normal(
            Scenario::new("net-blackhole", 200)
                .domain("network")
                .actor(
                    router(1, "net_echo", 1)
                        .with("period", "5")
                        .with("echo", "false"),
                )
                .actor(builtin(2, "network/blackhole", "network/interface", 1))
                .link("network/link", &[(1, 0), (2, 0)], &[]),
        ),
    ]
}

/// The `scenarios/` directory.
pub fn directory() -> PathBuf {
    guests::workspace().join("scenarios")
}

/// The shipped modules: `scenarios/modules`.
pub fn modules_dir() -> PathBuf {
    directory().join("modules")
}

/// The bytes of a shipped module.
pub fn module_bytes(name: &str) -> Vec<u8> {
    let path = modules_dir().join(format!("{name}.wasm"));
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (write the shipped modules with `florn-conformance scenarios`)",
            path.display()
        )
    })
}

/// The SHA-256 of every shipped module, by name.
pub fn module_hashes() -> BTreeMap<String, String> {
    guests::SDK
        .iter()
        .map(|n| (n.to_string(), florn_host::run::sha256_hex(&module_bytes(n))))
        .collect()
}

/// How this host's fresh build of the guests compares with the shipped modules: per guest,
/// whether the bytes are equal; per scenario, whether a run over the fresh build gives the
/// recorded chain (RFC 001 Tier 2 across build hosts).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FreshReport {
    pub guests: Vec<(String, bool)>,
    pub scenarios: Vec<(String, bool)>,
}

pub fn fresh_report(recorded: &Digests) -> FreshReport {
    let fresh = guests::hashes();
    let shipped = module_hashes();
    let guests = guests::SDK
        .iter()
        .map(|n| (n.to_string(), fresh.get(*n) == shipped.get(*n)))
        .collect();
    let scenarios = definitions()
        .into_iter()
        .map(|s| {
            let run = crate::host::run(&s.scenario);
            let equal = recorded
                .runs
                .get(&s.scenario.name)
                .is_some_and(|r| r.chain == run.chain_hex());
            (s.scenario.name, equal)
        })
        .collect();
    FreshReport { guests, scenarios }
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
    /// The admitted hashes under cost-model version 2 (RFC 010), beside the default's.
    pub admitted_2: BTreeMap<String, String>,
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
        t.push_str("\n[admitted-2]\n");
        for (k, v) in &self.admitted_2 {
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
            admitted_2: strings("admitted-2"),
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
        modules: module_hashes(),
        ..Default::default()
    };
    for name in guests::SDK {
        let mut admission = crate::host::admission();
        if let Ok(a) = admission.admit(name, &module_bytes(name), &crate::host::kit_limits()) {
            d.admitted.insert(name.to_string(), a.admitted_hex());
        }
        if let Ok(a) =
            admission.admit_under(name, &module_bytes(name), &crate::host::kit_limits(), 2)
        {
            d.admitted_2.insert(name.to_string(), a.admitted_hex());
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
    let modules = dir.join("modules");
    std::fs::create_dir_all(&modules).map_err(|e| format!("{}: {e}", modules.display()))?;
    for n in guests::SDK {
        let path = modules.join(format!("{n}.wasm"));
        std::fs::write(&path, guests::bytes(n)).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    // The crossing benchmark's guest, for the browser Host's run of the benchmark.
    let path = modules.join("kit-crossing.wasm");
    std::fs::write(&path, guests::crossing()).map_err(|e| format!("{}: {e}", path.display()))?;
    let hashes = module_hashes();
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
