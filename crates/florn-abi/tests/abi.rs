//! RFC 002 §2: every type of the subset round-trips through memory and through flat values; the
//! three contract faults; canonical NaN; the spill of more than sixteen flat parameters; the core
//! world's signatures agree with the parsed WIT.
use florn_abi::core_world;
use florn_abi::*;

fn mem() -> VecMemory {
    VecMemory::new(65_536)
}

fn all_types() -> Vec<(Ty, Val)> {
    vec![
        (Ty::Bool, Val::Bool(true)),
        (Ty::U8, Val::U8(200)),
        (Ty::U16, Val::U16(60_000)),
        (Ty::U32, Val::U32(4_000_000_000)),
        (Ty::U64, Val::U64(u64::MAX - 1)),
        (Ty::S8, Val::S8(-5)),
        (Ty::S16, Val::S16(-30_000)),
        (Ty::S32, Val::S32(-2_000_000_000)),
        (Ty::S64, Val::S64(i64::MIN + 1)),
        (Ty::F32, Val::F32(1.5)),
        (Ty::F64, Val::F64(-2.25e100)),
        (Ty::String, Val::str("florn — フローン")),
        (Ty::list(Ty::U8), Val::bytes(&[0, 1, 2, 255])),
        (
            Ty::list(Ty::U64),
            Val::List(vec![Val::U64(1), Val::U64(2), Val::U64(3)]),
        ),
        (
            Ty::list(Ty::String),
            Val::List(vec![Val::str("a"), Val::str(""), Val::str("ccc")]),
        ),
        (Ty::Enum(3), Val::Enum(2)),
        (
            core_world::port(),
            Val::Record(vec![Val::U32(1), Val::U32(0), Val::str("eth0")]),
        ),
        (
            Ty::list(core_world::port()),
            Val::List(vec![
                Val::Record(vec![Val::U32(0), Val::U32(0), Val::str("eth0")]),
                Val::Record(vec![Val::U32(1), Val::U32(0), Val::str("eth1")]),
            ]),
        ),
        (
            Ty::option(Ty::U32),
            Val::Option(Some(Box::new(Val::U32(7)))),
        ),
        (Ty::option(Ty::String), Val::Option(None)),
        (
            Ty::result(Some(Ty::U64), Some(Ty::String)),
            Val::Result(Ok(Some(Box::new(Val::U64(9))))),
        ),
        (
            Ty::result(Some(Ty::U64), Some(Ty::String)),
            Val::Result(Err(Some(Box::new(Val::str("no"))))),
        ),
        (
            Ty::result(None, Some(Ty::Enum(2))),
            Val::Result(Err(Some(Box::new(Val::Enum(1))))),
        ),
        (Ty::result(None, None), Val::Result(Ok(None))),
        (
            Ty::variant(&[
                ("none", None),
                ("f", Some(Ty::F32)),
                ("big", Some(Ty::U64)),
                ("text", Some(Ty::String)),
            ]),
            Val::Variant(1, Some(Box::new(Val::F32(0.5)))),
        ),
        (
            Ty::variant(&[
                ("none", None),
                ("f", Some(Ty::F32)),
                ("big", Some(Ty::U64)),
                ("text", Some(Ty::String)),
            ]),
            Val::Variant(3, Some(Box::new(Val::str("hi")))),
        ),
        (
            Ty::variant(&[("none", None), ("f", Some(Ty::F32))]),
            Val::Variant(0, None),
        ),
        (
            Ty::record(&[
                ("a", Ty::U8),
                ("b", Ty::option(Ty::F64)),
                ("c", Ty::list(Ty::S16)),
            ]),
            Val::Record(vec![
                Val::U8(1),
                Val::Option(Some(Box::new(Val::F64(2.0)))),
                Val::List(vec![Val::S16(-1), Val::S16(1)]),
            ]),
        ),
    ]
}

#[test]
fn every_type_round_trips_through_memory() {
    for (ty, val) in all_types() {
        let mut m = mem();
        let addr = m.realloc(0, 0, ty.align(), ty.size()).unwrap();
        store(&mut m, &ty, &val, addr).unwrap_or_else(|e| panic!("{ty:?}: {e}"));
        let back = load(&m, &ty, addr).unwrap_or_else(|e| panic!("{ty:?}: {e}"));
        assert_eq!(back, val, "{ty:?}");
    }
}

