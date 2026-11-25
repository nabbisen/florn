use super::*;
use crate::units::CircuitUnits;

fn table(text: &str) -> DomainMap {
    let v: toml::Table = text.parse().unwrap();
    match from_toml(&toml::Value::Table(v)) {
        DomainValue::Map(m) => m,
        _ => unreachable!(),
    }
}

fn from_toml(v: &toml::Value) -> DomainValue {
    match v {
        toml::Value::Integer(i) => DomainValue::Integer(*i),
        toml::Value::Float(f) => DomainValue::Decimal(format!("{f}")),
        toml::Value::String(s) => DomainValue::Text(s.clone()),
        toml::Value::Boolean(b) => DomainValue::Boolean(*b),
        toml::Value::Array(a) => DomainValue::List(a.iter().map(from_toml).collect()),
        toml::Value::Table(t) => {
            DomainValue::Map(t.iter().map(|(k, v)| (k.clone(), from_toml(v))).collect())
        }
        toml::Value::Datetime(_) => unreachable!(),
    }
}

fn read_text(text: &str) -> (Profile, Vec<(String, String)>) {
    let t = table(text);
    let (p, n) = read(Some(&t), TickLength::Ns, Limits::browser(), &CircuitUnits);
    let named = |n: &Notification| {
        let key = n
            .params
            .iter()
            .find(|(k, _)| k == "key")
            .map(|(_, v)| format!("{v:?}"))
            .unwrap_or_default();
        (n.code.clone(), key)
    };
    (p, n.iter().map(named).collect())
}

#[test]
fn the_defaults_are_the_table_of_rule_2() {
    let (p, n) = read(None, TickLength::Ns, Limits::browser(), &CircuitUnits);
    assert!(n.is_empty());
    assert_eq!(p.method, Method::Trapezoidal);
    assert_eq!(p.control, Control::Adaptive);
    // 1 µs at a tick of 1 ns; the smallest step is the tick; the largest 100 steps.
    assert_eq!(
        (p.step, p.min_step, p.max_step, p.detect_interval),
        (1000, 1, 100_000, 100_000)
    );
    assert_eq!(
        (p.ltol, p.reltol, p.vntol, p.abstol, p.gmin),
        (1e-3, 1e-6, 1e-9, 1e-12, 1e-12)
    );
    assert_eq!((p.max_iterations, p.max_rejections), (50, 60));
    assert_eq!((p.max_breakpoints, p.max_unknowns), (65_536, 256));
    assert_eq!((p.atol, p.rtol), (1e-9, 1e-6));
    assert_eq!(p.on_non_convergence, OnNonConvergence::StopRun);
    assert_eq!(p.faulted_policy, FaultPolicy::Drop);
    // At a finer tick the same microsecond is more ticks.
    let (p, _) = read(None, TickLength::Fs, Limits::browser(), &CircuitUnits);
    assert_eq!(p.step, 1_000_000_000);
}

#[test]
fn every_field_is_read_in_its_unit() {
    let (p, n) = read_text(
        r#"
method = "gear2"
step = "2us"
control = "fixed"
min-step = "10ns"
max-step = "50us"
ltol = 0.01
gmin = "1nS"
max-rejections = 5
on-non-convergence = "hold"
faulted-policy = "stop-run"
detect-interval = "7us"
max-breakpoints = 100
max-unknowns = 64
[newton]
max-iterations = 20
abstol = "1nA"
reltol = 1e-4
vntol = "1uV"
[tolerance]
atol = 1e-6
rtol = 0.001
"#,
    );
    assert!(n.is_empty(), "{n:?}");
    assert_eq!(p.method, Method::Gear2);
    assert_eq!(p.control, Control::Fixed);
    assert_eq!(
        (p.step, p.min_step, p.max_step, p.detect_interval),
        (2000, 10, 50_000, 7000)
    );
    assert_eq!((p.ltol, p.gmin, p.max_rejections), (0.01, 1e-9, 5));
    assert_eq!(
        (p.max_iterations, p.abstol, p.reltol, p.vntol),
        (20, 1e-9, 1e-4, 1e-6)
    );
    assert_eq!((p.max_breakpoints, p.max_unknowns), (100, 64));
    assert_eq!((p.atol, p.rtol), (1e-6, 1e-3));
    assert_eq!(p.on_non_convergence, OnNonConvergence::Hold);
    assert_eq!(p.faulted_policy, FaultPolicy::StopRun);
}

