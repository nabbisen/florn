//! The runner's English table: one sentence per code of RFC 007 §4 with named placeholders, the
//! English source of RFC 007 rule 17. The catalogue mechanism of the shells arrives in 0.2; this
//! table moves into it then. Values are formatted here by their type (rule 6); nothing in the
//! Host produced a sentence.

use florn_control::{Notification, Value};

/// `(code, what)` with `{name}` placeholders for the code's parameters.
pub const TABLE: &[(&str, &str)] = &[
    (
        "manifest.too-large",
        "The manifest has too many {what}: {found}, where at most {maximum} are allowed.",
    ),
    (
        "manifest.invalid",
        "The manifest is not valid TOML: {reason}.",
    ),
    (
        "manifest.schema-version",
        "The manifest declares schema version {found}; this florn reads {supported}.",
    ),
    ("manifest.type", "The field {field} must be {expected}."),
    (
        "manifest.unknown-key",
        "The key {key} is not part of a manifest.",
    ),
    ("manifest.missing-field", "The field {field} is required."),
    (
        "identity.duplicate",
        "The identity {identity} is used by both {first} and {second}.",
    ),
    ("identity.reserved", "The identity {identity} is reserved."),
    (
        "identity.port-duplicate",
        "Actor {actor} declares port {port} twice.",
    ),
    (
        "name.duplicate",
        "The name {name} is used by two elements of kind {kind}.",
    ),
    (
        "reference.unresolved",
        "The reference {reference} names no {kind}.",
    ),
    (
        "domain.undeclared",
        "The domain {domain} is used at {where} but not declared.",
    ),
    (
        "module.hash-mismatch",
        "The module {module} declares the hash {declared}, but its bytes hash to {actual}.",
    ),
    (
        "module.contract-mismatch",
        "The module {module} declares the contract {declared}, but its identity is {found}.",
    ),
    (
        "connection.too-few-ports",
        "Connection {connection} has {ports} port; two or more are needed.",
    ),
    (
        "quantity.missing-unit",
        "The field {field} needs a unit of {dimension}.",
    ),
    (
        "quantity.not-whole-ticks",
        "The field {field} is {value}, which is not a whole number of ticks of {tick}; the smallest tick that fits is {smallest}.",
    ),
    (
        "quantity.not-whole-unit",
        "The field {field} is {value}, which is not whole in {unit}.",
    ),
    (
        "quantity.unknown-unit",
        "The field {field} uses the unit {unit}, which no declared domain knows.",
    ),
    (
        "quantity.out-of-range",
        "The field {field} is {value}, which is out of range.",
    ),
    (
        "limit.above-maximum",
        "The limit {limit} is {value}; this Host allows at most {maximum}.",
    ),
    (
        "time.end-before-change",
        "The change {change} is scheduled at {at}, after the end {end}.",
    ),
    (
        "config.missing-parameter",
        "Actor {actor} needs the parameter {parameter}.",
    ),
    (
        "config.unknown-parameter",
        "Actor {actor} sets the parameter {parameter}, which its module does not declare.",
    ),
    (
        "config.type",
        "Actor {actor} sets the parameter {parameter}, which must be {expected}.",
    ),
    (
        "module.invalid",
        "The module is not valid WebAssembly: {reason}.",
    ),
    (
        "module.too-large",
        "The module has too many {what}: {found}, where at most {maximum} are allowed.",
    ),
    (
        "module.feature-refused",
        "The module uses the feature {feature}, which florn does not admit.",
    ),
    ("module.memory", "The module's memory is refused: {reason}."),
    ("module.start-section", "The module has a start section."),
    (
        "module.table-mutation",
        "The module uses {instruction}, which florn does not admit.",
    ),
    (
        "module.import-refused",
        "The module imports {module}.{name}, which is not part of its world.",
    ),
    (
        "module.export-missing",
        "The module does not export {name} with the expected signature.",
    ),
    (
        "module.export-reserved",
        "The module exports {name}, a reserved name.",
    ),
    (
        "module.initial-memory-above-limit",
        "The module's initial memory is {pages} pages; the class allows {limit}.",
    ),
    (
        "module.manifest-missing",
        "The module has no {section} section.",
    ),
    (
        "module.manifest-schema",
        "The module's manifest is malformed: {reason}.",
    ),
    (
        "module.contract-unsupported",
        "The module declares the world {declared}; this Host supports {supported}.",
    ),
    (
        "module.port-type-unknown",
        "Actor {actor} has the port {port} of type {type}, which its module does not declare.",
    ),
    (
        "module.port-count",
        "Actor {actor} has {found} ports of kind {kind}; between {min} and {max} are needed.",
    ),
    (
        "limit.below-need",
        "Actor {actor} needs {need} of {limit}, but its class allows {value}.",
    ),
    (
        "admission.transform-failed",
        "The admission transformation failed: {reason}.",
    ),
    (
        "admission.result",
        "The module {module} was admitted as {admitted}; its admitted hash is {hash}.",
    ),
    (
        "host.capacity-exceeded",
        "The scenario needs {need} {what}; this Host holds {capacity}.",
    ),
    (
        "host.capacity-warning",
        "The run uses {used} of {capacity} {what}.",
    ),
    (
        "host.capacity",
        "This Host holds {instances} instances and {memory} of memory.",
    ),
    (
        "host.instance-failed",
        "Actor {actor} could not be instantiated: {text}.",
    ),
    (
        "host.engine-refused",
        "The engine refused the module {module}: {text}.",
    ),
    (
        "host.not-reproducible",
        "The run ended on a condition of the machine, {reason}, and is not reproducible: {text}.",
    ),
    (
        "host.swapping",
        "{resident} of {actors} actors are resident; a swap costs {cost} per KB.",
    ),
    ("host.limited-by", "The run is limited by {what}."),
    (
        "actor.faulted",
        "Actor {actor} faulted with {fault} after {metered} units of work; the policy is {policy}.",
    ),
    ("actor.text", "Actor {actor} says: {text}."),
    ("run.ended", "The run ended: {reason}."),
    (
        "run.limit-exceeded",
        "The run exceeded {what} ({limit}); the origins were {origins}.",
    ),
    (
        "intervention.applied",
        "An intervention of kind {kind} was applied at {at}.",
    ),
    (
        "telemetry.dropped",
        "{records} telemetry records were dropped on the {path} path.",
    ),
    ("telemetry.transport", "Telemetry travels by {transport}."),
    (
        "control.version",
        "The front end speaks interface version {front-end}; this Host speaks {host}.",
    ),
    (
        "control.invalid",
        "The command {command} was not applied: {reason} at {field}.",
    ),
    (
        "control.not-now",
        "The command {command} does not apply while the run is {state}.",
    ),
    (
        "package.partial",
        "The package is missing {missing} modules.",
    ),
    (
        "package.entry-mismatch",
        "The package entry {entry} declares {declared} but hashes to {actual}.",
    ),
    (
        "file.too-large",
        "The {kind} file is {size}; at most {maximum} is read.",
    ),
    (
        "file.unreadable",
        "The {kind} file could not be read: {reason}.",
    ),
    (
        "export.preview",
        "The {kind} export will be {size}; payload bytes included: {payloads}.",
    ),
    (
        "replay.matched",
        "The replay matched the record at every one of {points} stored points.",
    ),
    (
        "replay.diverged",
        "The replay diverged from the record between records {from} and {to}.",
    ),
    (
        "checkpoint.mismatch",
        "The record's {what} is {expected}; this run's is {found}.",
    ),
    (
        "storage.cache",
        "{items} items are kept in this browser; export to keep them.",
    ),
    (
        "message.unknown",
        "An unknown message {code} arrived with the parameters {params}.",
    ),
];

