//! The interface's labels in English and Japanese hold the same keys (RFC 007 rules 14 to 17).
//! `leptos_i18n` reports a key missing in one language as a deprecation warning with a fallback,
//! not as a build error, so the claim that a missing key fails the build does not hold; this
//! test is the control: the set of key paths of `locales/en.json` equals that of `locales/ja.json`.
//! The codes' sentences have their own test in `catalogue.rs`.
use std::collections::BTreeSet;
use std::iter::Peekable;
use std::str::Chars;

/// A reader of the one JSON shape a locale file has: objects of objects and strings.
struct Reader<'a> {
    chars: Peekable<Chars<'a>>,
}

impl Reader<'_> {
    fn skip(&mut self) {
        while self.chars.next_if(|c| c.is_whitespace()).is_some() {}
    }

    fn expect(&mut self, want: char) {
        self.skip();
        let got = self.chars.next();
        assert_eq!(got, Some(want), "a locale file is malformed");
    }

    fn string(&mut self) -> String {
        self.expect('"');
        let mut s = String::new();
        loop {
            match self.chars.next() {
                Some('"') => return s,
                Some('\\') => match self.chars.next() {
                    // Only keys are compared; the text of an escape does not matter.
                    Some('u') => {
                        for _ in 0..4 {
                            self.chars.next();
                        }
                        s.push('?');
                    }
                    Some(c) => s.push(c),
                    None => panic!("a locale file ends inside a string"),
                },
                Some(c) => s.push(c),
                None => panic!("a locale file ends inside a string"),
            }
        }
    }

    fn object(&mut self, prefix: &str, out: &mut BTreeSet<String>) {
        self.expect('{');
        self.skip();
        if self.chars.next_if_eq(&'}').is_some() {
            return;
        }
        loop {
            let key = self.string();
            self.expect(':');
            self.skip();
            let path = if prefix.is_empty() {
                key
            } else {
                format!("{prefix}.{key}")
            };
            match self.chars.peek() {
                Some('{') => self.object(&path, out),
                Some('"') => {
                    self.string();
                    out.insert(path);
                }
                other => panic!("a locale file holds objects and strings only, not {other:?}"),
            }
            self.skip();
            if self.chars.next_if_eq(&',').is_none() {
                self.expect('}');
                return;
            }
        }
    }
}

/// The dotted path of every string in a locale file.
fn key_paths(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    Reader {
        chars: text.chars().peekable(),
    }
    .object("", &mut out);
    out
}

/// The paths in one set and not in the other, each marked with the side that has it.
fn difference(a: &BTreeSet<String>, b: &BTreeSet<String>) -> Vec<String> {
    let only_a = a.difference(b).map(|k| format!("only in the first: {k}"));
    let only_b = b.difference(a).map(|k| format!("only in the second: {k}"));
    only_a.chain(only_b).collect()
}

#[test]
fn english_and_japanese_hold_the_same_keys() {
    let en = key_paths(include_str!("../locales/en.json"));
    let ja = key_paths(include_str!("../locales/ja.json"));
    assert!(
        en.len() > 100,
        "the English file was read: {} keys",
        en.len()
    );
    let diff = difference(&en, &ja);
    assert!(
        diff.is_empty(),
        "the labels of the two languages differ:\n{}",
        diff.join("\n")
    );
}

#[test]
fn a_key_missing_in_one_language_is_found() {
    let whole = key_paths(r#"{"a": {"b": "x", "c": "yé"}, "d": "z"}"#);
    let missing = key_paths(r#"{"a": {"b": "x"}, "d": "z"}"#);
    assert_eq!(whole.len(), 3);
    assert_eq!(
        difference(&whole, &missing),
        vec!["only in the first: a.c".to_string()]
    );
    assert_eq!(
        difference(&missing, &whole),
        vec!["only in the second: a.c".to_string()]
    );
    // A key renamed in one language is a key missing in each.
    let renamed = key_paths(r#"{"a": {"b": "x", "cc": "y"}, "d": "z"}"#);
    assert_eq!(difference(&whole, &renamed).len(), 2);
}

#[test]
fn the_reader_follows_nesting_and_empty_objects() {
    let paths = key_paths(r#"{ "x": {}, "y": { "z": { "w": "v" } }, "u": "t" }"#);
    let want: BTreeSet<String> = ["u", "y.z.w"].iter().map(|s| s.to_string()).collect();
    assert_eq!(paths, want);
}
