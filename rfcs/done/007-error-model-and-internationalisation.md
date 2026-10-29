# RFC 007 — Error model and internationalisation

**Status.** Implemented (0.1.0, 2026-10-20); accepted 2026-10-14. Open question 3 decided at acceptance: unit symbols stay as symbols in every language (rule 19). Amended 2026-10-14 by PR-02: four codes added to section 4.1 (`manifest.too-large`, `manifest.invalid`, `manifest.type`, `quantity.out-of-range`) and two enumerations, as the model crate needed them. Amended 2026-10-17 by PR-06: one code added to section 4.2 (`module.export-reserved`), as the admission crate needed it. Amended 2026-10-19 by PR-07: one code added to section 4.3 (`run.limit-exceeded`, the run faults of RFC 001 rule 13) and its enumeration `run-limit`. Amended 2026-10-20 by PR-11: one code added to section 4.3 (`actor.text`), the channel 0 text of RFC 002 rule 33 as it reaches the diagnostic log. Amended 2026-10-27 by PR-15 (RFC 008 §11): two codes added to section 4.1 (`connection.mixed-types`, RFC 008 rule 2; `domain.unknown`, a declared domain this Host does not provide) and one to section 4.2 (`module.export-unsupported`, RFC 008 rule 6); one code added to section 4.3 (`domain.event`, a domain's observation of its own event, RFC 008 rule 3) and `boolean` to the `field-type` enumeration. Amended 2026-10-27 by PR-16 (RFC 008 §11): table 4.6 with the network domain's codes of RFC 008 rule 18 and `network.telemetry-layout` (a declared channel whose layout differs from the one the view reads, rule 17); the enumerations `drop-reason` and `link-state`. Amended 2026-10-29 by PR-23 (RFC 009 section 8): section 4.4 gains `checkpoint.taken`, `checkpoint.budget` and `run.rewound`; the enumeration `checkpoint-field` gains `instant` and `actors`.
**Tracks.** Requirements CTL-003, CTL-004, UI-009, UI-011, UI-012, SEC-008, NFR-009, ACT-005, ACT-011, DATA-007. Requirement IDs refer to `docs/src/requirements.md`.
**Touches.** The three families of error; the form of every message the Host produces for a person: a code, typed parameters, a severity, a scope; the catalogue of codes named by RFC 001 to 006 and the external design; the diagnostic log; the translation catalogues of every front end and the rules by which text is produced from codes; the handling of languages, numbers, units, and text input.
**Depends on.** [RFC 001](./001-determinism-contract.md) rules 24 and 65; [RFC 002](./002-guest-contract.md) rules 41 and 42; [RFC 003](./003-module-admission-and-limits.md) §2 and §5; [RFC 004](./004-core-model.md) §7; [RFC 005](./005-host-architecture.md) §6; [RFC 006](./006-security-baseline-browser-host.md) §5 to §7; the external design (`docs/src/external-design.md`) §3.8, §3.12, §5, §8.

## Summary

The Host never produces a sentence. It produces a **code** with **typed parameters**, a **severity**, and a **scope**; the front end turns that into text in the user's language from a **catalogue** whose keys are checked when the front end is built. Three families of error are kept apart in the types: a value a guest receives from a domain function, which is not an error of the Host; a **fault**, which is a trace record and an actor's state; and a **notification**, which is what a person sees. Every code has one meaning for ever, one entry in the catalogue of §4, one test that produces it, and one presentation: what happened, why, and what to do.

Internationalisation follows from the same rule: no string a person sees is written in code; English is the source language and Japanese the second; a missing translation is a build error; numbers, units, and times are formatted by the front end from values, and the logical world knows no language.

## Background

The requirements name three families (CTL-004) because the earlier drafts conflated them: a guest's "error" return, a guest's fault, and a message to the user were one word, and an English sentence from the Host would have been untranslatable (CTL-003). Since then RFC 001 to 006 and the external design have named about sixty codes without a catalogue, and the proof of concept on the user interface built the catalogue mechanism — compile-time keys, interpolation, a language switch that keeps typed text, input methods — that this RFC makes a rule. The external design §3.8 fixed how a message is shown: a state with a cause and a next step, never a generic failure.

## Terms

| Term | Meaning |
|---|---|
| **Code** | A stable identifier of one kind of message: `<area>.<name>`, lowercase ASCII and hyphens. |
| **Parameter** | A typed value that travels with a code: an identity, a name, a quantity with its unit, a count, a size, an instant, another code, or an opaque text. |
| **Notification** | A message of the Host to a person: code, parameters, severity, scope, and what it ends. |
| **Fault** | The abnormal end of an activation or a run (RFC 001); a trace record; one of a closed set of kinds. |
| **Guest result** | A value a guest receives from a domain function; part of the contract, never a notification. |
| **Catalogue** | A file per language that maps keys to text with named placeholders. The front end's own strings and the texts of every code live in it. |
| **Key** | The address of one text in the catalogue. A code's texts have the keys `code.<area>.<name>.what`, `.why`, `.next`. |
| **Severity** | `error`, `warning`, `info`. |
| **Scope** | What the message is about: the run, an actor, a connection, a field of the manifest, a file, a command. |

## 1. Three families

**Rule 1.** A **guest result** is a value of the contract. The core contract has none (RFC 002 rule 42). A domain contract may define functions that return `result<T, E>` where `E` is a WIT `enum` or `variant` of conditions of the domain's model, documented in the domain's RFC. A guest result is never a notification, never a fault, and never traced unless the domain's RFC says that a particular value is an effect.

**Rule 2.** A **fault** is one of the six kinds of RFC 002 rule 41, in the trace as a record and on the actor as a state. Its codes are below. A fault produces one notification (`actor.faulted`) whose parameter carries the fault's code; the fault itself is not a notification.

| Fault kind | Code | Parameter | Meaning |
|---|---|---|---|
| trap | `fault.trap` | `text`: the engine's trap text, opaque | The guest executed `unreachable`, divided by zero, accessed memory out of bounds, or otherwise trapped. |
| budget | `fault.budget` | `budget: count` | The execution budget of the activation was exhausted. |
| stack | `fault.stack` | `limit: count` | The stack height limit was reached. |
| memory | `fault.memory` | `limit: bytes` | The Host could not complete a lowering because the guest's allocator returned an address outside its memory after growth was refused at the limit. |
| quota | `fault.quota` | `quota: name`, `limit: count or bytes` | A quota of RFC 003 §5 was exceeded. |
| contract | `fault.contract` | `rule: text`, `detail: text` | A contract violation of RFC 002: an undeclared port or channel, invalid UTF-8, a list outside memory, a value out of range, a call out of sequence. |

**Rule 3.** A **notification** is what the Host sends to a front end for a person, over the control interface (RFC 005 rule 26) and into the diagnostic log. It has this shape, in the canonical encoding of RFC 004 §5.

| Field | Type | Content |
|---|---|---|
| `code` | text | `<area>.<name>` |
| `severity` | enum | `error`, `warning`, `info` |
| `scope` | record of optional references | `run`, `actor` (identity), `connection` (identity), `port`, `field` (path in the manifest), `file` (name), `command` (name) |
| `params` | map of name to typed value | The parameters the code declares, no more and no fewer |
| `ends` | enum | `nothing`, `command`, `activation`, `run` |
| `at` | optional instant | The instant of the logical world the message concerns |

A Host fault (RFC 001 rule 65) is a notification with `ends: run` and the area `host`, and a record in the run record that marks the run not reproducible.

## 2. Codes

**Rule 4.** A code is `<area>.<name>`: lowercase ASCII letters, digits, and hyphens; one dot. The areas are closed and listed in §4; a new area is an amendment of this RFC. A name states the condition, not the fix.

**Rule 5.** A code's meaning never changes. A code is added with the release that produces it, listed in the catalogue of §4 with its parameters, severity, scope, and what it ends. A code is never reused; a code no longer produced stays in the catalogue marked retired, so that old diagnostic logs still render.

**Rule 6.** Parameters are typed. Each type has one formatting rule per language in the front end; the Host formats nothing.

| Type | Encoded as | Shown as |
|---|---|---|
| `identity` | u32, with the element's name when known | The name, with the identity on request: "r3 (3)" |
| `name` | text | As is |
| `count` | u64 | A number with the language's grouping: "10,000,000" / "10,000,000" |
| `bytes` | u64 | A size with a binary unit: "4 MiB", "131 KiB" |
| `quantity` | number text and unit text (RFC 004 rule 30) | The number and the unit symbol, never translated: "5ms", "1.2ohm" |
| `tick`, `instant` | u64; u64 and u32 | A time with a unit chosen from the scenario's tick length, and the microstep when it is not zero: "12.345 s", "12.345 s (+3)" |
| `code` | text | The other code's own `what` text, or the code itself if unknown |
| `text` | text | As is, in a monospace style, never translated: an engine's trap text, a field path, a file name |
| `enum` | text from a closed set the code declares | The catalogue text for `enum.<set>.<value>` |

**Rule 7.** A text parameter carries nothing a person would have to read to understand the message; it is detail. The code and the typed parameters alone must be enough to render `what` and `why`.

## 3. Presentation

**Rule 8.** Every code has three texts in every catalogue: `what` (one sentence, in the glossary's words), `why` (the cause the Host knows, from the parameters), and `next` (what the person can do; may be empty for `info`). The front end shows them as the external design §3.8 defines; it never shows a code alone to a person except in the diagnostic view, and never hides one.

**Rule 9.** Severity decides the presentation, not the wording: `error` is a state shown where it applies (the field, the actor, the run) and in the message list; `warning` is in the status area and the list; `info` is in the status area. Status is never colour alone: each severity has a word and a shape.

**Rule 10.** An unknown code — a Host newer than the front end, or a diagnostic log from another version — is rendered as `message.unknown` with the code and the parameters shown raw. Nothing is dropped.

**Rule 11.** What a message ends is shown: "The activation ended; the actor's later events are dropped as the scenario's policy says", "The run ended", "The command was not applied". A notification with `ends: nothing` is never worded as if something stopped.

## 4. The catalogue of codes

The codes of RFC 001 to 006 and the external design, with their parameters, severity, scope, and what they end. `R` marks a refusal (the command or the load is not applied). Parameters are listed as `name: type`.

### 4.1 Manifest and scenario (RFC 004 §7)

| Code | Parameters | Severity | Scope | Ends |
|---|---|---|---|---|
| `manifest.too-large` | `what: enum(manifest-count)`, `found: count`, `maximum: count` | error | file | command |
| `manifest.invalid` | `reason: text` | error | file | command |
| `manifest.schema-version` | `found: count`, `supported: count` | error | file | command |
| `manifest.type` | `field: text`, `expected: enum(field-type)` | error | field | command |
| `manifest.unknown-key` | `key: text` | error | field | command |
| `manifest.missing-field` | `field: text` | error | field | command |
| `identity.duplicate` | `identity: identity`, `first: name`, `second: name` | error | field | command |
| `identity.reserved` | `identity: identity` | error | field | command |
| `identity.port-duplicate` | `actor: identity`, `port: identity` | error | field | command |
| `name.duplicate` | `name: name`, `kind: enum(element)` | error | field | command |
| `reference.unresolved` | `reference: text`, `kind: enum(element)` | error | field | command |
| `domain.undeclared` | `domain: name`, `where: text` | error | field | command |
| `module.hash-mismatch` | `module: name`, `declared: text`, `actual: text` | error | file | command |
| `module.contract-mismatch` | `module: name`, `declared: text`, `found: text` | error | file | command |
| `connection.too-few-ports` | `connection: identity`, `ports: count` | error | field | command |
| `quantity.missing-unit` | `field: text`, `dimension: enum(dimension)` | error | field | command |
| `quantity.not-whole-ticks` | `field: text`, `value: quantity`, `tick: quantity`, `smallest: quantity` | error | field | command |
| `quantity.not-whole-unit` | `field: text`, `value: quantity`, `unit: text` | error | field | command |
| `quantity.unknown-unit` | `field: text`, `unit: text` | error | field | command |
| `quantity.out-of-range` | `field: text`, `value: quantity` | error | field | command |
| `limit.above-maximum` | `limit: name`, `value: count`, `maximum: count` | error | field | command |
| `time.end-before-change` | `change: text`, `at: instant`, `end: instant` | error | field | command |
| `config.missing-parameter` | `actor: identity`, `parameter: name` | error | field | command |
| `config.unknown-parameter` | `actor: identity`, `parameter: name` | error | field | command |
| `config.type` | `actor: identity`, `parameter: name`, `expected: enum(parameter-type)` | error | field | command |
| `connection.mixed-types` | `connection: name` | error | field | command |
| `domain.unknown` | `domain: name` | error | field | command |

### 4.2 Module and admission (RFC 002 §3, RFC 003 §2, §5)

| Code | Parameters | Severity | Scope | Ends |
|---|---|---|---|---|
| `module.invalid` | `reason: text` | error | file | command |
| `module.too-large` | `what: enum(module-count)`, `found: count`, `maximum: count` | error | file | command |
| `module.feature-refused` | `feature: name` | error | file | command |
| `module.memory` | `reason: enum(memory-rule)` | error | file | command |
| `module.start-section` | — | error | file | command |
| `module.table-mutation` | `instruction: name` | error | file | command |
| `module.import-refused` | `module: name`, `name: name` | error | file | command |
| `module.export-missing` | `name: name` | error | file | command |
| `module.export-reserved` | `name: name` | error | file | command |
| `module.initial-memory-above-limit` | `pages: count`, `limit: count` | error | file | command |
| `module.manifest-missing` | `section: enum(manifest-section)` | error | file | command |
| `module.manifest-schema` | `reason: text` | error | file | command |
| `module.contract-unsupported` | `declared: text`, `supported: text` | error | file | command |
| `module.port-type-unknown` | `actor: identity`, `port: name`, `type: text` | error | field | command |
| `module.port-count` | `actor: identity`, `kind: name`, `found: count`, `min: count`, `max: count` | error | field | command |
| `limit.below-need` | `actor: identity`, `limit: name`, `need: count`, `value: count` | error | field | command |
| `admission.transform-failed` | `reason: text` | error | file | command |
| `admission.result` | `module: name`, `hash: text`, `admitted: enum(trust-state)` | info | file | nothing |
| `module.export-unsupported` | `name: name` | error | file | command |

### 4.3 Host, capacity, and the run (RFC 003 rule 20, RFC 005 §4, §7)

| Code | Parameters | Severity | Scope | Ends |
|---|---|---|---|---|
| `host.capacity-exceeded` | `need: count`, `capacity: count`, `what: enum(capacity-kind)` | error | run | command |
| `host.capacity-warning` | `used: count`, `capacity: count`, `what: enum(capacity-kind)` | warning | run | nothing |
| `host.capacity` | `instances: count`, `memory: bytes` | info | run | nothing |
| `host.instance-failed` | `actor: identity`, `text: text` | error | run | run |
| `host.engine-refused` | `module: name`, `text: text` | error | run | run |
| `host.not-reproducible` | `reason: enum(machine-condition)`, `text: text` | error | run | run |
| `host.swapping` | `resident: count`, `actors: count`, `cost: count` (µs per KB) | info | run | nothing |
| `host.limited-by` | `what: enum(limiting-factor)` | info | run | nothing |
| `actor.faulted` | `actor: identity`, `fault: code`, `metered: count`, `policy: enum(faulted-policy)` | error | actor | activation |
| `actor.text` | `actor: identity`, `text: text` | info | actor | nothing |
| `run.ended` | `reason: enum(end-reason)` | info | run | run |
| `run.limit-exceeded` | `what: enum(run-limit)`, `limit: count`, `origins: text` | error | run | run |
| `intervention.applied` | `at: instant`, `kind: name` | info | run | nothing |
| `telemetry.dropped` | `records: count`, `path: enum(display-path)` | info | run | nothing |
| `telemetry.transport` | `transport: enum(transport)` | info | run | nothing |
| `domain.event` | `domain: name`, `event: name`, `target: identity`, `detail: text` | info | run | nothing |

### 4.4 Control interface, files, replay, checkpoint (RFC 005 §6, RFC 003 rule 22)

| Code | Parameters | Severity | Scope | Ends |
|---|---|---|---|---|
| `control.version` | `front-end: text`, `host: text` | error | command | command |
| `control.invalid` | `command: name`, `reason: enum(invalid-reason)`, `field: text` | error | command | command |
| `control.not-now` | `command: name`, `state: enum(run-state)` | error | command | command |
| `package.partial` | `missing: count` | warning | file | nothing |
| `package.entry-mismatch` | `entry: text`, `declared: text`, `actual: text` | error | file | command |
| `file.too-large` | `kind: enum(file-kind)`, `size: bytes`, `maximum: bytes` | error | file | command |
| `file.unreadable` | `kind: enum(file-kind)`, `reason: text` | error | file | command |
| `export.preview` | `kind: enum(file-kind)`, `size: bytes`, `payloads: enum(yes-no)` | info | file | nothing |
| `replay.matched` | `points: count` | info | run | nothing |
| `replay.diverged` | `from: count`, `to: count` | error | run | nothing |
| `checkpoint.mismatch` | `what: enum(checkpoint-field)`, `expected: text`, `found: text` | error | file | command |
| `checkpoint.taken` | `instant: tick`, `bytes: bytes`, `kept: count` | info | run | nothing |
| `checkpoint.budget` | `kept: count`, `dropped: count`, `bytes: bytes` | info | run | nothing |
| `run.rewound` | `to: tick`, `undone: count` | info | run | nothing |
| `storage.cache` | `items: count` | info | file | nothing |
| `message.unknown` | `code: text`, `params: text` | warning | — | nothing |

### 4.5 Enumerations

Each `enum(set)` above is a closed set with catalogue texts `enum.<set>.<value>`: `element` (actor, connection, port, module, domain), `dimension` (time, bytes, and the domains' registered dimensions), `parameter-type` (the types of RFC 002 rule 13), `module-count` (functions, globals, tables, segments, imports, exports, custom-sections, bytes), `memory-rule` (count, imported, shared, 64-bit, not-exported), `manifest-section` (contract-identity, florn-manifest), `trust-state` (validated, rejected), `capacity-kind` (instances, memory), `machine-condition` (memory-refused, engine-stack), `limiting-factor` (actors, solver, telemetry, display, memory), `faulted-policy` (drop, stop-run, and the domains' policies), `end-reason` (end-reached, user, host-fault), `run-limit` (microsteps-per-tick, events-per-instant), `display-path` (ring, messages), `transport` (shared, messages), `invalid-reason` (unknown-command, missing-field, wrong-type, out-of-range, unknown-field), `run-state` (no-scenario, invalid, ready, running, paused, ended), `file-kind` (manifest, package, module, run-record, checkpoint, diagnostic-log), `checkpoint-field` (module-hash, admission-version, cost-model, scenario-hash), `yes-no`, `manifest-count` (bytes, text-bytes, domains, modules, actors, ports, connections, changes, limit-classes, package-bytes, package-entries), `field-type` (table, text, unsigned-integer, identity, quantity, list, tick-length, tie-policy, actor-kind, domain/name, sha256-hex, boolean), `drop-reason` (down, too-large, loss, faulted), `link-state` (up, down).

### 4.6 Network domain (RFC 008 rule 18)

| Code | Parameters | Severity | Scope | Ends |
|---|---|---|---|---|
| `network.link-ports` | `connection: name`, `found: count` | error | field | command |
| `network.port-type` | `connection: name`, `port: name`, `type: text` | error | field | command |
| `network.attr-unknown` | `connection: name`, `key: name` | error | field | command |
| `network.attr-type` | `connection: name`, `key: name`, `expected: text` | error | field | command |
| `network.protocol-unknown` | `module: name`, `protocol: text` | warning | file | nothing |
| `network.telemetry-layout` | `module: name`, `channel: name` | warning | file | nothing |

## 5. The diagnostic log

**Rule 12.** The diagnostic log (SEC-008) is the sequence of every notification of severity `info` and above, plus the admission results, each with the wall-clock time as metadata and the run's identifier. It is written as the canonical encoding of rule 3 (and, on export, additionally as one JSON object per line), language-independent, so that any front end renders it in any language later. It never contains a sentence. It is exportable alone or inside the run record (RFC 005 rule 29).

**Rule 13.** For a determinism discrepancy (NFR-009), the log carries enough to name the first record that differs: the replay notification's interval and, on request, the per-actor chains of RFC 005 rule 24.

## 6. Catalogues

**Rule 14.** Every string a person sees in a front end comes from a catalogue: the front end's own labels, hints, and titles, and the `what`/`why`/`next` texts of every code and every enumeration value. No string a person sees is written in code. The glossary paper test's rule stands: a glossary term has one translation per language, kept in a `term` section of the catalogue and reused by reference where the catalogue format allows and by review where it does not.

**Rule 15.** The catalogue is one file per language in the front end's own format; for the browser shell, the `leptos_i18n` JSON files with named placeholders (`{{ actor }}`) and its plural forms. Keys are checked when the front end is built: a key used in code that is absent from any language, a placeholder that differs between languages, or an unused key is a build error. There is no run-time fallback to another language, because a missing translation is a defect, not a condition to tolerate.

**Rule 16.** Texts are whole sentences with named placeholders. No sentence is assembled from fragments, no word order is assumed, and no plural is formed by concatenation. Placeholders are typed by the parameter they show (rule 6); the formatting of the value belongs to the front end, not to the text.

**Rule 17.** English is the source language: the catalogue in which a key is first written and the one the glossary binds. Japanese is the second language of the first release; both ship together, and a release with a key missing in either does not build. A third language is added by adding its file; the build then requires every key in it too.

**Rule 18.** The language is chosen by the user and remembered as a per-browser convenience; the first choice is the first of the browser's languages that florn has, else English. Switching language changes every text without a reload and keeps everything the user typed and selected (verified in the proof of concept).

## 7. Numbers, units, time, and text

**Rule 19.** The logical world knows no language (RFC 001 rule 63): nothing in the Host, the trace, the manifest, or the run record is localised. Formatting happens in the front end, from typed values, with the platform's formatter for the chosen language for numbers and grouping. Unit symbols are the scenario's and are not translated in any language: the user writes `5ms` in the scenario and reads `5ms` everywhere, in English and in Japanese alike (decided by the owner at acceptance). A logical time is shown with a unit chosen from the tick length and the magnitude, and as a tick count on request. Wall-clock times exist only as metadata and are shown in the user's locale.

**Rule 20.** Text input follows RFC 005 rule 39: a front end never writes into a field while the user composes; a field is set only when the selection changes; input methods are part of the conformance kit's tests. Text the user types — names, notes — is stored as UTF-8 and shown as typed, including on the canvas, with the system's fonts.

**Rule 21.** Layout does not assume a text direction or a text length: labels may be three times longer in one language than in another, and a later right-to-left language must not require a redesign. The first release ships two left-to-right languages and tests the layout with the longest of the two.

## 8. Verification

| Property | Test |
|---|---|
| Three families | The types of the Host do not allow a guest result to become a notification or a fault, or a fault to be shown without `actor.faulted`; checked by the type system and a review of each domain RFC |
| Every code produced | For every code of §4, one test in the conformance kit produces it under both Hosts with the declared parameters and nothing else |
| Catalogue completeness | The build fails on a key missing in any language, a placeholder mismatch, or an unused key; a test adds a key to one language only and expects the failure |
| Unknown code | A diagnostic log with a code the front end does not know renders `message.unknown` with the raw code and parameters |
| Presentation | For every code of severity `error`, the front end shows `what`, `why`, and `next`, at the scope the code declares, and the word and shape of the severity |
| Formatting | A quantity, a count, a size, and an instant render per language as rule 6 says, with the unit symbol unchanged |
| Language switch | AC-012: every text changes without reload; typed text and selection remain |
| Diagnostic log | Exported alone and inside a run record; rendered in both languages from the same bytes; contains no sentence |
| Forbidden words | The catalogues contain none of the words of RFC 006 §6 in the forbidden sense |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| English sentences from the Host, translated by the front end | Untranslatable: a sentence has no structure to translate from; CTL-003 forbids it |
| Numeric codes | A number says nothing in a log or a test; a dotted name does |
| Run-time fallback to English for a missing translation | Hides a defect from the person who would notice it; a build error is seen by the person who can fix it |
| A message-format library with its own syntax in every catalogue | Named placeholders and the catalogue tool's plural forms cover the first release; a richer format is a later amendment if a language needs it |
| Loading catalogues at run time | The browser application is static and offline; compile-time catalogues are checked and cost nothing to load |
| One family of error with a flag | The three families have different lifetimes and audiences; CTL-004 |

## Prior art

| Topic | Precedent |
|---|---|
| Codes with typed parameters, text in the client | Compiler diagnostics with stable error codes; protocol error codes with structured details |
| Catalogues checked at build time | The i18n libraries of typed front-end frameworks; `leptos_i18n` as used in the proof of concept |
| What, why, next | The project's own external design §3.8, derived from UI-009 |

## Consequences for other RFCs

| RFC | Must provide |
|---|---|
| 005 | The notification type of rule 3 in the control interface's types and encoding; the diagnostic log writer |
| 002, 003, 004, 006 | Each refusal they name is produced with exactly the parameters of §4; a new refusal adds a row to §4 in the same change |
| Domain RFCs | Their guest results as WIT types (rule 1); their faulted-actor policies as `faulted-policy` values; their events' codes; their dimensions |
| External design | §3.8 and §3.12 are the presentation of this RFC |
| Conformance kit | One test per code; the catalogue checks; AC-012 |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | Whether the catalogue format needs a richer plural and gender syntax than the first two languages require | The first language that needs it |
| 2 | How the native shell loads its catalogues and shares them with the browser shell's | Phase 6, with the native shell |
| 3 | ~~Whether unit symbols get a per-language display form or stay as symbols everywhere~~ Decided by the owner at acceptance: symbols everywhere; see rule 19. | — |
