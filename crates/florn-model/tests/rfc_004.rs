//! The verification table of RFC 004 §9, and the worked example of §8.
use std::collections::BTreeMap;

use florn_model::*;

const EXAMPLE: &str = r#"
florn = 1
name = "ospf-triangle"
domains = ["network"]

[time]
tick = "1ns"
end = "60s"

[order]
tie = "by-identity"

[seed]
root = 42

[modules.ospf]
sha256 = "9f2c4e0000000000000000000000000000000000000000000000000000000000"
contract = "florn:network/router@0.1.0"

[[actors]]
id = 1
name = "r1"
kind = "module"
module = "ospf"
class = "router"
ports = [{ id = 0, name = "eth0", type = "network/ethernet" },
         { id = 1, name = "eth1", type = "network/ethernet" }]
[actors.config]
router-id = "10.0.0.1"

[[actors]]
id = 2
name = "r2"
kind = "module"
module = "ospf"
class = "router"
ports = [{ id = 0, name = "eth0", type = "network/ethernet" },
         { id = 1, name = "eth1", type = "network/ethernet" }]
[actors.config]
router-id = "10.0.0.2"

[[actors]]
id = 3
name = "r3"
kind = "module"
module = "ospf"
class = "router"
ports = [{ id = 0, name = "eth0", type = "network/ethernet" },
         { id = 1, name = "eth1", type = "network/ethernet" }]
[actors.config]
router-id = "10.0.0.3"

[[connections]]
id = 100
name = "l12"
ports = ["r1.eth0", "r2.eth0"]
[connections.attrs]
delay = "5ms"
loss = 0
cost = 10

[[connections]]
id = 101
name = "l23"
ports = ["r2.eth1", "r3.eth0"]
[connections.attrs]
delay = "5ms"
loss = 0
cost = 10

[[connections]]
id = 102
name = "l31"
ports = ["r3.eth1", "r1.eth1"]
[connections.attrs]
delay = "20ms"
loss = 0.001
cost = 40

[[changes]]
at = "30s"
target = "l31"
[changes.payload]
enabled = false

[limits.router]
budget = 50000000

[views.positions]
r1 = [100, 100]
r2 = [300, 100]
r3 = [200, 260]
"#;

fn parse(text: &str) -> Result<Manifest, Errors> {
    Manifest::parse(text, &HostMaxima::browser())
}

#[test]
fn worked_example_parses() {
    let m = parse(EXAMPLE).unwrap();
    assert_eq!(m.name, "ospf-triangle");
    assert_eq!(m.time.end, 60_000_000_000);
    assert_eq!(m.actors.len(), 3);
    assert_eq!(m.connections.len(), 3);
    assert_eq!(
        m.connection(102).unwrap().ports,
        vec![PortRef { actor: 3, port: 1 }, PortRef { actor: 1, port: 1 }]
    );
    assert_eq!(m.changes[0].at, 30_000_000_000);
    assert_eq!(m.changes[0].target, ChangeTarget::Connection(102));
    assert_eq!(m.limits_for("router").budget, Some(50_000_000));
    assert_eq!(m.limits_for("default").budget, None);
    assert_eq!(m.views.len(), 1);
    assert_eq!(
        m.actor(1).unwrap().config.get("router-id"),
        Some(&DomainValue::Text("10.0.0.1".into()))
    );
    assert_eq!(
        m.connection(102).unwrap().attrs.get("loss"),
        Some(&DomainValue::Decimal("0.001".into()))
    );
    assert_eq!(m.world_identity("network"), Some(0xF000_0010));
}

