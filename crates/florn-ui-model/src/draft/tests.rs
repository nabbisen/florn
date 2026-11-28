use super::*;
use crate::fields::Problem;

#[test]
fn a_draft_writes_its_manifest() {
    let mut d = Draft::new("lab");
    d.end.type_text("3000ns");
    d.end.commit();
    d.add_module(DraftModule {
        name: "distance_vector".into(),
        sha256: "ab".repeat(32),
        contract: "florn:network/router@0.1.0".into(),
        summary: None,
    });
    let a = d.place("distance_vector", "network/interface", 2).unwrap();
    let b = d.place("distance_vector", "network/interface", 2).unwrap();
    assert_eq!((a, b), (1, 2));
    assert!(d.place("nothing", "network/interface", 1).is_none());
    let c = d.connect("network/link", &[(a, 0), (b, 0)]);
    assert_eq!(c, FIRST_CONNECTION_ID);
    d.set_attr(c, "delay", "2ns");
    d.set_attr(c, "loss", "0.1");
    d.set_config(a, "infinity", "16");
    d.schedule("1000ns", c, &[("enabled", "false")]);
    assert_eq!(d.domains(), vec!["core", "network"]);
    assert_eq!(d.problems(), 0);
    let t = d.to_toml();
    assert!(t.contains("domains = [\"core\", \"network\"]"));
    assert!(t.contains("[modules.distance_vector]\nsha256 = \"abab"));
    assert!(t.contains("ports = [{ id = 0, name = \"p0\", type = \"network/interface\" }, { id = 1, name = \"p1\", type = \"network/interface\" }]"));
    assert!(t.contains("[actors.config]\ninfinity = 16\n"));
    assert!(t.contains("type = \"network/link\"\nports = [\"a1.p0\", \"a2.p0\"]\n[connections.attrs]\ndelay = \"2ns\"\nloss = 0.1\n"));
    assert!(t.contains(
        "[[changes]]\nat = \"1000ns\"\ntarget = \"c0\"\n[changes.payload]\nenabled = false\n"
    ));
    // A bare number where a quantity goes is a problem the draft tells itself.
    d.set_attr(c, "delay", "5");
    assert_eq!(d.problems(), 1);
    assert!(d.to_toml().contains("delay = \"2ns\""), "not committed");
}

#[test]
fn placed_actors_leave_optional_parameters_to_the_module() {
    use florn_control::message::{ModuleSummary, ParamSummary};
    let param = |name: &str, required: bool| ParamSummary {
        name: name.into(),
        ty: "u64".into(),
        required,
        default: (!required).then(|| "7".to_string()),
        unit: None,
    };
    let summary = ModuleSummary {
        name: "m".into(),
        source_sha256: String::new(),
        contract: String::new(),
        kind: "k".into(),
        description: None,
        port_kinds: Vec::new(),
        params: vec![param("size", true), param("period", false)],
        channels: Vec::new(),
        needs: Vec::new(),
        tables: Vec::new(),
    };
    let mut d = Draft::new("x");
    d.add_module(DraftModule {
        name: "m".into(),
        sha256: String::new(),
        contract: String::new(),
        summary: Some(summary),
    });
    let a = d.place("m", "network/interface", 1).unwrap();
    let actor = &d.actors[0];
    assert_eq!(
        actor.config[0].problem,
        Some(Problem::Empty),
        "required, to be filled"
    );
    assert!(actor.config[1].optional && actor.config[1].is_valid());
    assert!(
        !d.to_toml().contains("period"),
        "left to the module's default"
    );
    d.set_config(a, "size", "3");
    assert!(d.to_toml().contains("[actors.config]\nsize = 3\n"));
}

#[test]
fn the_hosts_refusals_attach_to_their_fields() {
    use florn_control::{Ends, Notification, Scope, Severity};
    let mut d = Draft::new("lab");
    d.add_module(DraftModule {
        name: "m".into(),
        sha256: String::new(),
        contract: String::new(),
        summary: None,
    });
    let a = d.place("m", "network/interface", 1).unwrap();
    let c = d.connect("network/link", &[(a, 0)]);
    d.set_attr(c, "delay", "2ns");
    let at = |field: &str| {
        let s = Scope {
            field: Some(field.into()),
            ..Scope::default()
        };
        Notification::new("network.attr-type", Severity::Error, s, Ends::Command)
    };
    let n = d.apply_errors(&[
        at("connections[0].attrs.delay"),
        at("time.end"),
        at("connections[0].name"),
        at("nowhere"),
    ]);
    assert_eq!(n, 3);
    assert_eq!(d.connections[0].attrs[0].errors.len(), 1);
    assert_eq!(d.end.errors.len(), 1);
    assert_eq!(d.connections[0].name.errors.len(), 1);
    assert_eq!(d.errors.len(), 4);
    assert_eq!(
        d.problems(),
        3,
        "the three refused fields, the empty end among them"
    );
}