pub fn what(code: &str) -> Option<&'static str> {
    TABLE.iter().find(|(c, _)| *c == code).map(|(_, w)| *w)
}

/// A value formatted for English (RFC 007 rule 6): counts grouped, bytes in binary units,
/// quantities as written, instants as ticks with the microstep when not zero.
pub fn format_value(v: &Value) -> String {
    match v {
        Value::Identity(i) => i.to_string(),
        Value::Name(s) | Value::Code(s) | Value::Text(s) => s.clone(),
        Value::Count(c) => group(*c),
        Value::Bytes(b) => bytes(*b),
        Value::Quantity(n, u) => format!("{n}{u}"),
        Value::Tick(t) => format!("tick {}", group(*t)),
        Value::Instant(at) => {
            if at.microstep == 0 {
                format!("tick {}", group(at.tick))
            } else {
                format!("tick {} (+{})", group(at.tick), at.microstep)
            }
        }
        Value::Enum(e) => e.replace('-', " "),
    }
}

fn group(n: u64) -> String {
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

fn bytes(b: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else if v.fract() == 0.0 {
        format!("{v:.0} {}", UNITS[u])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// The sentence for a notification; an unknown code is shown raw (rule 10).
pub fn render(n: &Notification) -> String {
    let mut text = match what(&n.code) {
        Some(w) => w.to_string(),
        None => {
            // RFC 007 rule 10: an unknown code is rendered as `message.unknown`, with the code
            // and the parameters shown raw.
            let params: Vec<String> = n
                .params
                .iter()
                .map(|(k, v)| format!("{k}={}", format_value(v)))
                .collect();
            let sentence = what("message.unknown").unwrap_or("{code} ({params})");
            return sentence
                .replace("{code}", &n.code)
                .replace("{params}", &params.join(", "));
        }
    };
    for (name, v) in &n.params {
        text = text.replace(&format!("{{{name}}}"), &format_value(v));
    }
    text
}

/// One line: the severity, the code, the sentence, and where it applies.
pub fn line(n: &Notification) -> String {
    let mut s = format!("{}: {}: {}", n.severity.as_str(), n.code, render(n));
    if let Some(f) = &n.scope.field {
        s.push_str(&format!(" [{f}]"));
    } else if let Some(f) = &n.scope.file {
        s.push_str(&format!(" [{f}]"));
    } else if let Some(a) = n.scope.actor {
        s.push_str(&format!(" [actor {a}]"));
    }
    s
}

#[cfg(test)]
mod tests {
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
}
