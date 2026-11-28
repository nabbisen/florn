//! RFC 013 rule 20, item 2: the numeric fields of a sensor's report and of a `signal-out` send, in
//! the unit of their signal with its factor to SI.

use super::*;
use crate::tests::Deck;

fn plant() -> Plant {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .builtin(4, "am", "ammeter", "period = \"1ms\"")
        .net("a", &["v1.p", "vm.p", "am.a"])
        .net("b", &["am.b", "r1.a"])
        .ground("g", &["v1.n", "r1.b", "vm.n"])
        .guest(10, "ctl", &[("in", "circuit/signal-in")])
        .guest(
            11,
            "ctl2",
            &[("in", "circuit/signal-in"), ("out", "circuit/signal-out")],
        )
        .signal("volts", &["vm.v", "ctl.in"], "unit = \"mV\"")
        .plant()
}

fn bits(x: f64) -> Vec<u8> {
    x.to_le_bytes().to_vec()
}

#[test]
fn a_probe_report_is_the_value_in_the_unit_of_the_sensors_signal() {
    let p = plant();
    let n = numeric_fields(
        &p,
        &Guests::default(),
        3,
        Site::Report {
            channel: CHANNEL_PROBE,
        },
        &bits(1234.5),
    )
    .unwrap();
    assert_eq!(n.fields.len(), 1);
    let f = &n.fields[0];
    assert_eq!(
        (f.name.as_str(), f.unit.as_str(), f.factor, f.value),
        ("value", "mV", 1e-3, 1234.5)
    );
    assert!(n.exact.is_empty());
}

#[test]
fn a_sensor_on_no_signal_reports_in_its_own_si_unit() {
    let p = plant();
    let n = numeric_fields(
        &p,
        &Guests::default(),
        4,
        Site::Report {
            channel: CHANNEL_PROBE,
        },
        &bits(0.25),
    )
    .unwrap();
    let f = &n.fields[0];
    assert_eq!((f.unit.as_str(), f.factor), ("A", 1.0));
}

#[test]
fn a_send_on_a_signal_out_port_is_the_value_in_the_unit_of_its_signal() {
    let p = plant();
    let port = p
        .devices
        .iter()
        .find(|d| d.actor == 3)
        .map(|d| match &d.element {
            crate::plant::Element::Voltmeter { out_port, .. } => out_port.unwrap(),
            _ => unreachable!(),
        });
    let n = numeric_fields(
        &p,
        &Guests::default(),
        3,
        Site::Send {
            port: port.unwrap(),
        },
        &bits(7.0),
    )
    .unwrap();
    let f = &n.fields[0];
    assert_eq!(
        (f.name.as_str(), f.unit.as_str(), f.factor),
        ("output", "mV", 1e-3)
    );
}

#[test]
fn a_send_on_a_sensors_output_port_on_no_signal_is_numeric_in_its_si_unit() {
    // The ammeter's `i` port is connected to nothing; it still sends its sample, and the eight
    // bytes are a number, not a digest (rule 20, item 2).
    let p = plant();
    let port_of = |actor: u32| {
        p.devices
            .iter()
            .find(|d| d.actor == actor)
            .and_then(|d| match &d.element {
                crate::plant::Element::Ammeter { out_port, .. }
                | crate::plant::Element::Voltmeter { out_port, .. } => *out_port,
                _ => None,
            })
            .unwrap()
    };
    assert!(!p.port_signal.contains_key(&(4, port_of(4))));
    let n = numeric_fields(
        &p,
        &Guests::default(),
        4,
        Site::Send { port: port_of(4) },
        &bits(0.25),
    )
    .unwrap();
    let f = &n.fields[0];
    assert_eq!(
        (f.name.as_str(), f.unit.as_str(), f.factor, f.value),
        ("output", "A", 1.0, 0.25)
    );
    // Another port of that sensor, and a port of an actor that sends no sample, are not.
    assert!(
        numeric_fields(
            &p,
            &Guests::default(),
            4,
            Site::Send { port: 0 },
            &bits(0.25)
        )
        .is_none()
    );
    assert!(
        numeric_fields(
            &p,
            &Guests::default(),
            2,
            Site::Send { port: 0 },
            &bits(0.25)
        )
        .is_none()
    );
}

#[test]
fn what_is_not_a_probe_value_or_eight_bytes_is_not_numeric() {
    let p = plant();
    assert!(
        numeric_fields(
            &p,
            &Guests::default(),
            3,
            Site::Report { channel: 2 },
            &bits(1.0)
        )
        .is_none()
    );
    assert!(
        numeric_fields(
            &p,
            &Guests::default(),
            3,
            Site::Report {
                channel: CHANNEL_PROBE
            },
            &[1, 2, 3]
        )
        .is_none()
    );
    assert!(
        numeric_fields(
            &p,
            &Guests::default(),
            1,
            Site::Report {
                channel: CHANNEL_PROBE
            },
            &bits(1.0)
        )
        .is_none()
    );
    assert!(
        numeric_fields(
            &p,
            &Guests::default(),
            3,
            Site::Send { port: 99 },
            &bits(1.0)
        )
        .is_none()
    );
}

#[test]
fn a_comparators_condition_has_its_output_as_the_number_and_its_located_tick_exact() {
    // RFC 013 rule 20, item 2: `condition`'s output is a numeric field (a dimensionless one), and
    // the located crossing, a whole tick, is compared exactly; the output sent on the `y` port is
    // numeric as well, with no unit.
    let p = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"1V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(
            3,
            "cmp",
            "comparator",
            "threshold = \"0.5V\"\nhysteresis = \"0V\"",
        )
        .net("a", &["v1.p", "r1.a", "cmp.p"])
        .ground("g", &["v1.n", "r1.b", "cmp.n"])
        .plant();
    let mut payload = 1.0f64.to_le_bytes().to_vec();
    payload.extend_from_slice(&916_326u64.to_le_bytes());
    let n = numeric_fields(
        &p,
        &Guests::default(),
        3,
        Site::Report {
            channel: crate::CHANNEL_CONDITION,
        },
        &payload,
    )
    .unwrap();
    assert_eq!(n.fields.len(), 1);
    let f = &n.fields[0];
    assert_eq!(
        (f.name.as_str(), f.unit.as_str(), f.factor, f.value),
        ("output", "", 1.0, 1.0)
    );
    assert_eq!(n.exact, 916_326u64.to_le_bytes());
    // A report of another length is no condition.
    assert!(
        numeric_fields(
            &p,
            &Guests::default(),
            3,
            Site::Report {
                channel: crate::CHANNEL_CONDITION
            },
            &payload[..8]
        )
        .is_none()
    );
    // The output sent on its port.
    let port = p
        .devices
        .iter()
        .find_map(|d| match &d.element {
            crate::plant::Element::Comparator { out_port, .. } => *out_port,
            _ => None,
        })
        .expect("the `y` port");
    let s = numeric_fields(
        &p,
        &Guests::default(),
        3,
        Site::Send { port },
        &1.0f64.to_le_bytes(),
    )
    .unwrap();
    assert_eq!(s.fields[0].name, "output");
    assert_eq!(s.fields[0].unit, "");
}
