use super::*;
use florn_ui_model::florn_control::display::Record;
use florn_ui_model::florn_control::message::{
    ActorSummary, ChannelSummary, ModuleSummary, PortSummary, ScenarioSummary,
};
use florn_ui_model::florn_control::{Answer, At, Message};
use florn_ui_model::plot::{MARGIN_LEFT, PlotViewport};
use florn_ui_model::topology::Point;

fn port(id: u32, name: &str, ty: &str) -> PortSummary {
    PortSummary {
        id,
        name: name.into(),
        ty: ty.into(),
    }
}

/// The motor (id 2, its speed in `rad/s`) and the controller (id 5, three `f64`s of `control`,
/// the output dimensionless).
fn scenario() -> ScenarioSummary {
    ScenarioSummary {
        name: "motor-loop".into(),
        sha256: String::new(),
        tick: "1ns".into(),
        end: 4_000_000_000,
        tie: "by-identity".into(),
        seed: 1,
        actors: vec![
            ActorSummary {
                id: 2,
                name: "m1".into(),
                module: None,
                class: "default".into(),
                ports: vec![port(2, "speed", "circuit/signal-out")],
                config: Vec::new(),
                builtin: Some("circuit/dc-motor".into()),
            },
            ActorSummary {
                id: 5,
                name: "pid".into(),
                module: Some("pid".into()),
                class: "default".into(),
                ports: Vec::new(),
                config: Vec::new(),
                builtin: None,
            },
        ],
        connections: Vec::new(),
        modules: vec![ModuleSummary {
            name: "pid".into(),
            source_sha256: String::new(),
            contract: String::new(),
            kind: "controller".into(),
            description: None,
            port_kinds: Vec::new(),
            params: Vec::new(),
            channels: vec![ChannelSummary {
                channel: 3,
                name: "control".into(),
                fields: ["input", "output", "error"]
                    .iter()
                    .map(|n| (n.to_string(), "f64".to_string()))
                    .collect(),
            }],
            needs: Vec::new(),
            tables: vec![(
                "circuit".into(),
                vec![(
                    "control-units".into(),
                    "Map({\"input\": Text(\"rad/s\"), \"error\": Text(\"rad/s\")})".into(),
                )],
            )],
        }],
        missing_modules: Vec::new(),
        domains: vec!["core".into(), "circuit".into()],
        changes: Vec::new(),
        classes: Vec::new(),
        limits: Vec::new(),
    }
}

fn model() -> ViewModel {
    let mut vm = ViewModel::new();
    vm.apply(&Message::Answer(Answer::Scenario(Some(scenario()))));
    vm.plots.viewport = PlotViewport {
        from: 0,
        span: 1_000,
        follow: false,
        size: Point { x: 664.0, y: 300.0 },
    };
    vm
}

fn sample(vm: &mut ViewModel, place: u16, index: u8, tick: u64, value: f32) {
    vm.display(&[Record::Sample {
        index,
        actor: place,
        value,
        tick,
    }]);
}

fn polylines(scene: &Scene) -> Vec<(&Vec<Point>, u8)> {
    scene
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Clip { items, .. } => Some(items),
            _ => None,
        })
        .flatten()
        .filter_map(|i| match i {
            Item::Polyline { points, dash, .. } => Some((points, *dash)),
            _ => None,
        })
        .collect()
}

fn axes(scene: &Scene) -> Vec<&Axis> {
    scene
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Axis(a) => Some(a),
            _ => None,
        })
        .collect()
}

#[test]
fn sensors_are_drawn_and_controllers_wait_for_their_toggle() {
    let mut vm = model();
    sample(&mut vm, 0, 0, 100, 5.0);
    sample(&mut vm, 1, 1, 100, 0.5);
    sample(&mut vm, 1, 0, 100, 4.0);
    sample(&mut vm, 1, 2, 100, 1.0);
    let s = waveforms(&mut vm);
    assert_eq!(polylines(&s).len(), 1, "the motor's speed only");
    assert_eq!((s.description.series, s.description.lanes), (1, 1));
    vm.plots.toggle(5, 0);
    vm.plots.toggle(5, 1);
    vm.plots.toggle(5, 2);
    let s = waveforms(&mut vm);
    assert_eq!(polylines(&s).len(), 4);
    // The speed, the controller's input and error are `rad/s`: one lane; its output is
    // dimensionless: another.
    assert_eq!((s.description.series, s.description.lanes), (4, 2));
}