#[test]
fn every_type_round_trips_through_flat_values() {
    for (ty, val) in all_types() {
        let mut m = mem();
        let flats = lower_flat_params(
            &mut m,
            std::slice::from_ref(&ty),
            std::slice::from_ref(&val),
        )
        .unwrap_or_else(|e| panic!("{ty:?}: {e}"));
        assert_eq!(flats.len(), ty.flat_count(), "{ty:?}");
        assert!(
            flats.iter().zip(ty.flat_types()).all(|(f, t)| f.ty() == t),
            "{ty:?}: flat types"
        );
        let back = lift_flat_params(&m, std::slice::from_ref(&ty), &flats)
            .unwrap_or_else(|e| panic!("{ty:?}: {e}"));
        assert_eq!(back, vec![val], "{ty:?}");
    }
}

#[test]
fn strict_lifting_produces_the_three_contract_faults() {
    let mut m = mem();
    // invalid UTF-8
    m.write(100, &[0xff, 0xfe]).unwrap();
    let e = lift_flat_params(&m, &[Ty::String], &[Flat::I32(100), Flat::I32(2)]).unwrap_err();
    assert_eq!(e, AbiError::InvalidUtf8 { addr: 100, len: 2 });
    assert_eq!(e.rule(), "RFC 002 rule 8");
    // a list outside memory
    let e = lift_flat_params(
        &m,
        &[Ty::list(Ty::U8)],
        &[Flat::I32(65_000), Flat::I32(1_000)],
    )
    .unwrap_err();
    assert_eq!(
        e,
        AbiError::OutOfRange {
            addr: 65_000,
            len: 1_000,
            size: 65_536
        }
    );
    let e = lift_flat_params(
        &m,
        &[Ty::list(Ty::U64)],
        &[Flat::I32(0), Flat::I32(i32::MAX)],
    )
    .unwrap_err();
    assert!(matches!(
        e,
        AbiError::OutOfRange { .. } | AbiError::Length { .. }
    ));
    // an enumeration out of range, in flats and in memory
    let e = lift_flat_params(&m, &[Ty::Enum(3)], &[Flat::I32(3)]).unwrap_err();
    assert_eq!(e, AbiError::Discriminant { found: 3, cases: 3 });
    m.write(200, &[9]).unwrap();
    assert_eq!(
        load(&m, &Ty::option(Ty::U8), 200).unwrap_err(),
        AbiError::Discriminant { found: 9, cases: 2 }
    );
    // a bad pointer in a record field
    m.write(300, &[0xff, 0xff, 0xff, 0x7f, 5, 0, 0, 0]).unwrap();
    assert!(matches!(
        load(&m, &Ty::String, 300).unwrap_err(),
        AbiError::OutOfRange { .. }
    ));
}

#[test]
fn nan_is_canonical_in_both_directions() {
    let mut m = mem();
    let payload_nan = f64::from_bits(0x7ff8_0000_0000_1234);
    let flats = lower_flat_params(
        &mut m,
        &[Ty::F64, Ty::F32],
        &[Val::F64(payload_nan), Val::F32(f32::from_bits(0xffc0_0001))],
    )
    .unwrap();
    // NaN never compares equal: compare the bits.
    assert!(matches!(flats[0], Flat::F64(x) if x.to_bits() == 0x7ff8_0000_0000_0000));
    assert!(matches!(flats[1], Flat::F32(x) if x.to_bits() == 0x7fc0_0000));
    let back = lift_flat_params(
        &m,
        &[Ty::F64],
        &[Flat::F64(f64::from_bits(0xfff8_0000_0000_0000))],
    )
    .unwrap();
    assert!(matches!(back[0], Val::F64(x) if x.to_bits() == 0x7ff8_0000_0000_0000));
    // a NaN stored to memory is canonical in memory too
    store(&mut m, &Ty::F32, &Val::F32(f32::NAN), 16).unwrap();
    assert_eq!(m.read(16, 4).unwrap(), 0x7fc0_0000u32.to_le_bytes());
}

