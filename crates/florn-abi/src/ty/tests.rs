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
