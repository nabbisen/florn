//! The built-in kinds of RFC 012 rule 23: each with its fixed ports, its typed parameters and the
//! one value a scenario may change while the run goes (rule 15). The table is the RFC's, row by
//! row; the validation reads it and the plant is built from it.

/// What a port is: a pin on a net, or the input or output of a signal (rule 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Pin,
    SignalIn,
    SignalOut,
}

impl Role {
    /// The port type a scenario gives a port of this role.
    pub fn port_type(self) -> &'static str {
        match self {
            Self::Pin => crate::PIN,
            Self::SignalIn => crate::SIGNAL_IN,
            Self::SignalOut => crate::SIGNAL_OUT,
        }
    }
}

/// A port of a kind. A signal port a kind marks `optional` may be absent from the actor without a
/// warning; one that is declared and on no signal reads 0 (rule 4) and is warned about.
#[derive(Clone, Copy, Debug)]
pub struct PortSpec {
    pub name: &'static str,
    pub role: Role,
    pub optional: bool,
    /// The dimension a signal on this port holds, `None` for a dimensionless port (rule 23's
    /// `cmd`, `ctl` and the comparator's `y`) and for the `constant`'s, which takes its value's.
    pub dimension: Option<&'static str>,
}

/// The values a number or a quantity may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    Any,
    /// Above zero.
    Positive,
    /// Zero or more.
    NonNegative,
}

impl Range {
    pub fn holds(self, x: f64) -> bool {
        match self {
            Self::Any => true,
            Self::Positive => x > 0.0,
            Self::NonNegative => x >= 0.0,
        }
    }
    pub fn words(self) -> &'static str {
        match self {
            Self::Any => "",
            Self::Positive => " above zero",
            Self::NonNegative => " of zero or more",
        }
    }
}

/// The type of a parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    /// A quantity of a dimension, written with its unit, held in the SI unit.
    Real(&'static str, Range),
    /// A plain number.
    Number(Range),
    /// A time in whole ticks (RFC 004 rule 25).
    Ticks,
    /// The `constant`'s value: a quantity in the unit of its signal, or a plain number (rule 23).
    Held,
}

/// A parameter of a kind: its name, its type, its default (`None`: required, or absent when it is
/// optional).
#[derive(Clone, Copy, Debug)]
pub struct ParamSpec {
    pub name: &'static str,
    pub ty: Ty,
    pub default: Option<f64>,
    pub optional: bool,
}

const fn required(name: &'static str, ty: Ty) -> ParamSpec {
    ParamSpec {
        name,
        ty,
        default: None,
        optional: false,
    }
}
const fn optional(name: &'static str, ty: Ty) -> ParamSpec {
    ParamSpec {
        name,
        ty,
        default: None,
        optional: true,
    }
}
const fn defaulted(name: &'static str, ty: Ty, default: f64) -> ParamSpec {
    ParamSpec {
        name,
        ty,
        default: Some(default),
        optional: false,
    }
}

const fn pin(name: &'static str) -> PortSpec {
    PortSpec {
        name,
        role: Role::Pin,
        optional: false,
        dimension: None,
    }
}
const fn input(name: &'static str, optional: bool, dimension: Option<&'static str>) -> PortSpec {
    PortSpec {
        name,
        role: Role::SignalIn,
        optional,
        dimension,
    }
}
const fn output(name: &'static str, dimension: Option<&'static str>) -> PortSpec {
    PortSpec {
        name,
        role: Role::SignalOut,
        optional: false,
        dimension,
    }
}

/// A kind: its name after the domain prefix, its ports, its parameters and its settable value.
#[derive(Clone, Copy, Debug)]
pub struct Kind {
    pub name: &'static str,
    pub ports: &'static [PortSpec],
    pub params: &'static [ParamSpec],
    /// The parameter a scheduled change or a `source` intervention sets (rule 15).
    pub settable: Option<&'static str>,
    /// Whether the kind samples (a sensor, rule 10): it has a `period` and a `phase`.
    pub sensor: bool,
}

const R: Ty = Ty::Real("resistance", Range::Positive);
const VOLT: Ty = Ty::Real("voltage", Range::Any);
const AMP: Ty = Ty::Real("current", Range::Any);

/// The `a−` pin of the motor is spelled with U+2212, as RFC 012 rule 23 writes it.
pub const MOTOR_NEGATIVE: &str = "a\u{2212}";

const PERIOD: ParamSpec = ParamSpec {
    name: "period",
    ty: Ty::Ticks,
    default: None,
    optional: false,
};
const PHASE: ParamSpec = ParamSpec {
    name: "phase",
    ty: Ty::Ticks,
    default: Some(0.0),
    optional: false,
};

