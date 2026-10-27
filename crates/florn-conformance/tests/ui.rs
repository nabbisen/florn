//! The view model and the scene over the real Host (RFC 005 rules 37 and 41; checklist C19):
//! the journeys' data flow without a browser. J1: open the triangle, run it, watch the
//! topology, the timeline and an inspector show neighbours forming and routes converging.
//! J3: take a link down as an intervention, see it listed with its instant, reconverge, step,
//! inspect a frame, replay. J2: author the triangle from an empty draft, with validation at
//! the field. And the rules of RFC 005 rule 39 a field obeys: typed text is kept, the model
//! never writes into a field being typed into, a language switch (nothing here) changes
//! nothing of it.
use florn_admission::Admission;
use florn_conformance::{host, shipped};
use florn_control::display::Record;
use florn_control::{Envelope, ExportKind, Message, Pacing, RunState, Unit};
use florn_host::Controller;
use florn_platform::FixedPlatform;
use florn_ui_model::timeline::Kind;
use florn_ui_model::{Inspector, Selection, ViewModel};
use florn_ui_scene::{Item, build, fit_all, hit_test};

type C = Controller<florn_engine_wasmtime::WasmtimeEngine>;

fn controller() -> C {
    Controller::new(
        host::engine(),
        Admission::default(),
        Box::new(FixedPlatform::default()),
        host::domains(),
        host::options(),
    )
}

/// A shell's loop: send what the view model wants, feed it every reply and the display
/// batch, until it wants nothing more.
fn pump(c: &mut C, vm: &mut ViewModel) {
    loop {
        let out = vm.take_outbox();
        if out.is_empty() {
            break;
        }
        for m in out {
            for reply in c.handle(Envelope::new(m)) {
                vm.apply(&reply);
            }
            let batch: Vec<Record> = c.take_display();
            if !batch.is_empty() {
                vm.display(&batch);
            }
        }
    }
}

/// Advance in slices like a paced shell, pumping after each.
fn advance_to(c: &mut C, vm: &mut ViewModel, tick: u64, slice: u64) {
    while vm.run.now.is_none_or(|n| n.tick < tick) && vm.run.state != RunState::Ended {
        let now = vm.topology.instant;
        vm.step(Unit::ToTick((now + slice).min(tick)));
        pump(c, vm);
        if vm.topology.instant >= tick {
            break;
        }
    }
}

fn open_triangle(c: &mut C, vm: &mut ViewModel) {
    let s = shipped::dv_triangle(100);
    vm.connect();
    pump(c, vm);
    vm.open_scenario(s.text(&s.hashes()).into_bytes());
    pump(c, vm);
    for bytes in s.modules().into_values() {
        vm.load_module(bytes);
    }
    pump(c, vm);
}

#[test]
fn j1_the_triangle_runs_and_its_routes_converge_in_the_views() {
    let mut c = controller();
    let mut vm = ViewModel::new();
    open_triangle(&mut c, &mut vm);
    assert_eq!(vm.status.transport, None, "no transport was declared");
    assert!(vm.support.is_some() && vm.capacity.is_some());
    let s = vm.scenario.as_ref().expect("the scenario");
    assert_eq!(s.actors.len(), 3);
    assert_eq!(s.domains, vec!["core", "network"]);
    assert!(s.classes.iter().any(|(c, n)| *c == 4 && n == "frame"));
    assert_eq!(vm.topology.nodes.len(), 3);
    assert_eq!(vm.topology.links.len(), 3);
    assert!(vm.topology.links.iter().all(|l| l.up));
    // The scheduled changes are listed before the run starts.
    assert_eq!(
        vm.timeline
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Change)
            .count(),
        2
    );
    assert_eq!(vm.run.state, RunState::Ready);

    vm.new_run(Some(1));
    pump(&mut c, &mut vm);
    assert_eq!(vm.run.state, RunState::Paused);
    assert_eq!(vm.run.seed, Some(1));
    advance_to(&mut c, &mut vm, 900, 50);
    // Routes converged: r1's inspector shows a route to every other router, and the
    // timeline holds neighbour and route reports, activations of the frame class, and the
    // domain's events.
    vm.select(Selection::Actor(1));
    pump(&mut c, &mut vm);
    let Inspector::Actor(a) = &vm.inspector else {
        panic!("an actor inspector");
    };
    assert!(a.alive && !a.down);
    assert_eq!(a.summary.name, "a1");
    assert_eq!(
        a.config.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["router-id", "period", "infinity"]
    );
    assert_eq!(a.config[1].committed, "100ns");
    let route = a
        .reports
        .iter()
        .find(|r| r.name == "route")
        .expect("a route report");
    assert!(route.fields.iter().any(|(k, _)| k == "prefix"));
    let routes: Vec<&florn_ui_model::timeline::Entry> = vm
        .timeline
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Report && e.code == "route" && e.origin == 1)
        .collect();
    assert!(routes.len() >= 2, "{routes:?}");
    assert!(
        routes
            .iter()
            .any(|e| e.detail.contains("prefix=2") || e.detail.contains("prefix=r2"))
    );
    assert!(
        vm.timeline
            .entries
            .iter()
            .any(|e| e.kind == Kind::Report && e.code == "neighbour")
    );
    assert!(
        vm.timeline
            .entries
            .iter()
            .any(|e| e.kind == Kind::Activation && e.class == 4)
    );
    assert_eq!(vm.timeline.class_name(4), Some("frame"));
    // Flights were drawn: the scene at the instant reached placed moving objects, and the
    // topology's activity marks the routers.
    assert!(vm.topology.nodes.iter().any(|n| n.last_active > 0));
    fit_all(&mut vm);
    let scene = build(&vm, vm.topology.instant);
    assert_eq!(scene.nodes_visible, 3);
    assert_eq!(scene.description.selected.as_deref(), Some("a1"));
    // The filter by actor keeps r1's entries only.
    vm.timeline.filter.actor = Some(1);
    let visible = vm.timeline.visible();
    assert!(!visible.is_empty());
    assert!(visible.iter().all(|&i| {
        let e = &vm.timeline.entries[i];
        e.origin == 1 || e.target == 1
    }));
    vm.timeline.filter.actor = None;
}

