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
        "quantity.dimension",
        "The field {field} is {value}, which is not a quantity of {expected}.",
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
        "connection.mixed-types",
        "Connection {connection} joins ports of more than one domain.",
    ),
    (
        "domain.unknown",
        "The scenario declares the domain {domain}, which this Host does not provide.",
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
        "module.export-unsupported",
        "The function {name} of the module's world has a signature no Host can call.",
    ),
    (
        "module.segment-too-long",
        "Function {function} of the module runs {cost} instructions between two budget checks; the class's budget is {budget}.",
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
        "domain.event",
        "The {domain} domain recorded {event} at {target}: {detail}.",
    ),
    (
        "network.link-ports",
        "The link {connection} joins {found} interfaces; a link joins exactly two.",
    ),
    (
        "network.port-type",
        "Port {port} of connection {connection} has the type {type}, which is not a network interface.",
    ),
    (
        "network.attr-unknown",
        "Connection {connection} sets {key}, which the network domain does not know.",
    ),
    (
        "network.attr-type",
        "Connection {connection} sets {key}, which must be {expected}.",
    ),
    (
        "network.protocol-unknown",
        "The module {module} declares the protocol {protocol}, for which there is no decoder; its frames are shown as bytes.",
    ),
    (
        "network.telemetry-layout",
        "The module {module} declares the channel {channel} with a layout the network view does not read.",
    ),
    (
        "circuit.no-ground",
        "The circuit has nets and none of them is the ground; set ground = true on one.",
    ),
    (
        "circuit.two-grounds",
        "The net {connection} is a second ground; a scenario has exactly one.",
    ),
    (
        "circuit.pin-open",
        "The pin {port} of {actor} is on no net.",
    ),
    (
        "circuit.port-type",
        "Port {port} of connection {connection} has the type {type}, which the connection does not take.",
    ),
    (
        "circuit.signal-ports",
        "The signal {connection} has {outs} outputs; it needs exactly one.",
    ),
    (
        "circuit.signal-dimension",
        "The port {port} of the signal {connection} does not hold a quantity of {expected}.",
    ),
    (
        "circuit.signal-unconnected",
        "The input {port} of {actor} is on no signal and reads 0.",
    ),
    (
        "circuit.attr-unknown",
        "Connection {connection} sets {key}, which the circuit domain does not know; it knows {expected}.",
    ),
    (
        "circuit.attr-type",
        "Connection {connection} sets {key}, which must be {expected}.",
    ),
    (
        "circuit.param-unknown",
        "Actor {actor} sets {key}, which its kind does not take; it takes {expected}.",
    ),
    (
        "circuit.param-type",
        "Actor {actor} sets {key}, which must be {expected}.",
    ),
    (
        "circuit.param-missing",
        "Actor {actor} needs the parameter {key}, {expected}.",
    ),
    (
        "circuit.profile-unknown",
        "The [circuit] table has the key {key}, which is no solver setting; the settings are {expected}.",
    ),
    (
        "circuit.profile-value",
        "The solver setting {key} must be {expected}.",
    ),
    (
        "circuit.no-period",
        "Actor {actor} samples and needs a period.",
    ),
    (
        "circuit.period-zero",
        "Actor {actor} has a period of zero ticks; it must be at least 1.",
    ),
    (
        "circuit.kind-unknown",
        "The module {module} declares the circuit kind {kind}, which the domain does not have.",
    ),
    (
        "circuit.device-limit",
        "The scenario has {found} guest device models; this release admits at most {maximum}.",
    ),
    (
        "circuit.device-ports",
        "The device module {module} declares {found} pin port kinds; it needs exactly one, of two to sixteen pins.",
    ),
    (
        "circuit.plant-unobserved",
        "The scenario has a controller or a settable value and no sensor or detector to observe the plant.",
    ),
    (
        "circuit.floating-node",
        "The net {connection} has no direct-current path to ground, so its voltage is set by gmin and means nothing; give a capacitor an ic or add a resistor.",
    ),
    (
        "circuit.too-large",
        "The plant has {unknowns} unknowns, above the bound {maximum}; it would need {bytes}.",
    ),
    (
        "circuit.no-operating-point",
        "The initial solve found no operating point: the unknown {unknown}, a {kind}, after {iterations} iterations; give a capacitor or an inductor an ic, or change the sources.",
    ),
    (
        "circuit.stamp-invalid",
        "The device {actor} returned an invalid stamp: {reason}.",
    ),
    (
        "circuit.device-faulted",
        "The device {actor} faulted and is absent from the plant from now on.",
    ),
    (
        "circuit.non-convergence",
        "The plant did not converge at {tick}: the largest residual is at {actor}, after {iterations} iterations.",
    ),
    (
        "circuit.step-rejected",
        "At {tick} {attempts} attempts were rejected, the first because of {cause}.",
    ),
    (
        "circuit.step-minimum",
        "At {tick} the error test failed at the smallest step, {step} ticks.",
    ),
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
        "replay.exempt-only",
        "The replay agrees with the record: {compared} records compared, and the runs differ only in what a continuous actor consumed, which the comparison does not read.",
    ),
    (
        "replay.within-tolerance",
        "The replay agrees with the record within the tolerance: {compared} records compared, {differing} differ, the largest in {worst} by {largest} at tick {at}.",
    ),
    (
        "replay.value-differs",
        "At record {record} the field {field} was {recorded} in the record and is {replayed} in the replay, beyond the tolerance.",
    ),
    (
        "replay.record-differs",
        "The replay differs from the record at record {record}: {what}.",
    ),
    (
        "replay.needs-payloads",
        "The replay cannot be compared within a tolerance: the trace or the payload bytes of {side} are missing.",
    ),
    (
        "checkpoint.taken",
        "A checkpoint was taken at tick {instant} ({bytes}); {kept} are kept.",
    ),
    (
        "checkpoint.budget",
        "The checkpoints' memory budget dropped {dropped} older checkpoints; {kept} are kept ({bytes}).",
    ),
    (
        "run.rewound",
        "The run went back to tick {to}; {undone} interventions after it were undone.",
    ),
    (
        "storage.evicted",
        "The browser let the {kind} {name} go from its storage; the file is the durable form.",
    ),
    (
        "module.component",
        "{module} is a WebAssembly Component, which florn does not run; {reason}.",
    ),
    (
        "run.branched",
        "A branch from tick {instant} of run {parent}; the parent's record is kept.",
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
mod tests;
