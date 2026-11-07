use super::*;

#[test]
fn fields_encode_as_rule_13_says() {
    let mut e = Encoder::default();
    e.bool(true);
    e.u64(2);
    e.s64(-1);
    e.f64(f64::NAN);
    e.text("hi");
    e.bytes(&[9]);
    let mut expect = alloc::vec![1u8];
    expect.extend_from_slice(&2u64.to_le_bytes());
    expect.extend_from_slice(&(-1i64).to_le_bytes());
    expect.extend_from_slice(&0x7ff8_0000_0000_0000u64.to_le_bytes());
    expect.extend_from_slice(&[2, 0, 0, 0, b'h', b'i', 1, 0, 0, 0, 9]);
    assert_eq!(e.finish(), expect);
}
