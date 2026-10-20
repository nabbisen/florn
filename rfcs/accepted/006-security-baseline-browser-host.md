# RFC 006 — Security baseline for the browser Host

**Status.** Accepted (2026-10-14). Open question 1 decided at acceptance: MPL-2.0 and every other copyleft licence are refused outright (rule 6). Section 7 carries the exact maxima since 2026-10-20 (PR-13).
**Tracks.** Requirements SEC-001 to SEC-012, WEB-002, WEB-005, WEB-006, WEB-009, WEB-010, WEB-012, ACT-003, ACT-005, ACT-006, ACT-010, DATA-003, DATA-007, MIG-006, MIG-008, AUTO-005. Requirement IDs refer to `docs/src/requirements.md`.
**Touches.** The threat model of the browser Host, scoped to what is built; the controls that answer it, each pointing at the RFC that specifies the mechanism; what florn deliberately does not claim; the security review that every release performs; the supply-chain policy.
**Depends on.** [RFC 002](./002-guest-contract.md) (what a guest can reach), [RFC 003](./003-module-admission-and-limits.md) (admission and limits), [RFC 004](./004-core-model.md) (manifest and package identity), [RFC 005](./005-host-architecture.md) (the Host, the control interface, hosting), and the external design (`docs/src/external-design.md`, §1, §3.9, §6).

## Summary

florn runs untrusted executable code on one person's machine, inside a browser. The baseline is small and it is entirely built: a guest can reach nothing but the functions of its contract; every module passes one admission step that bounds what it can consume; nothing leaves the machine unless the user exports it, and the application contacts no origin but its own; every module and scenario is identified by the hash of its bytes; the application is served from an immutable versioned address under the isolation headers; and the dependencies are permissive and checked on every build.

This RFC states that model as threats, controls, and the things florn does not claim, so that the next release can check whether a change added a data flow, an integration, or a capability — and update the model rather than assume it.

## Background

The earlier draft modelled a managed, multi-user product: seven human roles, an authorization matrix, module signing, audit retention, and a native capability broker. The owner removed all of it from the baseline: florn is a single-user local tool with no identity to bind roles or signatures to (SEC-010), and a document that says "signed", "role", or "audit" without a key holder or an identity makes a claim the product cannot honour. What remains is the part that is real, and the proofs of concept measured each mechanism: the strict Content Security Policy under which a core module runs with no generated script; the fault points that are identical under every engine; the instance ceiling the browser imposes; the headers that isolation needs; and the empty network log.

## Terms

The terms of the requirements glossary apply. In addition:

| Term | Meaning |
|---|---|
| **Asset** | Something whose loss, alteration, or disclosure would harm the user: their data, their runs' integrity, their machine, the application's availability. |
| **Trust boundary** | A line across which data or control passes from a less trusted party to a more trusted one, and where validation happens. |
| **Trust state** | What florn knows about a module: *validated* (admitted) or *rejected*. Nothing else (SEC-010). |
| **Threat** | A way an actor could harm an asset across a boundary. |
| **Control** | What florn does about a threat, specified in a named RFC and verified by a named test. |

## 1. Scope and assumptions

**Rule 1.** This RFC covers the browser Host and its user interface as built for releases 0.1 to 0.5, and the native headless Host where §9 says so. It does not cover the native graphical application (release 0.6) or any native capability that touches real resources (MIG-006); those get an RFC of their own before they exist.

**Rule 2.** Assumptions. The user's machine, operating system, and browser are trusted; an attacker who already runs code as the user is outside the model. The browser's same-origin policy and its WebAssembly sandbox are the platform's boundaries and are assumed to hold. The user is one person who does not attack themself.

**Rule 3.** The model is scoped to what is built. A release that adds a data flow (a new file format, a new transport), an external integration (a new origin, a native resource), or a capability (a new Host function a guest can call) updates this RFC in the same release (SEC-011, roadmap release checklist).

## 2. Assets

| Asset | Harm to avoid | Owner of the control |
|---|---|---|
| The user's scenarios, modules, run records, and telemetry | Leaving the machine without the user's action; being read by a guest | §5 controls C1, C7, C8 |
| The integrity of a run | A result that differs from what the inputs determine, silently; a run that cannot be reproduced | C9, C10, C11 |
| The availability of the Host and the browser | A guest or a scenario that freezes the Host, the page, or the browser | C2, C3, C4, C5 |
| The user's machine | A guest or the application reaching outside the browser's sandbox | C1, the browser |
| The integrity of the application itself | Running an altered application or an altered dependency | C12, C13 |

