use super::*;
use crate::series::{Sample, Series, Source};

fn viewport() -> PlotViewport {
    PlotViewport {
        from: 0,
        span: 1_000,
        follow: true,
        size: Point { x: 664.0, y: 300.0 },
    }
}

fn series(controller: bool) -> Series {
    let mut s = Series::new(
        Source {
            actor: 2,
            index: 0,
            channel: 1,
            offset: 0,
            field: "speed".into(),
        },
        "m1.speed".into(),
        None,
        controller,
    );
    s.push(Sample {
        tick: 1,
        value: 1.0,
    });
    s
}

#[test]
fn the_plot_area_is_inside_the_margins_and_a_tick_maps_into_it() {
    let v = viewport();
    let [x, y, w, h] = v.area();
    assert_eq!((x, y), (MARGIN_LEFT, MARGIN_TOP));
    assert_eq!(w, 664.0 - MARGIN_LEFT - MARGIN_RIGHT);
    assert_eq!(h, 300.0 - MARGIN_TOP - MARGIN_BOTTOM);
    assert_eq!(v.x_of(0), x);
    assert_eq!(v.x_of(500), x + w / 2.0);
    assert_eq!(v.tick_at(x + w / 2.0), 500);
    assert_eq!(v.to(), 999);
    assert_eq!(v.columns(), w as usize);
}

#[test]
fn a_following_window_ends_at_the_instant_and_a_released_one_stays() {
    let mut v = viewport();
    v.follow_run(500);
    assert_eq!(v.from, 0, "before the window has filled it starts at 0");
    v.follow_run(5_000);
    assert_eq!((v.from, v.to()), (4_001, 5_000));
    v.pan_by(10.0);
    assert!(!v.follow, "a drag releases it");
    let from = v.from;
    v.follow_run(9_000);
    assert_eq!(v.from, from, "a released window does not move");
}

#[test]
fn zoom_keeps_the_tick_under_the_pointer_and_releases_the_window() {
    let mut v = viewport();
    let anchor_px = v.area()[0] + v.area()[2] / 4.0;
    let before = v.tick_at(anchor_px);
    v.zoom_at(anchor_px, 2.0);
    assert!(!v.follow);
    assert_eq!(v.span, 500);
    let after = v.tick_at(anchor_px);
    assert!(before.abs_diff(after) <= 1, "{before} {after}");
}

#[test]
fn a_pan_to_before_the_start_stops_at_the_start() {
    let mut v = viewport();
    v.pan_by(10_000.0);
    assert_eq!(v.from, 0);
}

#[test]
fn fit_shows_what_the_run_produced_and_follows_again() {
    let mut v = viewport();
    v.pan_by(5.0);
    v.fit(100, 4_099);
    assert_eq!((v.from, v.span, v.follow), (100, 4_000, true));
    v.fit(0, 0);
    assert_eq!(v.span, 2, "never an empty window");
}

#[test]
fn sensors_are_on_by_default_and_controllers_off_and_a_toggle_flips_either() {
    let mut p = Plots::default();
    let (sensor, controller) = (series(false), series(true));
    assert!(p.shown(&sensor));
    assert!(!p.shown(&controller));
    p.toggle(2, 0);
    assert!(
        !p.shown(&sensor) && p.shown(&controller),
        "same actor and index in the fixture"
    );
    p.toggle(2, 0);
    assert!(p.shown(&sensor));
    p.toggle(2, 0);
    p.reset("1us");
    assert!(p.shown(&sensor), "a new scenario forgets the toggles");
    assert_eq!(p.seconds_per_tick, 1e-6);
}

#[test]
fn a_ticks_length_is_read_from_the_scenarios_text() {
    assert_eq!(time_base("1ns"), Some(1e-9));
    assert_eq!(time_base("100us"), Some(100e-6));
    assert_eq!(time_base("1ms"), Some(1e-3));
    assert_eq!(time_base("2s"), Some(2.0));
    assert_eq!(time_base("ns"), None);
    assert_eq!(time_base("3parsecs"), None);
}

#[test]
fn an_axis_steps_by_one_two_or_five_times_a_power_of_ten() {
    assert_eq!(nice_step(10.0, 5), 2.0);
    assert_eq!(nice_step(1.0, 10), 0.1);
    assert_eq!(nice_step(0.04, 4), 0.01);
    assert_eq!(nice_step(7.0, 3), 5.0);
    assert_eq!(nice_step(0.0, 5), 1.0);
    assert_eq!(nice_step(f64::NAN, 5), 1.0);
    assert_eq!(nice_step(100.0, 0), 100.0);
}
