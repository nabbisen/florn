//! The circuit domain under the kit's Host (RFC 012; RFC 013): the domain with the native Host's
//! maxima, a builder of scenario text over built-ins and nets, and the scenarios the circuit tests
//! share. A circuit scenario has no guest, so it needs no module.

use std::collections::BTreeMap;

use florn_admission::Admission;
use florn_control::Notification;
use florn_domain_circuit::{CircuitDomain, Limits};
use florn_engine_wasmtime::WasmtimeEngine;
use florn_host::{Domains, HostError, Options, Run, Seed};
use florn_model::{HostMaxima, Manifest};
use florn_platform::FixedPlatform;

use crate::host::{self};

pub use crate::host::Host;

/// The Host's domains for a circuit scenario: the core and the circuit domain with the native
/// Host's maxima (RFC 013 rule 18).
pub fn domains() -> Domains {
    Domains::core().with(|| CircuitDomain::new(Limits::native()))
}

/// The same with the browser Host's maxima.
pub fn browser_domains() -> Domains {
    Domains::core().with(|| CircuitDomain::new(Limits::browser()))
}

/// A circuit scenario under construction. Parts are built-ins with the ports of their kind;
/// nets and signals name their ports as `actor.port`.
#[derive(Clone, Debug)]
pub struct Deck {
    name: String,
    end: String,
    profile: String,
    actors: String,
    connections: Vec<(String, String, Vec<String>, String)>,
    /// The guest modules of the scenario, by name: what `Run::new` is given.
    modules: BTreeMap<String, Vec<u8>>,
}

impl Deck {
    pub fn new(name: &str, end: &str) -> Self {
        Self {
            name: name.to_string(),
            end: end.to_string(),
            profile: String::new(),
            actors: String::new(),
            connections: Vec::new(),
            modules: BTreeMap::new(),
        }
    }

    /// A guest actor of the module `module` (its bytes given) with the ports given: a name and a
    /// type each.
    pub fn guest(
        mut self,
        id: u32,
        name: &str,
        module: &str,
        bytes: Vec<u8>,
        ports: &[(&str, &str)],
    ) -> Self {
        let list: Vec<String> = ports
            .iter()
            .enumerate()
            .map(|(i, (n, t))| format!("{{ id = {i}, name = \"{n}\", type = \"{t}\" }}"))
            .collect();
        self.actors.push_str(&format!(
            "\n[[actors]]\nid = {id}\nname = \"{name}\"\nkind = \"module\"\nmodule = \"{module}\"\nports = [{}]\n",
            list.join(", ")
        ));
        self.modules.insert(module.to_string(), bytes);
        self
    }

    /// The `[circuit]` table, as TOML lines (dotted keys for the subtables).
    pub fn profile(mut self, lines: &str) -> Self {
        self.profile = lines.to_string();
        self
    }

    /// A built-in with the given ports (name and type) and `[actors.config]` lines.
    pub fn part_with(
        mut self,
        id: u32,
        name: &str,
        kind: &str,
        ports: &[(&str, &str)],
        config: &str,
    ) -> Self {
        let list: Vec<String> = ports
            .iter()
            .enumerate()
            .map(|(i, (n, t))| format!("{{ id = {i}, name = \"{n}\", type = \"{t}\" }}"))
            .collect();
        self.actors.push_str(&format!(
            "\n[[actors]]\nid = {id}\nname = \"{name}\"\nkind = \"builtin\"\nbuiltin = \"circuit/{kind}\"\nports = [{}]\n[actors.config]\n{config}\n",
            list.join(", ")
        ));
        self
    }

    /// A built-in with the ports of its kind (the optional signal input `cmd` or `load` included
    /// when `optional` names it).
    pub fn part(self, id: u32, name: &str, kind: &str, optional: &[&str], config: &str) -> Self {
        let pins: &[&str] = match kind {
            "resistor" | "capacitor" | "inductor" | "switch" | "ammeter" => &["a", "b"],
            "voltage-source" | "current-source" | "voltmeter" | "comparator" => &["p", "n"],
            "diode" => &["a", "k"],
            "dc-motor" => &["a+", "a-"],
            _ => &[],
        };
        let mut ports: Vec<(&str, &str)> = pins.iter().map(|p| (*p, "circuit/pin")).collect();
        match kind {
            "switch" => ports.push(("ctl", "circuit/signal-in")),
            "voltmeter" => ports.push(("v", "circuit/signal-out")),
            "ammeter" => ports.push(("i", "circuit/signal-out")),
            "constant" => ports.push(("y", "circuit/signal-out")),
            "comparator" => ports.push(("y", "circuit/signal-out")),
            "dc-motor" => ports.push(("speed", "circuit/signal-out")),
            _ => {}
        }
        for name_ in optional {
            ports.push((name_, "circuit/signal-in"));
        }
        self.part_with(id, name, kind, &ports, config)
    }

    fn connection(mut self, name: &str, ty: &str, pins: &[&str], attrs: &str) -> Self {
        self.connections.push((
            name.to_string(),
            ty.to_string(),
            pins.iter().map(|p| p.to_string()).collect(),
            attrs.to_string(),
        ));
        self
    }

