use super::unhex;

#[test]
fn hex_is_read_by_digits_and_never_splits_a_character() {
    assert_eq!(unhex("00ff10"), Some(vec![0, 0xff, 0x10]));
    assert_eq!(unhex("AbCd"), Some(vec![0xab, 0xcd]));
    assert_eq!(unhex(""), Some(Vec::new()));
    assert_eq!(unhex("abc"), None);
    assert_eq!(unhex("0g"), None);
    // A sign is not a digit, though `from_str_radix` takes one.
    assert_eq!(unhex("+a"), None);
    assert_eq!(unhex("-1"), None);
    // An even number of bytes with a multi-byte character across a pair: refused, no panic.
    assert_eq!(unhex("aЛa"), None);
    assert_eq!(unhex("Л"), None);
    assert_eq!(unhex(&"あ".repeat(4)), None);
}
