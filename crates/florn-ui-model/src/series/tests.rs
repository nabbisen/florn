use super::*;
use crate::workspace::Drawn;
use florn_control::message::{
    ActorSummary, ChannelSummary, ConnectionSummary, ModuleSummary, PortSummary, ReportEntry,
};
use florn_control::{Answer, At, Message};

fn port(id: u32, name: &str, ty: &str) -> PortSummary {
    PortSummary {
        id,
        name: name.into(),
        ty: ty.into(),
    }
}

/// The motor loop's actors, by ids that leave a gap: a source, the motor with its sensor, a
/// constant, and the controller with a `control` channel of three `f64`s and a text channel.
fn scenario() -> ScenarioSummary {
    let builtin = |id: u32, name: &str, kind: &str, ports: Vec<PortSummary>| ActorSummary {
        id,
        name: name.into(),
        module: None,
        class: "default".into(),
        ports,
        config: Vec::new(),
        builtin: Some(kind.into()),
    };
    let pid = ActorSummary {
        id: 5,
        name: "pid".into(),
        module: Some("pid".into()),
        class: "default".into(),
        ports: vec![port(2, "output", "circuit/signal-out")],
        config: Vec::new(),
        builtin: None,
    };
    let module = ModuleSummary {
        name: "pid".into(),
        source_sha256: String::new(),
        contract: "florn:circuit/controller@0.1.0".into(),
        kind: "controller".into(),
        description: None,
        port_kinds: Vec::new(),
        params: Vec::new(),
        channels: vec![
            ChannelSummary {
                channel: 7,
                name: "note".into(),
                fields: vec![
                    ("count".into(), "u64".into()),
                    ("why".into(), "text".into()),
                ],
            },
            ChannelSummary {
                channel: 3,
                name: "control".into(),
                fields: vec![
                    ("input".into(), "f64".into()),
                    ("output".into(), "f64".into()),
                    ("error".into(), "f64".into()),
                ],
            },
        ],
        needs: Vec::new(),
        tables: vec![(
            "circuit".into(),
            vec![
                ("kind".into(), "controller".into()),
                (
                    "control-units".into(),
                    "Map({\"input\": Text(\"rad/s\"), \"error\": Text(\"rad/s\")})".into(),
                ),
            ],
        )],
    };
    ScenarioSummary {
        name: "motor-loop".into(),
        sha256: String::new(),
        tick: "1ns".into(),
        end: 4_000_000_000,
        tie: "by-identity".into(),
        seed: 1,
        actors: vec![
            pid,
            builtin(
                2,
                "m1",
                "circuit/dc-motor",
                vec![
                    port(0, "a+", "circuit/pin"),
                    port(1, "a-", "circuit/pin"),
                    port(2, "speed", "circuit/signal-out"),
                    port(3, "load", "circuit/signal-in"),
                ],
            ),
            builtin(
                1,
                "vs",
                "circuit/voltage-source",
                vec![port(0, "p", "circuit/pin")],
            ),
            builtin(
                3,
                "sp",
                "circuit/constant",
                vec![port(0, "y", "circuit/signal-out")],
            ),
        ],
        connections: vec![ConnectionSummary {
            id: 102,
            name: "speed".into(),
            ports: vec![(2, 2), (5, 0)],
            ty: Some("circuit/signal".into()),
            attrs: Vec::new(),
            per_port: Vec::new(),
        }],
        modules: vec![module],
        missing_modules: Vec::new(),
        domains: vec!["core".into(), "circuit".into()],
        changes: Vec::new(),
        classes: Vec::new(),
        limits: Vec::new(),
    }
}

fn set() -> SeriesSet {
    SeriesSet::of(&scenario(), &Drawn::CIRCUIT)
}

fn sample(tick: u64, value: f32) -> Sample {
    Sample { tick, value }
}

fn series_of(samples: &[(u64, f32)]) -> Series {
    let mut s = Series::new(
        Source {
            actor: 2,
            index: 0,
            channel: 1,
            offset: 0,
            field: "speed".into(),
        },
        "m1.speed".into(),
        Some("rad/s".into()),
        false,
    );
    for (t, v) in samples {
        s.push(sample(*t, *v));
    }
    s
}

