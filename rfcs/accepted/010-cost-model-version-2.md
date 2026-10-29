# RFC 010 — Cost model version 2: florn's own metering pass

**Status.** Accepted 2026-10-29, the owner's word on the day it was proposed, as proposed; open question 1 waits for the figures of the implementing pull request. Proposed 2026-10-29 for release 0.3.0 (Phase 3 of `ROADMAP.md`, "Metering cost", step 3). Implementation: Phase 3, release 0.3.0, by the handoff package `rfcs/handoffs/009-depth/`. Implementation notes, 2026-10-29, PR-24: the charges are made at the metered blocks of version 1 (their partition reproduced from the library, `end` and `else` free as there) into the local of rule 2; the checks of rule 3 are before calls (which covers the callee's entry, so no separate entry check), before a branch to a loop, before `return` and at the end. An activation that completes is metered as under version 1, instruction for instruction, and every shipped scenario without a fault reproduces its digest; an activation that traps between two checks is metered without the local's remainder, as rule 1 allows. A first build charged the global inline at every block instead, which metered even faulting activations exactly as version 1 but bought nothing in the browsers' engines, where an imported mutable global is reached through an indirection: Chromium 1.78× against 1.32× with the local on the protocol-style guest. Rule 5 is deferred: the stack height keeps the library's limiter in version 2, its cost measured with the rest; folding it is a later version if the figures ask. Rule 3's bound is the function's length, refused above the class's budget as `module.segment-too-long`. The default stays version 1 until the owner's decision on the figures (rule 9).

**Tracks.** Requirements ACT-006 and ACT-010; the roadmap's Phase 3 exit criterion on the cost benchmark ("at most 1.5× on the protocol-style guest and 2× on the compute-style guest under cost-model version 2, or the owner has decided again on the measured figures; runs recorded under version 1 still replay"). Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** RFC 003 section 3 (the transformation), rules 11 to 15 (the budget and the cost model), section 7 (the planned optimisation); RFC 005 rule 11 (the figures); RFC 006 rule 7 (a fuzz target); RFC 001 rule 46 (metered work in the trace); nothing in any contract, file format or interface.

## 1. What is being bought

