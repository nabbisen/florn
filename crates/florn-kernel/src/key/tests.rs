use super::*;

#[test]
fn key_order_is_field_order() {
    let base = EventKey {
        tick: 5,
        microstep: 1,
        class: 2,
        tie: 0,
        origin: 7,
        seq: 3,
    };
    assert!(
        EventKey {
            tick: 4,
            microstep: 9,
            ..base
        } < base
    );
    assert!(
        EventKey {
            microstep: 0,
            class: 9,
            ..base
        } < base
    );
    assert!(
        EventKey {
            class: 1,
            origin: 99,
            ..base
        } < base
    );
    assert!(
        EventKey {
            tie: 0,
            origin: 6,
            ..base
        } < base
    );
    assert!(EventKey { seq: 2, ..base } < base);
}

#[test]
fn tie_is_a_function_of_seed_instant_class_origin() {
    let a = TieHasher::new(42);
    let b = TieHasher::new(42);
    let c = TieHasher::new(43);
    let i = Instant::new(10, 0);
    assert_eq!(a.tie(i, 1, 1), b.tie(i, 1, 1));
    assert_ne!(a.tie(i, 1, 1), c.tie(i, 1, 1));
    assert_ne!(a.tie(i, 1, 1), a.tie(i, 1, 2));
    assert_ne!(a.tie(i, 1, 1), a.tie(Instant::new(11, 0), 1, 1));
}
