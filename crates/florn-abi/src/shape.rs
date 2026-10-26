//! The flat call shapes the Hosts can call and bind (RFC 008 rule 6). A world's export or import
//! lowers to one of them, or admission refuses the module with `module.export-unsupported`: the
//! browser adapter's trampolines are compiled for exactly this table, so a signature outside it
//! would be a run-time surprise there. Both adapters and the admission step read the table from
//! here, which keeps the refusal the same on every Host.

use crate::FlatType;

/// A flat shape: its number is what the browser adapter's trampolines are keyed by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Shape(u32);

struct Row {
    number: u32,
    params: &'static [FlatType],
    results: &'static [FlatType],
}

use FlatType::{I32, I64};

/// The table, in the order the numbers were assigned. Rows are never renumbered; a new signature
/// is a new row at the end.
const TABLE: &[Row] = &[
    // The core world's exports (RFC 002 rule 20) and `cabi_realloc`.
    Row {
        number: 1,
        params: &[I64, I32, I32, I32, I32, I32],
        results: &[],
    },
    Row {
        number: 2,
        params: &[I64, I32, I32, I32],
        results: &[],
    },
    Row {
        number: 3,
        params: &[I64, I32],
        results: &[],
    },
    Row {
        number: 4,
        params: &[I32, I32, I32, I32],
        results: &[I32],
    },
    // The core world's imports.
    Row {
        number: 5,
        params: &[I32, I32, I32],
        results: &[],
    },
    Row {
        number: 6,
        params: &[I32, I64],
        results: &[],
    },
    Row {
        number: 7,
        params: &[I32],
        results: &[],
    },
    Row {
        number: 8,
        params: &[],
        results: &[I64],
    },
    // Beyond the core: an event with one small value (`on-link(now, port, up)`); a query by
    // port answered through a return pointer (`link-info(port) -> record`); a query by port
    // with one flat result.
    Row {
        number: 9,
        params: &[I64, I32, I32],
        results: &[],
    },
    Row {
        number: 10,
        params: &[I32, I32],
        results: &[],
    },
    Row {
        number: 11,
        params: &[I32],
        results: &[I32],
    },
];

impl Shape {
    pub const INIT: Shape = Shape(1);
    pub const MESSAGE: Shape = Shape(2);
    pub const TIMER: Shape = Shape(3);
    pub const REALLOC: Shape = Shape(4);

    /// The shape of a flat signature, if the table has it.
    pub fn of(params: &[FlatType], results: &[FlatType]) -> Option<Shape> {
        TABLE
            .iter()
            .find(|r| r.params == params && r.results == results)
            .map(|r| Shape(r.number))
    }

    pub fn from_number(n: u32) -> Option<Shape> {
        TABLE
            .iter()
            .find(|r| r.number == n)
            .map(|r| Shape(r.number))
    }

    pub fn number(self) -> u32 {
        self.0
    }

    fn row(self) -> &'static Row {
        TABLE
            .iter()
            .find(|r| r.number == self.0)
            .expect("a shape comes from the table")
    }

    pub fn params(self) -> &'static [FlatType] {
        self.row().params
    }

    pub fn results(self) -> &'static [FlatType] {
        self.row().results
    }

    /// Every shape, in table order.
    pub fn all() -> impl Iterator<Item = Shape> {
        TABLE.iter().map(|r| Shape(r.number))
    }

    /// The most flat parameters any shape carries.
    pub const MAX_PARAMS: usize = 6;
    /// How many imports of one shape beyond the core's a world may have: the browser adapter
    /// compiles that many trampolines per shape (RFC 008 rule 6).
    pub const IMPORT_SLOTS: u32 = 4;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_signatures_are_in_the_table() {
        use crate::core_world;
        for s in core_world::actor_functions() {
            let params: Vec<FlatType> = s
                .param_types()
                .iter()
                .flat_map(|t| t.flat_types())
                .collect();
            assert!(Shape::of(&params, &[]).is_some(), "{}", s.name);
        }
        for s in core_world::host_functions() {
            let params: Vec<FlatType> = s
                .param_types()
                .iter()
                .flat_map(|t| t.flat_types())
                .collect();
            let results: Vec<FlatType> = s.result.as_ref().map_or(Vec::new(), |t| t.flat_types());
            assert!(Shape::of(&params, &results).is_some(), "{}", s.name);
        }
        assert_eq!(
            Shape::of(&[I32, I32, I32, I32], &[I32]),
            Some(Shape::REALLOC)
        );
    }

    #[test]
    fn numbers_are_unique_and_bounded() {
        let mut seen = std::collections::BTreeSet::new();
        for r in TABLE {
            assert!(seen.insert(r.number), "shape {} twice", r.number);
            assert!(r.params.len() <= Shape::MAX_PARAMS);
            assert!(r.results.len() <= 1);
        }
        assert_eq!(Shape::of(&[I64, I64], &[]), None);
        assert_eq!(Shape::from_number(0), None);
    }
}
