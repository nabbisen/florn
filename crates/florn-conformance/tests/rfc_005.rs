//! RFC 005 §11, the rows that apply natively in 0.1 and are not already AC-003 (replay, the run
//! record), AC-004, AC-006 or the Host's and runner's own tests (the control interface, capacity).
use florn_conformance::host;
use florn_conformance::{bench, shipped};
use florn_engine::Engine;
use florn_host::Options;

#[test]
fn the_second_run_of_a_scenario_does_not_compile_the_module() {
    let s = shipped::all()
        .into_iter()
        .find(|s| s.scenario.name == "echo-pair")
        .unwrap()
        .scenario;
    let mut first = host::start(&s);
    first.run_to_end();
    let (engine, _) = first.into_parts();
    assert_eq!(engine.compiled_count(), 2, "two modules compiled once");
    let second = host::try_start_with(engine, &s, Options::default()).expect("starts");
    assert_eq!(
        second.engine().compiled_count(),
        2,
        "nothing compiled again"
    );
}

#[test]
fn the_crossing_benchmark_produces_its_figures() {
    let figures = bench::crossing(50);
    assert_eq!(figures.len(), 8);
    for f in &figures {
        assert!(f.micros.is_finite() && f.micros > 0.0, "{f:?}");
        eprintln!(
            "{} bytes, metered {}: {:.2} µs",
            f.bytes, f.metered, f.micros
        );
    }
    // The 64 KiB crossing is one copy each way, not one value per byte: it costs at most a
    // hundred times the call floor even in a debug build of the Host.
    let floor = figures
        .iter()
        .find(|f| f.bytes == 0 && !f.metered)
        .unwrap()
        .micros;
    let large = figures
        .iter()
        .find(|f| f.bytes == 65_536 && !f.metered)
        .unwrap()
        .micros;
    assert!(
        large < floor * 100.0 + 50.0,
        "64 KiB {large:.2} µs against a floor of {floor:.2} µs"
    );
}
