//! The SDK's copy of the contract equals the contract of record (RFC 002 rule 19).
#[test]
fn the_sdk_copy_of_the_contract_is_the_contract_of_record() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let copy = std::fs::read_to_string(root.join("wit/core.wit")).unwrap();
    let record =
        std::fs::read_to_string(root.join("../../contracts/florn-core/wit/core.wit")).unwrap();
    assert_eq!(
        copy, record,
        "copy contracts/florn-core/wit/core.wit to crates/florn-guest/wit/core.wit"
    );
}
