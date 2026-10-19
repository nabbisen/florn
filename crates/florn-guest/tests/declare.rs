//! The declaration (RFC 002 §3.1): the manifest text and the config decoder come from one
//! declaration; the decoder reads what the Host encodes (rule 13); channel records encode as
//! rule 16 says. Runs natively: the bindings' exports are plain functions here.

mod guest {
    florn_guest::declare! {
        world: "florn:core/guest@0.1.0",
        kind: "router",
        description: "OSPF single-area router",
        ports: [("eth", "network/ethernet", 1, 8)],
        params: [
            (router_id, "router-id", text, true),
            (hello, "hello", ticks, false, default = "10s"),
            (cost, "cost", u64, false, default = 10),
            (price, "price", integer, false, default = "12.34USD", unit = "0.01USD"),
            (flag, "flag", bool, false, default = false),
            (blob, "blob", bytes, false, default = "00ff"),
            (gain, "gain", real, false, default = "1.5ohm", unit = "ohm"),
            (offset, "offset", s64, false, default = -3),
            (ratio, "ratio", f64, false, default = 0.25),
        ],
        telemetry: [
            (1, "neighbour-state", NeighbourState, [(neighbour, "neighbour", u64), (state, "state", text)]),
            (2, "flags", Flags, [(up, "up", bool)]),
        ],
        resources: [("memory", "4MiB"), ("pending-timers", 16)],
    }
}

#[test]
fn the_manifest_is_the_declaration() {
    let m = guest::MANIFEST;
    assert!(m.starts_with("florn-module = 1\nworld = \"florn:core/guest@0.1.0\"\nkind = \"router\"\ndescription = \"OSPF single-area router\"\nsdk = \"florn-guest "), "{m}");
    assert!(
        m.contains("\n[[ports]]\nkind = \"eth\"\ntype = \"network/ethernet\"\nmin = 1\nmax = 8\n"),
        "{m}"
    );
    assert!(
        m.contains(
            "\n[[params]]\nname = \"router-id\"\ntype = \"text\"\nrequired = true\n\n[[params]]"
        ),
        "{m}"
    );
    assert!(m.contains("\n[[params]]\nname = \"hello\"\ntype = \"ticks\"\nrequired = false\ndefault = \"10s\"\n"), "{m}");
    assert!(m.contains("\n[[params]]\nname = \"price\"\ntype = \"integer\"\nrequired = false\ndefault = \"12.34USD\"\nunit = \"0.01USD\"\n"), "{m}");
    assert!(
        m.contains(
            "\n[[params]]\nname = \"offset\"\ntype = \"s64\"\nrequired = false\ndefault = -3\n"
        ),
        "{m}"
    );
    assert!(m.contains("\n[[telemetry]]\nchannel = 1\nname = \"neighbour-state\"\nfields = [{ name = \"neighbour\", type = \"u64\" }, { name = \"state\", type = \"text\" }, ]\n"), "{m}");
    // Resource keys are quoted TOML keys, which read the same as bare ones.
    assert!(
        m.ends_with("\n[resources]\n\"memory\" = \"4MiB\"\n\"pending-timers\" = 16\n"),
        "{m}"
    );
    // The section carries the same bytes.
    assert_eq!(&guest::FLORN_MANIFEST[..], m.as_bytes());
}

#[test]
fn the_decoder_reads_the_record_of_rule_13() {
    // text, ticks, u64, integer, bool, bytes, real, s64, f64 in declaration order.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(b"10.0.0.1");
    bytes.extend_from_slice(&10_000_000_000u64.to_le_bytes());
    bytes.extend_from_slice(&10u64.to_le_bytes());
    bytes.extend_from_slice(&1234i64.to_le_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&[0x00, 0xff]);
    bytes.extend_from_slice(&1.5f64.to_le_bytes());
    bytes.extend_from_slice(&(-3i64).to_le_bytes());
    bytes.extend_from_slice(&0.25f64.to_le_bytes());
    let c = guest::Config::decode(&bytes);
    assert_eq!(
        c,
        guest::Config {
            router_id: "10.0.0.1".into(),
            hello: 10_000_000_000,
            cost: 10,
            price: 1234,
            flag: true,
            blob: vec![0x00, 0xff],
            gain: 1.5,
            offset: -3,
            ratio: 0.25,
        }
    );
}

#[test]
#[should_panic(expected = "the guest trapped")]
fn a_short_record_traps() {
    let _ = guest::Config::decode(&[1, 0, 0, 0]);
}

#[test]
fn channel_records_encode_in_declared_order() {
    let r = guest::NeighbourState {
        neighbour: 7,
        state: "full".into(),
    };
    assert_eq!(guest::NeighbourState::CHANNEL, 1);
    let mut expect = 7u64.to_le_bytes().to_vec();
    expect.extend_from_slice(&4u32.to_le_bytes());
    expect.extend_from_slice(b"full");
    assert_eq!(r.encode(), expect);
    assert_eq!(guest::Flags { up: true }.encode(), vec![1]);
    assert_eq!(guest::Flags::CHANNEL, 2);
}
