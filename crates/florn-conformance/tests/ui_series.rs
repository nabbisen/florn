//! The view model's series over the real Host (RFC 014 rules 3 to 5; checklist B-52): the display's
//! `sample` records and the trace's report payloads draw the same series; the last value of each
//! equals the last report to `f32` rounding and the counts are equal; what the ring let go is
//! filled from `trace-window`; a built-in scenario is kept by the authoring draft and read back
//! as the scenario it was; and the workspace is chosen by the domains.
use florn_admission::Admission;
use florn_conformance::circuit_scenarios::{motor_loop, motor_loop_text};
use florn_conformance::host;
use florn_conformance::scenario::Scenario;
use florn_control::display::Record;
use florn_control::message::TraceEntry;
use florn_control::{Answer, Envelope, Message, Query, RunState, Unit};
use florn_host::Controller;
use florn_platform::FixedPlatform;
use florn_ui_model::ViewModel;
use florn_ui_model::series::{RING, Sample, Series};
use florn_ui_model::workspace::{PanelKind, SceneKind};

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

/// The scenario opened in a fresh controller and view model, a run started, run to its end.
fn run_to_end(s: &Scenario) -> (C, ViewModel) {
    let mut c = controller();
    let mut vm = ViewModel::new();
    vm.connect();
    pump(&mut c, &mut vm);
    vm.open_scenario(s.text(&s.hashes()).into_bytes());
    pump(&mut c, &mut vm);
    for bytes in s.modules().into_values() {
        vm.load_module(bytes);
    }
    pump(&mut c, &mut vm);
    vm.new_run(Some(1));
    pump(&mut c, &mut vm);
    while vm.run.state != RunState::Ended {
        vm.step(Unit::ToTick(vm.run.now.map_or(0, |a| a.tick) + 50_000_000));
        pump(&mut c, &mut vm);
    }
    (c, vm)
}

/// Every trace entry of the run, in the pieces `trace-window` answers.
fn trace(c: &mut C, records: u64) -> Vec<TraceEntry> {
    let mut all = Vec::new();
    let mut from = 0;
    while from < records {
        let count = (records - from).min(u64::from(Query::MAX_WINDOW)) as u32;
        for m in c.handle(Envelope::new(Message::Query(Query::TraceWindow {
            from,
            count,
        }))) {
            if let Message::Answer(Answer::TraceWindow(entries)) = m {
                from += entries.len() as u64;
                all.extend(entries);
            }
        }
    }
    all
}

fn variant(change: impl FnOnce(String) -> String) -> Scenario {
    Scenario::raw(
        "motor-loop-fast",
        1_500_000_000,
        change(motor_loop_text()),
        &["pid"],
    )
}

#[test]
fn the_last_value_of_each_series_is_the_last_report_of_the_trace_and_the_counts_are_equal() {
    let (mut c, vm) = run_to_end(&motor_loop());
    let entries = trace(&mut c, vm.run.records);
    let decoded = vm.series.decode(&entries);
    assert!(!decoded.is_empty());
    let labels: Vec<&str> = vm.series.list().iter().map(|s| s.label.as_str()).collect();
    assert!(
        labels.contains(&"m1.speed") && labels.contains(&"pid.output"),
        "{labels:?}"
    );
    for (i, s) in vm.series.list().iter().enumerate() {
        let from_trace: Vec<Sample> = decoded
            .iter()
            .filter(|(n, _)| *n == i)
            .map(|(_, s)| *s)
            .collect();
        assert_eq!(
            s.count() as usize,
            from_trace.len(),
            "{}: the counts are equal",
            s.label
        );
        assert_eq!(
            s.latest(),
            from_trace.last().copied(),
            "{}: the last value",
            s.label
        );
        if s.kept() as u64 == s.count() {
            assert_eq!(
                s.samples().collect::<Vec<_>>(),
                from_trace,
                "{}: all of them",
                s.label
            );
        }
    }
    let speed = vm.series.get(2, 0).expect("the motor's speed");
    assert!(speed.count() >= 1_000, "{} samples", speed.count());
    assert_eq!(speed.unit.as_deref(), Some("rad/s"));
    assert!(!vm.series.get(5, 1).expect("pid.output").unit.is_some());
}

