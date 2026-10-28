//! The codes' sentences (RFC 007 rules 14 to 17): every notification the Host sends is a code
//! with typed parameters, and the shell turns it into text in the person's language from
//! `catalogue/<lang>.json`, a flat object of `code.<code>` sentences with `{parameter}`
//! placeholders and `enum.<set>.<value>` texts. Both files are in the crate; the test
//! `tests/catalogue.rs` holds them to the catalogue of codes.
use florn_ui_model::florn_control::{CATALOGUE, Notification, ParamType, Value};

const EN: &str = include_str!("../../catalogue/en.json");
const JA: &str = include_str!("../../catalogue/ja.json");

/// The language of a catalogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Ja,
}

/// A flat JSON object of strings, read once per language.
fn entries(lang: Lang) -> Vec<(String, String)> {
    let text = match lang {
        Lang::En => EN,
        Lang::Ja => JA,
    };
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

type Table = Vec<(String, String)>;

thread_local! {
    static TABLES: (Table, Table) = (entries(Lang::En), entries(Lang::Ja));
}

fn lookup(lang: Lang, key: &str) -> Option<String> {
    TABLES.with(|(en, ja)| {
        let t = match lang {
            Lang::En => en,
            Lang::Ja => ja,
        };
        t.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    })
}

/// A label of the interface with values filled in (`label.<key>` in the catalogue files): the
/// labels that carry a number or a name, which the compile-time catalogue cannot fill into a
/// plain string.
pub fn label(lang: Lang, key: &str, values: &[(&str, String)]) -> String {
    let mut out = lookup(lang, &format!("label.{key}")).unwrap_or_else(|| key.to_string());
    for (k, v) in values {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// A learning lab's text (`lab.<name>.<part>`), or nothing when the lab has none.
pub fn lab_text(lang: Lang, name: &str, part: &str) -> Option<String> {
    lookup(lang, &format!("lab.{name}.{part}"))
}

/// An enumeration value's text, or the value itself when the set has none.
pub fn enum_text(lang: Lang, set: &str, value: &str) -> String {
    lookup(lang, &format!("enum.{set}.{value}")).unwrap_or_else(|| value.replace('-', " "))
}

/// Digits grouped by thousands, as English and Japanese both write counts.
pub fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn bytes_text(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// A parameter's value as text (RFC 007 rule 6): counts grouped, bytes in binary units, an
/// enumeration by its text, an instant as a tick with the microstep when not zero.
pub fn value_text(lang: Lang, set: Option<&str>, v: &Value) -> String {
    match v {
        Value::Identity(i) => i.to_string(),
        Value::Name(s) | Value::Code(s) | Value::Text(s) => s.clone(),
        Value::Count(c) => group(*c),
        Value::Bytes(b) => bytes_text(*b),
        Value::Quantity(n, u) => format!("{n}{u}"),
        Value::Tick(t) => group(*t),
        Value::Instant(at) => {
            if at.microstep == 0 {
                group(at.tick)
            } else {
                format!("{} (+{})", group(at.tick), at.microstep)
            }
        }
        Value::Enum(e) => match set {
            Some(set) => enum_text(lang, set, e),
            None => e.replace('-', " "),
        },
    }
}

/// The sentence of a notification in the language, its parameters filled in; an unknown code
/// is said to be one (`message.unknown`).
pub fn render(lang: Lang, n: &Notification) -> String {
    let entry = CATALOGUE.iter().find(|e| e.name == n.code);
    let Some(template) = lookup(lang, &format!("code.{}", n.code)) else {
        let params = n
            .params
            .iter()
            .map(|(k, v)| format!("{k}={}", value_text(lang, None, v)))
            .collect::<Vec<_>>()
            .join(", ");
        let t = lookup(lang, "code.message.unknown").unwrap_or_default();
        return t.replace("{code}", &n.code).replace("{params}", &params);
    };
    let mut out = template;
    for (k, v) in &n.params {
        let set = entry.and_then(|e| {
            e.params.iter().find_map(|(name, ty)| match ty {
                ParamType::Enum(s) if name == k => Some(*s),
                _ => None,
            })
        });
        out = out.replace(&format!("{{{k}}}"), &value_text(lang, set, v));
    }
    // A parameter the notification did not carry (a field, a command) reads as nothing.
    while let Some(i) = out.find('{') {
        let Some(j) = out[i..].find('}') else { break };
        out.replace_range(i..i + j + 1, "");
    }
    out
}