#[test]
fn hash_is_independent_of_formatting() {
    let a = parse(EXAMPLE).unwrap();
    // comments, key order, whitespace, and views changed
    let b_text = EXAMPLE
        .replace(
            "[seed]\nroot = 42",
            "# a comment\n[seed]\nroot   =   42 # here",
        )
        .replace(
            "tick = \"1ns\"\nend = \"60s\"",
            "end = \"60s\"\ntick = \"1ns\"",
        )
        .replace("r1 = [100, 100]", "r1 = [1, 1]");
    let b = parse(&b_text).unwrap();
    assert_eq!(a.scenario_hash(), b.scenario_hash());
    // actors listed in another order
    let c_text = {
        let i1 = EXAMPLE.find("[[actors]]\nid = 1").unwrap();
        let i2 = EXAMPLE.find("[[actors]]\nid = 2").unwrap();
        let i3 = EXAMPLE.find("[[actors]]\nid = 3").unwrap();
        let end = EXAMPLE.find("[[connections]]").unwrap();
        format!(
            "{}{}{}{}{}",
            &EXAMPLE[..i1],
            &EXAMPLE[i3..end],
            &EXAMPLE[i1..i2],
            &EXAMPLE[i2..i3],
            &EXAMPLE[end..]
        )
    };
    let c = parse(&c_text).unwrap();
    assert_eq!(a.scenario_hash(), c.scenario_hash());
}

#[test]
fn hash_depends_on_content() {
    let a = parse(EXAMPLE).unwrap();
    let b = parse(&EXAMPLE.replace("id = 3\nname = \"r3\"", "id = 4\nname = \"r3\"")).unwrap();
    assert_ne!(a.scenario_hash(), b.scenario_hash(), "one identity");
    let c = parse(&EXAMPLE.replace("delay = \"20ms\"", "delay = \"21ms\"")).unwrap();
    assert_ne!(a.scenario_hash(), c.scenario_hash(), "one domain field");
    let d = parse(&EXAMPLE.replace("end = \"60s\"", "end = \"61s\"")).unwrap();
    assert_ne!(a.scenario_hash(), d.scenario_hash(), "one quantity");
}

#[test]
fn identities_are_stable_under_insertion() {
    let a = parse(EXAMPLE).unwrap();
    let inserted = EXAMPLE.replace(
        "[[actors]]\nid = 1\n",
        "[[actors]]\nid = 9\nname = \"r9\"\nkind = \"module\"\nmodule = \"ospf\"\nports = [{ id = 0, name = \"eth0\", type = \"network/ethernet\" }]\n\n[[actors]]\nid = 1\n",
    );
    let b = parse(&inserted).unwrap();
    for id in [1, 2, 3] {
        assert_eq!(a.actor(id).unwrap().name, b.actor(id).unwrap().name);
    }
    assert_eq!(b.actor(9).unwrap().class, "default");
}

#[test]
fn hyperedge_and_multi_membership() {
    let text = EXAMPLE.replace(
        "[[changes]]",
        "[[connections]]\nid = 104\nname = \"all\"\nports = [\"r1.eth0\", \"r2.eth0\", \"r3.eth0\"]\n[connections.attrs.per-port]\n\"r2.eth0\" = { delay = \"10us\" }\n\n[[changes]]",
    );
    let m = parse(&text).unwrap();
    let c = m.connection(104).unwrap();
    assert_eq!(c.ports.len(), 3);
    assert_eq!(
        c.per_port
            .get(&PortRef { actor: 2, port: 0 })
            .unwrap()
            .get("delay"),
        Some(&DomainValue::Text("10us".into()))
    );
    // r1.eth0 is in l12 and in all
    assert!(
        m.connection(100)
            .unwrap()
            .ports
            .contains(&PortRef { actor: 1, port: 0 })
    );
}