#[test]
fn each_lane_has_its_own_value_axis_and_the_time_axis_is_shared() {
    let mut vm = model();
    for i in 0..3 {
        vm.plots.toggle(5, i);
    }
    sample(&mut vm, 1, 1, 100, 0.5);
    let s = waveforms(&mut vm);
    let axes = axes(&s);
    let units: Vec<(&str, bool)> = axes
        .iter()
        .map(|a| (a.unit.as_str(), a.horizontal))
        .collect();
    assert_eq!(units, [("rad/s", false), ("", false), ("s", true)]);
    let time = axes[2];
    assert!(time.ticks.len() >= 2 && time.ticks.len() <= 64);
    assert!(
        time.ticks
            .iter()
            .all(|(x, _)| *x >= 0.0 && *x <= time.length + 0.5)
    );
    assert!(time.ticks.windows(2).all(|w| w[0].1 < w[1].1), "in order");
}

#[test]
fn a_sensor_is_joined_and_a_controller_holds_its_value_flat_until_the_next_sample() {
    let mut vm = model();
    vm.plots.toggle(5, 1);
    for (t, v) in [(100, 1.0), (300, 3.0), (500, 2.0)] {
        sample(&mut vm, 0, 0, t, v);
    }
    for (t, v) in [(100, 0.1), (300, 0.3), (500, 0.2)] {
        sample(&mut vm, 1, 1, t, v);
    }
    vm.apply(&Message::Progress(
        florn_ui_model::florn_control::Progress {
            state: florn_ui_model::florn_control::RunState::Paused,
            now: At {
                tick: 600,
                microstep: 0,
            },
            handled: 0,
            records: 0,
        },
    ));
    let s = waveforms(&mut vm);
    let lines = polylines(&s);
    assert_eq!(lines.len(), 2);
    let (sensor, _) = lines[0];
    let (controller, _) = lines[1];
    assert_eq!(sensor.len(), 3, "one point for each sample, joined");
    // Three samples as steps: the first point, then for each later one a horizontal run into it
    // and the rise, and the last value held on to the instant.
    assert_eq!(controller.len(), 1 + 2 * 2 + 1);
    let flat = controller
        .windows(2)
        .filter(|w| w[0].y == w[1].y && w[0].x < w[1].x)
        .count();
    assert_eq!(flat, 3, "{controller:?}");
    assert!(
        (controller.last().map(|p| p.x).unwrap_or(0.0) - vm.plots.viewport.x_of(600)).abs() < 0.01
    );
}