#[test]
fn what_the_ring_let_go_is_filled_from_the_trace_and_equals_the_live_series() {
    // A sensor of 250 us for 1.5 s produces 6,000 samples: more than the ring holds.
    let s = variant(|t| t.replacen("period = \"1ms\"", "period = \"250us\"", 1));
    let (mut c, mut vm) = run_to_end(&s);
    let entries = trace(&mut c, vm.run.records);
    let speed: &Series = vm.series.get(2, 0).expect("the motor's speed");
    assert_eq!(speed.count(), 6_000);
    assert_eq!(speed.kept(), RING);
    let newest: Vec<Sample> = speed.samples().collect();
    vm.series.backfill(&entries);
    let speed = vm.series.get(2, 0).expect("the motor's speed");
    assert_eq!(speed.kept(), 6_000, "the whole run again");
    let all: Vec<Sample> = speed.samples().collect();
    assert_eq!(
        all[all.len() - RING..],
        newest[..],
        "the ring's samples are the newest of them"
    );
    // The older ones are the trace's: the same ticks, 250 us apart.
    assert!(all.windows(2).all(|w| w[1].tick - w[0].tick == 250_000));
    vm.series.forget_older();
    assert_eq!(vm.series.get(2, 0).map(Series::kept), Some(RING));
}

#[test]
fn the_motor_loop_gets_the_circuits_workspace() {
    let (_, vm) = run_to_end(&motor_loop());
    assert_eq!(vm.workspace.scene(), SceneKind::Schematic);
    assert!(vm.workspace.shows(PanelKind::Plots));
    assert_eq!(vm.plots.seconds_per_tick, 1e-9);
}

/// The scenario the draft of a loaded scenario writes, loaded again by a fresh Host: its summary.
fn reloaded(s: &Scenario) -> (ViewModel, String) {
    let mut c = controller();
    let mut vm = ViewModel::new();
    vm.connect();
    pump(&mut c, &mut vm);
    vm.open_scenario(s.text(&s.hashes()).into_bytes());
    pump(&mut c, &mut vm);
    for bytes in s.modules().into_values() {
        vm.load_module(bytes);
    }
    pump(&mut c, &mut vm);
    let text = vm.draft.to_toml();
    let mut c2 = controller();
    let mut vm2 = ViewModel::new();
    vm2.connect();
    pump(&mut c2, &mut vm2);
    vm2.open_scenario(text.clone().into_bytes());
    pump(&mut c2, &mut vm2);
    for bytes in s.modules().into_values() {
        vm2.load_module(bytes);
    }
    pump(&mut c2, &mut vm2);
    (vm2, text)
}

#[test]
fn motor_loop_loaded_into_the_draft_and_saved_reads_back_to_the_same_scenario_hash() {
    // RFC 014 rule 15, item 1 and its row of the verification table: a scenario made of built-ins and
    // a guest controller is kept whole by the draft, with its change of the circuit's world.
    let s = motor_loop();
    let mut c = controller();
    let mut vm = ViewModel::new();
    vm.connect();
    pump(&mut c, &mut vm);
    vm.open_scenario(s.text(&s.hashes()).into_bytes());
    pump(&mut c, &mut vm);
    for bytes in s.modules().into_values() {
        vm.load_module(bytes);
    }
    pump(&mut c, &mut vm);
    let first = vm.scenario.clone().expect("loaded");
    assert!(vm.draft.dropped.is_empty() && vm.draft.can_apply());
    let (again, text) = reloaded(&s);
    let second = again.scenario.as_ref().expect("the draft's manifest loads");
    assert!(again.draft.dropped.is_empty());
    assert_eq!(
        second.sha256, first.sha256,
        "the same scenario hash:\n{text}"
    );
    assert_eq!(second.actors, first.actors);
    assert_eq!(second.connections, first.connections);
    assert_eq!(second.changes, first.changes);
}