## 3. Trust boundaries

```text
   Z5 distribution origin ──(HTTPS, immutable version path)──► Z1 shell (main thread)
                                                                   │ control interface: validated commands
   Z0 inputs: modules, manifests, packages, run records,           ▼
      checkpoints, files chosen by the user ──(validation)──► Z2 Host (dedicated worker)
                                                                   │ admission, imports of the world only
                                                                   ▼
                                                              Z3 guest instances (WebAssembly sandboxes
                                                                   inside the Host's worker)
   Z4 origin storage (a cache) and files on disk ◄──(export preview)── Z1 / Z2
```

| Zone | Holds | Trust | What crosses into it is validated by |
|---|---|---|---|
| Z0 — inputs | Everything the user opens or imports | Untrusted | The Host's parsers and the admission step (C5, C6) |
| Z1 — shell | The user interface on the main thread | Trusted code; untrusted as a source of commands | The Host validates every command (C14) |
| Z2 — Host | Kernel, admission, domains, control interface, in a worker | Trusted; the control boundary | — |
| Z3 — guests | Instances of admitted modules, in the same worker as the Host | Untrusted | The WebAssembly sandbox, the contract, admission, the limits (C1 to C4) |
| Z4 — storage | Origin storage and the user's files | The user's data | Export is explicit; storage is a cache (C7) |
| Z5 — origin | The address the application is served from | Trusted for the version it serves; the only remote party | Immutable version paths over HTTPS; hashes in the release notes (C12) |

Guests run inside the Host's worker, not in a worker of their own: the boundary between Z2 and Z3 is the WebAssembly sandbox plus the admission step, not process isolation. This is stated so that nobody assumes more.

## 4. Threat actors

| Actor | Can | Cannot |
|---|---|---|
| **T1 — malicious module author** | Supply a module that misbehaves in every way WebAssembly allows: loops, recursion, memory growth, bad pointers, floods, malformed sections, forbidden imports, misuse of the contract | Run anything but WebAssembly under the contract; see or touch anything outside its own memory |
| **T2 — malicious scenario or package author** | Supply a manifest or package that is huge, malformed, self-contradictory, or that references a module by a false hash | Make the Host allocate before validating; run a module whose bytes do not hash as declared |
| **T3 — malicious artifact source** | Hand the user a file that claims to be something else | Defeat identity by hash; make a tampered file admit as the original |
| **T4 — hostile or compromised origin, or the network between** | Serve an altered application | Alter a version already cached by the user's service worker; hide from the hashes in the release notes |
| **T5 — malicious or defective dependency** | Enter the build through the Rust ecosystem | Pass the licence and advisory checks unnoticed; add script at run time (there is none) |

Retired from the earlier draft: the over-privileged user (there is one user and no privilege to escalate) and the external network adversary (the browser Host has no network).

## 5. Threats and controls

Each control names the RFC that specifies the mechanism and the test that verifies it.

