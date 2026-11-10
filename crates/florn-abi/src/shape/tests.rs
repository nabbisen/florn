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
    assert_eq!(Shape::of(&[F64, F64], &[]), None);
    assert_eq!(Shape::from_number(0), None);
}

#[test]
fn the_circuit_calls_are_rows_at_the_end_and_nothing_is_renumbered() {
    // RFC 012 finding F-1: `on-sample(i64, i64)`, `begin-step(i64, f64)`, `reject-step()`,
    // `evaluate`'s export `(i32, i32) -> i32` and `set-output(i32, f64)`.
    let rows = [
        (12, vec![I64, I64], vec![]),
        (13, vec![I64, F64], vec![]),
        (14, vec![], vec![]),
        (15, vec![I32, I32], vec![I32]),
        (16, vec![I32, F64], vec![]),
    ];
    for (number, params, results) in rows {
        let shape = Shape::of(&params, &results).unwrap_or_else(|| panic!("row {number}"));
        assert_eq!(shape.number(), number);
        assert_eq!(shape.params(), params.as_slice());
        assert_eq!(shape.results(), results.as_slice());
    }
    // The rows that were there keep their numbers (the browser adapter's trampolines are keyed by
    // them).
    assert_eq!(Shape::INIT.number(), 1);
    assert_eq!(Shape::REALLOC.number(), 4);
    assert_eq!(Shape::of(&[I32], &[I32]).unwrap().number(), 11);
    assert_eq!(Shape::all().count(), 16);
    // The post-return call is the shape of `cancel-timer`.
    assert_eq!(Shape::POST_RETURN, Shape::of(&[I32], &[]).unwrap());
}

#[test]
fn no_two_rows_have_the_same_flat_signature() {
    // `Shape::of` finds a row by its signature; two rows with one signature would make the second
    // unreachable and the browser adapter's trampoline of it dead.
    for (i, a) in TABLE.iter().enumerate() {
        for b in &TABLE[i + 1..] {
            assert!(
                a.params != b.params || a.results != b.results,
                "rows {} and {} are one signature",
                a.number,
                b.number
            );
        }
    }
}