#[test]
fn the_worlds_identity_base_the_draft_writes_is_the_models() {
    assert_eq!(
        florn_ui_model::draft::WORLD_IDENTITY_BASE,
        florn_model::manifest::WORLD_IDENTITY_BASE
    );
}

#[test]
fn every_shipped_scenario_reads_back_from_the_draft_to_its_hash_but_the_one_with_a_profile() {
    // `rc-step` has a `[circuit]` table (the solver's profile, RFC 013 rule 18) that the scenario
    // summary does not carry (empty as it is: its presence is part of the canonical form), so its
    // draft writes the scenario without it: another hash. It is the one scenario of the kit that has
    // a domain table; the review request puts the finding to the
    // architect (a trailing optional section of the summary, as PR-43 did for the built-in's name).
    // The test names it, so that a second one, or its repair, is noticed.
    let mut differ = Vec::new();
    for shipped in florn_conformance::shipped::all() {
        let s = shipped.scenario;
        let mut c = controller();
        let mut vm = ViewModel::new();
        vm.connect();
        pump(&mut c, &mut vm);
        vm.open_scenario(s.text(&s.hashes()).into_bytes());
        pump(&mut c, &mut vm);
        for bytes in s.modules().into_values() {
            vm.load_module(bytes);
        }
        pump(&mut c, &mut vm);
        let Some(first) = vm.scenario.clone() else {
            continue;
        };
        if !vm.draft.can_apply() {
            differ.push(format!("{}: dropped {:?}", s.name, vm.draft.dropped));
            continue;
        }
        let (again, _) = reloaded(&s);
        match again.scenario.as_ref() {
            Some(second) if second.sha256 == first.sha256 => {}
            Some(_) => differ.push(format!("{}: another hash", s.name)),
            None => differ.push(format!("{}: does not load", s.name)),
        }
    }
    assert_eq!(differ, ["rc-step: another hash"], "{differ:#?}");
}

// ---- the pictures of the review request: the waveform scene of a real run, drawn as SVG ----

mod picture {
    use florn_ui_scene::{Axis, Color, Item, Scene, Stroke};
    use std::fmt::Write;

