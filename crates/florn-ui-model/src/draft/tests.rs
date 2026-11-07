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
