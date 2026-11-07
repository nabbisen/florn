use super::*;

#[test]
fn seeds_differ_and_the_clock_moves_forward() {
    let mut p = NativePlatform::new();
    let a = p.new_seed();
    let b = p.new_seed();
    assert_ne!(a, b);
    assert!(p.wall_clock_ms() > 1_700_000_000_000);
    assert_eq!(p.capacity().instances, NativePlatform::DEFAULT_INSTANCES);
}