#[test]
fn j3_a_link_intervention_is_listed_applied_reconverges_and_replays() {
    let mut c = controller();
    let mut vm = ViewModel::new();
    open_triangle(&mut c, &mut vm);
    vm.new_run(Some(1));
    pump(&mut c, &mut vm);
    advance_to(&mut c, &mut vm, 500, 50);
    // The control says what it does: take c1 (r2-r3) down at the next instant. The Host
    // applies it and reports the instant; the list shows it.
    vm.set_link(1001, false);
    pump(&mut c, &mut vm);
    assert_eq!(vm.interventions.len(), 1);
    assert_eq!(vm.interventions[0].kind, "link");
    assert_eq!(vm.interventions[0].target, 1001);
    vm.step(Unit::OneEvent);
    pump(&mut c, &mut vm);
    assert!(
        vm.interventions[0].applied_at.is_some(),
        "{:?}",
        vm.interventions
    );
    assert!(
        !vm.topology.link(1001).unwrap().up,
        "the topology shows the link down"
    );
    assert!(
        vm.timeline
            .entries
            .iter()
            .any(|e| e.kind == Kind::Event && e.code == "link-changed" && e.target == 1001)
    );
    assert!(
        vm.timeline
            .entries
            .iter()
            .any(|e| e.kind == Kind::Intervention)
    );
    // The connection's inspector says so too.
    vm.select(Selection::Connection(1001));
    pump(&mut c, &mut vm);
    let Inspector::Connection(ci) = &vm.inspector else {
        panic!("a connection inspector");
    };
    assert!(!ci.up);
    assert_eq!(ci.summary.ty.as_deref(), Some("network/link"));
    assert_eq!(ci.attrs[0].name, "delay");
    // Reconvergence: r2 reaches r3 through r1 within three periods.
    advance_to(&mut c, &mut vm, 900, 50);
    vm.select(Selection::Actor(2));
    pump(&mut c, &mut vm);
    let Inspector::Actor(a) = &vm.inspector else {
        panic!("an actor inspector");
    };
    let route = a
        .reports
        .iter()
        .find(|r| r.name == "route")
        .expect("a route");
    assert!(!route.fields.is_empty());
    // The scene: the link down is dashed; hit testing the link's middle selects it.
    fit_all(&mut vm);
    let scene = build(&vm, vm.topology.instant);
    assert_eq!(scene.description.links_down, 1);
    assert!(
        scene
            .items
            .iter()
            .any(|i| matches!(i, Item::Lines { stroke, .. } if stroke.dashed))
    );
    let l = florn_ui_scene::Layout::of(&vm.topology);
    let vp = vm.topology.viewport;
    let (p2, p3) = (
        vp.world_to_screen(l.nodes[1]),
        vp.world_to_screen(l.nodes[2]),
    );
    let mid = florn_ui_model::Point {
        x: (p2.x + p3.x) / 2.0,
        y: (p2.y + p3.y) / 2.0,
    };
    assert_eq!(hit_test(&vm, mid, 6.0), Selection::Connection(1001));
    // Inspect a frame: a handled frame's send, its bytes and its decoded fields.
    let (entry, _) = vm
        .timeline
        .entries
        .iter()
        .enumerate()
        .find(|(_, e)| e.kind == Kind::Activation && !e.sends.is_empty() && e.target == 1)
        .map(|(i, e)| (i, e.clone()))
        .expect("an activation with a send");
    vm.select(Selection::Message { entry, send: 0 });
    pump(&mut c, &mut vm);
    let Inspector::Message(m) = &vm.inspector else {
        panic!("a message inspector");
    };
    assert!(!m.pending);
    assert_eq!(m.protocol.as_deref(), Some("distance-vector"));
    let payload = m.payload.as_ref().expect("the run keeps payloads");
    assert_eq!(payload.len() as u32, m.send.len);
    let decoded = m
        .decoded
        .as_ref()
        .expect("the domain decodes distance-vector");
    assert!(decoded.iter().any(|(k, _)| k == "sender"), "{decoded:?}");
    // Export the record and replay it: the comparison matches and the interventions apply.
    vm.confirm_export(ExportKind::RunRecord {
        trace: true,
        payloads: false,
    });
    let mut record = None;
    for m in vm.take_outbox() {
        for reply in c.handle(Envelope::new(m)) {
            if let Message::Answer(florn_control::Answer::Exported { bytes, .. }) = &reply {
                record = Some(bytes.clone());
            }
            vm.apply(&reply);
        }
    }
    let record = record.expect("exported");
    vm.replay(record);
    pump(&mut c, &mut vm);
    assert_eq!(vm.run.state, RunState::Paused);
    assert!(
        vm.interventions.is_empty(),
        "a new run: the list starts again"
    );
    assert_eq!(vm.kept.len(), 1, "the ended run's interventions are kept");
    vm.run();
    pump(&mut c, &mut vm);
    assert_eq!(vm.run.state, RunState::Ended);
    assert_eq!(vm.status.replay.as_deref(), Some("replay.matched"));
    assert!(vm.run.chain.is_some());
    // The replay applied the recorded intervention at its instant, and the list says so.
    assert_eq!(vm.interventions.len(), 1, "{:?}", vm.interventions);
    assert_eq!(vm.interventions[0].kind, "link");
    assert_eq!(vm.interventions[0].applied_at, vm.kept[0].applied_at);
}

