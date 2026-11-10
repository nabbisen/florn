use super::*;

#[test]
fn layouts_follow_the_canonical_abi() {
    let port = Ty::record(&[("id", Ty::U32), ("kind", Ty::U32), ("name", Ty::String)]);
    assert_eq!(port.size(), 16);
    assert_eq!(port.align(), 4);
    assert_eq!(port.field_offsets(), vec![0, 4, 8]);
    assert_eq!(
        port.flat_types(),
        vec![FlatType::I32, FlatType::I32, FlatType::I32, FlatType::I32]
    );
    let mixed = Ty::record(&[("a", Ty::U8), ("b", Ty::U64), ("c", Ty::U16)]);
    assert_eq!(mixed.field_offsets(), vec![0, 8, 16]);
    assert_eq!(mixed.size(), 24);
    assert_eq!(Ty::option(Ty::U32).size(), 8);
    assert_eq!(Ty::option(Ty::U32).payload_offset(), 4);
    assert_eq!(Ty::result(Some(Ty::U64), Some(Ty::U8)).size(), 16);
    assert_eq!(
        Ty::result(Some(Ty::F32), Some(Ty::U32)).flat_types(),
        vec![FlatType::I32, FlatType::I32]
    );
    assert_eq!(
        Ty::result(Some(Ty::F32), Some(Ty::U64)).flat_types(),
        vec![FlatType::I32, FlatType::I64]
    );
    assert_eq!(Ty::Enum(3).size(), 1);
    assert_eq!(Ty::Enum(300).size(), 2);
    let v = Ty::variant(&[("none", None), ("some", Some(Ty::U64))]);
    assert_eq!(v.size(), 16);
    assert_eq!(v.flat_types(), vec![FlatType::I32, FlatType::I64]);
}

#[test]
fn an_import_takes_a_return_pointer_and_an_export_returns_one() {
    // RFC 002 rule 2, the canonical ABI: a result of more than one flat value comes back through a
    // pointer. For a function the guest imports the pointer is its last parameter and it returns
    // nothing; for a function it exports it returns the pointer and its parameters are unchanged.
    let list = Ty::list(Ty::F64);
    let (p, r) = Ty::import_signature(&[Ty::U32], Some(&list));
    assert_eq!((p, r), (vec![FlatType::I32, FlatType::I32], vec![]));
    let (p, r) = Ty::export_signature(std::slice::from_ref(&list), Some(&list));
    assert_eq!(
        (p, r),
        (vec![FlatType::I32, FlatType::I32], vec![FlatType::I32]),
        "evaluate: (i32, i32) -> i32"
    );
    // A result of one flat value is returned as it is, in both directions.
    assert_eq!(
        Ty::import_signature(&[], Some(&Ty::U64)),
        (vec![], vec![FlatType::I64])
    );
    assert_eq!(
        Ty::export_signature(&[Ty::F64], Some(&Ty::U64)),
        (vec![FlatType::F64], vec![FlatType::I64])
    );
    // No result: no change from the parameters.
    assert_eq!(
        Ty::export_signature(&[Ty::U64, Ty::F64], None),
        (vec![FlatType::I64, FlatType::F64], vec![])
    );
    // More than sixteen flat parameters travel through one pointer in both.
    let many = vec![Ty::U32; 17];
    assert_eq!(Ty::export_signature(&many, None).0, vec![FlatType::I32]);
    assert_eq!(Ty::import_signature(&many, None).0, vec![FlatType::I32]);
}

#[test]
fn a_result_needs_a_post_return_when_it_is_or_holds_a_list_or_a_string() {
    let list = Ty::list(Ty::F64);
    assert!(list.needs_post_return());
    assert!(Ty::String.needs_post_return());
    assert!(Ty::option(Ty::String).needs_post_return());
    assert!(Ty::record(&[("n", Ty::U32), ("xs", list.clone())]).needs_post_return());
    assert!(Ty::result(Some(Ty::U32), Some(Ty::String)).needs_post_return());
    assert!(Ty::variant(&[("a", None), ("b", Some(list))]).needs_post_return());
    // Scalars, enumerations and records of them leave nothing behind.
    for t in [Ty::Bool, Ty::U64, Ty::F64, Ty::Enum(3)] {
        assert!(!t.needs_post_return(), "{t:?}");
    }
    assert!(!Ty::record(&[("a", Ty::U32), ("b", Ty::F64)]).needs_post_return());
    assert!(!Ty::option(Ty::U8).needs_post_return());
}
