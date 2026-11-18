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

/// RFC 008 rule 15: the network contract's file equals the RFC's block.
#[test]
fn network_wit_equals_rfc_008() {
    let rfc = std::fs::read_to_string(root().join("rfcs/done/008-network-domain.md"))
        .or_else(|_| std::fs::read_to_string(root().join("rfcs/done/008-network-domain.md")))
        .unwrap();
    let file =
        std::fs::read_to_string(root().join("contracts/florn-network/wit/network.wit")).unwrap();
    assert_eq!(
        first_wit_block(&rfc),
        file,
        "contracts/florn-network/wit/network.wit differs from RFC 008 §6"
    );
}

/// The network package resolves over the core package, with the world `router`.
#[test]
fn network_wit_parses_as_package_florn_network() {
    let mut resolve = wit_parser::Resolve::default();
    resolve
        .push_dir(root().join("contracts/florn-core/wit"))
        .expect("the core WIT parses");
    let (pkg, _) = resolve
        .push_dir(root().join("contracts/florn-network/wit"))
        .expect("the network WIT parses");
    let name = &resolve.packages[pkg].name;
    assert_eq!(name.namespace, "florn");
    assert_eq!(name.name, "network");
    assert!(resolve.select_world(&[pkg], Some("router")).is_ok());
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

// ---------------------------------------------------------------------------------------------
// A contract of record in several blocks (RFC 002 rule 19; RFC 012 rules 18 to 22).
//
// RFC 012 gives `florn:circuit@0.1.0` in two ```wit blocks (the controller's interfaces and world,
// then the device's, with prose between them) and `florn:cps@0.1.0` in a third. The file of a
// contract is the blocks that belong to it, in the order they appear, one blank line between two
// of them: a block ends with its newline, so the join is one more.
// ---------------------------------------------------------------------------------------------

/// Every ```wit block of a Markdown text, in order, each with its final newline.
fn wit_blocks(markdown: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = markdown;
    while let Some(at) = rest.find("```wit\n") {
        let body = &rest[at + "```wit\n".len()..];
        let end = body.find("```").expect("end of the block");
        blocks.push(&body[..end]);
        rest = &body[end + "```".len()..];
    }
    blocks
}

/// What a file must be to equal the blocks `which` (0-based, in the order they appear) of a text.
fn record_of(markdown: &str, which: &[usize]) -> String {
    let blocks = wit_blocks(markdown);
    which
        .iter()
        .map(|i| *blocks.get(*i).unwrap_or_else(|| panic!("no block {i}")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `Ok` when `file` is the record of the blocks `which`, else which block differs and how.
fn same_as_record(markdown: &str, which: &[usize], file: &str) -> Result<(), String> {
    let record = record_of(markdown, which);
    if record == file {
        return Ok(());
    }
    // Name the first block whose text is not at its place in the file.
    let mut at = 0;
    for i in which {
        let block = wit_blocks(markdown)[*i];
        let there = file.get(at..at + block.len());
        if there != Some(block) {
            return Err(format!(
                "block {i} of the text is not in the file at byte {at}: the file has {:?}",
                there.unwrap_or("<the end of the file>")
            ));
        }
        at += block.len() + 1;
    }
    Err("the file has more than the blocks of the text".into())
}

const TWO_BLOCKS: &str = "# A contract\n\nThe first part.\n\n```wit\npackage florn:x@0.1.0;\n\ninterface one {\n    f: func();\n}\n```\n\nThe second part, after some prose.\n\n```wit\ninterface two {\n    g: func(a: u32) -> u32;\n}\n```\n\nAnd another contract, which is not this one's:\n\n```wit\npackage florn:y@0.1.0;\n```\n";

#[test]
fn the_blocks_of_a_text_are_found_in_order() {
    let blocks = wit_blocks(TWO_BLOCKS);
    assert_eq!(blocks.len(), 3);
    assert!(blocks[0].starts_with("package florn:x@0.1.0;"));
    assert!(blocks[1].starts_with("interface two {"));
    assert!(blocks[2].starts_with("package florn:y@0.1.0;"));
    assert!(blocks.iter().all(|b| b.ends_with('\n')));
}

#[test]
fn a_file_of_two_blocks_equals_them_joined_by_one_blank_line() {
    let file = "package florn:x@0.1.0;\n\ninterface one {\n    f: func();\n}\n\ninterface two {\n    g: func(a: u32) -> u32;\n}\n";
    assert_eq!(same_as_record(TWO_BLOCKS, &[0, 1], file), Ok(()));
    // One block alone is the single-block case.
    assert_eq!(
        same_as_record(TWO_BLOCKS, &[2], "package florn:y@0.1.0;\n"),
        Ok(())
    );
}

#[test]
fn it_fails_when_either_block_is_edited_in_the_text_or_in_the_file() {
    let file = "package florn:x@0.1.0;\n\ninterface one {\n    f: func();\n}\n\ninterface two {\n    g: func(a: u32) -> u32;\n}\n";
    // The second block of the text edited: the file no longer equals it, and it is block 1 that
    // is named.
    let edited_text = TWO_BLOCKS.replace("g: func(a: u32) -> u32;", "g: func(a: u64) -> u32;");
    let e = same_as_record(&edited_text, &[0, 1], file).unwrap_err();
    assert!(e.starts_with("block 1 "), "{e}");
    // The first block edited in the text: block 0.
    let edited_text = TWO_BLOCKS.replace("f: func();", "f: func(x: u8);");
    let e = same_as_record(&edited_text, &[0, 1], file).unwrap_err();
    assert!(e.starts_with("block 0 "), "{e}");
    // The file edited, in either direction.
    let e = same_as_record(TWO_BLOCKS, &[0, 1], &file.replace("func(a", "func(b")).unwrap_err();
    assert!(e.starts_with("block 1 "), "{e}");
    let e = same_as_record(TWO_BLOCKS, &[0, 1], &format!("{file}\n// more\n")).unwrap_err();
    assert!(e.contains("more than"), "{e}");
    let e = same_as_record(
        TWO_BLOCKS,
        &[0, 1],
        &file.replace("\n\ninterface two", "\ninterface two"),
    )
    .unwrap_err();
    assert!(e.starts_with("block 1 "), "a missing blank line: {e}");
    // A file with only the first block.
    let only_first = "package florn:x@0.1.0;\n\ninterface one {\n    f: func();\n}\n";
    assert!(same_as_record(TWO_BLOCKS, &[0, 1], only_first).is_err());
}

#[test]
fn the_blocks_of_the_text_in_the_repository_are_found_by_the_same_reading() {
    // The reading is the one the single-block tests use: the first block of RFC 002 is the core
    // contract, whatever else the text holds.
    let rfc = std::fs::read_to_string(root().join("rfcs/done/002-guest-contract.md")).unwrap();
    let file = std::fs::read_to_string(root().join("contracts/florn-core/wit/core.wit")).unwrap();
    assert_eq!(same_as_record(&rfc, &[0], &file), Ok(()));
    assert_eq!(wit_blocks(&rfc)[0], first_wit_block(&rfc));
}
