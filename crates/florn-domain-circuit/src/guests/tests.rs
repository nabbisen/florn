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

const TWO_CONTROLS: &str = r#"
florn-module = 1
world = "florn:circuit/controller@0.1.0"
kind = "pid"

[circuit]
kind = "controller"
control-units = { input = "rad/s", error = "rad/s" }

[[telemetry]]
channel = 1
name = "control"
fields = [{ name = "input", type = "f64" }, { name = "output", type = "f64" }, { name = "error", type = "f64" }]

[[telemetry]]
channel = 2
name = "control"
fields = [{ name = "flag", type = "bool" }, { name = "again", type = "f64" }]

[[telemetry]]
channel = 3
name = "note"
fields = [{ name = "text", type = "text" }, { name = "late", type = "f64" }]

[[telemetry]]
channel = 4
name = "stats"
fields = [{ name = "mean", type = "f64" }]
"#;

#[test]
fn the_f64_fields_of_a_layout_have_the_offsets_of_the_fixed_prefix() {
    let g = read(&[(7, TWO_CONTROLS)]);
    let info = g.get(7).unwrap();
    let offsets = |c| f64_offsets(info.channel(c).unwrap());
    assert_eq!(
        offsets(1),
        [
            ("input".to_string(), 0),
            ("output".to_string(), 8),
            ("error".to_string(), 16)
        ]
    );
    // A bool is one byte: the `f64` after it is at 1. A text ends the prefix: `late` has no offset.
    assert_eq!(offsets(2), [("again".to_string(), 1)]);
    assert!(offsets(3).is_empty());
}

#[test]
fn the_units_of_a_controllers_series_are_its_control_units() {
    let g = read(&[(7, TWO_CONTROLS)]);
    let units = &g.get(7).unwrap().units;
    assert_eq!(units.get("input").map(String::as_str), Some("rad/s"));
    assert_eq!(units.get("error").map(String::as_str), Some("rad/s"));
    assert_eq!(units.get("output"), None, "left out: dimensionless");
}

#[test]
fn the_series_of_a_guest_run_on_across_its_control_channels() {
    // Channel 1 has three `f64` fields (0 to 2), channel 2 one (3), channel 3 one before it (4).
    let g = read(&[(7, TWO_CONTROLS)]);
    assert_eq!(g.series_base(7, 1), Some(0));
    assert_eq!(g.series_base(7, 2), Some(3));
    assert_eq!(g.series_base(7, 3), Some(4));
}

#[test]
fn a_guests_report_is_drawn_for_its_control_channels_only_and_with_the_running_index() {
    use crate::series::display_series;
    let g = read(&[(7, TWO_CONTROLS)]);
    let mut first = Vec::new();
    for v in [1.5f64, 2.5, 3.5] {
        first.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(
        display_series(None, &g, 7, 1, &first),
        [(0, 1.5), (1, 2.5), (2, 3.5)]
    );
    let mut second = vec![1u8];
    second.extend_from_slice(&9.5f64.to_le_bytes());
    assert_eq!(display_series(None, &g, 7, 2, &second), [(3, 9.5)]);
    // A channel that is not named `control` is not drawn; nor is an actor that is no guest here.
    assert!(display_series(None, &g, 7, 3, &[0; 12]).is_empty());
    assert!(
        display_series(None, &g, 7, 4, &1.0f64.to_le_bytes()).is_empty(),
        "a channel not named control, whatever its fields"
    );
    assert!(display_series(None, &g, 8, 1, &first).is_empty());
}

#[test]
fn a_prefixed_unit_has_its_factor_to_si() {
    use crate::numeric::numeric_fields;
    use florn_host::domain::Site;
    let text = TWO_CONTROLS.replace("input = \"rad/s\"", "input = \"mV\"");
    let g = read(&[(7, &text)]);
    let plant = crate::tests::Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("a", &["v1.p", "r1.a", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "vm.n"])
        .plant();
    let mut first = Vec::new();
    for v in [1.5f64, 2.5, 3.5] {
        first.extend_from_slice(&v.to_le_bytes());
    }
    let n = numeric_fields(&plant, &g, 7, Site::Report { channel: 1 }, &first).unwrap();
    assert_eq!(
        (n.fields[0].unit.as_str(), n.fields[0].factor),
        ("mV", 1e-3)
    );
}

#[test]
fn a_guests_control_report_has_its_numbers_in_the_control_units_and_the_rest_exact() {
    use crate::numeric::numeric_fields;
    use florn_host::domain::Site;
    let g = read(&[(7, TWO_CONTROLS)]);
    let plant = crate::tests::Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("a", &["v1.p", "r1.a", "vm.p"])
        .ground("g", &["v1.n", "r1.b", "vm.n"])
        .plant();
    let mut first = Vec::new();
    for v in [1.5f64, 2.5, 3.5] {
        first.extend_from_slice(&v.to_le_bytes());
    }
    let n = numeric_fields(&plant, &g, 7, Site::Report { channel: 1 }, &first).unwrap();
    let f: Vec<_> = n
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.unit.as_str(), f.factor, f.value))
        .collect();
    assert_eq!(
        f,
        [
            ("input", "rad/s", 1.0, 1.5),
            ("output", "", 1.0, 2.5),
            ("error", "rad/s", 1.0, 3.5)
        ]
    );
    assert!(n.exact.is_empty());
    // A bool before the number is compared exactly; the number takes the tolerance.
    let mut second = vec![1u8];
    second.extend_from_slice(&9.5f64.to_le_bytes());
    let n = numeric_fields(&plant, &g, 7, Site::Report { channel: 2 }, &second).unwrap();
    assert_eq!((n.fields.len(), n.exact.clone()), (1, vec![1u8]));
    // Text before the number: no fixed offset, compared by digest.
    assert!(numeric_fields(&plant, &g, 7, Site::Report { channel: 3 }, &[0; 12]).is_none());
}