/// Every kind, in the RFC's order.
pub static KINDS: &[Kind] = &[
    Kind {
        name: "resistor",
        ports: &[pin("a"), pin("b")],
        params: &[required("r", R)],
        settable: None,
        sensor: false,
    },
    Kind {
        name: "capacitor",
        ports: &[pin("a"), pin("b")],
        params: &[
            required("c", Ty::Real("capacitance", Range::Positive)),
            optional("ic", VOLT),
        ],
        settable: None,
        sensor: false,
    },
    Kind {
        name: "inductor",
        ports: &[pin("a"), pin("b")],
        params: &[
            required("l", Ty::Real("inductance", Range::Positive)),
            optional("ic", AMP),
        ],
        settable: None,
        sensor: false,
    },
    Kind {
        name: "voltage-source",
        ports: &[pin("p"), pin("n"), input("cmd", true, None)],
        params: &[defaulted("v", VOLT, 0.0), defaulted("gain", VOLT, 1.0)],
        settable: Some("v"),
        sensor: false,
    },
    Kind {
        name: "current-source",
        ports: &[pin("p"), pin("n"), input("cmd", true, None)],
        params: &[defaulted("i", AMP, 0.0), defaulted("gain", AMP, 1.0)],
        settable: Some("i"),
        sensor: false,
    },
    Kind {
        name: "diode",
        ports: &[pin("a"), pin("k")],
        params: &[
            defaulted("is", Ty::Real("current", Range::Positive), 1e-14),
            defaulted("n", Ty::Number(Range::Positive), 1.0),
            defaulted("gmin", Ty::Real("conductance", Range::NonNegative), 1e-12),
        ],
        settable: None,
        sensor: false,
    },
    Kind {
        name: "switch",
        ports: &[pin("a"), pin("b"), input("ctl", false, None)],
        params: &[
            required("ron", R),
            required("roff", R),
            defaulted("threshold", Ty::Number(Range::Any), 0.5),
        ],
        settable: None,
        sensor: false,
    },
    Kind {
        name: "voltmeter",
        ports: &[pin("p"), pin("n"), output("v", Some("voltage"))],
        params: &[PERIOD, PHASE],
        settable: None,
        sensor: true,
    },
    Kind {
        name: "ammeter",
        ports: &[pin("a"), pin("b"), output("i", Some("current"))],
        params: &[PERIOD, PHASE],
        settable: None,
        sensor: true,
    },
    Kind {
        name: "constant",
        ports: &[output("y", None)],
        params: &[defaulted("value", Ty::Held, 0.0)],
        settable: Some("value"),
        sensor: false,
    },
    Kind {
        name: "comparator",
        ports: &[pin("p"), pin("n"), output("y", None)],
        params: &[
            defaulted("threshold", VOLT, 0.0),
            defaulted("hysteresis", Ty::Real("voltage", Range::NonNegative), 0.0),
        ],
        settable: Some("threshold"),
        sensor: false,
    },
    Kind {
        name: "dc-motor",
        ports: &[
            pin("a+"),
            pin(MOTOR_NEGATIVE),
            output("speed", Some("angular-speed")),
            input("load", true, Some("torque")),
        ],
        params: &[
            required("r", R),
            required("l", Ty::Real("inductance", Range::Positive)),
            required("kt", Ty::Real("torque-per-current", Range::Positive)),
            required("ke", Ty::Real("voltage-per-speed", Range::Positive)),
            required("j", Ty::Real("inertia", Range::Positive)),
            required("b", Ty::Real("friction", Range::NonNegative)),
            PERIOD,
            PHASE,
        ],
        settable: None,
        sensor: true,
    },
];

/// The kind named by a built-in's `builtin = "circuit/<name>"`.
pub fn by_builtin(builtin: &str) -> Option<&'static Kind> {
    let name = builtin.strip_prefix("circuit/")?;
    KINDS.iter().find(|k| k.name == name)
}

impl Kind {
    /// The name a scenario writes: `circuit/<name>`.
    pub fn full_name(&self) -> &'static str {
        match self.name {
            "resistor" => "circuit/resistor",
            "capacitor" => "circuit/capacitor",
            "inductor" => "circuit/inductor",
            "voltage-source" => "circuit/voltage-source",
            "current-source" => "circuit/current-source",
            "diode" => "circuit/diode",
            "switch" => "circuit/switch",
            "voltmeter" => "circuit/voltmeter",
            "ammeter" => "circuit/ammeter",
            "constant" => "circuit/constant",
            "comparator" => "circuit/comparator",
            _ => "circuit/dc-motor",
        }
    }
    pub fn port(&self, name: &str) -> Option<&'static PortSpec> {
        self.ports.iter().find(|p| p.name == name)
    }
    pub fn param(&self, name: &str) -> Option<&'static ParamSpec> {
        self.params.iter().find(|p| p.name == name)
    }
    /// The parameter names, for the `expected` of `circuit.param-unknown`.
    pub fn param_names(&self) -> String {
        self.params
            .iter()
            .map(|p| p.name)
            .collect::<Vec<_>>()
            .join(", ")
    }
    /// The channels a sensor or a detector reports on (rule 24): `probe` is 1 and `condition` is 2.
    pub fn channels(&self) -> Vec<u32> {
        match self.name {
            "voltmeter" | "ammeter" | "dc-motor" => vec![crate::CHANNEL_PROBE],
            "comparator" => vec![crate::CHANNEL_CONDITION],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;
