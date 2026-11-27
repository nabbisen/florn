use florn_kernel::{EventKey, Identity};

use super::*;
use crate::compare::{Effect, Number, Payload, RecordDifference};
use crate::domain::{Numeric, Site};

/// A reading with a tolerance and a set of continuous actors; the rows are built by hand.
struct Fake {
    tolerance: Option<Tolerance>,
    continuous: Vec<Identity>,
}

impl Reading for Fake {
    fn tolerance(&self) -> Option<Tolerance> {
        self.tolerance
    }
    fn continuous_actor(&self, actor: Identity) -> bool {
        self.continuous.contains(&actor)
    }
    fn numeric_fields(&self, _: Identity, _: Site, _: &[u8]) -> Option<Numeric> {
        None
    }
}

const TOLERANCE: Tolerance = Tolerance {
    atol: 1e-9,
    rtol: 1e-6,
};

fn continuous() -> Fake {
    Fake {
        tolerance: Some(TOLERANCE),
        continuous: vec![3],
    }
}

fn row(tick: u64, work: u64, value: f64) -> Row {
    Row::Handled {
        key: EventKey {
            tick,
            microstep: 0,
            class: 7,
            tie: 3,
            origin: 1,
            seq: 1,
        },
        target: 3,
        work,
        draws: 0,
        effects: vec![Effect::Report {
            seq: 0,
            channel: 1,
            len: 8,
            payload: Payload::Numbers {
                fields: vec![Number {
                    name: "v".into(),
                    unit: "V".into(),
                    factor: 1.0f64.to_bits(),
                    value: value.to_bits(),
                }],
                rest: *blake3::hash(&[]).as_bytes(),
            },
        }],
    }
}

fn file(rows: &[Row]) -> String {
    results::write(rows)
}

#[test]
fn the_same_rows_are_matched_and_the_word_says_so() {
    let rows = vec![row(1, 5, 1.0), row(2, 5, 2.0)];
    let g = against(&file(&rows), &rows, &continuous());
    assert_eq!(g, Golden::Matched { compared: 2 });
    assert_eq!(g.word(), "matched");
    assert!(g.agrees());
    assert_eq!((g.compared(), g.differing()), (2, 0));
    assert_eq!(g.line(), "matched, 2 records");
}

#[test]
fn a_last_bit_is_within_tolerance_and_a_difference_beyond_it_is_diverged() {
    let golden = file(&[row(1, 5, 1.0), row(2, 5, 2.0)]);
    let last_bit = f64::from_bits(2.0f64.to_bits() + 1);
    let g = against(
        &golden,
        &[row(1, 5, 1.0), row(2, 5, last_bit)],
        &continuous(),
    );
    assert_eq!(g.word(), "within-tolerance");
    assert!(g.agrees());
    assert_eq!((g.compared(), g.differing()), (2, 1));
    assert_eq!(
        g.line(),
        "within-tolerance, 2 records, 1 differing, 0 with other work"
    );
    let g = against(&golden, &[row(1, 5, 1.0), row(2, 5, 2.5)], &continuous());
    assert_eq!(g.word(), "diverged");
    assert!(!g.agrees());
    assert!(
        g.line()
            .starts_with("diverged at record 2: v is 2.5 V and 2 V"),
        "{}",
        g.line()
    );
    assert_eq!((g.compared(), g.differing()), (0, 0));
}

#[test]
fn the_work_of_a_continuous_actor_alone_is_within_tolerance_and_a_discrete_actor_is_not() {
    let golden = file(&[row(1, 5, 1.0)]);
    let g = against(&golden, &[row(1, 6, 1.0)], &continuous());
    assert_eq!(g.word(), "within-tolerance");
    assert_eq!((g.differing(), g.consumption_differing()), (0, 1));
    // The same difference for an actor that is not of a continuous class is a structure.
    let discrete = Fake {
        tolerance: Some(TOLERANCE),
        continuous: vec![],
    };
    match against(&golden, &[row(1, 6, 1.0)], &discrete) {
        Golden::Diverged(d) => assert_eq!(
            *d,
            Difference::Record {
                record: 1,
                what: RecordDifference::Structure
            }
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_scenario_without_a_tolerance_is_compared_exactly() {
    let none = Fake {
        tolerance: None,
        continuous: vec![3],
    };
    let golden = file(&[row(1, 5, 1.0)]);
    assert_eq!(against(&golden, &[row(1, 5, 1.0)], &none).word(), "matched");
    let last_bit = f64::from_bits(1.0f64.to_bits() + 1);
    assert_eq!(
        against(&golden, &[row(1, 5, last_bit)], &none).word(),
        "diverged"
    );
}

#[test]
fn a_count_that_differs_and_a_file_that_is_not_one_are_named() {
    let golden = file(&[row(1, 5, 1.0), row(2, 5, 2.0)]);
    match against(&golden, &[row(1, 5, 1.0)], &continuous()) {
        Golden::Diverged(d) => assert_eq!(
            *d,
            Difference::Record {
                record: 2,
                what: RecordDifference::Length
            }
        ),
        other => panic!("{other:?}"),
    }
    let g = against("not a results file\n", &[], &continuous());
    assert_eq!(g.word(), "unreadable");
    assert!(!g.agrees());
    assert!(g.line().starts_with("unreadable: "), "{}", g.line());
}