| # | Threat | Control | Specified in | Verified by |
|---|---|---|---|---|
| C1 | A guest reaches outside its sandbox: files, network, clock, DOM, Host memory | A guest imports only the functions of its world; every other import is refused at admission; no ambient interface exists; the Host lifts values from the guest's memory with range checks and never hands a reference into Host memory | RFC 002 rules 3, 8; RFC 003 rule 5 | Hostile guests with a WASI import and with out-of-range pointers are refused or faulted; AUTO-005 |
| C2 | A guest consumes unbounded execution | Execution budget per activation, counted by instrumentation identically under every engine | RFC 003 §4 | AC-002: the same fault point under every engine |
| C3 | A guest consumes unbounded stack or memory | Stack limiter; memory maximum set at admission; the engine's refusal of growth is returned to the guest, never a crash | RFC 003 §3, §5 | AC-002; the memory-growth guest |
| C4 | A guest floods the Host with effects, payloads, timers, telemetry | Quotas enforced in the import handlers; a violation is the activation's fault and its effects are discarded | RFC 003 rule 17 | The flood guest; each quota exceeded by one |
| C5 | A malformed module crashes the Host or makes it allocate without bound | Validation before anything; size and count maxima; no allocation proportional to a count before the count is checked | RFC 003 §2, rule 4 | Fuzzing of the validator and the lowering code; a module declaring absurd counts is refused in bounded memory |
| C6 | A malformed scenario, package, run record, or checkpoint does the same | Parsers with coded refusals; limits checked before allocation; a checkpoint is restored only when module hash, admission version, and cost model match | RFC 004 §7, rule 39; RFC 003 rule 22; RFC 005 rule 29 | Fuzzing of the manifest and run-record parsers; a manifest with 10⁷ actors refused in bounded memory |
| C7 | Data leaves the machine | No network interface exists in the Host or in guests; the application contacts no origin but its own; export is explicit, previewed, and a file the user hands over; there is no URL that carries scenario content | RFC 005 rules 30, 34; external design §3.9 | AC-013: the network log holds no other origin; the export preview matches the file |
| C8 | A guest exfiltrates through telemetry or the trace | Telemetry goes only to the Host and the display; the trace and the diagnostic log stay on the machine until exported with the preview of C7 | RFC 002 §5.5; RFC 005 §5 | Covered by C7 |
| C9 | A module is replaced by another with the same name | Identity is the SHA-256 of the bytes, in the manifest, the package, and the run identity; a package whose module bytes do not hash as declared is refused; the interface shows the hash | RFC 004 rules 16, 34; RFC 001 rule 40 | A renamed module file fails `module.hash-mismatch` |
| C10 | A run's result is altered or misattributed | The run record is never an input to execution; replay re-executes from the inputs and reports divergence; the run identity carries every input and the environment | RFC 001 rules 41, 49, 50 | AC-003 |
| C11 | State leaks between runs or actors | Every actor has its own linear memory; a new run instantiates fresh; restore creates a fresh instance | RFC 002 rule 39; RFC 003 §6; ACT-003 | The checkpoint tests; the isolation test of the conformance kit |
| C12 | The user runs an altered application | Each version at an immutable address over HTTPS; the application never updates itself; the release notes list the SHA-256 of every served file; the service worker caches what was served and the interface shows the version and its origin | RFC 005 rule 35; external design §6 | A release checklist item; the version shown matches the release notes |
| C13 | A dependency brings copyleft, a known vulnerability, or run-time script | Permissive licences only, checked on every build; advisories checked on every build; versions pinned; no script generated at run time and no third-party script in the application, not even for the hosting workaround | SEC-012; RFC 005 rule 44; OD-12 | `cargo deny check` and `cargo audit` in CI; the policy forbids inline and generated script (POC-1 M5) |
| C14 | The shell, or something it renders, acts on the Host without validation | Every command is validated by the Host; an invalid one changes nothing; only `intervene` changes the logical world and is recorded | RFC 005 rules 25, 27 | Each command with invalid parameters produces its code and no change |
| C15 | Text from a module or a scenario is interpreted as markup or script in the shell | The shell renders every string from a manifest, a module, or a file as text; the policy forbids inline script; canvas labels are drawn as text | RFC 005 rule 34; the framework's escaping | A scenario whose actor name is `<script>` and a manifest whose description is markup render as text |
| C16 | A scenario exceeds what the browser can hold and the page dies | Capacity is probed at start-up and checked before anything is instantiated; a warning precedes a limit; instantiation failures are Host faults, never crashes | RFC 003 rule 20; RFC 005 rules 10, 18 | A scenario beyond capacity is refused with the numbers; POC-4's failure modes |
| C17 | Shared memory exposes the page to cross-origin timing attacks | The isolation headers that shared memory requires are the browser's mitigation, and the application loads nothing cross-origin | RFC 005 rule 33 | The headers are present on every served and cached response |

## 6. What florn does not claim

**Rule 4.** florn makes none of these claims, and no document, label, or message may suggest them.