fn builtin_scenario() -> ScenarioSummary {
    use florn_control::message::{ActorSummary, ChangeSummary, ConnectionSummary, PortSummary};
    let port = |id: u32, name: &str, ty: &str| PortSummary {
        id,
        name: name.into(),
        ty: ty.into(),
    };
    ScenarioSummary {
        name: "lab".into(),
        sha256: String::new(),
        tick: "1ns".into(),
        end: 5_000,
        tie: "by-identity".into(),
        seed: 1,
        actors: vec![
            ActorSummary {
                id: 1,
                name: "vs".into(),
                module: None,
                class: "default".into(),
                ports: vec![port(0, "p", "circuit/pin"), port(1, "n", "circuit/pin")],
                config: vec![("v".into(), "5V".into())],
                builtin: Some("circuit/voltage-source".into()),
            },
            ActorSummary {
                id: 2,
                name: "r1".into(),
                module: None,
                class: "default".into(),
                ports: vec![port(0, "a", "circuit/pin"), port(1, "b", "circuit/pin")],
                config: vec![("r".into(), "1kohm".into())],
                builtin: Some("circuit/resistor".into()),
            },
        ],
        connections: vec![ConnectionSummary {
            id: 100,
            name: "bus".into(),
            ports: vec![(1, 0), (2, 0)],
            ty: Some("circuit/net".into()),
            attrs: Vec::new(),
            per_port: Vec::new(),
        }],
        modules: Vec::new(),
        missing_modules: Vec::new(),
        domains: vec!["core".into(), "circuit".into()],
        changes: vec![ChangeSummary {
            at: 1_000,
            target: WORLD_IDENTITY_BASE + 1,
            payload: vec![("actor".into(), "1".into()), ("value".into(), "3V".into())],
        }],
        classes: Vec::new(),
        limits: Vec::new(),
    }
}

#[test]
fn a_draft_keeps_the_built_ins_of_its_scenario_and_writes_them_back() {
    let d = Draft::from_scenario(&builtin_scenario());
    assert!(d.dropped.is_empty() && d.can_apply());
    assert_eq!(d.actors.len(), 2);
    assert_eq!(
        d.actors[0].builtin.as_deref(),
        Some("circuit/voltage-source")
    );
    assert_eq!(d.actors[0].module, "");
    assert_eq!(d.actors[0].kind_name(), "circuit/voltage-source");
    let t = d.to_toml();
    assert!(t.contains("domains = [\"core\", \"circuit\"]"), "{t}");
    assert!(!t.contains("[modules."), "a built-in has no module:\n{t}");
    assert!(
        t.contains("name = \"vs\"\nkind = \"builtin\"\nbuiltin = \"circuit/voltage-source\"\nports = [{ id = 0, name = \"p\", type = \"circuit/pin\" }, { id = 1, name = \"n\", type = \"circuit/pin\" }]\n[actors.config]\nv = \"5V\"\n"),
        "{t}"
    );
    assert!(t.contains("[actors.config]\nr = \"1kohm\"\n"), "{t}");
    assert!(t.contains("ports = [\"vs.p\", \"r1.a\"]"), "{t}");
}

#[test]
fn a_scheduled_change_of_a_domains_world_is_written_to_that_world() {
    let d = Draft::from_scenario(&builtin_scenario());
    let t = d.to_toml();
    assert!(
        t.contains("[[changes]]\nat = \"1000ns\"\ntarget = \"world:circuit\"\n[changes.payload]\nactor = 1\nvalue = \"3V\"\n"),
        "{t}"
    );
}

#[test]
fn a_host_that_does_not_carry_a_built_ins_name_still_costs_the_draft_the_actor() {
    // Before 0.4 the summary had no name for a built-in: the draft says which actor it could not
    // take and is neither applied nor saved (PR-33), as it was.
    let mut s = builtin_scenario();
    s.actors[1].builtin = None;
    let d = Draft::from_scenario(&s);
    assert_eq!(d.dropped, vec![(2, "r1".to_string())]);
    assert!(!d.can_apply());
}

#[test]
fn a_built_in_and_a_module_actor_side_by_side_are_both_written() {
    let mut s = builtin_scenario();
    s.modules.push(florn_control::message::ModuleSummary {
        name: "pid".into(),
        source_sha256: "cd".repeat(32),
        contract: "florn:circuit/controller@0.1.0".into(),
        kind: "controller".into(),
        description: None,
        port_kinds: Vec::new(),
        params: Vec::new(),
        channels: Vec::new(),
        needs: Vec::new(),
        tables: Vec::new(),
    });
    s.actors.push(florn_control::message::ActorSummary {
        id: 3,
        name: "ctl".into(),
        module: Some("pid".into()),
        class: "default".into(),
        ports: Vec::new(),
        config: Vec::new(),
        builtin: None,
    });
    let d = Draft::from_scenario(&s);
    assert_eq!(d.actors[2].kind_name(), "pid");
    let t = d.to_toml();
    assert!(t.contains("[modules.pid]\nsha256 = \"cdcd"), "{t}");
    assert!(
        t.contains("name = \"ctl\"\nkind = \"module\"\nmodule = \"pid\"\n"),
        "{t}"
    );
}

#[test]
fn an_actor_with_a_module_is_the_modules_whatever_else_the_summary_says() {
    let mut s = builtin_scenario();
    s.modules.push(florn_control::message::ModuleSummary {
        name: "pid".into(),
        source_sha256: "cd".repeat(32),
        contract: String::new(),
        kind: "controller".into(),
        description: None,
        port_kinds: Vec::new(),
        params: Vec::new(),
        channels: Vec::new(),
        needs: Vec::new(),
        tables: Vec::new(),
    });
    s.actors[0].module = Some("pid".into());
    let d = Draft::from_scenario(&s);
    assert_eq!(d.actors[0].builtin, None);
    assert_eq!(d.actors[0].kind_name(), "pid");
}
