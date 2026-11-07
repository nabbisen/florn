use super::*;
use florn_ui_model::florn_control::message::{
    ActorSummary, ConnectionSummary, PortSummary, ScenarioSummary,
};
use florn_ui_model::florn_control::{Answer, Message};

fn triangle() -> ViewModel {
    let port = |id: u32| PortSummary {
        id,
        name: format!("p{id}"),
        ty: "network/interface".into(),
    };
    let actor = |id: u32| ActorSummary {
        id,
        name: format!("r{id}"),
        module: Some("dv".into()),
        class: "default".into(),
        ports: vec![port(0), port(1)],
        config: Vec::new(),
    };
    let link = |id: u32, a: (u32, u32), b: (u32, u32)| ConnectionSummary {
        id,
        name: format!("c{id}"),
        ports: vec![a, b],
        ty: Some("network/link".into()),
        attrs: Vec::new(),
        per_port: Vec::new(),
    };
    let s = ScenarioSummary {
        name: "t".into(),
        sha256: String::new(),
        tick: "1ns".into(),
        end: 100,
        tie: "by-identity".into(),
        seed: 1,
        actors: vec![actor(1), actor(2), actor(3)],
        connections: vec![
            link(1000, (1, 0), (2, 0)),
            link(1001, (2, 1), (3, 0)),
            link(1002, (3, 1), (1, 1)),
        ],
        modules: Vec::new(),
        missing_modules: Vec::new(),
        domains: vec!["core".into(), "network".into()],
        changes: Vec::new(),
        classes: Vec::new(),
        limits: Vec::new(),
    };
    let mut vm = ViewModel::new();
    vm.apply(&Message::Answer(Answer::Scenario(Some(s))));
    fit_all(&mut vm);
    vm
}

#[test]
fn the_triangle_is_drawn_whole_and_a_flight_moves() {
    use florn_ui_model::florn_control::display::Record;
    let mut vm = triangle();
    let s = build(&vm, 0);
    assert_eq!(s.nodes_visible, 3);
    assert_eq!(s.description.links, 3);
    let segs = s
        .items
        .iter()
        .find_map(|i| match i {
            Item::Lines { segments, .. } => Some(segments.len()),
            _ => None,
        })
        .unwrap();
    assert_eq!(segs, 3);
    // A flight on c0 from r1 (port index 0): at its sent tick it is on r1, half way at
    // half its delay, and gone once it arrived.
    vm.display(&[
        Record::Flight {
            connection: 0,
            port: 0,
            delay: 10.0,
            sent: 20,
        },
        Record::Instant { tick: 20 },
    ]);
    let at = |vm: &ViewModel, tick: u64| -> Vec<Point> {
        build(vm, tick)
            .items
            .iter()
            .find_map(|i| match i {
                Item::Circles { centers, fill, .. } if *fill == FLIGHT => Some(centers.clone()),
                _ => None,
            })
            .unwrap()
    };
    let l = Layout::of(&vm.topology);
    let r1 = vm.topology.viewport.world_to_screen(l.nodes[0]);
    let r2 = vm.topology.viewport.world_to_screen(l.nodes[1]);
    let p0 = at(&vm, 20)[0];
    let p5 = at(&vm, 25)[0];
    assert!((p0.x - r1.x).abs() < 1e-3 && (p0.y - r1.y).abs() < 1e-3);
    assert!(((p5.x - (r1.x + r2.x) / 2.0).abs()) < 1e-3);
    assert_eq!(
        at(&vm, 30).len(),
        1,
        "at its destination, the delivery not yet handled"
    );
    vm.display(&[Record::Instant { tick: 31 }]);
    assert!(at(&vm, 31).is_empty());
    assert_eq!(build(&vm, 31).description.flights, 0);
}

#[test]
fn hit_testing_finds_nodes_and_links_and_states_show_by_shape() {
    let mut vm = triangle();
    let l = Layout::of(&vm.topology);
    let vp = vm.topology.viewport;
    let r2 = vp.world_to_screen(l.nodes[1]);
    assert_eq!(hit_test(&vm, r2, 8.0), Selection::Actor(2));
    let r3 = vp.world_to_screen(l.nodes[2]);
    let mid = Point {
        x: (r2.x + r3.x) / 2.0,
        y: (r2.y + r3.y) / 2.0,
    };
    assert_eq!(hit_test(&vm, mid, 8.0), Selection::Connection(1001));
    assert_eq!(
        hit_test(
            &vm,
            Point {
                x: -500.0,
                y: -500.0
            },
            8.0
        ),
        Selection::None
    );
    vm.topology.link_changed(1001, "enabled=false delay=2");
    vm.topology.faulted(3);
    vm.select(Selection::Actor(1));
    let s = build(&vm, 0);
    assert_eq!(s.description.links_down, 1);
    assert_eq!(s.description.faulted, 1);
    assert_eq!(s.description.selected.as_deref(), Some("r1"));
    assert!(
        s.items
            .iter()
            .any(|i| matches!(i, Item::Lines { stroke, .. } if stroke.dashed))
    );
    assert!(s.items.iter().any(|i| matches!(i, Item::Crosses { .. })));
}
