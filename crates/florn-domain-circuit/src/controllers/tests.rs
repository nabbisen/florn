//! RFC 012 rules 10 and 18: a controller's period and phase from its module's `[circuit]` table and
//! the actor's config, and the world the Host binds.

use florn_admission::manifest::{ModuleManifest, parse_manifest};

use super::*;

const MANIFEST: &str = r#"
florn-module = 1
world = "florn:circuit/controller@0.1.0"
kind = "pid"

[[params]]
name = "period"
type = "ticks"
required = true

[[params]]
name = "phase"
type = "ticks"
required = false
default = "5ns"

[circuit]
kind = "controller"
period = "period"
phase = "phase"
"#;

fn manifest(text: &str) -> ModuleManifest {
    parse_manifest(text).expect("the manifest parses")
}

fn configs(entries: &[(&str, &str)]) -> BTreeMap<Identity, (String, DomainMap)> {
    let map: DomainMap = entries
        .iter()
        .map(|(k, v)| (k.to_string(), DomainValue::Text(v.to_string())))
        .collect();
    [(5, ("pid".to_string(), map))].into_iter().collect()
}

fn run(
    text: &str,
    entries: &[(&str, &str)],
) -> Result<Vec<Sampler>, Box<florn_control::Notification>> {
    let m = manifest(text);
    samplers_of(&[(5, "pid", &m)], &[5], &configs(entries), TickLength::Ns)
}

#[test]
fn the_period_and_the_phase_are_the_actors_config_or_the_modules_default() {
    let s = run(MANIFEST, &[("period", "1ms")]).unwrap();
    assert_eq!(
        s,
        [Sampler {
            actor: 5,
            period: 1_000_000,
            next_due: 5,
            index: 0
        }]
    );
    let s = run(MANIFEST, &[("period", "2us"), ("phase", "1ns")]).unwrap();
    assert_eq!((s[0].period, s[0].next_due), (2_000, 1));
}

#[test]
fn a_controller_without_a_period_or_with_one_of_zero_is_refused_naming_the_actor() {
    // The table names no period; or names a parameter the module lacks.
    let no_table = MANIFEST.replace("period = \"period\"\n", "");
    let e = run(&no_table, &[("period", "1ms")]).unwrap_err();
    assert_eq!(e.code, "circuit.no-period");
    let missing = MANIFEST.replace("period = \"period\"\n", "period = \"every\"\n");
    assert_eq!(run(&missing, &[]).unwrap_err().code, "circuit.no-period");
    let e = run(MANIFEST, &[("period", "0ns")]).unwrap_err();
    assert_eq!(e.code, "circuit.period-zero");
    assert_eq!(e.params[0].1, Value::Name("pid".into()));
    // A period that is no whole number of ticks is the core's refusal.
    assert_eq!(
        run(MANIFEST, &[("period", "1.5ns")]).unwrap_err().code,
        "quantity.not-whole-ticks"
    );
}

#[test]
fn only_the_controllers_are_sampled() {
    let m = manifest(MANIFEST);
    let s = samplers_of(
        &[(5, "pid", &m)],
        &[],
        &configs(&[("period", "1ms")]),
        TickLength::Ns,
    );
    assert!(s.unwrap().is_empty());
}

#[test]
fn the_world_has_the_two_imports_and_the_one_export_of_rule_18() {
    let w = world();
    assert_eq!(w.name, "florn:circuit/controller@0.1.0");
    let names: Vec<&str> = w.imports[0].1.iter().map(|s| s.name).collect();
    assert_eq!(names, ["read-input", "set-output"]);
    assert_eq!(w.exports[0].1[0].name, "on-sample");
    assert_eq!(ON_SAMPLE, format!("{}#on-sample", w.exports[0].0));
    // `set-output` is recorded as a send of the value's eight bytes on the port.
    assert_eq!(
        (w.records[0].function, w.records[0].port, w.records[0].value),
        ("set-output", 0, 1)
    );
    assert!(
        w.compounds.is_empty(),
        "a controller has no compound activation"
    );
}
