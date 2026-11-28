//! The SDK's copies of the four contracts equal the contracts of record (RFC 002 rule 19).
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
        (
            "wit/circuit/circuit.wit",
            "../../contracts/florn-circuit/wit/circuit.wit",
        ),
        ("wit/cps/cps.wit", "../../contracts/florn-cps/wit/cps.wit"),
    ] {
        assert_eq!(
            std::fs::read_to_string(root.join(copy)).unwrap(),
            std::fs::read_to_string(root.join(record)).unwrap(),
            "copy {record} to crates/florn-guest-cps/{copy}"
        );
    }
}