#[test]
fn j2_the_triangle_authored_from_an_empty_draft_runs_to_the_runners_digest() {
    let mut c = controller();
    let mut vm = ViewModel::new();
    vm.connect();
    pump(&mut c, &mut vm);
    let s = shipped::dv_triangle(100);
    let hashes = s.hashes();
    let modules = s.modules();
    // The module is imported and admitted first (§3.9): its manifest is shown.
    let dv = modules.get("distance_vector").expect("the module").clone();
    vm.admit(dv.clone());
    let mut admitted = None;
    for m in vm.take_outbox() {
        for reply in c.handle(Envelope::new(m)) {
            if let Message::Answer(florn_control::Answer::Admitted { module, .. }) = &reply {
                admitted = Some(module.clone());
            }
            vm.apply(&reply);
        }
    }
    let admitted = admitted.expect("admitted");
    assert_eq!(admitted.kind, "distance-vector");
    assert!(admitted.tables.iter().any(|(t, f)| {
        t == "network"
            && f.iter()
                .any(|(k, v)| k == "protocol" && v == "distance-vector")
    }));
    // Place three routers, connect them, set a link's delay and loss.
    let d = &mut vm.draft;
    d.name.type_text("triangle");
    d.name.commit();
    d.end.type_text("3000ns");
    d.end.commit();
    d.add_module(florn_ui_model::draft::DraftModule {
        name: "distance_vector".into(),
        sha256: hashes["distance_vector"].clone(),
        contract: admitted.contract.clone(),
        summary: Some(admitted.clone()),
    });
    let r1 = d.place("distance_vector", "network/interface", 2).unwrap();
    let r2 = d.place("distance_vector", "network/interface", 2).unwrap();
    let r3 = d.place("distance_vector", "network/interface", 2).unwrap();
    for (id, _) in [(r1, 0), (r2, 0), (r3, 0)] {
        d.set_config(id, "period", "100ns");
        d.set_config(id, "infinity", "16");
    }
    let c0 = d.connect("network/link", &[(r1, 0), (r2, 0)]);
    let c1 = d.connect("network/link", &[(r2, 1), (r3, 0)]);
    let c2 = d.connect("network/link", &[(r3, 1), (r1, 1)]);
    for l in [c0, c1, c2] {
        d.set_attr(l, "delay", "2ns");
    }
    // Validation at the field: a bare number where a quantity goes is refused by the field
    // itself, before the Host sees it.
    let f = d.set_attr(c0, "delay", "5").unwrap();
    assert_eq!(f.problem, Some(florn_ui_model::fields::Problem::NeedsUnit));
    assert!(!f.is_valid());
    assert_eq!(d.problems(), 1);
    d.set_attr(c0, "delay", "2ns");
    d.set_attr(c0, "loss", "0.0");
    assert_eq!(d.problems(), 0);
    d.schedule("1000ns", c0, &[("enabled", "false")]);
    d.schedule("2000ns", c0, &[("enabled", "true")]);
    // A value the Host refuses comes back to its field, and the scenario cannot run.
    d.set_attr(c1, "delay", "2kg");
    vm.load_draft();
    pump(&mut c, &mut vm);
    assert!(!vm.draft.errors.is_empty(), "the Host refused");
    let delay = vm.draft.connections[1]
        .attrs
        .iter()
        .find(|f| f.name == "delay")
        .unwrap();
    assert!(
        !delay.errors.is_empty(),
        "the refusal is at the field: {:?}",
        vm.draft.errors
    );
    assert_eq!(vm.run.state, RunState::Invalid);
    vm.draft.set_attr(c1, "delay", "2ns");
    vm.load_draft();
    pump(&mut c, &mut vm);
    assert!(vm.draft.errors.is_empty(), "{:?}", vm.draft.errors);
    vm.load_module(dv);
    pump(&mut c, &mut vm);
    assert_eq!(vm.run.state, RunState::Ready);
    // The run converges, and the digest equals the runner's for the shipped scenario.
    vm.new_run(Some(1));
    pump(&mut c, &mut vm);
    vm.run();
    pump(&mut c, &mut vm);
    assert_eq!(vm.run.state, RunState::Ended);
    let runner = host::run(&shipped::dv_triangle(100));
    assert_eq!(vm.run.chain.as_deref(), Some(runner.chain_hex().as_str()));
}