#[test]
fn a_scenario_declares_its_series_in_id_order_with_label_unit_and_place() {
    let set = set();
    type Row<'a> = (String, u32, u8, u32, usize, Option<&'a str>, bool);
    let rows: Vec<Row> = set
        .list()
        .iter()
        .map(|s| {
            (
                s.label.clone(),
                s.source.actor,
                s.source.index,
                s.source.channel,
                s.source.offset,
                s.unit.as_deref(),
                s.controller,
            )
        })
        .collect();
    // The motor (id 2) before the controller (id 5); a built-in's label is its actor's name and
    // its `signal-out` port's, and its unit is the sensor's. The controller's indexes count the
    // `f64`s of the channels before it (the `note` channel has none), its offsets are in the
    // report, its units are the module's `control-units` and a field they leave out has none.
    assert_eq!(
        rows,
        vec![
            ("m1.speed".to_string(), 2, 0, 1, 0, Some("rad/s"), false),
            ("pid.input".to_string(), 5, 0, 3, 0, Some("rad/s"), true),
            ("pid.output".to_string(), 5, 1, 3, 8, None, true),
            ("pid.error".to_string(), 5, 2, 3, 16, Some("rad/s"), true),
        ]
    );
}

#[test]
fn a_display_record_finds_its_series_by_the_actors_place_in_id_order() {
    let mut set = set();
    // Ids are 1, 2, 3, 5: place 1 is the motor, place 3 the controller.
    assert!(set.display(1, 0, 12.5, 1_000_000));
    assert!(set.display(3, 1, 0.25, 1_000_000));
    assert!(!set.display(1, 1, 1.0, 5), "the motor has no second field");
    assert!(!set.display(0, 0, 1.0, 5), "a source draws nothing");
    assert!(!set.display(9, 0, 1.0, 5), "no ninth actor");
    assert_eq!(
        set.get(2, 0).and_then(Series::latest),
        Some(sample(1_000_000, 12.5))
    );
    assert_eq!(
        set.get(5, 1).and_then(Series::latest),
        Some(sample(1_000_000, 0.25))
    );
    assert_eq!(set.get(5, 0).map(Series::count), Some(0));
}

#[test]
fn a_domain_with_nothing_drawn_declares_no_series() {
    assert!(SeriesSet::of(&scenario(), &Drawn::NONE).is_empty());
}

#[test]
fn the_ring_keeps_the_newest_and_the_count_keeps_them_all() {
    let mut s = series_of(&[]);
    for t in 0..(RING as u64 + 10) {
        s.push(sample(t, t as f32));
    }
    assert_eq!(s.kept(), RING);
    assert_eq!(s.count(), RING as u64 + 10);
    assert_eq!(s.oldest(), Some(10));
    assert_eq!(s.latest(), Some(sample(RING as u64 + 9, (RING + 9) as f32)));
}

#[test]
fn the_value_held_at_a_tick_is_the_last_sample_at_or_before_it() {
    let s = series_of(&[(10, 1.0), (20, 2.0), (30, 3.0)]);
    assert_eq!(s.held_at(9), None);
    assert_eq!(s.held_at(10), Some(sample(10, 1.0)));
    assert_eq!(s.held_at(25), Some(sample(20, 2.0)));
    assert_eq!(s.held_at(1_000), Some(sample(30, 3.0)));
}

#[test]
fn columns_keep_the_extremes_of_each_pixel_column() {
    // A hundred ticks in ten columns: ticks 0..10 are the first column and so on.
    let mut s = series_of(&[]);
    for t in 0..100u64 {
        s.push(sample(
            t,
            if t % 10 == 3 {
                100.0 + t as f32
            } else {
                t as f32
            },
        ));
    }
    let cells = s.columns(0, 99, 10).to_vec();
    assert_eq!(cells.len(), 10);
    for (i, c) in cells.iter().enumerate() {
        let c = c.expect("every column holds samples");
        let base = (i * 10) as f32;
        assert_eq!(c.min, base, "column {i}");
        assert_eq!(c.max, 100.0 + base + 3.0, "column {i}");
        assert_eq!(c.first, base);
        assert_eq!(c.last, base + 9.0);
    }
}