    fn rgb(c: Color) -> String {
        format!("rgb({},{},{})", c.0, c.1, c.2)
    }
    fn number(v: f64) -> String {
        let t = format!("{v:.6}");
        let t = t.trim_end_matches('0').trim_end_matches('.');
        if t.is_empty() || t == "-0" {
            "0".into()
        } else {
            t.into()
        }
    }
    fn dash(n: u8) -> &'static str {
        ["", "8 4", "2 3", "10 3 2 3"][usize::from(n) % 4]
    }
    fn stroke(s: &Stroke, pattern: &str) -> String {
        let d = if pattern.is_empty() && !s.dashed {
            String::new()
        } else if pattern.is_empty() {
            " stroke-dasharray=\"6 4\"".into()
        } else {
            format!(" stroke-dasharray=\"{pattern}\"")
        };
        format!(
            "stroke=\"{}\" stroke-width=\"{}\" fill=\"none\"{d}",
            rgb(s.color),
            s.width
        )
    }
    fn axis(out: &mut String, a: &Axis) {
        let (ox, oy) = (f64::from(a.origin.x), f64::from(a.origin.y));
        let top = a.ticks.iter().map(|t| t.1.abs()).fold(0.0, f64::max);
        let (scale, unit) = if a.unit == "s" {
            if top >= 1.0 || top == 0.0 {
                (1.0, "s")
            } else if top >= 1e-3 {
                (1e3, "ms")
            } else if top >= 1e-6 {
                (1e6, "µs")
            } else {
                (1e9, "ns")
            }
        } else {
            (1.0, a.unit.as_str())
        };
        let len = f64::from(a.length);
        let (x1, y1) = if a.horizontal {
            (ox + len, oy)
        } else {
            (ox, oy - len)
        };
        let _ = writeln!(
            out,
            "<line x1=\"{ox}\" y1=\"{oy}\" x2=\"{x1}\" y2=\"{y1}\" {}/>",
            stroke(&a.stroke, "")
        );
        for (offset, value) in &a.ticks {
            let o = f64::from(*offset);
            let text = number(value * scale);
            if a.horizontal {
                let _ = writeln!(
                    out,
                    "<line x1=\"{x}\" y1=\"{oy}\" x2=\"{x}\" y2=\"{}\" {}/><text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" font-size=\"{}\" fill=\"{}\">{text}</text>",
                    oy + 4.0,
                    stroke(&a.stroke, ""),
                    oy + 4.0 + f64::from(a.font_px),
                    a.font_px,
                    rgb(a.fill),
                    x = ox + o
                );
            } else {
                let y = oy - o;
                let _ = writeln!(
                    out,
                    "<line x1=\"{ox}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" {}/><text x=\"{}\" y=\"{}\" text-anchor=\"end\" font-size=\"{}\" fill=\"{}\">{text}</text>",
                    ox - 4.0,
                    stroke(&a.stroke, ""),
                    ox - 6.0,
                    y + f64::from(a.font_px) * 0.35,
                    a.font_px,
                    rgb(a.fill)
                );
            }
        }
        if !unit.is_empty() {
            let (x, y) = if a.horizontal {
                (ox + len, oy + 4.0 + 2.0 * f64::from(a.font_px))
            } else {
                (ox - 6.0, oy - len - 2.0)
            };
            let _ = writeln!(
                out,
                "<text x=\"{x}\" y=\"{y}\" text-anchor=\"end\" font-size=\"{}\" fill=\"{}\">{unit}</text>",
                a.font_px,
                rgb(a.fill)
            );
        }
    }
    fn items(out: &mut String, list: &[Item], clip: &mut usize) {
        for item in list {
            match item {
                Item::Lines {
                    segments,
                    stroke: s,
                } => {
                    for g in segments {
                        let _ = writeln!(
                            out,
                            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" {}/>",
                            g[0],
                            g[1],
                            g[2],
                            g[3],
                            stroke(s, "")
                        );
                    }
                }
                Item::Polyline {
                    points,
                    stroke: s,
                    dash: d,
                } => {
                    let pts: Vec<String> = points
                        .iter()
                        .map(|p| format!("{:.2},{:.2}", p.x, p.y))
                        .collect();
                    let _ = writeln!(
                        out,
                        "<polyline points=\"{}\" {}/>",
                        pts.join(" "),
                        stroke(s, dash(*d))
                    );
                }
                Item::Rects {
                    rects,
                    fill,
                    stroke: s,
                } => {
                    for r in rects {
                        let f = fill.map_or("none".to_string(), rgb);
                        let st = s.as_ref().map_or(String::new(), |s| {
                            format!("stroke=\"{}\" stroke-width=\"{}\"", rgb(s.color), s.width)
                        });
                        let _ = writeln!(
                            out,
                            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{f}\" {st}/>",
                            r[0], r[1], r[2], r[3]
                        );
                    }
                }
                Item::Axis(a) => axis(out, a),
                Item::Clip { rect, items: inner } => {
                    *clip += 1;
                    let _ = writeln!(
                        out,
                        "<clipPath id=\"c{clip}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath><g clip-path=\"url(#c{clip})\">",
                        rect[0], rect[1], rect[2], rect[3]
                    );
                    items(out, inner, clip);
                    out.push_str("</g>\n");
                }
                Item::Labels {
                    labels,
                    font_px,
                    fill,
                } => {
                    for (p, t) in labels {
                        let _ = writeln!(
                            out,
                            "<text x=\"{}\" y=\"{}\" font-size=\"{font_px}\" fill=\"{}\">{t}</text>",
                            p.x,
                            p.y,
                            rgb(*fill)
                        );
                    }
                }
                Item::Circles { .. } | Item::Crosses { .. } => {}
            }
        }
    }
    pub fn svg(scene: &Scene, w: f32, h: f32, title: &str) -> String {
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"system-ui, sans-serif\"><title>{title}</title><rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n"
        );
        items(&mut out, &scene.items, &mut 0);
        out.push_str("</svg>\n");
        out
    }
}

