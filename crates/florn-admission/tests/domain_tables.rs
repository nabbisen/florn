//! RFC 008 rule 16: a module's table for a domain is carried opaquely to the domain, nested tables
//! and lists included (RFC 012 rule 18 writes a controller's `control-units` as a table in its
//! `[circuit]` table).
use florn_admission::manifest::parse_manifest;
use florn_model::manifest::DomainValue;

#[test]
fn a_domains_table_keeps_its_nested_tables_and_lists() {
    let m = parse_manifest(
        r#"
florn-module = 1
world = "florn:circuit/controller@0.1.0"
kind = "pid"

[circuit]
kind = "controller"
control-units = { input = "rad/s", error = "rad/s" }
names = ["a", "b"]
"#,
    )
    .unwrap();
    let t = &m.domain_tables["circuit"];
    assert_eq!(t["kind"], DomainValue::Text("controller".into()));
    let DomainValue::Map(units) = &t["control-units"] else {
        panic!("a table: {:?}", t["control-units"])
    };
    assert_eq!(units["input"], DomainValue::Text("rad/s".into()));
    assert_eq!(units["error"], DomainValue::Text("rad/s".into()));
    assert_eq!(
        t["names"],
        DomainValue::List(vec![
            DomainValue::Text("a".into()),
            DomainValue::Text("b".into())
        ])
    );
}