#[test]
fn a_column_nobody_sampled_in_is_empty_and_a_sample_outside_the_window_counts_nowhere() {
    let mut s = series_of(&[(0, 1.0), (95, 2.0), (500, 9.0)]);
    let cells = s.columns(0, 99, 10).to_vec();
    assert!(cells[0].is_some() && cells[9].is_some());
    assert!(cells[1..9].iter().all(Option::is_none));
    assert_eq!(cells[9].map(|c| c.max), Some(2.0));
}

#[test]
fn the_columns_a_sample_updates_are_those_a_rebuild_gives() {
    let mut s = series_of(&[]);
    for t in 0..50u64 {
        s.push(sample(t, (t % 7) as f32));
    }
    let _ = s.columns(0, 99, 10);
    for t in 50..100u64 {
        s.push(sample(t, (t % 5) as f32 - 2.0));
    }
    let kept = s.columns(0, 99, 10).to_vec();
    let mut fresh = s.clone();
    fresh.columns = None;
    assert_eq!(kept, fresh.columns(0, 99, 10).to_vec());
}

#[test]
fn a_sample_the_ring_lets_go_of_leaves_the_columns_it_was_counted_in() {
    let mut s = series_of(&[]);
    for t in 0..RING as u64 {
        s.push(sample(t, if t == 0 { 1_000.0 } else { 1.0 }));
    }
    let before = s.columns(0, RING as u64 - 1, 8).to_vec();
    assert_eq!(before[0].map(|c| c.max), Some(1_000.0));
    s.push(sample(RING as u64, 1.0));
    let after = s.columns(0, RING as u64 - 1, 8).to_vec();
    assert_eq!(
        after[0].map(|c| c.max),
        Some(1.0),
        "tick 0 is gone from the ring"
    );
}

#[test]
fn a_million_samples_keep_the_ring_and_a_window_of_the_panels_width() {
    // A sensor at every tick for a million ticks (RFC 014 rule 4's reduction row): what is kept is
    // the ring, and what a window gives is its width, however many samples were produced.
    let mut s = series_of(&[]);
    for t in 0..1_000_000u64 {
        s.push(sample(t, (t % 1_000) as f32));
    }
    assert_eq!(s.count(), 1_000_000);
    assert_eq!(s.kept(), RING);
    let cells = s.columns(996_000, 999_999, 800);
    assert_eq!(cells.len(), 800);
    assert!(cells.iter().all(Option::is_some));
}

#[test]
fn what_lies_before_the_ring_is_filled_from_the_trace_once_and_let_go() {
    let mut s = series_of(&[(100, 1.0), (110, 2.0)]);
    s.backfill(&[
        sample(50, 0.5),
        sample(60, 0.6),
        sample(100, 9.0),
        sample(110, 9.0),
    ]);
    let ticks: Vec<u64> = s.samples().map(|p| p.tick).collect();
    assert_eq!(
        ticks,
        [50, 60, 100, 110],
        "a sample the series has is not taken twice"
    );
    assert_eq!(s.held_at(55), Some(sample(50, 0.5)));
    assert_eq!(
        s.held_at(60),
        Some(sample(60, 0.6)),
        "at a sample's own tick it is that sample"
    );
    assert_eq!(s.held_at(49), None);
    assert_eq!(s.oldest(), Some(50));
    assert_eq!(
        s.count(),
        2,
        "the run produced two; the trace's are not new"
    );
    s.backfill(&[sample(10, 0.1), sample(20, 0.2)]);
    assert_eq!(s.oldest(), Some(10), "a window further back goes in front");
    s.forget_older();
    assert_eq!(s.oldest(), Some(100));
    assert_eq!(s.kept(), 2);
}

fn report(actor: u32, channel: u32, tick: u64, values: &[f64]) -> TraceEntry {
    TraceEntry::Handled {
        at: At { tick, microstep: 0 },
        class: 7,
        origin: actor,
        seq: tick,
        target: actor,
        metered: 0,
        draws: 0,
        effects: 1,
        sends: Vec::new(),
        reports: vec![ReportEntry {
            seq: tick,
            channel,
            len: 8 * values.len() as u32,
            payload: Some(values.iter().flat_map(|v| v.to_le_bytes()).collect()),
        }],
    }
}

