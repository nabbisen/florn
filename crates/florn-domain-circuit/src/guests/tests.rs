//! The guests the start hook reads (RFC 012 finding F-18).

use florn_admission::manifest::parse_manifest;

use super::*;

fn read(manifests: &[(u32, &str)]) -> Guests {
    let parsed: Vec<_> = manifests
        .iter()
        .map(|(a, m)| (*a, parse_manifest(m).expect("the manifest parses")))
        .collect();
    Guests::from_manifests(parsed.iter().map(|(a, m)| (*a, m)))
}

const CONTROLLER: &str = r#"
florn-module = 1
world = "florn:circuit/controller@0.1.0"
kind = "controller"

[circuit]
kind = "controller"
period = "period"

[[telemetry]]
channel = 2
name = "control"
fields = [{ name = "input", type = "f64" }, { name = "output", type = "f64" }, { name = "error", type = "f64" }]

[[telemetry]]
channel = 1
name = "stats"
fields = [{ name = "count", type = "u64" }, { name = "mean", type = "f64" }]
"#;

const DEVICE: &str = r#"
florn-module = 1
world = "florn:circuit/device@0.1.0"
kind = "device"

[circuit]
kind = "device"
"#;

#[test]
fn the_series_base_counts_the_declared_f64_fields_in_channel_order() {
    let g = read(&[(7, CONTROLLER)]);
    // Channel 1 (`stats`, one `f64`) comes before channel 2 whatever the manifest's order.
    assert_eq!(g.series_base(7, 1), Some(0));
    assert_eq!(g.series_base(7, 2), Some(1));
    assert_eq!(g.series_base(7, 3), None);
    assert_eq!(g.series_base(8, 1), None);
}

#[test]
fn two_device_modules_are_counted_and_a_module_without_the_table_is_not_a_guest_of_the_domain() {
    let plain = "florn-module = 1\nworld = \"florn:core/guest@0.1.0\"\nkind = \"plain\"\n";
    let g = read(&[(1, plain), (2, DEVICE), (3, DEVICE), (4, CONTROLLER)]);
    assert_eq!(g.devices(), 2);
    assert!(g.get(1).is_none());
    assert_eq!(g.get(4).map(|i| i.kind), Some(GuestKind::Controller));
}