| Not claimed | Why | What the user sees instead |
|---|---|---|
| Modules are signed, verified, or trusted | No identity to bind a signature to; no key holder | The hash and the origin the user supplied; the trust states *validated* and *rejected* |
| Actions are audited | No identity to attribute them to | A diagnostic log of what the Host did, for diagnosis |
| Roles or permissions | One user | Nothing |
| Protection against an attacker on the user's machine or in the user's browser | Outside the model (rule 2) | Nothing |
| Isolation of guests from each other beyond the WebAssembly sandbox | Guests share the Host's worker | The statement in §3 |
| Confidentiality of exported files | Once exported, a file is the user's to protect | The export preview says what the file contains |
| Any security property of the native graphical application or of native resource access | Not built; separate RFC | Nothing |

The words *signed*, *audit*, *role*, *permission*, and *trusted module* do not appear in the user interface or in the documentation for these releases.

## 7. Input validation maxima

The maxima below bound every allocation that untrusted input could size. They are Host constants, published in the support matrix, and a scenario may only lower the ones RFC 004 exposes.

| Input | Maximum | Where checked |
|---|---|---|
| Module file | 32 MiB; 100,000 functions, 10,000 globals, 16 tables with 1,000,000 elements, 10,000 segments with 32 MiB of data, 1,000 imports, 1,000 exports, 64 custom sections | RFC 003 rule 5, before parsing further |
| Manifest text | 4 MiB; 16 domains, 10,000 modules, 100,000 actors, 4,096 ports per actor, 1,000,000 connections, 4,096 ports per connection, 100,000 scheduled changes, 1,024 limit classes, names of 256 bytes; counts checked before the elements are parsed | RFC 004 rule 39 |
| Package | 10,256 entries, 2 GiB in all, a module entry 32 MiB; entries hashed as they are read | RFC 004 §6 |
| Run record | 1,000,000 entries, an entry 1 GiB, 4 GiB in all; a chunk is parsed only after its declared size is checked | RFC 005 rule 29 |
| Checkpoint | Memory size at most the actor's memory limit; global and table counts as the layout declares | RFC 003 rule 22 |
| Payload, telemetry, effects, timers | The quotas of RFC 003 §5: 64 KiB, 16 KiB, 1,000 and 64 by default; 16 MiB, 16 MiB, 100,000 and 65,536 at most | The import handlers |
| Control-interface message | Unknown fields refused; a trace window of at most 10,000 records | RFC 005 rule 27 |

The values were set with the defaults of RFC 003 rule 19 in Phase 1 (2026-10-20, PR-13) and are the constants of the model, admission, record and control crates.

## 8. The security review

**Rule 5.** Every release performs the review below before it is tagged (roadmap release checklist, item 2). The architect performs it; the verifier runs the tests; the owner sees the result in the release notes.

| Step | Question | Evidence |
|---|---|---|
| 1 | Did this release add a data flow, an external integration, or a capability? | The diff of the control interface, the contracts, the file formats, the origins, the Host functions |
| 2 | If yes, is this RFC updated in the same release? | The RFC's change note |
| 3 | Do the controls of §5 still hold? | The named tests pass on every supported engine |
| 4 | Do the dependencies pass? | `cargo deny check licenses advisories bans sources` and `cargo audit` clean; the allow-list of rule 6 |
| 5 | Did fuzzing run since the last release? | The fuzz targets of rule 7 ran for the agreed time without a crash, or the crashes are fixed |
| 6 | Does any document or label make a claim of §6? | A search for the forbidden words |
| 7 | Are the served files' hashes in the release notes? | The release notes |

**Rule 6.** Licence allow-list for anything linked into florn: Apache-2.0, Apache-2.0 WITH LLVM-exception, MIT, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, 0BSD, Unicode-3.0, Unicode-DFS-2016, CC0-1.0. Anything else fails the build, including MPL-2.0 and every other copyleft licence, weak or strong; a crate offering a choice of licences passes if one of its choices is on the list. An exception is an owner decision recorded as an amendment of this rule, never a per-crate override in the build configuration. Measured at acceptance: the Stage 0 dependency trees (324 crates in the browser interface stack, 168 in the core and engine stack, 331 in the telemetry stack) contain no copyleft crate, so the rule costs nothing today; the crates it is known to exclude (`colored`, `option-ext` through `dirs`, `resvg` and `usvg`) all have permissive alternatives. Test-only tools (Playwright and its browsers) are not linked and are outside the list.