#[test]
fn the_trace_gives_the_same_samples_as_the_display_to_float_rounding() {
    let mut live = set();
    let trace: Vec<TraceEntry> = (1..=20u64)
        .flat_map(|k| {
            let t = k * 1_000_000;
            let speed = 0.1 * k as f64 + 1.0 / 3.0;
            let (input, output, error) = (speed, 0.01 * k as f64, 100.0 - speed);
            // The display carries what the Host drew of the same reports.
            live.display(1, 0, speed as f32, t);
            live.display(3, 0, input as f32, t);
            live.display(3, 1, output as f32, t);
            live.display(3, 2, error as f32, t);
            [
                report(2, 1, t, &[speed]),
                report(5, 3, t, &[input, output, error]),
            ]
        })
        .collect();
    let decoded = live.decode(&trace);
    assert_eq!(
        decoded.len(),
        20 * 4,
        "one sample for each drawn field of each report"
    );
    for (i, s) in live.list().iter().enumerate() {
        let from_trace: Vec<Sample> = decoded
            .iter()
            .filter(|(n, _)| *n == i)
            .map(|(_, s)| *s)
            .collect();
        let shown: Vec<Sample> = s.samples().collect();
        assert_eq!(from_trace, shown, "{}", s.label);
        assert_eq!(s.count(), 20, "{}: the counts are equal", s.label);
    }
}

#[test]
fn a_trace_entry_with_no_payload_or_of_another_channel_gives_nothing() {
    let set = set();
    let mut bare = report(2, 1, 5, &[1.0]);
    if let TraceEntry::Handled { reports, .. } = &mut bare {
        reports[0].payload = None;
    }
    let other_channel = report(2, 9, 5, &[1.0]);
    let short = report(5, 3, 5, &[1.0]);
    assert!(set.decode(&[bare, other_channel]).is_empty());
    assert_eq!(
        set.decode(&[short]).len(),
        1,
        "the one field a short payload holds"
    );
}

#[test]
fn a_new_run_empties_the_series_and_keeps_their_declaration() {
    let mut set = set();
    set.display(1, 0, 1.0, 1);
    set.clear_run();
    assert_eq!(set.list().len(), 4);
    assert_eq!(set.get(2, 0).map(Series::count), Some(0));
    assert_eq!(set.get(2, 0).and_then(Series::latest), None);
}

#[test]
fn the_view_model_feeds_the_series_from_the_display_and_declares_them_with_the_scenario() {
    let mut vm = crate::ViewModel::new();
    assert!(vm.series.is_empty());
    vm.apply(&Message::Answer(Answer::Scenario(Some(scenario()))));
    assert_eq!(vm.series.list().len(), 4);
    vm.display(&[
        florn_control::display::Record::Sample {
            index: 0,
            actor: 1,
            value: 3.5,
            tick: 2_000_000,
        },
        florn_control::display::Record::Instant { tick: 2_000_000 },
    ]);
    assert_eq!(
        vm.series.get(2, 0).and_then(Series::latest),
        Some(sample(2_000_000, 3.5))
    );
    assert_eq!(
        vm.topology.instant, 2_000_000,
        "the topology still reads the batch"
    );
    // The next scenario is another declaration; none, no series.
    vm.apply(&Message::Answer(Answer::Scenario(None)));
    assert!(vm.series.is_empty());
}

