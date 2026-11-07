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