#[test]
fn units() {
    let e = parse(
        &EXAMPLE
            .replace(
                "delay = \"5ms\"\nloss = 0\ncost = 10\n\n[[connections]]\nid = 101",
                "delay = \"5ms\"\nloss = 0\ncost = 10\n\n[[connections]]\nid = 101",
            )
            .replace("end = \"60s\"", "end = 60"),
    )
    .unwrap_err();
    assert_eq!(e.codes(), vec!["quantity.missing-unit"]);
    let e = parse(&EXAMPLE.replace("end = \"60s\"", "end = \"1.5ns\"")).unwrap_err();
    assert_eq!(e.codes(), vec!["quantity.not-whole-ticks"]);
    match &e.0[0].kind {
        Kind::QuantityNotWholeTicks { smallest, .. } => {
            assert_eq!(smallest, &("1".to_string(), "ps".to_string()))
        }
        other => panic!("{other:?}"),
    }
    let e = parse(&EXAMPLE.replace("end = \"60s\"", "end = \"60min\"")).unwrap_err();
    assert_eq!(e.codes(), vec!["quantity.unknown-unit"]);
    let e = parse(
        &EXAMPLE
            .replace("end = \"60s\"", "end = \"600y\"")
            .replace("tick = \"1ns\"", "tick = \"1fs\""),
    )
    .unwrap_err();
    assert_eq!(e.codes(), vec!["quantity.unknown-unit"]);
    let e = parse(
        &EXAMPLE
            .replace("end = \"60s\"", "end = \"1000000000000s\"")
            .replace("tick = \"1ns\"", "tick = \"1fs\""),
    )
    .unwrap_err();
    assert_eq!(e.codes(), vec!["quantity.out-of-range"]);
}

#[test]
fn limits_above_maximum_and_memory_pages() {
    let e = parse(&EXAMPLE.replace("budget = 50000000", "budget = 5000000000")).unwrap_err();
    assert_eq!(e.codes(), vec!["limit.above-maximum"]);
    let e = parse(&EXAMPLE.replace("budget = 50000000", "memory = \"100KiB\"")).unwrap_err();
    assert_eq!(e.codes(), vec!["quantity.not-whole-unit"]);
    let m = parse(&EXAMPLE.replace("budget = 50000000", "memory = \"4MiB\"")).unwrap();
    assert_eq!(m.limits_for("router").memory_bytes, Some(4 * 1024 * 1024));
}

#[test]
fn every_error_in_one_pass() {
    let text = EXAMPLE
        .replace("florn = 1", "florn = 2")
        .replace("id = 2\nname = \"r2\"", "id = 1\nname = \"r2\"")
        .replace(
            "ports = [\"r3.eth1\", \"r1.eth1\"]",
            "ports = [\"r3.eth9\"]",
        )
        .replace("at = \"30s\"", "at = \"70s\"")
        .replace("[seed]\nroot = 42", "[seed]\nroot = 42\nsalt = 1");
    let e = parse(&text).unwrap_err();
    let mut codes = e.codes();
    codes.sort();
    assert_eq!(
        codes,
        vec![
            "connection.too-few-ports",
            "identity.duplicate",
            "manifest.schema-version",
            "manifest.unknown-key",
            "reference.unresolved",
            "time.end-before-change"
        ]
    );
    let dup =
        e.0.iter()
            .find(|x| x.code() == "identity.duplicate")
            .unwrap();
    assert_eq!(dup.field.as_deref(), Some("actors[1].id"));
    assert!(
        matches!(&dup.kind, Kind::IdentityDuplicate { identity: 1, first, second } if first == "r1" && second == "r2")
    );
}

#[test]
fn reserved_identities_and_undeclared_domains() {
    let e = parse(&EXAMPLE.replace(
        "id = 100\nname = \"l12\"",
        "id = 4026531840\nname = \"l12\"",
    ))
    .unwrap_err();
    assert_eq!(e.codes(), vec!["identity.reserved"]);
    let e = parse(&EXAMPLE.replace("type = \"network/ethernet\" },\n         { id = 1, name = \"eth1\", type = \"network/ethernet\" }]\n[actors.config]\nrouter-id = \"10.0.0.3\"", "type = \"network/ethernet\" },\n         { id = 1, name = \"eth1\", type = \"circuit/pin\" }]\n[actors.config]\nrouter-id = \"10.0.0.3\"")).unwrap_err();
    assert_eq!(e.codes(), vec!["domain.undeclared"]);
    let e = parse(&EXAMPLE.replace(
        "domains = [\"network\"]",
        "domains = [\"network\", \"circuit\"]\n\n[circuit]\nsolver = { }",
    ))
    .map(|m| m.domain_tables.len());
    assert_eq!(e.unwrap(), 1);
}

