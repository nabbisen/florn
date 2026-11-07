use super::*;
use florn_control::{At, CATALOGUE};

/// Every code has a sentence whose placeholders are exactly its parameters (RFC 007 rule 15).
#[test]
fn the_table_covers_the_catalogue_exactly() {
    assert_eq!(TABLE.len(), CATALOGUE.len());
    for e in CATALOGUE {
        let w = what(e.name).unwrap_or_else(|| panic!("{} has no sentence", e.name));
        let mut placeholders: Vec<String> = Vec::new();
        let mut rest = w;
        while let Some(i) = rest.find('{') {
            let j = rest[i..].find('}').expect("closed placeholder") + i;
            placeholders.push(rest[i + 1..j].to_string());
            rest = &rest[j + 1..];
        }
        let mut params: Vec<String> = e.params.iter().map(|(n, _)| n.to_string()).collect();
        placeholders.sort();
        placeholders.dedup();
        params.sort();
        assert_eq!(placeholders, params, "{}", e.name);
        assert!(w.ends_with('.'), "{}: a whole sentence", e.name);
    }
}

#[test]
fn an_unknown_code_is_rendered_with_its_code_and_parameters() {
    let n = florn_control::Notification::new(
        "future.code",
        florn_control::Severity::Info,
        florn_control::Scope::run(),
        florn_control::Ends::Nothing,
    )
    .with("x", Value::Count(7));
    let text = render(&n);
    assert!(
        text.contains("unknown message") && text.contains("future.code") && text.contains("7"),
        "{text}"
    );
}

#[test]
fn values_are_formatted_by_type() {
    assert_eq!(format_value(&Value::Count(10_000_000)), "10,000,000");
    assert_eq!(format_value(&Value::Bytes(4 * 1024 * 1024)), "4 MiB");
    assert_eq!(format_value(&Value::Bytes(1536)), "1.5 KiB");
    assert_eq!(format_value(&Value::Bytes(12)), "12 B");
    assert_eq!(
        format_value(&Value::Quantity("5".into(), "ms".into())),
        "5ms"
    );
    assert_eq!(
        format_value(&Value::Instant(At {
            tick: 12_345,
            microstep: 3
        })),
        "tick 12,345 (+3)"
    );
    assert_eq!(
        format_value(&Value::Enum("end-reached".into())),
        "end reached"
    );
    let n = florn_control::Code::RunEnded.notification(
        florn_control::Scope::run(),
        vec![Value::Enum("end-reached".into())],
    );
    assert_eq!(render(&n), "The run ended: end reached.");
    assert_eq!(line(&n), "info: run.ended: The run ended: end reached.");
}