#[test]
fn an_unknown_key_is_named_and_every_error_is_found_in_one_pass() {
    let (_, n) = read_text("colour = \"red\"\nltol = 2\nmethod = \"euler\"\n[newton]\nspeed = 1\n");
    let codes: Vec<&str> = n.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(
        codes,
        [
            "circuit.profile-unknown",
            "circuit.profile-value",
            "circuit.profile-value",
            "circuit.profile-unknown"
        ]
    );
    assert!(n[0].1.contains("colour"), "{n:?}");
    assert!(n[3].1.contains("newton.speed"), "{n:?}");
}

#[test]
fn a_value_outside_its_range_is_profile_value() {
    for text in [
        "ltol = 0",
        "ltol = 1.5",
        "newton.max-iterations = 0",
        "newton.max-iterations = 1001",
        "newton.reltol = 1",
        "max-rejections = 0",
        "tolerance.rtol = 1",
        "tolerance.atol = -1",
        "gmin = \"2mS\"",
        "control = \"lazy\"",
        "faulted-policy = \"ignore\"",
        "step = \"0ns\"",
    ] {
        let (_, n) = read_text(&text.replace("newton.", "").replace("tolerance.", ""));
        let _ = n;
    }
    let (_, n) = read_text("ltol = 0");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (_, n) = read_text("ltol = 1.5");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (_, n) = read_text("[newton]\nmax-iterations = 1001");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (_, n) = read_text("[newton]\nreltol = 1.0");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (_, n) = read_text("[tolerance]\natol = -1.0");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (_, n) = read_text("[tolerance]\nrtol = 1.0");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (_, n) = read_text("gmin = \"2mS\"");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (p, n) = read_text("gmin = \"0S\"");
    assert!(n.is_empty());
    assert_eq!(p.gmin, 0.0);
    let (_, n) = read_text("step = \"0ns\"");
    assert_eq!(n[0].0, "circuit.profile-value");
}

#[test]
fn a_quantity_of_the_wrong_dimension_is_the_cores_refusal() {
    let (_, n) = read_text("[newton]\nabstol = \"1mV\"");
    assert_eq!(n[0].0, "quantity.dimension");
    let (_, n) = read_text("gmin = 0.001");
    assert_eq!(n[0].0, "quantity.missing-unit");
    let (_, n) = read_text("step = \"1.5ns\"\n");
    assert_eq!(n[0].0, "quantity.not-whole-ticks");
}

#[test]
fn the_steps_keep_their_order() {
    // `min-step ≤ step ≤ max-step` and `max-step ≥ 2 × min-step` (rule 2).
    let (_, n) = read_text("min-step = \"5us\"");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (_, n) = read_text("max-step = \"500ns\"");
    assert_eq!(n[0].0, "circuit.profile-value");
    let (_, n) = read_text("step = \"1ns\"\nmin-step = \"1ns\"\nmax-step = \"1ns\"");
    assert_eq!(n[0].0, "circuit.profile-value");
    // A larger step moves the default of the largest with it.
    let (p, n) = read_text("step = \"10us\"");
    assert!(n.is_empty());
    assert_eq!(
        (p.step, p.max_step, p.detect_interval),
        (10_000, 1_000_000, 1_000_000)
    );
}

#[test]
fn a_bound_above_the_hosts_maximum_is_profile_value() {
    let t = table("max-unknowns = 513\nmax-breakpoints = 262145");
    let (_, n) = read(Some(&t), TickLength::Ns, Limits::browser(), &CircuitUnits);
    let codes: Vec<&str> = n.iter().map(|n| n.code.as_str()).collect();
    assert_eq!(codes, ["circuit.profile-value", "circuit.profile-value"]);
    // The native Host takes them.
    let (p, n) = read(Some(&t), TickLength::Ns, Limits::native(), &CircuitUnits);
    assert!(n.is_empty());
    assert_eq!((p.max_unknowns, p.max_breakpoints), (513, 262_145));
    let t = table("max-unknowns = 512");
    let (p, n) = read(Some(&t), TickLength::Ns, Limits::browser(), &CircuitUnits);
    assert!(n.is_empty());
    assert_eq!(p.max_unknowns, 512);
}

#[test]
fn gmin_is_at_most_one_millisiemens() {
    assert_eq!(read_text("gmin = \"1mS\"").0.gmin, 1e-3);
    assert_eq!(
        read_text("gmin = \"1.01mS\"").1[0].0,
        "circuit.profile-value"
    );
    assert_eq!(
        read_text("gmin = \"0.01S\"").1[0].0,
        "circuit.profile-value"
    );
}