#[test]
fn fields_keep_typed_text_across_refreshes_and_selection_sets_them() {
    let mut c = controller();
    let mut vm = ViewModel::new();
    open_triangle(&mut c, &mut vm);
    vm.select(Selection::Actor(2));
    pump(&mut c, &mut vm);
    let Inspector::Actor(a) = &mut vm.inspector else {
        panic!("an actor inspector");
    };
    let period = a.config.iter().position(|f| f.name == "period").unwrap();
    a.config[period].type_text("250n");
    // Selecting again sets the fields from the model (rule 39: only then).
    vm.select_step(0);
    let Inspector::Actor(a) = &vm.inspector else {
        panic!()
    };
    assert_eq!(a.summary.id, 2);
    assert_eq!(
        a.config[period].text, "100ns",
        "selecting sets the fields from the model"
    );
    let Inspector::Actor(a) = &mut vm.inspector else {
        panic!()
    };
    a.config[period].type_text("250n");
    // The Host answers the actor query meanwhile (a refresh): the typed text stays.
    pump(&mut c, &mut vm);
    let Inspector::Actor(a) = &vm.inspector else {
        panic!()
    };
    assert_eq!(
        a.config[period].text, "250n",
        "the answer did not write into the field"
    );
    assert!(a.config[period].dirty);
    // A language switch is the shell's; nothing of the typed text lives here to change.
    vm.select_step(1);
    let Inspector::Actor(a) = &vm.inspector else {
        panic!()
    };
    assert_eq!(a.summary.id, 3);
    assert_eq!(a.config[period].text, "100ns");
    // The scene draws the selection and the keyboard walks the list.
    vm.select_step(-1);
    assert_eq!(vm.selection, Selection::Actor(2));
    vm.select_step(2);
    assert_eq!(vm.selection, Selection::Actor(1));
    vm.set_speed(Pacing::TicksPerSecond(1_000));
    pump(&mut c, &mut vm);
    assert_eq!(vm.run.pacing, Pacing::TicksPerSecond(1_000));
}
