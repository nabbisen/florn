use super::*;

#[test]
fn the_table_is_the_twelve_rows_of_rule_23_in_order() {
    let names: Vec<&str> = KINDS.iter().map(|k| k.name).collect();
    assert_eq!(
        names,
        [
            "resistor",
            "capacitor",
            "inductor",
            "voltage-source",
            "current-source",
            "diode",
            "switch",
            "voltmeter",
            "ammeter",
            "constant",
            "comparator",
            "dc-motor"
        ]
    );
}

#[test]
fn the_settable_values_are_the_four_of_rule_15() {
    let settable: Vec<(&str, &str)> = KINDS
        .iter()
        .filter_map(|k| k.settable.map(|s| (k.name, s)))
        .collect();
    assert_eq!(
        settable,
        [
            ("voltage-source", "v"),
            ("current-source", "i"),
            ("constant", "value"),
            ("comparator", "threshold")
        ]
    );
    for k in KINDS {
        if let Some(s) = k.settable {
            assert!(k.param(s).is_some(), "{}", k.name);
        }
    }
}

#[test]
fn the_channels_are_the_numbers_rule_24_freezes() {
    assert_eq!(crate::CHANNEL_PROBE, 1);
    assert_eq!(crate::CHANNEL_CONDITION, 2);
    let of = |n: &str| by_builtin(&format!("circuit/{n}")).unwrap().channels();
    assert_eq!(of("voltmeter"), [1]);
    assert_eq!(of("ammeter"), [1]);
    assert_eq!(of("dc-motor"), [1]);
    assert_eq!(of("comparator"), [2]);
    assert!(of("resistor").is_empty());
    assert!(of("constant").is_empty());
}

#[test]
fn a_sensor_is_a_kind_with_a_period() {
    for k in KINDS {
        assert_eq!(k.sensor, k.param("period").is_some(), "{}", k.name);
    }
}

#[test]
fn a_kind_is_found_by_its_builtin_name_only_in_this_domain() {
    assert!(by_builtin("circuit/resistor").is_some());
    assert!(by_builtin("network/resistor").is_none());
    assert!(by_builtin("circuit/transistor").is_none());
    assert!(by_builtin("resistor").is_none());
}
