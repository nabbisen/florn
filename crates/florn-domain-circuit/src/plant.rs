//! The plant as the solver sees it (RFC 013 rule 3): the nets, the devices in ascending actor
//! identity with the nodes of their pins and the signals they read, and the unknowns of modified
//! nodal analysis in their fixed order, which depends on identities only (RFC 001 rule 16).

use crate::profile::Profile;

/// A node of the matrix: the index of a non-ground net, or `None` for the ground (voltage 0).
pub type Node = Option<usize>;

/// A net of the scenario: a connection of type `circuit/net`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Net {
    pub id: u32,
    pub name: String,
    pub ground: bool,
}

/// A signal of the scenario (rule 3), with the value it holds in its own unit (rule 5).
#[derive(Clone, Debug, PartialEq)]
pub struct Signal {
    pub id: u32,
    pub name: String,
    /// The unit it holds its value in; `None`: dimensionless.
    pub unit: Option<String>,
    pub dimension: Option<&'static str>,
    /// The unit's factor to the SI unit, 1 for a dimensionless signal.
    pub factor: f64,
    /// The held value (rule 4), in the signal's unit: before the producer's first output, its
    /// `initial`; at start, a `constant`'s value and a comparator's 0 (rule 30).
    pub held: f64,
    pub producer: Option<(u32, String)>,
}

/// What an element is, with its parameters in SI units, the nodes of its pins and the unknowns it
/// owns. A signal is an index into [`Plant::signals`].
#[derive(Clone, Debug, PartialEq)]
pub enum Element {
    Resistor {
        a: Node,
        b: Node,
        g: f64,
    },
    Capacitor {
        a: Node,
        b: Node,
        c: f64,
        ic: Option<f64>,
        dc: Option<usize>,
    },
    Inductor {
        a: Node,
        b: Node,
        l: f64,
        ic: Option<f64>,
        branch: usize,
    },
    VoltageSource {
        p: Node,
        n: Node,
        v: f64,
        gain: f64,
        cmd: Option<usize>,
        branch: usize,
    },
    CurrentSource {
        p: Node,
        n: Node,
        i: f64,
        gain: f64,
        cmd: Option<usize>,
    },
    Diode {
        a: Node,
        k: Node,
        is: f64,
        n: f64,
        gmin: f64,
    },
    Switch {
        a: Node,
        b: Node,
        ron: f64,
        roff: f64,
        threshold: f64,
        ctl: Option<usize>,
    },
    Voltmeter {
        p: Node,
        n: Node,
        out: Option<usize>,
        /// The identity of the `signal-out` port the sample is sent on.
        out_port: Option<u32>,
        period: u64,
        phase: u64,
    },
    Ammeter {
        a: Node,
        b: Node,
        out: Option<usize>,
        out_port: Option<u32>,
        period: u64,
        phase: u64,
        branch: usize,
    },
    Constant {
        out: Option<usize>,
        value: f64,
    },
    Comparator {
        p: Node,
        n: Node,
        out: Option<usize>,
        threshold: f64,
        hysteresis: f64,
    },
    Motor {
        p: Node,
        n: Node,
        r: f64,
        l: f64,
        kt: f64,
        ke: f64,
        j: f64,
        b: f64,
        load: Option<usize>,
        out: Option<usize>,
        out_port: Option<u32>,
        period: u64,
        phase: u64,
        branch: usize,
        state: usize,
    },
}

/// A built-in actor of the plant.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub actor: u32,
    pub name: String,
    pub kind: &'static str,
    pub element: Element,
}

/// The guest device as the solver sees it: its actor, the nodes of its pins in ascending port
/// identity (the order of the trial and of the stamps), and where it stands among the built-ins:
/// devices are visited in ascending actor identity, built-ins and the guest alike (RFC 013 rule 5),
/// so it is stamped before the built-in at index `before` of [`Plant::devices`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuestPins {
    pub actor: u32,
    pub name: String,
    pub nodes: Vec<Node>,
    pub before: usize,
}

/// What an unknown is, for the code that names it (`unknown-kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownKind {
    Voltage,
    Current,
    State,
}

impl UnknownKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Voltage => "voltage",
            Self::Current => "current",
            Self::State => "state",
        }
    }
}

/// One unknown: what it is called in a notification and what kind it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unknown {
    pub name: String,
    pub kind: UnknownKind,
}

/// The plant: everything the solver needs, built once from the validated scenario.
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub profile: Profile,
    /// The tick's length in seconds, the scenario's exact decimal rounded once to the nearest
    /// `f64` at composition (RFC 013 rule 14): a step of `n` ticks is `n × tick` seconds.
    pub tick: f64,
    /// The non-ground nets in ascending connection identity: net `k` is unknown `k`.
    pub nets: Vec<Net>,
    pub signals: Vec<Signal>,
    /// In ascending actor identity (RFC 013 rule 5: the order of every evaluation).
    pub devices: Vec<Device>,
    /// The unknowns of rule 3 in their order: the net voltages, the branch currents, the states,
    /// and the branch currents the initial solve adds for a capacitor with an `ic`.
    pub unknowns: Vec<Unknown>,
    /// How many of the unknowns are rule 3's (the others exist in the initial solve only).
    pub transient_unknowns: usize,
    /// The signal (an index into [`Plant::signals`]) a port is on, by actor and port identity:
    /// a producer's, a built-in consumer's.
    pub port_signal: std::collections::BTreeMap<(u32, u32), usize>,
    /// The nodes of the pins of every guest actor, in ascending port identity.
    pub module_pins: std::collections::BTreeMap<u32, Vec<Node>>,
    /// The scenario's names of the guest actors.
    pub module_names: std::collections::BTreeMap<u32, String>,
    /// The guest device model of the scenario, once the start hook has found it (RFC 012 rule 19):
    /// at most one.
    pub guest: Option<GuestPins>,
    /// The scenario's end tick (exclusive, RFC 004 section 3.3) and the circuit domain's world
    /// identity (RFC 004 rule 8): the target and the origin of the plant's own events.
    pub end: u64,
    pub world: u32,
}

impl Plant {
    /// The guest device of the scenario is the actor `actor`: its pins and its place in the order
    /// of the evaluation. `None` when the actor has no pins in the plant.
    pub fn set_guest(&mut self, actor: u32) {
        self.guest = self.module_pins.get(&actor).map(|nodes| GuestPins {
            actor,
            name: self.module_names.get(&actor).cloned().unwrap_or_default(),
            nodes: nodes.clone(),
            before: self.devices.iter().take_while(|d| d.actor < actor).count(),
        });
    }

    /// The voltages of the guest device's pins at the unknowns `x`, in ascending port identity: the
    /// trial of an evaluation and the solution of `accept-step` (RFC 012 rule 19).
    pub fn guest_voltages(&self, x: &[f64]) -> Vec<f64> {
        self.guest.as_ref().map_or_else(Vec::new, |g| {
            g.nodes.iter().map(|n| n.map_or(0.0, |k| x[k])).collect()
        })
    }

    /// The size of the matrix of the initial solve.
    pub fn size(&self) -> usize {
        self.unknowns.len()
    }

    /// The bytes the solver needs for `n` unknowns and `max_breakpoints` pending breakpoints
    /// (RFC 013 rule 18): the dense matrix, the vectors and history, the breakpoints.
    pub fn estimate(n: u64, max_breakpoints: u64) -> u64 {
        8 * n * n + 8 * 6 * n + 32 * max_breakpoints
    }
}