    pub fn net(self, name: &str, pins: &[&str]) -> Self {
        self.connection(name, "circuit/net", pins, "")
    }

    pub fn ground(self, name: &str, pins: &[&str]) -> Self {
        self.connection(name, "circuit/net", pins, "ground = true")
    }

    /// A net with attribute lines (`ground = "yes"`, to provoke a refusal).
    pub fn connection_with_attrs(self, name: &str, pins: &[&str], attrs: &str) -> Self {
        self.connection(name, "circuit/net", pins, attrs)
    }

    pub fn signal(self, name: &str, ports: &[&str], attrs: &str) -> Self {
        self.connection(name, "circuit/signal", ports, attrs)
    }

    /// The connection identity of the n-th connection written (they are numbered from 100).
    pub fn connection_id(n: usize) -> u32 {
        100 + n as u32
    }

    pub fn text(&self) -> String {
        let modules: String = self
            .modules
            .iter()
            .map(|(name, bytes)| {
                format!(
                    "\n[modules.{name}]\nsha256 = \"{}\"\ncontract = \"florn:core/guest@0.1.0\"\n",
                    florn_host::run::sha256_hex(bytes)
                )
            })
            .collect();
        let mut text = format!(
            "florn = 1\nname = \"{}\"\ndomains = [\"core\", \"circuit\"]\n\n[time]\ntick = \"1ns\"\nend = \"{}\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 1\n\n[limits.default]\nmemory = \"2MiB\"\n{modules}\n[circuit]\n{}\n{}",
            self.name, self.end, self.profile, self.actors
        );
        for (i, (name, ty, pins, attrs)) in self.connections.iter().enumerate() {
            let ports: Vec<String> = pins.iter().map(|p| format!("\"{p}\"")).collect();
            let attrs = if attrs.is_empty() {
                String::new()
            } else {
                format!("[connections.attrs]\n{attrs}\n")
            };
            text.push_str(&format!(
                "\n[[connections]]\nid = {}\nname = \"{name}\"\ntype = \"{ty}\"\nports = [{}]\n{attrs}",
                Self::connection_id(i),
                ports.join(", ")
            ));
        }
        text
    }

    pub fn manifest(&self) -> Manifest {
        let text = self.text();
        Manifest::parse(&text, &HostMaxima::native())
            .unwrap_or_else(|e| panic!("{:?}\n{text}", e.codes()))
    }
}

/// Start a run of a deck under the Host with the given domains.
pub fn try_start_in(domains: &Domains, deck: &Deck, options: Options) -> Result<Host, HostError> {
    let manifest = deck.manifest();
    let seed = manifest.seed;
    Run::new(
        host::engine(),
        Admission::default(),
        &mut FixedPlatform::default(),
        domains,
        manifest,
        &deck.modules,
        Seed::Given(seed),
        options,
    )
}

pub fn try_start(deck: &Deck) -> Result<Host, HostError> {
    try_start_in(&domains(), deck, host::options())
}

pub fn start(deck: &Deck) -> Host {
    try_start(deck).unwrap_or_else(|e| panic!("{}: {e}\n{}", deck.name, deck.text()))
}

/// What a run of this engine type is refused with: the notifications of the domains, or the codes
/// of the core's refusals.
pub fn refusals(r: Result<Host, HostError>) -> Vec<Notification> {
    match r {
        Ok(_) => Vec::new(),
        Err(HostError::Domain(n)) => n,
        Err(HostError::Refused(e)) => e
            .codes()
            .into_iter()
            .map(|c| {
                Notification::new(
                    c,
                    florn_control::Severity::Error,
                    florn_control::Scope::default(),
                    florn_control::Ends::Command,
                )
            })
            .collect(),
        Err(other) => panic!("{other}"),
    }
}

/// `rc-step` at DC (RFC 013 rule 21): a 1 kohm resistor and a 1 uF capacitor with `ic = 0V` on a
/// voltage source of 1 V, a voltmeter across the capacitor with period 100 us, to 5 ms, the
/// default profile. The scheduled change to 0 V at 2 ms and the samples come with the transient
/// (PR-45 and PR-46); at tick 0 the operating point is the capacitor's `ic`.
pub fn rc_step() -> Deck {
    Deck::new("rc-step", "5ms")
        .part(1, "vs", "voltage-source", &[], "v = \"1V\"")
        .part(2, "r", "resistor", &[], "r = \"1kohm\"")
        .part(3, "c", "capacitor", &[], "c = \"1uF\"\nic = \"0V\"")
        .part(4, "vm", "voltmeter", &[], "period = \"100us\"")
        .net("in", &["vs.p", "r.a"])
        .net("cap", &["r.b", "c.a", "vm.p"])
        .ground("gnd", &["vs.n", "c.b", "vm.n"])
}

/// The engine this module's runs use, for a test that builds its own.
pub fn engine() -> WasmtimeEngine {
    host::engine()
}
