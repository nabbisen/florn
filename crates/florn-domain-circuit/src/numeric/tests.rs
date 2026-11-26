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
fn what_is_not_a_probe_value_or_eight_bytes_is_not_numeric() {
    let p = plant();
    assert!(numeric_fields(&p, 3, Site::Report { channel: 2 }, &bits(1.0)).is_none());
    assert!(
        numeric_fields(
            &p,
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
            1,
            Site::Report {
                channel: CHANNEL_PROBE
            },
            &bits(1.0)
        )
        .is_none()
    );
    assert!(numeric_fields(&p, 3, Site::Send { port: 99 }, &bits(1.0)).is_none());
}