#[test]
fn every_point_is_inside_its_lane_and_the_clip_is_the_lane() {
    let mut vm = model();
    for t in 0..200u64 {
        sample(&mut vm, 0, 0, t * 5, ((t % 20) as f32 - 10.0) * 100.0);
    }
    let s = waveforms(&mut vm);
    let [ax, ay, aw, _] = vm.plots.viewport.area();
    for i in &s.items {
        if let Item::Clip { rect, items } = i {
            assert_eq!(rect[0], ax);
            assert_eq!(rect[2], aw);
            assert!(rect[1] >= ay);
            for it in items {
                if let Item::Polyline { points, .. } = it {
                    for p in points {
                        assert!(p.y >= rect[1] && p.y <= rect[1] + rect[3], "{p:?} {rect:?}");
                        assert!(p.x >= ax - 0.01 && p.x <= ax + aw + 0.01, "{p:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn a_million_samples_draw_one_polyline_of_at_most_two_points_a_column() {
    // RFC 014 rule 4 and its reduction row: the objects drawn are under the ceiling however many
    // samples the run produced.
    let mut vm = model();
    vm.plots.toggle(5, 0);
    vm.plots.toggle(5, 1);
    vm.plots.toggle(5, 2);
    for t in 0..1_000_000u64 {
        sample(&mut vm, 0, 0, t, (t % 1_000) as f32);
        if t % 50 == 0 {
            sample(&mut vm, 1, 1, t, 0.25);
        }
    }
    vm.plots.viewport.from = 996_000;
    vm.plots.viewport.span = 4_000;
    let columns = vm.plots.viewport.columns();
    let s = waveforms(&mut vm);
    let lines = polylines(&s);
    assert!(lines.len() >= 2);
    for (points, _) in &lines {
        assert!(points.len() <= 2 * columns + 2, "{} points", points.len());
    }
    assert!(
        s.objects() <= crate::FLIGHT_CEILING,
        "{} objects",
        s.objects()
    );
    assert!(
        vm.series
            .list()
            .iter()
            .all(|s| s.kept() <= florn_ui_model::series::RING)
    );
}

#[test]
fn the_cursor_is_a_line_at_the_instant_inside_the_panel() {
    let mut vm = model();
    sample(&mut vm, 0, 0, 100, 1.0);
    vm.plots.cursor = Some(250);
    let s = waveforms(&mut vm);
    let x = vm.plots.viewport.x_of(250);
    let cursor = s.items.iter().find_map(|i| match i {
        Item::Lines { segments, stroke } if stroke.color == CURSOR => Some(segments[0]),
        _ => None,
    });
    assert_eq!(cursor.map(|c| (c[0], c[2])), Some((x, x)));
    vm.plots.cursor = Some(5_000);
    let s = waveforms(&mut vm);
    assert!(
        !s.items
            .iter()
            .any(|i| matches!(i, Item::Lines { stroke, .. } if stroke.color == CURSOR)),
        "a cursor outside the window is not drawn"
    );
}

#[test]
fn each_series_carries_its_label_and_the_series_differ_by_dash_as_well_as_colour() {
    let mut vm = model();
    vm.plots.toggle(5, 0);
    sample(&mut vm, 0, 0, 100, 1.0);
    sample(&mut vm, 1, 0, 100, 2.0);
    let s = waveforms(&mut vm);
    let dashes: Vec<u8> = polylines(&s).iter().map(|(_, d)| *d).collect();
    assert_eq!(dashes.len(), 2);
    assert_ne!(dashes[0], dashes[1]);
    let labels: Vec<&str> = s
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Labels { labels, .. } => Some(labels),
            _ => None,
        })
        .flatten()
        .map(|(_, t)| t.as_str())
        .collect();
    assert_eq!(labels, ["m1.speed", "pid.input"]);
    for c in PALETTE {
        // Non-text contrast against white, 3 to 1 (WCAG 2.2 1.4.11).
        let lum = |v: u8| {
            let c = f64::from(v) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let l = 0.2126 * lum(c.0) + 0.7152 * lum(c.1) + 0.0722 * lum(c.2);
        assert!(1.05 / (l + 0.05) >= 3.0, "{c:?}");
    }
}

#[test]
fn nothing_shown_is_a_time_axis_alone_and_a_flat_series_gets_a_range() {
    let mut vm = model();
    vm.plots.toggle(2, 0);
    let s = waveforms(&mut vm);
    assert_eq!(s.description.lanes, 0);
    assert_eq!(axes(&s).len(), 1);
    assert!(polylines(&s).is_empty());
    vm.plots.toggle(2, 0);
    sample(&mut vm, 0, 0, 100, 7.0);
    sample(&mut vm, 0, 0, 200, 7.0);
    let s = waveforms(&mut vm);
    let (points, _) = polylines(&s).remove(0);
    assert!(points.iter().all(|p| p.y.is_finite()));
    assert_eq!(padded(Some((7.0, 7.0))), (6.0, 8.0));
    assert_eq!(padded(None), (0.0, 1.0));
}

#[test]
fn the_axis_labels_lie_left_of_the_area() {
    let vm = model();
    const { assert!(MARGIN_LEFT >= 48.0, "room for the value labels") };
    assert_eq!(vm.plots.viewport.area()[0], MARGIN_LEFT);
}

#[test]
fn a_column_is_walked_from_its_first_value_towards_its_last() {
    // More samples than columns: each column gives its extremes in the order the series went, so
    // a rising column is drawn up and a falling one down.
    let mut vm = model();
    vm.plots.viewport.span = 4_000;
    let width = vm.plots.viewport.columns() as u64;
    assert!(width < 4_000, "more samples than columns");
    for t in 0..4_000u64 {
        // The first column rises, the second falls (by the sign), and so on.
        let column = t * width / 4_000;
        let v = if column.is_multiple_of(2) {
            t as f32
        } else {
            -(t as f32)
        };
        sample(&mut vm, 0, 0, t, v);
    }
    let s = waveforms(&mut vm);
    let (points, _) = polylines(&s).remove(0);
    assert!(points.len() >= 4);
    // y grows downwards: a rising column's first point is the lower (larger y), and a falling
    // column's is the higher.
    assert!(
        points[0].y > points[1].y,
        "column 0 rises: {:?}",
        &points[..2]
    );
    assert!(
        points[2].y < points[3].y,
        "column 1 falls: {:?}",
        &points[2..4]
    );
}

#[test]
fn a_window_with_exactly_as_many_samples_as_columns_is_drawn_sample_by_sample() {
    let mut vm = model();
    let width = vm.plots.viewport.columns() as u64;
    vm.plots.viewport.span = width * 4;
    for k in 0..width {
        sample(&mut vm, 0, 0, k * 4, (k % 7) as f32);
    }
    let s = waveforms(&mut vm);
    let (points, _) = polylines(&s).remove(0);
    assert_eq!(points.len() as u64, width, "one point for each sample");
    sample(&mut vm, 0, 0, width * 4 - 1, 3.0);
    let s = waveforms(&mut vm);
    let (points, _) = polylines(&s).remove(0);
    assert!(
        points.len() as u64 > width && points.len() as u64 <= 2 * width,
        "now by columns"
    );
}

#[test]
fn a_column_that_ends_where_it_began_is_walked_up_and_then_down() {
    let mut vm = model();
    let width = vm.plots.viewport.columns() as u64;
    vm.plots.viewport.span = width * 8;
    // Each column: 0, 5, 0 (first and last the same, the extremes apart).
    for k in 0..width {
        sample(&mut vm, 0, 0, k * 8, 0.0);
        sample(&mut vm, 0, 0, k * 8 + 2, 5.0);
        sample(&mut vm, 0, 0, k * 8 + 4, 0.0);
    }
    let s = waveforms(&mut vm);
    let (points, _) = polylines(&s).remove(0);
    assert_eq!(points.len() as u64, 2 * width);
    assert!(points[0].y > points[1].y, "up first: {:?}", &points[..2]);
}

#[test]
fn the_cursor_at_the_windows_last_tick_is_drawn_and_a_value_range_has_air_around_it() {
    let mut vm = model();
    sample(&mut vm, 0, 0, 100, 1.0);
    sample(&mut vm, 0, 0, 200, 9.0);
    vm.plots.cursor = Some(vm.plots.viewport.to());
    let s = waveforms(&mut vm);
    assert!(
        s.items
            .iter()
            .any(|i| matches!(i, Item::Lines { stroke, .. } if stroke.color == CURSOR))
    );
    // The extremes are inside the lane, not on its frame.
    let (points, _) = polylines(&s).remove(0);
    let [_, ay, _, ah] = vm.plots.viewport.area();
    let (lo, hi) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
        (lo.min(p.y), hi.max(p.y))
    });
    assert!(
        lo > ay + 0.5 && hi < ay + ah - 0.5,
        "{lo} {hi} in {ay}..{}",
        ay + ah
    );
}

#[test]
fn the_value_held_as_the_window_begins_starts_the_line_at_its_left_edge() {
    let mut vm = model();
    sample(&mut vm, 0, 0, 100, 1.0);
    sample(&mut vm, 0, 0, 700, 3.0);
    vm.plots.viewport.from = 500;
    let s = waveforms(&mut vm);
    let (points, _) = polylines(&s).remove(0);
    assert_eq!(points[0].x, vm.plots.viewport.area()[0], "{points:?}");
    assert_eq!(points.len(), 2, "the held value and the sample inside");
}

#[test]
fn a_polyline_is_one_object_however_many_points_it_joins() {
    let line = Item::Polyline {
        points: vec![Point { x: 0.0, y: 0.0 }; 10],
        stroke: Stroke {
            color: CURSOR,
            width: 1.0,
            dashed: false,
        },
        dash: 0,
    };
    assert_eq!(line.objects(), 1);
    assert_eq!(
        Item::Clip {
            rect: [0.0; 4],
            items: vec![line.clone(), line]
        }
        .objects(),
        2
    );
}
