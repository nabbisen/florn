//! The shell's catalogues (RFC 007 rules 14 to 17; checklist C20): English and Japanese each
//! hold one sentence per code of the catalogue with the same placeholders, and one text per
//! value of every enumeration; nothing more, nothing less. The labels of the interface are
//! checked at compile time by `leptos_i18n`; the codes' sentences, rendered at run time, are
//! checked here.
use std::collections::BTreeSet;

use florn_control::catalogue::ENUMERATIONS;
use florn_control::{CATALOGUE, ParamType};

/// A flat JSON object of strings: `{"key": "text", ...}`, with the escapes JSON allows.
fn flat_json(text: &str) -> Vec<(String, String)> {
    let mut chars = text.chars();
    let mut strings = Vec::new();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        loop {
            match chars.next() {
                Some('"') | None => break,
                Some('\\') => match chars.next() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('u') => {
                        let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                        if let Some(ch) =
                            u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            s.push(ch);
                        }
                    }
                    Some(other) => s.push(other),
                    None => break,
                },
                Some(ch) => s.push(ch),
            }
        }
        strings.push(s);
    }
    strings
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect()
}

fn placeholders(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('}') else { break };
        out.insert(after[..j].to_string());
        rest = &after[j + 1..];
    }
    out
}

#[test]
fn both_catalogues_cover_every_code_and_enumeration_exactly() {
    let en = flat_json(include_str!("../catalogue/en.json"));
    let ja = flat_json(include_str!("../catalogue/ja.json"));
    let keys = |c: &[(String, String)]| c.iter().map(|(k, _)| k.clone()).collect::<BTreeSet<_>>();
    assert_eq!(keys(&en), keys(&ja), "the same keys in both languages");
    let mut expected = BTreeSet::new();
    for e in CATALOGUE {
        expected.insert(format!("code.{}", e.name));
    }
    for (set, values) in ENUMERATIONS {
        for v in *values {
            expected.insert(format!("enum.{set}.{v}"));
        }
    }
    // The labels the shell fills in at run time (`label.*`) are the same keys in both.
    let labels: BTreeSet<String> = keys(&en)
        .into_iter()
        .filter(|k| k.starts_with("label."))
        .collect();
    assert!(!labels.is_empty());
    let mut all = expected.clone();
    all.extend(labels.iter().cloned());
    assert_eq!(
        keys(&en),
        all,
        "one sentence per code, one text per enumeration value, the labels"
    );
    for k in &labels {
        let text = |c: &[(String, String)]| {
            c.iter()
                .find(|(x, _)| x == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(
            placeholders(&text(&en)),
            placeholders(&text(&ja)),
            "{k}: the same placeholders"
        );
        assert!(
            !placeholders(&text(&en)).is_empty(),
            "{k}: a label with values"
        );
    }
    for e in CATALOGUE {
        let key = format!("code.{}", e.name);
        let text = |c: &[(String, String)]| {
            c.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        let (te, tj) = (text(&en), text(&ja));
        assert_eq!(
            placeholders(&te),
            placeholders(&tj),
            "{}: the same placeholders",
            e.name
        );
        for p in placeholders(&te) {
            assert!(
                e.params.iter().any(|(n, _)| *n == p),
                "{}: {{{p}}} is no parameter",
                e.name
            );
        }
        for (_, ty) in e.params {
            if let ParamType::Enum(set) = ty {
                assert!(
                    ENUMERATIONS.iter().any(|(s, _)| s == set),
                    "{}: enumeration {set}",
                    e.name
                );
            }
        }
        assert!(!te.trim().is_empty() && !tj.trim().is_empty());
    }
}