**Rule 7.** Fuzz targets, run with `cargo-fuzz` by the verifier: the module validator; the admission transformation on valid modules (the output must validate); the canonical ABI lifting of every type of the subset; the manifest parser; the package reader; the run-record and checkpoint readers; the control-interface decoder. Mutation testing (`cargo-mutants`) measures whether the hostile-guest tests would notice a removed check.

## 9. The native headless Host

**Rule 8.** The native headless Host of release 0.1 has the same baseline with these differences: there is no hosting (§3 Z5 does not exist; the binary is a release artifact with its hash in the release notes); the Host reads and writes the files the user names on the command line and nothing else; there is no shell, so C14 and C15 apply to the command line's parsing instead. It grants no capability outside the virtual world (MIG-006, MIG-008): until the RFC that defines capability profiles exists, the native Host has no file, network, or device access that the browser Host does not have, except the files named on its command line.

## 10. Verification

| Property | Test |
|---|---|
| Every control of §5 | Its named test, in the conformance kit, under both Hosts and every supported engine |
| No other origin | A full browser session with every feature used; the network log holds only the application's origin (AC-013) |
| No generated script | The policy of RFC 005 rule 33 reports no violation in a full session; the served files contain no `eval`, `new Function`, or `blob:` script |
| Maxima | For each input of §7, an input one above the maximum is refused in bounded memory and time; one below is accepted |
| Forbidden words | The documentation and the catalogues contain none of the words of §6 in the forbidden sense |
| Dependencies | `cargo deny` and `cargo audit` pass in CI on every push |
| Fuzzing | Each target of rule 7 runs in CI for a fixed time per release |
| Trust states | The interface shows *validated* or *rejected* and nothing else for a module |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| A worker per guest for process-level isolation | The browser's instance ceiling and memory are per renderer process anyway; a worker per actor costs far more than the WebAssembly sandbox adds; and the Host would cross a message boundary per activation (0.2–0.3 µs becomes tens of µs). Stated as not claimed instead. |
| Module signing and a trust store | No identity, no key holder (SEC-010). A tampered module is already a different module by hash. |
| Encryption of origin storage | The browser's storage is already the user's; a secret on the same machine protects nothing against the attacker of rule 2. Files are the durable form and the user's to protect. |
| A permission dialog for guest capabilities | There are no capabilities to grant in the browser Host. When native capability profiles exist, their RFC designs the grant. |
| Subresource integrity attributes on the application's own files | Useful against a compromised origin only if the page itself is intact; the page is served from the same origin. The hashes in the release notes and the immutable path are the control; SRI may be added without an RFC. |
| Allowing MPL-2.0, by default or per crate | Weak copyleft; the owner's policy is permissive only, and no crate florn links needs it. Refused outright; an exception would amend rule 6. |

## Prior art

| Topic | Precedent |
|---|---|
| WebAssembly as a sandbox for untrusted code | The design of WebAssembly itself; the deterministic-execution guidance of the Wasmtime project |
| Threat modelling by assets, boundaries, actors, and controls | STRIDE-style modelling, reduced to the threats the architecture actually has |
| Local-first applications that contact no third party | The project's own requirements, enforced by a Content Security Policy |
| Licence and advisory gates in CI | `cargo-deny`, `cargo-audit`, as used across the Rust ecosystem |

## Consequences for other RFCs

| RFC | Must provide |
|---|---|
| 007 — Error model | Codes for every refusal named in §5 and §7, and the catalogue rule that none of the words of §6 appears |
| 003 | The maxima of §7 next to its defaults (rule 19) |
| 005 | The release-notes hashes (C12); the fuzz targets' crates |
| Native capability profiles (future RFC, before 0.6 or MIG-006) | The capability broker, the grant, and the update of this model |
| External design | §3.9's trust states and the export preview are this RFC's C7 and C9 |
| ROADMAP | Release checklist item 2 cites this RFC's §8; S0-19's CI runs the checks of rule 5 steps 4 and 5 |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | ~~Is MPL-2.0 acceptable per crate, or refused outright?~~ Decided by the owner at acceptance: refused outright; see rule 6. | — |
| 2 | Whether the served files' hashes are also embedded as subresource integrity attributes | Phase 2, with the hosting |
| 3 | The fixed fuzzing time per release | The verifier, in Phase 1 |
