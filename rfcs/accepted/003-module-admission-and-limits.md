# RFC 003 — Module admission and resource limits

**Status.** Accepted (2026-10-14). Amended 2026-10-14 with the results of the proof of concept on snapshot and restore: rule 5 (table mutation refused), rule 23 (measured cost), open question 2. Amended 2026-10-17 by PR-06: rule 9 records the two library behaviours the wrapper corrects. Open question 4 resolved 2026-10-20 by PR-11 (the conformance kit's AArch64 runs). Open question 1 resolved 2026-10-20 by PR-13: rule 19 carries the measured defaults and maxima, rule 20 the measured ceilings.
**Tracks.** Requirements ACT-003, ACT-005, ACT-006, ACT-007, ACT-008, ACT-010, SEC-001, SEC-002, SEC-004, SEC-005, DES-006, DATA-003, DATA-005, NFR-011, NFR-013, NFR-014, AC-002, AC-005, AC-006. Requirement IDs refer to `docs/src/requirements.md`.
**Touches.** The step every module passes before it can act; the transformation that makes a module meterable, limited, and exportable; the cost model and its place in the run identity; the budgets, limits, and quotas and the faults they produce; the export and restore of guest state.
**Depends on.** [RFC 001 — Determinism contract](./001-determinism-contract.md) rules 24, 46, and 64 to 66. [RFC 002 — Guest contract](./002-guest-contract.md) for the artifact, the manifest, the admitted features, and the fault kinds. [RFC 004 — Core model](./004-core-model.md) for `[modules]`, `[limits]`, and the validation codes.
**Informed by.** Stage 0 proofs of concept 2 (deterministic limits and containment), 3 (boundary crossing cost), and 4 (instance scaling). The metering-cost plan recorded in `ROADMAP.md` is restated in §7.

## Summary

Every module passes one admission step, identical under both Hosts, before it can act: it is validated, its manifest is read, the scenario's needs are estimated, the module is transformed so that execution, stack depth, and memory are counted by florn rather than by the engine, and only then is it instantiated with exactly the imports of its contract. The transformation has a version, and so does the cost model it applies; both are part of the run identity, and the Host keeps every published version so that an old run replays under the version that produced it.

Exhausting a budget, a limit, or a quota ends the activation with a fault at the same point under every engine. The cost of this is measured and accepted: 2–4× on guest execution with the first cost model; a second cost model with a measured target is planned (§7).

## Background

Resource limits, fault containment, state export, and cross-host determinism all rest on one step that no earlier document mentioned: the Host must transform a guest when it admits it. The proof of concept built that step and measured it.

| Finding | Consequence here |
|---|---|
| Fault kind, metered work at the fault, and the guest's own counter were identical under Wasmtime, Chromium, and Firefox for every hostile guest, both builds, every backend: 108 runs, no difference. | Rule 1: one step, one implementation, both Hosts. |
| Without a stack limiter the engine's own limit decides, and it differs by engine (Wasmtime overflowed at a different depth from the injected limit). | Rule 12: deterministic stack limiting is mandatory. |
| A stack fault was indistinguishable from an ordinary trap, and after it the instance could not even answer a diagnostic call. | Rule 13: the height global is exported; classification reads globals, never calls the guest. |
| Setting the budget just before the exported function trapped every metered module during the Host's own lowering call. | Rule 10: the budget is in place before any guest code of the activation runs. |
| The memory-limit step, as first written, dropped custom sections, and the contract identity travels in one. | Rule 7: every pass preserves custom sections. |
| The library applied its stack limiter after its gas pass and miscounted; the opposite order is correct. | Rule 7: the order of passes is fixed by florn and tested. |
| Metering cost 1.9–4.0× on a protocol-style guest, 1.4–2.9× on a compute-style guest; the engine's native fuel costs 1.3–2.5× on the same guests and is not identical across engines. | §7: cost accepted now, optimisation planned, both behind one version mechanism. |
| The two global accesses that reset and read the budget cost 0.5–1 µs per activation in the browser when made through JavaScript. | Rule 11: no JavaScript value conversion on the budget path. |
| A browser renderer process holds about 125 (Chromium) or 1,000 (Firefox) instances regardless of their size. | Rule 20: capacity is probed and checked before anything is instantiated. |

## Terms

The terms of RFC 001 and RFC 002 apply. In addition:

| Term | Meaning |
|---|---|
| **Source module** | The bytes the user supplied. Its SHA-256 is the module identity (RFC 004 rule 16). |
| **Admitted module** | The bytes after transformation. The Host instantiates these. Their SHA-256 is recorded as a check value. |
| **Transformation** | The sequence of passes of §3 that turns a source module into an admitted module. It has a version. |
| **Cost model** | The rule by which executed work is counted, and the instrumentation that counts it. It has a version and a table. |
| **Metered work** | The work an activation consumed, in the cost model's units. Recorded in the trace (RFC 001 rule 46). |
| **Budget** | The metered work one activation may consume. |
| **Limit** | A bound on stack height or memory size. |
| **Quota** | A bound on what an activation may ask of the Host: effects, payload, pending timers, telemetry. |
| **Need** | A resource a module declares in its manifest that it requires (RFC 002 rule 17). |
| **Capacity** | What a Host build on a given machine can hold: resident instances, memory. Probed at start-up. |

## 1. One step, both Hosts

**Rule 1.** Every module passes the admission step before any instance of it exists. The step is one implementation in the Host core, compiled for the browser and for native targets (ARCH-009). No part of it is performed by the engine. Its output — the admitted module's bytes and the admission record — is identical under both Hosts for the same source module, the same scenario, and the same versions; the conformance kit checks this with the transformed-bytes hash.

**Rule 2.** The step has five stages, in this order. A refusal at any stage stops the step, reports every error found so far as coded messages (RFC 004 rule 38 style), and leaves nothing instantiated.

| Stage | Does | Refuses with |
|---|---|---|
| 1 Validate | Checks the source bytes against §2. | `module.*` codes of §2 |
| 2 Read manifest | Reads both custom sections of RFC 002 rule 11; checks the contract version. | `module.manifest-*`, `module.contract-*` |
| 3 Estimate | Computes what the scenario needs from this module and from every actor of it; compares with limits and capacity (§5). | `limit.below-need`, `module.initial-memory-above-limit`, `host.capacity-exceeded` |
| 4 Transform | Applies the passes of §3; validates the result; hashes it. | `admission.transform-failed` (a defect of florn, never an expected outcome) |
| 5 Instantiate | Creates each instance with exactly the imports of the world and the Host's own globals; nothing else (SEC-002). | `host.engine-refused` (a Host fault; see rule 22) |

**Rule 3.** Stages 1 to 4 run once per source module per scenario, and their result is cached by (source hash, admission version, cost-model version, limits of the actor class). Stage 5 runs once per actor. Instances of one module share its compiled form (ACT-007).

**Rule 4.** Admission allocates nothing proportional to a count declared by the module or the scenario before that count has been checked against a maximum (SEC-005, RFC 004 rule 39). The maxima are in §5.

## 2. Validation

**Rule 5.** The source module must satisfy all of the following. Each failure has a code and names what was found.

| Check | Code |
|---|---|
| Well-formed WebAssembly binary, validated with the feature set below and nothing else | `module.invalid` (with the validator's reason) |
| Size at most the Host maximum for a module | `module.too-large` |
| Features used are within: mutable globals, sign extension, non-trapping float-to-int conversion, multi-value, bulk memory, reference types. Refused: threads, shared memory, SIMD, relaxed SIMD, exception handling, tail calls, 64-bit memory, multiple memories, garbage collection, custom page sizes, and every other proposal | `module.feature-refused` (with the feature) |
| No table-mutating instruction (`table.set`, `table.grow`, `table.fill`, `table.copy`, `table.init`, `elem.drop`): tables are filled by element segments at instantiation and never change, so that restore (§6) verifies their sizes and needs no mapping of function references between instances. No compiler-produced guest uses them. | `module.table-mutation` (with the instruction) |
| Exactly one memory, defined in the module (not imported), not shared, 32-bit, exported as `memory` | `module.memory` |
| No start section | `module.start-section` |
| Every import is a function of an interface of the declared world, with the canonical ABI signature, or a global or function that a Host-supplied interface of this RFC defines; nothing else, in particular nothing under `wasi:` and nothing named like the Host's own imports of §3 | `module.import-refused` (with the import's module and name) |
| Every export the world requires is present with the canonical ABI signature: the world's functions, `cabi_realloc`, and `cabi_post_*` where RFC 002 rule 2 requires it | `module.export-missing` (with the name) |
| Counts within the Host maxima: functions, globals, tables and their declared sizes, data and element segments and their total sizes, imports, exports, custom sections | `module.too-large` (with the count that exceeded) |
| Data segments fit in the declared initial memory; the declared initial memory is at most the actor class's memory limit | `module.initial-memory-above-limit` |

**Rule 6.** The feature set of rule 5 is the one Rust's default `wasm32-unknown-unknown` target emits. Extending it (for example to tail calls, which are deterministic) is an amendment to this RFC and to RFC 002 rule 4 together, after every supported engine and the transformation library accept the feature.

### 2.1 Why no start section

A start function runs at instantiation, outside any activation, before the budget is in place. Everything a guest does must be inside an activation (RFC 002 rule 36: `init` is the first one). Rust emits none; a module that has one is refused rather than metered specially.

## 3. Transformation

**Rule 7.** The transformation applies these passes in this order, each over the output of the previous one. The order is fixed by florn, tested by the conformance kit, and part of the admission version. Every pass preserves every custom section of its input.

| # | Pass | Does | Adds to the module |
|---|---|---|---|
| 1 | Cap memory | Sets the memory's maximum to the actor class's `memory` limit, in pages. A declared maximum above the limit is lowered; below it is kept. | Nothing |
| 2 | Limit stack | Instruments every function to count stack height in the cost model's stack units and to trap when the limit of the actor class is reached. | A mutable global holding the height, exported as `florn.stack` |
| 3 | Meter | Instruments every function to charge the cost model's units and to trap when the budget is exhausted. | An imported mutable `i64` global supplied by the Host, holding the remaining budget (§4) |
| 4 | Export state | Exports every mutable global and every table under reserved names (`florn.global.<n>`, `florn.table.<n>`), so that the state of an instance can be read and restored without the guest's cooperation (§6). | Exports only |
| 5 | Validate and hash | Validates the result with the feature set of rule 5 plus the imports and exports the passes added; computes the SHA-256 of the admitted module. | Nothing |

**Rule 8.** The Host supplies the global of pass 3 and reads the global of pass 2. A source module that imports or exports a name under `florn.` or the metering import of §4 is refused at stage 1 (rule 5).

**Rule 9.** The transformation library for admission version 1 is `fvm-wasm-instrument` 0.4, wrapped by florn: florn fixes the pass order, names the globals, preserves custom sections, and tests the output against recorded hashes. The wrapper is the only code that calls the library. When the library is replaced (§7, step 3), the admission version changes. Two behaviours of the library are corrected by the wrapper and are part of admission version 1: the library keeps only one custom section of a module, so the wrapper sets every custom section aside before the passes and appends them after, in their original order; and the library leaves the indices of exported globals unshifted after it inserts the budget global at index 0, so the wrapper shifts them.

### 3.1 Admission version

**Rule 10.** The admission version is an integer, `admission = 1` for the transformation of rule 7 with the library of rule 9. It identifies the exact passes, their order, their parameters, and the library versions. It is a run input (RFC 001 rule 40, item 3) and is recorded in the run identity (NFR-011). A change to any pass, including a correction, is a new version. The Host keeps every published version.

## 4. Execution budget and the cost model

**Rule 11.** The budget of an activation is set before any guest code of that activation runs, including the Host's own call to `cabi_realloc` when it lowers arguments. The Host sets the remaining-budget global to the actor class's `budget`, calls the export, and on return reads the global; the difference is the metered work of the activation, recorded in the trace (RFC 001 rule 46). In the browser Host, the global is set and read without a JavaScript value conversion: the Host's WebAssembly code reaches it through a shared global or a WebAssembly-to-WebAssembly call. The conformance kit's crossing benchmark reports the cost of these two accesses per release; the measured cost of doing it through JavaScript was 0.5–1 µs per activation.

**Rule 12.** When the budget is exhausted the instrumentation traps. The activation ends with the fault `budget` (RFC 002 rule 41). A compound activation (RFC 002 rule 44) has one budget for all its calls: the global is set at the opening call and read at the closing call.

**Rule 13.** Stack limiting is mandatory in every admitted module. When the height limit is reached the instrumentation traps. The Host classifies a trap after an activation by reading globals, never by calling the guest, in this order: remaining budget below zero → `budget`; height global at or above the limit → `stack`; otherwise → `trap`. After a `stack` or `budget` fault the instance is discarded; its memory may be read for the diagnostic log, and it is never called again.

**Rule 14.** The cost model has a version and a table. Version 1 charges one unit per instruction, as `fvm-wasm-instrument` 0.4 counts them with a constant cost rule, charges `memory.grow` one unit per call and nothing per page (memory is bounded by its own limit), and measures stack height in the library's units (the maximum operand stack and locals of each function, summed along the call chain). The table, in full, is a few hundred bytes; it is recorded with the version in the run record, so that a run can be interpreted when the version is no longer current (ACT-010).

**Rule 15.** The cost-model version is a run input and part of the run identity. The Host keeps every published version. A run recorded under version 1 replays under version 1, whatever the current default. The user interface shows metered work as a fraction of the budget; the unit is internal.

### 4.1 Why count by instrumentation

| Alternative | Why not |
|---|---|
| The engine's own fuel (Wasmtime) | Not available in browsers; where it exists its points of exhaustion are the engine's, not florn's, so a fault would occur at different places under different engines. Measured natively: 1.3–2.5× on the same guests, not materially cheaper. |
| A wall-clock watchdog | Whether a fault occurs would depend on the speed of the machine (RFC 001, alternatives). Observed without metering: the Host hangs with the guest. A watchdog exists only against defects of the Host itself and never decides a fault (ACT-006). |
| Instrumentation that differs per Host | Two implementations, two counts, two digests. The point of one step is one count. |

## 5. Limits, quotas, and capacity

**Rule 16.** These bounds exist from release 0.1. Each has a Host maximum per Host build, published in the support matrix; a scenario may only lower it (RFC 004 rule 15); a module's declared need must fit under the scenario's value (RFC 002 rule 17).

| Name | Bounds | Unit | Enforced by | On exceeding |
|---|---|---|---|---|
| `budget` | Metered work per activation | Cost-model units | Instrumentation (§4) | Fault `budget` |
| `stack` | Stack height | Cost-model stack units | Instrumentation (§3) | Fault `stack` |
| `memory` | Linear memory of the instance | Pages, written in bytes (`"4MiB"`) | The memory's maximum (§3, pass 1) | `memory.grow` returns −1 to the guest; no fault. The machine's refusal of memory within the limit is a Host fault (RFC 001 rule 65) |
| `effects-per-activation` | Effects recorded in one activation | Count | The import handlers | Fault `quota` |
| `payload` | Size of one `send` payload | Bytes | The `send` handler | Fault `quota` |
| `pending-timers` | Timers pending after commit | Count | The timer handlers, at commit | Fault `quota` |
| `telemetry-per-activation` | Sum of `report` payloads in one activation | Bytes | The `report` handler | Fault `quota` |

**Rule 17.** A quota is enforced inside the Host function the guest called: the handler refuses, the refusal becomes the activation's fault, and every effect of the activation is discarded (RFC 001 rule 24). Natively the handler returns an error; in the browser it throws; both arrive as the fault, measured identical.

**Rule 18.** The microstep and event limits of RFC 001 rule 13 are kernel limits on the run, not on an actor; they are declared in `[order]` (RFC 004) and are outside this RFC.

**Rule 19.** The defaults and Host maxima, measured in Phase 1 by the conformance kit (open question 1, resolved 2026-10-20). The measurements: the protocol-style reference activation (B1) consumes 122,000 cost-model units and the compute-style one (B2) 10,150,000; the echo guest's 2,300; a metered unit costs 0.06 to 0.26 ns across the engines, so the default budget bounds one activation to about 6 to 26 ms and the maximum to about a quarter of a second; the kit's guests need a stack height of at most 47; a Rust guest's memory starts at 1.1 MiB and the heap guest reaches 3.3 MiB; the effect flood faults after 17,000 units; a 64 KiB payload costs 2 µs to cross and 5 µs (native) to 80 µs (browser) to digest.

| Name | Default | Host maximum, browser | Host maximum, native |
|---|---|---|---|
| `budget` | 100,000,000 | 1,000,000,000 | 1,000,000,000 |
| `stack` | 1,024 | 65,536 | 65,536 |
| `memory` | 4 MiB | 64 MiB | 1 GiB |
| `effects-per-activation` | 1,000 | 100,000 | 100,000 |
| `payload` | 64 KiB | 16 MiB | 16 MiB |
| `pending-timers` | 64 | 65,536 | 65,536 |
| `telemetry-per-activation` | 16 KiB | 16 MiB | 16 MiB |
| Module size (rule 5) | — | 32 MiB | 32 MiB |

**Rule 20.** Capacity. At start-up the Host probes what it can hold: the number of instances it can create (the browser renderer's ceiling, about 125 in Chromium and 1,000 in Firefox, independent of instance size; the conformance kit's probe measured 123 and 998 in Phase 1, and 5,000 in WebKit without reaching a ceiling, at 50, 114 and 1,372 µs per instance; the browser Host assumes 120, 990 and 990 since a probe at start-up would use up what it measures; every browser reserved a 4 GiB memory; natively 20,000 instances of the echo guest in one store at 5.3 µs each, the probe's own bound) and the memory it can reserve. Stage 3 compares the scenario's resident need — one instance per guest-backed actor, each with its memory limit — with the capacity, and refuses with `host.capacity-exceeded` naming the need and the capacity before anything is instantiated (WEB-007). From release 0.3 the Host may hold more actors than resident instances by swapping their state (§6); the check then applies to the resident set.

## 6. State export and restore

**Rule 21.** The state of an instance is its linear memory, its mutable globals, and its tables; nothing else in core WebAssembly holds state. Pass 4 of §3 makes all of it reachable. The Host exports the state of an actor only between activations: the memory bytes up to the current size, every mutable global, every table, and the kernel's own state for the actor (pending timers, sequence number, random stream position). The Host-owned globals of passes 2 and 3 are not part of the state; they are reset on restore.

**Rule 22.** Restore creates a fresh instance from the same admitted module (same admitted hash), grows its memory to the saved size, writes the bytes, sets the globals, fills the tables, and restores the kernel state. Execution then continues with the same trace as if it had never been interrupted (RFC 001 rule 52, ACT-008). Before restoring, the Host checks the source hash, the admission version, and the cost-model version against the saved values (DATA-005) and refuses a mismatch with `checkpoint.mismatch`.

**Rule 23.** Export is a capability of every admitted module from release 0.1. Checkpoints that use it are a release 0.3 feature; the capability exists first so that no earlier decision precludes it. Measured in Stage 0 with two guests (one keeping state in memory, globals, and a function table; one a Rust protocol-style guest) under Wasmtime, Chromium, and Firefox: a restored run's trace is identical to an uninterrupted one, across a process or worker restart and after moving backward; export costs 0.04–0.31 ms and restore 0.12–0.32 ms per megabyte of memory at the median; the pass adds no measurable cost to activations; the admitted bytes are identical under both Hosts for the fixed pass order. A restore instantiates from the cached compiled module and never compiles (RFC 005 rules 9 and 10); without the cache a native restore cost 17 ms. Swapping an actor's state through a resident instance — restore, one activation, export — costs 50–60 µs at 131 KB of state, about 0.4 µs per KB, in every engine.

## 7. Known cost and planned optimisation

This section exists because the owner accepted the measured cost on condition that the plan is recorded where it cannot be missed. The plan is also in `ROADMAP.md`; this is its binding statement for the admission step.

**Measured, Stage 0.** With cost-model version 1 the metering costs 1.9–4.0× on a protocol-style guest (one activation parses a 1,500-byte buffer, updates a 1,000-entry ordered map, and builds a reply) and 1.4–2.9× on a compute-style guest (one activation multiplies two 64×64 matrices), across Wasmtime, Chromium, and Firefox. In absolute terms: 30–100 µs per activation of that size, against a kernel cost of about 2.5 µs per activation in the browser. For the first release's workload — protocol logic, hundreds of actors — this is accepted.

| Step | When | What |
|---|---|---|
| 1 | This RFC | The admission step and cost-model version 1 are specified as built and measured. The version and its table are run inputs. |
| 2 | Phase 1, release 0.1.0 | The conformance kit carries a cost benchmark: the protocol-style and compute-style guests above, 10,000 activations, run-only time, metered against unmetered, on every supported engine. Its figures appear in the notes of every minor release. |
| 3 | Phase 3, release 0.3.0 | florn's own metering pass replaces the library pass as cost-model version 2: charge per function entry and per loop back-edge with statically summed straight-line costs, the counter held in a local between checks, one global write per check. Target: at most 1.5× protocol-style and 2× compute-style. Runs recorded under version 1 replay under version 1. |
| 4 | Phase 3 review | If the target is missed, the owner decides again with the measured figures. If it is met, version 1 is kept for replay only and new runs default to version 2. |

**Rule 24.** Nothing in this RFC changes between the two cost models except the pass of §3 step 3 and the versions of §3.1 and §4. The engine adapters, the kernel, the trace, and the contracts are unaffected; that is why the cost can be accepted now without foreclosing the optimisation.

## 8. The run identity and the diagnostic log

**Rule 25.** Admission contributes these to the run inputs and the run record.

| Item | Where | Why |
|---|---|---|
| Source hash of every module | Run inputs (RFC 001 rule 40, item 2) | Module identity |
| Admission version | Run inputs (item 3) | The transformation decides fault points |
| Cost-model version and its table | Run inputs (item 3) and the run record | Metered work is in the trace |
| Limits of every actor class, as resolved | Run inputs (part of the scenario hash, RFC 004) | They decide fault points |
| Admitted hash of every module | Run record, as a check value | Detects a divergent transformation between Hosts |
| Admission results, refusals, capacity probe | Diagnostic log (SEC-008) | What was admitted, what was refused, and why |

**Rule 26.** `host.engine-refused` — the engine rejects an admitted module that florn validated — is a Host fault of the kind RFC 001 rule 65 describes: it ends the run, and the run record states that the condition belongs to the environment. It is expected never to occur; the conformance kit runs every admitted module of the kit under every supported engine to keep it that way.

## 9. NaN inside guests

**Rule 27.** The transformation does not canonicalise floating-point values inside a guest. Three engines on x86-64 agreed on every NaN bit pattern probed, including payload propagation; the known difference is the sign of the default NaN between x86-64 and AArch64. Values that cross the contract are canonical (RFC 002 rule 9); a guest that inspects NaN bits inside its own memory is outside the promise, and the SDK says so. This answers RFC 001's open question 1 for the contract and the trace; the cross-architecture case is confirmed when an AArch64 run exists. Canonicalising inside guests by instrumentation would cost on every floating-point instruction and would make native x86-64 disagree with browsers on x86-64, which no engine canonicalises.

## 10. Verification

Each property has a test in the conformance kit, run under both Hosts (AUTO-005, AUTO-006).

| Property | Test |
|---|---|
| Same fault point everywhere (AC-002) | The hostile guests — endless loop, unbounded recursion, memory growth, effect flood, bad pointer — fault with the same kind, the same metered work, and the same guest-side counter under every supported engine; a benign neighbour's records are unchanged by any of them; the Host handles events after each fault. |
| Transformed bytes | The admitted hash of every kit module is identical under both Hosts and equals the recorded value for the admission version. |
| Budget before lowering | A guest whose `cabi_realloc` loops faults as `budget`, not as a Host error. |
| Classification | A stack fault is reported as `stack`, a budget fault as `budget`, a plain `unreachable` as `trap`, without calling the guest after the fault. |
| Custom sections | The `component-type` and `florn.manifest` sections of the source module are present and byte-identical in the admitted module. |
| Validation | A module with a start section, with a second memory, with a `wasi:` import, with a SIMD instruction, with an import named `florn.stack`, and with a declared initial memory above the limit: each is refused with its code. |
| Quotas | Each quota of §5 is exceeded by one and faults as `quota` naming the quota; one below passes. |
| Memory | A guest that grows past its limit receives −1 at the same page count everywhere and does not fault. |
| Estimate before instantiate | A scenario declaring 10⁷ actors is refused by `host.capacity-exceeded` in bounded memory with nothing instantiated. |
| Capacity probe | The headless runner and the browser Host print the probed capacity at start-up. |
| Export and restore (AC-006) | A guest with a growing heap, a counter in a global, and a function table used indirectly: export after activation 5,000 into a fresh instance and continue; export to bytes, restart the process or page, restore and continue; export, continue, restore again and continue (move backward). All equal the uninterrupted digest over the corresponding records. |
| Versions | A run recorded under cost-model version 1 replays under version 1 when version 2 is the default; a checkpoint from another admission version is refused with `checkpoint.mismatch`. |
| Cost benchmark (§7, step 2) | The protocol-style and compute-style figures are produced and published per minor release. |
| NaN probe | The probe guest reports identical bits under every engine on one architecture; the AArch64 result is recorded when CI provides it. |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Engine-native fuel natively, instrumentation in the browser | Fault points differ between Hosts (option D of the proof of concept). |
| No metering; a wall-clock watchdog | The Host hangs with the guest; whether a fault occurs depends on the machine (option E). |
| Write florn's own metering pass before release 0.1 (option B) | Delays Phase 1 for a cost that the first workload accepts; the same version mechanism admits it later (§7). |
| Metering through a Host function call per charge | Measured at 34–500×. |
| `wasm-instrument` as the library | Cannot read the default Rust target (bulk memory). |
| Canonicalising NaN inside guests | §9. |
| Admission in the engine (Wasmtime's own limits, the browser's own validation) | Two implementations; limits would be the engine's. The engine still validates, as a second check. |
| Trusting a module's declared needs without estimating | A million-actor manifest must be refused before a million of anything exists (SEC-005). |
| Signed or trusted modules | SEC-010: trust states are *validated* and *rejected*; provenance is displayed, not enforced. |

## Prior art

| Topic | Precedent |
|---|---|
| Gas metering by instrumentation | Blockchain virtual machines, which must count identically on every validator; the library used is from that lineage |
| Stack-height instrumentation | The same |
| Deterministic WebAssembly | The guidance on deterministic execution published by the Wasmtime project |
| State as memory, globals, and tables | The WebAssembly core specification's definition of an instance's mutable state |

## Consequences for other RFCs

| RFC | Must provide |
|---|---|
| 002 | Rule 3 and rule 4 of RFC 002 are enforced here; the fault kinds of its rule 41 are produced here. |
| 005 — Host architecture | The instance pool and the capacity probe; the engine adapter's deterministic settings (no threads, no relaxed SIMD, store limits, the Host watchdog); where the budget global lives in the browser Host; the admission cache; the diagnostic log; the kernel state exported with an actor. |
| 006 — Security baseline | The threat model cites this step as the boundary: everything a guest can do is bounded by §5, and nothing outside its world reaches it. |
| 007 — Error model | Codes and text for the refusals of §2 and §5, the fault kinds, `host.capacity-exceeded`, `checkpoint.mismatch`, and `host.engine-refused`. |
| 001 | Open question 1 answered as in §9 for the contract and the trace; open question 5 (is metered work compared under Tier 2?) can now be answered yes, because the count is identical under every engine by construction. |
| Requirements | NFR-013 receives the Stage 0 figures: about 2.5 µs kernel cost per activation in the browser; 30–100 µs metering cost per activation of a protocol-style guest; the browser instance ceilings. |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | ~~The defaults and Host maxima of rule 19.~~ Resolved 2026-10-20 by the conformance kit's figures (PR-13): the table of rule 19 with its measurements; the budget default rose from 10,000,000 to 100,000,000 because the compute-style reference activation needs 10,150,000. | — |
| 2 | ~~The cost of the export pass and of export and restore (rule 23).~~ Resolved 2026-10-14 by the proof of concept on snapshot and restore; see rule 23. | — |
| 3 | Whether the browser Host reaches the budget global through a shared global or through a WebAssembly-to-WebAssembly call (rule 11), and the measured saving. | RFC 005, with the crossing benchmark. |
| 4 | ~~The sign of the default NaN on AArch64 and its consequence for a continuous domain's tolerance (§9).~~ Resolved 2026-10-20 by the conformance kit's AArch64 runs in CI: under Wasmtime the default NaN of `0/0` and `inf − inf` is positive (`0x7ff8…`) on AArch64 and negative (`0xfff8…`) on x86-64; `sqrt(−1)` through the SDK's `libm` and `−NaN × 1` agree on both. A continuous domain's comparison must treat every NaN as one value, as §9 does at the contract and in the trace; a guest that reports raw NaN bits on telemetry is architecture-dependent. | — |
| 5 | Whether `host.capacity-exceeded` is a refusal or an offer to swap (rule 20) once swapping exists. | Phase 3, with the owner. |
