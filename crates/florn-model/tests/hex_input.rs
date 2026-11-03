//! A module's `sha256` is 64 hexadecimal digits (RFC 004 rule 12 and section 3.5): any other text
//! of any length or alphabet is refused with a code, never a panic (RFC 005 rule 5). A text of 64
//! bytes may hold a multi-byte character, which a slice by two bytes would split.
use florn_model::*;

fn scenario(sha: &str) -> String {
    format!(
        "florn = 1\nname = \"hex\"\ndomains = [\"core\"]\n\n[time]\ntick = \"1ns\"\nend = \"10ns\"\n\n\
         [order]\ntie = \"by-identity\"\n\n[seed]\nroot = 1\n\n[limits.default]\nmemory = \"1MiB\"\n\n\
         [modules.m]\nsha256 = \"{sha}\"\ncontract = \"florn:core/guest@0.1.0\"\n"
    )
}

fn codes(sha: &str) -> Vec<&'static str> {
    match Manifest::parse(&scenario(sha), &HostMaxima::native()) {
        Ok(_) => Vec::new(),
        Err(e) => e.codes(),
    }
}

#[test]
fn a_digest_of_64_digits_is_read() {
    assert!(codes(&"ab".repeat(32)).is_empty());
    assert!(codes(&"AB".repeat(32)).is_empty());
}

#[test]
fn a_digest_that_is_not_64_hex_digits_is_refused_and_never_panics() {
    // 64 bytes with a two-byte character at an odd offset: a slice by two bytes splits it.
    let split = format!("a{}{}", "Л", "a".repeat(61));
    assert_eq!(split.len(), 64);
    // 64 bytes of two-byte characters; 64 bytes of a three-byte character and one more byte.
    let cyrillic = "Л".repeat(32);
    let kana = format!("{}a", "あ".repeat(21));
    assert_eq!((cyrillic.len(), kana.len()), (64, 64));
    for bad in [
        split,
        cyrillic,
        kana,
        // a sign is not a digit, though `from_str_radix` takes one
        format!("+a{}", "ab".repeat(31)),
        "ab".repeat(31) + "xz",
        "ab".repeat(31),
        "ab".repeat(33),
        String::new(),
    ] {
        let c = codes(&bad);
        assert!(!c.is_empty(), "{bad:?} is refused");
    }
}