#[test]
fn schema_version_and_missing_end() {
    let e = parse(&EXAMPLE.replace("florn = 1", "florn = 2")).unwrap_err();
    assert_eq!(e.codes(), vec!["manifest.schema-version"]);
    let e = parse(&EXAMPLE.replace("end = \"60s\"\n", "")).unwrap_err();
    assert_eq!(e.codes(), vec!["manifest.missing-field"]);
    assert_eq!(e.0[0].field.as_deref(), Some("time.end"));
}

#[test]
fn huge_manifest_is_refused_in_bounded_memory() {
    // 10^7 actors would be hundreds of megabytes of text: refused by the size maximum before parsing.
    let mut text = String::from(EXAMPLE);
    let one_actor = "\n[[actors]]\nid = 7\nname = \"x\"\nkind = \"module\"\nmodule = \"ospf\"\n";
    while text.len() <= maxima::MANIFEST_BYTES {
        text.push_str(one_actor);
    }
    let e = parse(&text).unwrap_err();
    assert_eq!(e.codes(), vec!["manifest.too-large"]);
}

#[test]
fn package_round_trip_and_hash_checks() {
    let module_bytes = b"\0asm\x01\0\0\0".to_vec();
    let hash = canonical::hex(&<sha2::Sha256 as sha2::Digest>::digest(&module_bytes));
    let text = EXAMPLE.replace(
        "9f2c4e0000000000000000000000000000000000000000000000000000000000",
        &hash,
    );
    let manifest = parse(&text).unwrap();
    let mut modules = BTreeMap::new();
    modules.insert(hash.clone(), module_bytes.clone());
    let meta = PackageMeta {
        partial: false,
        created_by: Some("test".into()),
        notes: None,
    };
    let bytes = Package::write(&text, &manifest, &modules, &meta, Some("# hello")).unwrap();
    let p = Package::read(&bytes, &HostMaxima::browser()).unwrap();
    assert_eq!(p.modules.get(&hash), Some(&module_bytes));
    assert_eq!(p.meta.created_by.as_deref(), Some("test"));
    assert_eq!(p.readme.as_deref(), Some("# hello"));
    assert!(p.missing.is_empty());
    // a renamed module file: the entry's name is not its hash
    let renamed = {
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let opts = zip::write::SimpleFileOptions::default();
        use std::io::Write;
        zip.start_file("manifest.toml", opts).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
        zip.start_file("package.toml", opts).unwrap();
        zip.write_all(b"florn-package = 1\npartial = false\n")
            .unwrap();
        zip.start_file(format!("modules/{hash}.wasm"), opts)
            .unwrap();
        zip.write_all(b"\0asm\x01\0\0\0\0").unwrap(); // different bytes
        zip.finish().unwrap();
        cursor.into_inner()
    };
    let e = Package::read(&renamed, &HostMaxima::browser()).unwrap_err();
    assert_eq!(e.codes(), vec!["module.hash-mismatch"]);
    // a partial package lists what is missing
    let partial = Package::write(
        &text,
        &manifest,
        &BTreeMap::new(),
        &PackageMeta {
            partial: true,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let p = Package::read(&partial, &HostMaxima::browser()).unwrap();
    assert_eq!(p.missing, vec![hash.clone()]);
    // a package that omits a module without saying so is refused
    let e = Package::write(
        &text,
        &manifest,
        &BTreeMap::new(),
        &PackageMeta::default(),
        None,
    )
    .unwrap_err();
    assert_eq!(e.codes(), vec!["package.partial"]);
}
