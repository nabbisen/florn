//! RFC 002 rule 19: the WIT text in the RFC is the contract of record, and the files under
//! `contracts/` must be identical to it. This test fails on any difference, in either direction.
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

/// The first ```wit block of a Markdown file.
fn first_wit_block(markdown: &str) -> &str {
    let start = markdown.find("```wit\n").expect("a ```wit block") + "```wit\n".len();
    let end = markdown[start..].find("```").expect("end of the block") + start;
    &markdown[start..end]
}

#[test]
fn core_wit_equals_rfc_002() {
    let rfc = std::fs::read_to_string(root().join("rfcs/done/002-guest-contract.md")).unwrap();
    let file = std::fs::read_to_string(root().join("contracts/florn-core/wit/core.wit")).unwrap();
    assert_eq!(
        first_wit_block(&rfc),
        file,
        "contracts/florn-core/wit/core.wit differs from RFC 002 §4"
    );
}

#[test]
fn core_wit_parses_as_package_florn_core() {
    let mut resolve = wit_parser::Resolve::default();
    let (pkg, _) = resolve
        .push_dir(root().join("contracts/florn-core/wit"))
        .expect("the WIT parses");
    let name = &resolve.packages[pkg].name;
    assert_eq!(name.namespace, "florn");
    assert_eq!(name.name, "core");
    assert_eq!(
        name.version.as_ref().map(|v| v.to_string()).as_deref(),
        Some("0.1.0")
    );
    let world = resolve
        .worlds
        .iter()
        .find(|(_, w)| w.name == "guest")
        .expect("world guest");
    assert_eq!(world.1.imports.len(), 2, "types and host are imported");
    assert_eq!(world.1.exports.len(), 1, "actor is exported");
}
