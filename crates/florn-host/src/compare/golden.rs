//! A run against a results file (RFC 013 rule 21): "every other Host and engine is compared to the
//! file by rule 20, as a second run". The file holds the rows of a recorded run (`results`), so the
//! comparison is the one of rule 20 over two row sequences, the tolerance the scenario's domains
//! declare, and nothing else is read. Native tests, the browser Host's worker and the kit's command
//! all call it, so that every engine is judged by the same function.

use super::results;
use super::{Agreement, Difference, Reading, Row, compare};
use crate::domain::Tolerance;

/// How a run stands against a results file.
#[derive(Clone, Debug, PartialEq)]
pub enum Golden {
    /// Every row is the same: the results are bit-identical (and so are the consumed work and the
    /// draws).
    Matched { compared: u64 },
    /// Every pair of rows agrees under rule 20 and at least one differs in a numeric field or in
    /// an exempt value.
    WithinTolerance(Agreement),
    /// A pair does not agree, or the counts differ: the first record that differs.
    Diverged(Box<Difference>),
    /// The file is not a results file: what is wrong with it.
    Unreadable(String),
}

impl Golden {
    /// The word the comparison is printed with (RFC 013 rule 20).
    pub fn word(&self) -> &'static str {
        match self {
            Self::Matched { .. } => "matched",
            Self::WithinTolerance(_) => "within-tolerance",
            Self::Diverged(_) => "diverged",
            Self::Unreadable(_) => "unreadable",
        }
    }

    /// Whether the run agrees with the file: `matched` or `within-tolerance`.
    pub fn agrees(&self) -> bool {
        matches!(self, Self::Matched { .. } | Self::WithinTolerance(_))
    }

    /// The pairs compared, where the comparison got to the end.
    pub fn compared(&self) -> u64 {
        match self {
            Self::Matched { compared } => *compared,
            Self::WithinTolerance(a) => a.compared,
            _ => 0,
        }
    }

    /// The pairs with a numeric field that differs.
    pub fn differing(&self) -> u64 {
        match self {
            Self::WithinTolerance(a) => a.differing,
            _ => 0,
        }
    }

    /// The pairs of a continuous actor whose consumed work or draws differ (exempt, rule 20 item 4).
    pub fn consumption_differing(&self) -> u64 {
        match self {
            Self::WithinTolerance(a) => a.consumption_differing,
            _ => 0,
        }
    }

    /// One line for a log or a table: the word and the figures.
    pub fn line(&self) -> String {
        match self {
            Self::Matched { compared } => format!("matched, {compared} records"),
            Self::WithinTolerance(a) => format!(
                "within-tolerance, {} records, {} differing, {} with other work",
                a.compared, a.differing, a.consumption_differing
            ),
            Self::Diverged(d) => match &**d {
                Difference::Record { record, what } => {
                    format!("diverged at record {record}: {}", what.as_str())
                }
                Difference::Value {
                    record,
                    field,
                    recorded,
                    replayed,
                } => format!(
                    "diverged at record {record}: {field} is {} {} and {} {}",
                    replayed.0, replayed.1, recorded.0, recorded.1
                ),
            },
            Self::Unreadable(why) => format!("unreadable: {why}"),
        }
    }
}

/// The rows `run` produced against the text of a results file. The tolerance is the domains'
/// (`reading`); a scenario of discrete domains has none, and its rows must be the same.
pub fn against(file: &str, run: &[Row], reading: &dyn Reading) -> Golden {
    let recorded = match results::parse(file) {
        Ok(rows) => rows,
        Err(why) => return Golden::Unreadable(why),
    };
    let exact = Tolerance {
        atol: 0.0,
        rtol: 0.0,
    };
    match compare(
        &recorded,
        run,
        reading.tolerance().unwrap_or(exact),
        reading,
    ) {
        Ok(a) if a.differing == 0 && a.consumption_differing == 0 => Golden::Matched {
            compared: a.compared,
        },
        Ok(a) => Golden::WithinTolerance(a),
        Err(d) => Golden::Diverged(d),
    }
}

#[cfg(test)]
mod tests;
