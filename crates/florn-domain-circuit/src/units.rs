//! The units the domain registers, each with its dimension (RFC 012 rule 5; finding F-13): the
//! nine SI-derived symbols with the prefixes `p`, `n`, `u` (alias `µ`), `m`, `k`, `M`, `G`, and the
//! five composite units with none. The factor of a unit is the exact decimal of its prefix: the
//! Host converts a quantity by exact decimal arithmetic and rounds once (RFC 001 rule 63).

use florn_admission::config::{UnitDef, Units};
use florn_model::Decimal;

const fn factor(exponent: i32) -> Decimal {
    Decimal {
        mantissa: 1,
        exponent,
    }
}

macro_rules! family {
    ($symbol:literal, $dimension:literal, $alias:literal) => {
        [
            UnitDef {
                symbol: $symbol,
                factor: factor(0),
                dimension: $dimension,
            },
            UnitDef {
                symbol: concat!("p", $symbol),
                factor: factor(-12),
                dimension: $dimension,
            },
            UnitDef {
                symbol: concat!("n", $symbol),
                factor: factor(-9),
                dimension: $dimension,
            },
            UnitDef {
                symbol: concat!("u", $symbol),
                factor: factor(-6),
                dimension: $dimension,
            },
            UnitDef {
                symbol: concat!("µ", $symbol),
                factor: factor(-6),
                dimension: $dimension,
            },
            UnitDef {
                symbol: concat!("m", $symbol),
                factor: factor(-3),
                dimension: $dimension,
            },
            UnitDef {
                symbol: concat!("k", $symbol),
                factor: factor(3),
                dimension: $dimension,
            },
            UnitDef {
                symbol: concat!("M", $symbol),
                factor: factor(6),
                dimension: $dimension,
            },
            UnitDef {
                symbol: concat!("G", $symbol),
                factor: factor(9),
                dimension: $dimension,
            },
        ]
    };
}

/// The prefixed families, by base symbol. `Ω` is the alias of `ohm` (rule 5).
const FAMILIES: [[UnitDef; 9]; 10] = [
    family!("V", "voltage", ""),
    family!("A", "current", ""),
    family!("ohm", "resistance", ""),
    family!("Ω", "resistance", ""),
    family!("S", "conductance", ""),
    family!("F", "capacitance", ""),
    family!("H", "inductance", ""),
    family!("W", "power", ""),
    family!("Hz", "frequency", ""),
    family!("Nm", "torque", ""),
];

/// The composite units, with no prefixes.
const COMPOSITE: [UnitDef; 5] = [
    UnitDef {
        symbol: "rad/s",
        factor: factor(0),
        dimension: "angular-speed",
    },
    UnitDef {
        symbol: "kgm2",
        factor: factor(0),
        dimension: "inertia",
    },
    UnitDef {
        symbol: "Nms",
        factor: factor(0),
        dimension: "friction",
    },
    UnitDef {
        symbol: "Nm/A",
        factor: factor(0),
        dimension: "torque-per-current",
    },
    UnitDef {
        symbol: "Vs/rad",
        factor: factor(0),
        dimension: "voltage-per-speed",
    },
];

/// Every unit of the domain, for `Domain::units`.
pub fn all() -> Vec<UnitDef> {
    FAMILIES
        .iter()
        .flatten()
        .chain(COMPOSITE.iter())
        .copied()
        .collect()
}

/// The domain's units as the config encoder and the domain's own attribute reading look them up.
#[derive(Clone, Copy, Debug, Default)]
pub struct CircuitUnits;

impl Units for CircuitUnits {
    fn unit(&self, symbol: &str) -> Option<UnitDef> {
        FAMILIES
            .iter()
            .flatten()
            .chain(COMPOSITE.iter())
            .find(|u| u.symbol == symbol)
            .copied()
    }
}

#[cfg(test)]
mod tests;
