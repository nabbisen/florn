use super::*;

fn domains(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn the_networks_workspace_stays_as_it_was() {
    for d in [&["core", "network"][..], &["core"], &[]] {
        let w = Workspace::of(&domains(d));
        assert_eq!(w.scenes, [SceneKind::Topology], "{d:?}");
        assert!(w.panels.is_empty(), "{d:?}");
        assert_eq!(w.drawn, Drawn::NONE);
        assert_eq!(w.reads, ["neighbour", "route", "peer"]);
    }
}

#[test]
fn a_scenario_that_declares_the_circuit_gets_the_schematic_and_the_plots() {
    let w = Workspace::of(&domains(&["core", "circuit"]));
    assert_eq!(w.scene(), SceneKind::Schematic);
    assert!(w.shows(PanelKind::Plots));
    assert_eq!(w.drawn, Drawn::CIRCUIT);
    assert!(
        w.reads.is_empty(),
        "its reports are series, not lines of the timeline"
    );
}

#[test]
fn a_scenario_of_both_keeps_the_topology_as_a_second_scene() {
    let w = Workspace::of(&domains(&["core", "network", "circuit"]));
    assert_eq!(w.scenes, [SceneKind::Schematic, SceneKind::Topology]);
}

#[test]
fn the_circuits_built_in_series_are_its_sensors_and_its_detector() {
    let names: Vec<&str> = Drawn::CIRCUIT.builtins.iter().map(|b| b.builtin).collect();
    assert_eq!(
        names,
        [
            "circuit/voltmeter",
            "circuit/ammeter",
            "circuit/dc-motor",
            "circuit/comparator"
        ]
    );
    assert_eq!(Drawn::CIRCUIT.channels, ["control"]);
}
