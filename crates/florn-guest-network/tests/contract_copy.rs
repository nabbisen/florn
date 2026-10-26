//! The SDK's copies of both contracts equal the contracts of record (RFC 002 rule 19).
#[test]
fn the_sdk_copies_are_the_contracts_of_record() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for (copy, record) in [
        (
            "wit/core/core.wit",
            "../../contracts/florn-core/wit/core.wit",
        ),
        (
            "wit/network/network.wit",
            "../../contracts/florn-network/wit/network.wit",
        ),
    ] {
        assert_eq!(
            std::fs::read_to_string(root.join(copy)).unwrap(),
            std::fs::read_to_string(root.join(record)).unwrap(),
            "copy {record} to crates/florn-guest-network/{copy}"
        );
    }
}