Deterministic metering costs 2 to 4 times guest execution today (RFC 003 section 7, measured in Stage 0 and on every release since: 0.2.0's figures are 2.3 to 2.9× on the protocol-style guest and 1.8 to 3.9× on the compute-style guest natively). The owner accepted that cost on the condition that its optimisation could not be missed; the plan names this RFC's content: florn's own pass, charging per function entry and per loop back-edge with statically summed straight-line costs, the counter held in a local between checks, one global write per check, released as cost-model version 2 behind the versioning mechanism of RFC 003 rule 15.

The cost of version 1 is not the counting but the checking: the library pass charges every basic block by reading the budget global, subtracting, comparing, branching and writing it back, at the start of every block. A protocol-style guest is many small blocks and calls; the compute-style guest is a tight loop whose body is a few blocks. The engine's own fuel (Wasmtime) does the same work with the engine's knowledge of registers and still costs 1.3 to 2.5×: the floor of charging at every block is above the target. The only way below it is to touch the global less often while charging exactly as much.

## 2. The pass

**Rule 1.** Version 2 charges the same table as version 1 (RFC 003 rule 14): one unit per instruction, one unit per `memory.grow` call and nothing per page. The metered work of an activation that does not fault is therefore the number of instructions it executed, as under version 1, and a run recorded under version 1 without a budget fault has the same trace digest under version 2. The kit asserts this on every recorded digest.

**Rule 2.** The pass divides every function into straight-line segments: a segment ends at an instruction that can transfer control (`block`, `loop`, `if`, `else`, `end`, `br`, `br_if`, `br_table`, `return`, `call`, `call_indirect`, `unreachable`) or at a label that control can reach. The cost of a segment is the sum of its instructions' units, known statically. At the start of each segment the pass adds that cost to a local of the function, `florn_budget_used`, with one `local.get`, one `i64.const`, one `i64.add` and one `local.set`; no global is read or written there.

**Rule 3.** A *check* subtracts the local from the budget global, writes the global back, resets the local, and traps when the global is below zero (the `budget` fault of RFC 003 rule 12, classified as there). Checks are placed at:

| Place | Why |
|---|---|
| Function entry, before the first segment's charge | A call is the only way into a function; every function's work is bounded between its entry and the next check |
| Every branch whose target is a `loop` label (a back-edge: `br`, `br_if`, and each `br_table` arm that targets one), before the branch | A loop is the only way to execute unbounded work without a call |
| Before every `call` and `call_indirect` | The callee's entry check must see the caller's charge; the caller's local does not survive the call |
| Before every `return` and at the function's `end` | The global is exact when the activation's export returns, so the Host reads the metered work as under version 1 |

Between two checks an activation executes at most one check-free path of the function, whose cost is static and finite: the pass computes it per function as the longest path between checks, and admission refuses a module whose longest path exceeds the `budget` of its class (`module.segment-too-long`, naming the function and the cost) so that no activation can run past its budget by more than one path.

**Rule 4.** The budget global is the same imported Host-owned global as version 1 (RFC 003 rule 11; the browser Host reaches it through the six-instruction module of RFC 005 rule 8), set before the activation and read after it. Neither Host changes its budget plumbing between the versions; the two versions differ only in the admitted bytes.

**Rule 5.** The stack height limit (RFC 003 rule 13) is implemented by the same pass in version 2, with version 1's rule: a function's frame is its maximum operand stack depth plus its locals; at entry the frame is added to the height global and compared with the limit, at every `return` and at `end` it is subtracted. The comparison is folded into the entry check of rule 3 (one `i32.or` of the two conditions, one branch), so that a call costs one global read-modify-write per concern and one branch, where version 1 costs two branches and a block charge. The fault is `stack`, classified as in rule 13 (height at or above the limit, read after the trap, never by calling the guest).

**Rule 6.** The pass is pass 2 and 3 of RFC 003 section 3 in one, placed as they are: after the memory cap and before the state export pass; the export pass exports the same Host-owned globals, and the state layout of RFC 003 rule 21 is unchanged in meaning (the `florn_budget_used` local is a local, not state; the height global is Host-owned and reset on restore as before). The admitted bytes are identical under both Hosts for the fixed pass order, as the kit's admitted-hash test asserts.

**Rule 7.** A metered module of version 2 validates under the admitted feature set of RFC 003 rule 4; the pass adds one local per function, one global import (the budget) and one global (the height), both as version 1 does, and no new feature. The fuzz target of RFC 006 rule 7 for the transformation gains version 2: on every valid input the output validates and, run to completion under Wasmtime with a budget that cannot be exhausted, counts the same work as version 1.

## 3. Versioning and the default

**Rule 8.** Version 2 is a cost-model version in the sense of RFC 003 rule 15: a run input, part of the run identity, recorded in every run record and checkpoint with its table. The Host keeps version 1 for as long as version 1 records exist, that is for every release of the 0.x and 1.x lines; a run recorded under version 1 replays under version 1 and the library pass stays in the admission crate for that purpose. A checkpoint taken under one version is refused under the other (`checkpoint.mismatch`, `cost-model`), as RFC 003 rule 22 says.

**Rule 9.** The default for new runs is decided by the conformance kit's figures on the GitHub runners, per the roadmap's step 4: when version 2 meets the target on every supported engine (at most 1.5× on the protocol-style guest and 2× on the compute-style guest, metered over unmetered), new runs default to version 2 from 0.3.0 and the release notes record the figures beside version 1's; when it does not, the owner decides on the measured figures before the release is tagged, and this RFC's status records the decision. A front end may name the version at `new-run` (`cost-model: 1 | 2`); the runner takes `--cost-model`; the shipped scenarios' digests are recorded under the default.

**Rule 10.** Admission under version 2 is the same admission step (RFC 003 rule 1) with the same results, refusals and codes, plus `module.segment-too-long` of rule 3. The estimate of RFC 003 section 3 reports the module's longest check-free path beside its other figures, so that a module author can see what the budget must exceed.

## 4. Verification

| Property | Test |
|---|---|
| Rule 1 | Every recorded digest of the shipped scenarios is reproduced under version 2 except the hostile `budget` scenario's, whose fault point is version 2's and is recorded for version 2 |
| Rule 2, rule 3 | A guest with a known instruction count per activation is charged that count exactly; a loop of N iterations faults at the same iteration under every engine (AC-002 under version 2); a function whose longest path exceeds the budget is refused at admission |
| Rule 4 | The browser Host's crossing figures (RFC 005 rule 11) are unchanged between the versions at the call floor |
| Rule 5 | The recursion guest faults with `stack` at the same depth under every engine and under both versions |
| Rule 6 | The admitted hash of every shipped module under version 2 is identical natively and in the browser |
| Rule 7 | The fuzz target runs for the agreed time without a crash and without a validation failure |
| Rule 8 | A 0.1.0 and a 0.2.0 run record replay to a match under version 1 on a Host whose default is version 2; a version 1 checkpoint is refused under version 2 with the field named |
| Rule 9 | The cost benchmark's table in the release notes, both versions, every engine |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Charging only at function entry and loop back-edges with the longest path's cost (an upper bound, no per-segment local) | Over-charges every function for its longest path; the metered work would no longer be the instructions executed, version 1's digests would not be reproducible, and a budget would fault on work that was not done |
| Keeping the library pass and tuning its cost rules | The cost is the check at every block, not the rules; no rule table changes that |
| The engine's fuel natively, the library pass in the browser | Different fault points under different Hosts: rejected in RFC 003 section 4.1 and again by the measurement in Stage 0 |
| A single check per activation (trust the longest path) | A loop makes the path unbounded; a check at every back-edge is the minimum that keeps the budget a bound |
| Replacing version 1 and converting old records | A run's inputs are its identity (RFC 001 rule 41); converting would invent a record that never ran. Version 1 is kept, as RFC 003 rule 15 promised |

## Prior art

| Topic | Precedent |
|---|---|
| Gas metering by basic block with a global counter | `fvm-wasm-instrument` and its ancestor `wasm-instrument` (Parity); the cost measured there |
| Hoisting the counter into a local and checking at loop headers and calls | Wasmtime's fuel instrumentation in Cranelift (which charges per block but keeps the counter in a register) and the "epoch interruption" alternative; the Dalvik and Java interpreters' back-edge safepoints |
| Static longest-path bound per function | Worst-case execution time analysis on reducible control flow; WebAssembly's structured control flow makes every loop explicit |
| Versioned cost models recorded in every run | RFC 003 rules 14 and 15 |

## Consequences for other RFCs and the requirements

| RFC or document | Consequence |
|---|---|
| RFC 003 | Rules 11 to 15 stand; rule 14 gains the version 2 row; section 7's row 3 implemented, row 4 decided by rule 9 here; section 2 gains the refusal `module.segment-too-long` |
| RFC 005 | Rule 11's figures gain the version column; rule 31's runner gains `--cost-model` |
| RFC 006 | Rule 7's transformation target covers version 2 |
| RFC 007 | Section 4.2 gains `module.segment-too-long` (`module: name`, `function: text`, `cost: count`, `budget: count`) |
| ROADMAP | "Metering cost" steps 3 and 4 |
| Requirements | ACT-006's cost, measured in every release's notes, falls to the target or the owner's figure |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | The default when the target is missed on one engine but met on the others (rule 9): version 2 everywhere, version 1 everywhere, or the measured figures published and the owner's choice | The owner, with the figures of the implementing pull request |
| 2 | Whether the per-segment local is an `i64` (this RFC) or an `i32` with a check when it could overflow; the choice is a measurement | The implementing pull request |