#[test]
fn a_move_backward_takes_the_samples_from_the_instant_it_reached_out_of_the_series() {
    // The run is as it stood at the start of the instant: the sample at it is not yet taken.
    let mut s = series_of(&[(10, 1.0), (20, 2.0), (30, 3.0), (40, 4.0)]);
    let _ = s.columns(0, 99, 10);
    s.truncate_from(30);
    assert_eq!(s.samples().map(|p| p.tick).collect::<Vec<_>>(), [10, 20]);
    assert_eq!(s.count(), 2);
    assert_eq!(
        s.columns(0, 99, 10)[3],
        None,
        "tick 30 is not counted in its column"
    );
    s.truncate_from(1_000);
    assert_eq!(s.count(), 2, "nothing from it on, nothing taken");
    s.backfill(&[sample(1, 0.1), sample(5, 0.5)]);
    s.truncate_from(5);
    assert_eq!(s.samples().map(|p| p.tick).collect::<Vec<_>>(), [1]);
}
#[test]
fn the_fields_of_a_channel_before_the_drawn_one_are_numbered_first_and_the_offsets_follow_the_layout()
 {
    // A `debug` channel of a flag, an `f64`, a counter and an `f64` comes before `control`: its two
    // `f64`s are indexes 0 and 1 (not drawn) and `control`'s are 2 to 4; the offsets skip the flag
    // (one byte) and the counter (eight).
    let mut s = scenario();
    s.modules[0].channels.insert(
        0,
        ChannelSummary {
            channel: 9,
            name: "debug".into(),
            fields: vec![
                ("flag".into(), "bool".into()),
                ("a".into(), "f64".into()),
                ("n".into(), "u64".into()),
                ("b".into(), "f64".into()),
            ],
        },
    );
    s.modules[0].channels[2]
        .fields
        .insert(0, ("seen".into(), "bool".into()));
    let set = SeriesSet::of(&s, &Drawn::CIRCUIT);
    let rows: Vec<(u8, usize, &str)> = set
        .list()
        .iter()
        .filter(|x| x.source.actor == 5)
        .map(|x| (x.source.index, x.source.offset, x.source.field.as_str()))
        .collect();
    assert_eq!(rows, [(2, 1, "input"), (3, 9, "output"), (4, 17, "error")]);
    // A field of variable size ends the fixed prefix: the `f64`s after it have no offset a reader
    // can know and are not series.
    s.modules[0].channels[2]
        .fields
        .insert(1, ("why".into(), "text".into()));
    let set = SeriesSet::of(&s, &Drawn::CIRCUIT);
    assert!(set.list().iter().all(|x| x.source.actor != 5));
}

#[test]
fn a_signals_own_unit_is_the_series_unit_before_the_sensors() {
    let mut s = scenario();
    s.connections[0].attrs.push(("unit".into(), "krpm".into()));
    let set = SeriesSet::of(&s, &Drawn::CIRCUIT);
    assert_eq!(set.get(2, 0).and_then(|x| x.unit.as_deref()), Some("krpm"));
}

#[test]
fn a_following_window_is_moved_by_the_displays_instant_record_and_a_released_one_is_not() {
    let mut vm = crate::ViewModel::new();
    vm.plots.viewport.span = 1_000;
    vm.display(&[florn_control::display::Record::Instant { tick: 5_000 }]);
    assert_eq!(vm.plots.viewport.from, 4_001);
    vm.plots.viewport.follow = false;
    vm.display(&[florn_control::display::Record::Instant { tick: 9_000 }]);
    assert_eq!(vm.plots.viewport.from, 4_001);
}

#[test]
fn the_columns_are_built_again_for_another_window_or_another_width() {
    let mut s = series_of(&[(0, 1.0), (50, 2.0), (150, 3.0)]);
    assert_eq!(s.columns(0, 99, 10).len(), 10);
    assert_eq!(s.columns(0, 99, 20).len(), 20, "another width");
    let wide = s.columns(0, 199, 20).to_vec();
    assert_eq!(
        wide.iter().flatten().count(),
        3,
        "another end: the sample at 150 is in"
    );
    let narrow = s.columns(0, 99, 20).to_vec();
    assert_eq!(narrow.iter().flatten().count(), 2, "and out again");
}

#[test]
fn a_unit_the_table_leaves_empty_is_no_unit() {
    let mut s = scenario();
    s.modules[0].tables[0].1[1].1 =
        "Map({\"input\": Text(\"\"), \"error\": Text(\"rad/s\")})".into();
    let set = SeriesSet::of(&s, &Drawn::CIRCUIT);
    assert_eq!(set.get(5, 0).and_then(|x| x.unit.as_deref()), None);
    assert_eq!(set.get(5, 2).and_then(|x| x.unit.as_deref()), Some("rad/s"));
}
