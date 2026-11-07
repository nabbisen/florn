use super::*;

#[test]
fn delays() {
    let i = Instant::new(10, 3);
    assert_eq!(i.after(0), Some(Instant::new(10, 4)));
    assert_eq!(i.after(5), Some(Instant::new(15, 0)));
    assert_eq!(Instant::new(MAX_TICK, 0).after(1), None);
    assert_eq!(
        Instant::new(MAX_TICK - 1, 0).after(1),
        Some(Instant::new(MAX_TICK, 0))
    );
    assert!(Instant::new(1, 0) < Instant::new(1, 1));
    assert!(Instant::new(1, 9) < Instant::new(2, 0));
}