#[test]
fn the_waveform_scene_of_the_motor_loop_is_drawn_and_kept_under_the_ceiling() {
    // The pictures of the review request are written when `FLORN_PICTURES` names a folder; the
    // assertions hold always: the plots of the real run, with the controller's series on, are a
    // handful of objects and the points of every polyline are bounded by the width.
    let (_, mut vm) = run_to_end(&motor_loop());
    vm.plots.viewport.size = florn_ui_model::Point { x: 800.0, y: 440.0 };
    let end = vm.run.now.map_or(0, |a| a.tick);
    vm.plots.viewport.fit(0, end);
    let sensors = florn_ui_scene::waveforms(&mut vm);
    for i in 0..3 {
        vm.plots.toggle(5, i);
    }
    let all = florn_ui_scene::waveforms(&mut vm);
    // The start-up: the first 60 ms, where the controller's steps are told apart.
    vm.plots.viewport.from = 0;
    vm.plots.viewport.span = 60_000_000;
    vm.plots.viewport.follow = false;
    let start = florn_ui_scene::waveforms(&mut vm);
    for scene in [&sensors, &all, &start] {
        assert!(scene.objects() < 100, "{} objects", scene.objects());
    }
    assert_eq!(
        (sensors.description.series, sensors.description.lanes),
        (1, 1)
    );
    assert_eq!((all.description.series, all.description.lanes), (4, 2));
    if let Ok(dir) = std::env::var("FLORN_PICTURES") {
        let dir = std::path::Path::new(&dir);
        std::fs::create_dir_all(dir).expect("the folder");
        for (name, title, scene) in [
            (
                "waveforms-sensors",
                "motor-loop: the motor's speed, the whole run",
                &sensors,
            ),
            (
                "waveforms-all",
                "motor-loop: the speed and the controller's series, in two lanes by unit",
                &all,
            ),
            (
                "waveforms-start",
                "motor-loop: the first 60 ms, the controller's steps",
                &start,
            ),
        ] {
            std::fs::write(
                dir.join(format!("{name}.svg")),
                picture::svg(scene, 800.0, 440.0, title),
            )
            .expect("the picture");
        }
    }
}

#[test]
fn a_move_backward_and_a_new_run_take_their_samples_out_of_the_series() {
    // RFC 009 rules 7 and 8 and RFC 014 rule 4: what the series hold after the instant a move
    // reached is gone, the run goes on from there under the same counts as the first time, and a
    // new run starts them again.
    let (mut c, mut vm) = run_to_end(&motor_loop());
    let whole = vm.series.get(2, 0).map(Series::count).expect("the speed");
    assert_eq!(whole, 1_500);
    assert!(vm.goto(500_000_000));
    pump(&mut c, &mut vm);
    let back = vm.series.get(2, 0).expect("the speed");
    assert!(
        back.latest().is_some_and(|s| s.tick < 500_000_000),
        "{:?}",
        back.latest()
    );
    assert_eq!(
        back.count(),
        500,
        "the samples at 0, 1 ms, ... 499 ms: the first is at start, none at the instant"
    );
    vm.run();
    pump(&mut c, &mut vm);
    while vm.run.state != RunState::Ended {
        pump(&mut c, &mut vm);
    }
    assert_eq!(
        vm.series.get(2, 0).map(Series::count),
        Some(whole),
        "the run again has the samples it had"
    );
    vm.new_run(Some(2));
    pump(&mut c, &mut vm);
    assert_eq!(
        vm.series.get(2, 0).map(Series::count),
        Some(0),
        "a new run starts them again"
    );
    assert!(vm.plots.viewport.follow && vm.plots.viewport.from == 0);
}
