use super::*;

#[test]
fn zoom_keeps_the_point_under_the_cursor() {
    let mut v = Viewport::default();
    let s = Point { x: 123.0, y: 456.0 };
    let before = v.screen_to_world(s);
    v.zoom_at(s, 1.7);
    let after = v.screen_to_world(s);
    assert!((before.x - after.x).abs() < 1e-3 && (before.y - after.y).abs() < 1e-3);
}

#[test]
fn flights_arrive_and_details_parse() {
    let f = Flight {
        connection: 0,
        port: 0,
        sent: 100,
        delay: 50.0,
    };
    assert_eq!(f.progress(100), 0.0);
    assert_eq!(f.progress(125), 0.5);
    assert_eq!(f.progress(150), 1.0);
    assert!(f.arrived(151));
    assert!(!f.arrived(150));
    assert_eq!(
        value_of("enabled=false delay=2 bandwidth=0", "enabled"),
        Some("false")
    );
    assert_eq!(value_of("up=true", "enabled"), None);
}