#[test]
fn more_than_sixteen_flats_are_spilled_through_memory() {
    let tys: Vec<Ty> = (0..9)
        .map(|_| Ty::U64)
        .chain(std::iter::once(Ty::String))
        .collect(); // 9 + 2 = 11 flats: not spilled
    let vals: Vec<Val> = (0..9)
        .map(Val::U64)
        .chain(std::iter::once(Val::str("x")))
        .collect();
    let mut m = mem();
    assert_eq!(lower_flat_params(&mut m, &tys, &vals).unwrap().len(), 11);
    let tys: Vec<Ty> = (0..17).map(|_| Ty::U32).collect();
    let vals: Vec<Val> = (0..17).map(Val::U32).collect();
    let flats = lower_flat_params(&mut m, &tys, &vals).unwrap();
    assert_eq!(flats.len(), 1, "one pointer");
    assert_eq!(lift_flat_params(&m, &tys, &flats).unwrap(), vals);
}

#[test]
fn results_of_one_flat_and_of_a_return_area() {
    let mut m = mem();
    assert_eq!(
        lower_result(&mut m, Some(&Ty::U64), Some(&Val::U64(42))).unwrap(),
        Some(Flat::I64(42))
    );
    assert_eq!(lower_result(&mut m, None, None).unwrap(), None);
    assert!(
        lower_result(&mut m, Some(&Ty::String), Some(&Val::str("x"))).is_err(),
        "no core import returns a string"
    );
    assert_eq!(
        lift_results(&m, Some(&Ty::U64), &[Flat::I64(7)]).unwrap(),
        Some(Val::U64(7))
    );
    // an export returning a string: a pointer to a return area
    let ret = m.realloc(0, 0, 4, 8).unwrap();
    store(&mut m, &Ty::String, &Val::str("done"), ret).unwrap();
    assert_eq!(
        lift_results(&m, Some(&Ty::String), &[Flat::I32(ret as i32)]).unwrap(),
        Some(Val::str("done"))
    );
    assert!(lift_results(&m, None, &[Flat::I32(1)]).is_err());
}

#[test]
fn core_world_signatures_flatten_as_the_proof_of_concept_observed() {
    let flats = |s: &core_world::Signature| {
        s.param_types()
            .iter()
            .flat_map(Ty::flat_types)
            .collect::<Vec<_>>()
    };
    let host = core_world::host_functions();
    assert_eq!(
        flats(&host[0]),
        vec![FlatType::I32, FlatType::I32, FlatType::I32],
        "send (i32 i32 i32)"
    );
    assert_eq!(
        flats(&host[1]),
        vec![FlatType::I32, FlatType::I64],
        "set-timer (i32 i64)"
    );
    assert_eq!(host[3].result, Some(Ty::U64));
    let actor = core_world::actor_functions();
    assert_eq!(
        flats(&actor[0]),
        vec![
            FlatType::I64,
            FlatType::I32,
            FlatType::I32,
            FlatType::I32,
            FlatType::I32,
            FlatType::I32
        ],
        "init"
    );
    assert_eq!(
        flats(&actor[1]),
        vec![FlatType::I64, FlatType::I32, FlatType::I32, FlatType::I32],
        "on-message"
    );
    assert_eq!(
        flats(&actor[2]),
        vec![FlatType::I64, FlatType::I32],
        "on-timer"
    );
}

#[test]
fn core_world_signatures_agree_with_the_contract_of_record() {
    // The parsed WIT of contracts/florn-core/wit against the hand-written signatures.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut resolve = wit_parser::Resolve::default();
    resolve
        .push_dir(root.join("contracts/florn-core/wit"))
        .unwrap();
    let names_of = |iface: &str| -> Vec<String> {
        let (_, i) = resolve
            .interfaces
            .iter()
            .find(|(_, i)| i.name.as_deref() == Some(iface))
            .unwrap();
        i.functions.keys().cloned().collect()
    };
    let host: Vec<String> = core_world::host_functions()
        .iter()
        .map(|s| s.name.to_string())
        .collect();
    let actor: Vec<String> = core_world::actor_functions()
        .iter()
        .map(|s| s.name.to_string())
        .collect();
    let mut h = names_of("host");
    h.sort();
    let mut hh = host.clone();
    hh.sort();
    assert_eq!(h, hh);
    let mut a = names_of("actor");
    a.sort();
    let mut aa = actor.clone();
    aa.sort();
    assert_eq!(a, aa);
    // parameter counts agree
    let (_, host_iface) = resolve
        .interfaces
        .iter()
        .find(|(_, i)| i.name.as_deref() == Some("host"))
        .unwrap();
    for s in core_world::host_functions() {
        let f = &host_iface.functions[s.name];
        assert_eq!(f.params.len(), s.params.len(), "{}", s.name);
        assert_eq!(f.result.is_some(), s.result.is_some(), "{}", s.name);
    }
}
