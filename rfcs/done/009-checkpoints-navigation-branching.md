# RFC 009 — Checkpoints, backward navigation, branching, actor swapping, and the application's depth

**Status.** Implemented (0.3.0, 2026-10-31: PR-23 to PR-28, the implementation notes below per pull request). Accepted 2026-10-29, the owner's word on the day it was proposed, as proposed: open question 1 decided (`resident` by default; swapping on request with the numbers shown) and open question 2 decided (a Component is refused on import in 0.3 with the reason). Proposed 2026-10-29 for release 0.3.0 (Phase 3 of `ROADMAP.md`). Implementation: Phase 3, release 0.3.0, by the handoff package `rfcs/handoffs/009-depth/`. Implementation notes, 2026-10-29, PR-23 (rules 1 to 6, the `checkpoint`, `restore`, `load-checkpoint` and `export` commands and the `checkpoints` query of section 8, `florn resume` and the runner's checkpoint options): the encoding is version 2 from 0.3.0, the composed world's origin sequence counters (RFC 001 rule 17) having been missing from version 1, which no release ever wrote to a file; `restore` by index performs rule 7 for a kept checkpoint and reports the undone interventions with `run.rewound` (rule 8), ahead of `advance { to }` backward; a replay re-applies them at their instants. Implementation notes, 2026-10-29, PR-25 (rules 7 to 9; `advance { to }` backward): the start of rule 2 is a kept checkpoint of the instances before `init`, never dropped by the budget, whose restore runs `init` again as a fresh run does, so that moving back to the beginning and a window from record 0 are served like any other; `advance { to }` with an earlier tick restores the nearest kept checkpoint at or before it and executes forward at once (paced, the way forward from the checkpoint is paced); detail by replay re-executes through the run's own instances around a checkpoint of the present, re-applying the undone interventions at their instants whatever the run, with nothing logged, kept or displayed on the way and the present's pending replay list and kept records put back afterwards; mid-instant (RFC 001 rule 51 allows no checkpoint there) the kept records alone answer. The browser Host's retention of records since the last checkpoint only (DATA-008) is the application's pull request. Implementation notes, 2026-10-29, PR-26 (rules 10 to 12; `branch`): the branch is the same Host run under a new identity, the instances staying in place, so that one pool serves it (section 10); it begins at `T`, where the checkpoint taken for it is its start of rule 2 and the parent's earlier checkpoints leave with the parent, so that moving back stops at `T` (`control.invalid out-of-range tick` earlier); the point `T` is stored in the branch's own chain, so that its replay compares there; `[parent]` carries `instant` as a tick with its `microstep`; the parent's record, ended by the user at `T` with the trace it kept and no payloads, is held by the Host until the next run starts and is offered through `export { kind: parent-record }`, an addition to section 8's table made here, as the parent is gone once the branch has started; a branch within an instant is `control.not-now`; a branch of a replay takes nothing of the record's remaining interventions (rule 10: the parent's inputs after `T` are nobody's). Implementation notes, 2026-10-29, PR-27 (rules 13 to 15; `new-run { residency }`): the pool is the Host run's own (`ensure_resident` before every activation: the least recently activated resident actor exported and dropped when the pool is full, the actor's swapped state written into a fresh instance from the compiled cache), and the kernel knows nothing of it; the resident set is the probed capacity less one instance of the Host's reserve, bounded by what the memory holds of the largest actor's limit, refused with `host.capacity-exceeded` naming the memory when one actor's limit exceeds it and naming the instances when nothing can be resident; the notice's cost per kilobyte is one swap out and in of the first actor at the run's start, timed by the platform's monotonic clock (a new platform operation; the probe's figure stays the platform's); a checkpoint reads a non-resident actor's swapped state as it is, a restore into a swapping run leaves every actor non-resident, and the checkpoint of a swapping run is byte for byte the resident run's; `resident` is the 0.2 form of `new-run` unchanged and `swap` one byte more, so that a 0.2 front end's message means `resident`; a replay keeps the Host's own residency option (the runner's `--residency`). The Wasmtime adapter gives every instance a store of its own, since Wasmtime frees an instance's memory with its store and never before: one store had held every instance ever made, and a swapping run met its instance limit after ten thousand swaps. Implementation notes, 2026-10-30, PR-28 (rules 16 to 22; the browser Host's DATA-008): origin storage is the Cache API of the application's origin with an index beside it, holding what the person opened, saved, imported or exported (a branch's parent record among them), listed with its size and an eviction said as a fact; a module imported from a file shows both hashes in the palette; a Component is refused at admission by the layer field of its header; "Go to…" and "Step to here" backward ask first when interventions after the instant would be undone, which the Host then unschedules (an intervention requested before the instant for an instant after it is undone too, its sequence number given back so that a replay's keys agree), and the undone list offers each to re-apply at its instant as a new request or to discard; the Host says `host.limited-by` from a paced slice that overran its share of the second, naming swapping when the swaps keep up with the activations, memory when the counted memory nears the capacity, the display when records were dropped, and the actors otherwise; a run from the end goes back as from a pause; the browser Host keeps the records since the last checkpoint only, an export's trace being assembled by replay; the manifest and two icons are served and cached with the application; the address check runs after every journey.

**Tracks.** Requirements ACT-008, DATA-005, DES-008, OBS-010, DATA-008, NFR-004, WEB-004, WEB-005, WEB-012, SEC-007; acceptance AC-006 and the 0.3 row of the acceptance table ("a run restored from a checkpoint continues with the digest of an uninterrupted run; a user imports a module through the UI and runs it"). Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** RFC 001 rules 51 to 55 (reserved since 0.1); RFC 003 rules 20, 22 and 23 and its open question 5; RFC 005 rules 17, 29 and 36, its command table's 0.3 row and its open question 5; RFC 002 rule 6 and its open question 4; RFC 006 section 7 (a new input) and section 5 (one control extended); RFC 007 section 4.4 (new codes); the external design sections 3.3, 3.7, 3.9 and 7.

## 1. One mechanism, three features

Release 0.1 made every actor's state exportable so that nothing later would be precluded (RFC 003 rule 23); the proof of concept showed restore into a fresh instance continuing with an identical trace in every engine, across a restart and backward in time. Release 0.2 added nothing to that path and used none of it. This RFC spends it three times:

| Feature | What it is | What it rests on |
|---|---|---|
| Checkpoints and backward navigation | The Host keeps checkpoints of the running run; "go to" an earlier instant restores one and executes forward | RFC 001 rules 51 to 53; RFC 003 rules 21 and 22 |
| Branching | A new run starts from a checkpoint of another with its own interventions from there; its identity names the parent | RFC 001 rule 54 |
| Actor swapping | More actors than the browser holds instances: a non-resident actor is exported state, restored before its activation | RFC 005 rule 17; RFC 003 rule 20 |

The three share the exported state, its canonical encoding, the compiled-module cache and the identity check, so they are one RFC. The application's remaining 0.3 items (origin storage as a cache, the installable application, performance transparency, safe sharing, large output) are small on their own and are specified here because each is a consequence of the three above or of the files they produce.

Nothing here changes the trace of a run. Every operation of this RFC is either an observation (RFC 001 rule 39) or the start of a run whose inputs are stated (RFC 001 rule 40).

## 2. Checkpoints

**Rule 1.** A checkpoint holds what RFC 001 rule 51 names, in the canonical binary encoding the Host has carried since 0.1 (`CHECKPOINT_VERSION` 1): the identity it must agree on before restore (scenario hash, every module's source hash by name, admission version, cost-model version, root seed); the kernel's state (the event queue, each actor's kernel state, the applied interventions with their instants, the sequence counters, the trace's chain and its stored points); every guest-backed actor's state (RFC 003 rule 21); every domain's own state (RFC 008 rule 1). A checkpoint written by one Host restores in the other.

**Rule 2.** A checkpoint is taken only at a boundary between instants (RFC 001 rule 51). The Host takes one by itself every `checkpoint-interval` handled events, at the first boundary after the count is reached; the interval is a Host option, 10,000 by default, never a scenario input. The front end may ask for one at any boundary with the `checkpoint` command. The start of the run counts as a checkpoint that costs nothing to keep: restoring it is execution from the inputs.

**Rule 3.** The Host keeps its checkpoints in memory within `checkpoint-memory`, a Host option (256 MiB natively, 64 MiB in the browser, both from the probed capacity of RFC 003 rule 20 and RFC 005 rule 18). When a new checkpoint would exceed the budget, the Host drops every second checkpoint among those older than the newest eight, oldest first, until the new one fits; the newest eight are never dropped by the budget. Recent instants stay dense, old ones sparse, and moving far back costs a longer forward execution rather than a refusal. The control interface lists the kept checkpoints (`checkpoints` query: instant, record index, bytes) and the status line shows their count and size.

**Rule 4.** Taking, keeping, dropping, exporting or restoring a checkpoint never changes the trace: a run observed through any sequence of these has the digest of a run that was not (RFC 001 rule 39). The conformance kit asserts it.

**Rule 5.** Restore is RFC 003 rule 22: a fresh instance per actor from the cached compiled module (never a compilation, RFC 005 rules 9 and 10), memory grown to the saved size and written, globals set, tables verified, the kernel and the domains restored, the Host-owned globals reset. Before any of it, the identity of rule 1 is checked field by field and a mismatch is refused with `checkpoint.mismatch` naming the field; nothing is instantiated for a refused checkpoint.

**Rule 6.** A checkpoint is also a file, `<name>.florn-checkpoint`: the encoding of rule 1 with no other content, the file kind `checkpoint` the control interface reserved in 0.1. It is exported after the preview of RFC 005 rule 30 (which states the instant, the actors, the size, and that it contains every actor's memory) and imported as any other file, through the Host's parser under the maxima of RFC 006 section 7, which gains the row: a checkpoint file of at most 2 GiB; an actor's memory at most that actor's memory limit; at most as many actors as the scenario declares; the counts checked before the bytes are read. A checkpoint file makes a run resumable in a later session or on the other Host (DATA-005): `load-checkpoint` with the scenario and its modules starts a run whose instant is the checkpoint's and whose inputs are the checkpoint's.

## 3. Backward navigation

**Rule 7.** Moving to an instant `T` earlier than now (RFC 001 rule 53) is one operation: the Host restores the nearest kept checkpoint at or before `T` (or the start), then executes forward to `T` without pausing. From `T` the run continues as it did the first time: by RFC 001 rule 52 its continuation has the same digest as long as the inputs stay the same. The trace records, chain points, telemetry and display records after `T` are discarded, and the run's record is the trace up to `T`.

**Rule 8.** Interventions that were applied after `T` leave the run's inputs when the run moves back to `T` (RFC 001 rule 35 lists inputs by their instants; an instant that has not happened again has no intervention). The Host reports them (`run.rewound` names how many), and the front end keeps them as *undone* interventions the user may re-apply at their instants or discard. A re-applied intervention is a new intervention request and is applied at the first boundary after it is received, as any other.

**Rule 9.** Detail for a window of time (RFC 001 rule 55, OBS-010) is produced by re-execution through the live instance pool, so that the browser needs no second pool: the Host checkpoints the current state at its boundary, restores the nearest kept checkpoint at or before the window's start, executes to the window's end with recording on (payloads kept when asked), answers the `trace-window` query from that recording, and restores the current state. The run's instant, inputs and trace are unchanged; the cost is the window's execution and two restores. The browser Host keeps in memory only the records since the last checkpoint and the stored chain points (DATA-008); every earlier window is produced this way.

## 4. Branching

**Rule 10.** A branch (RFC 001 rule 54, DES-008) is a new run started from the current instant `T` of the current run with the `branch` command. Its inputs are the current run's inputs: the same scenario, modules, admission and cost-model versions, root seed and contract versions, and the interventions applied at or before `T`; its own interventions follow from `T`. The parent is named in the identity: the parent's chain value at `T`, the record index at `T`, and `T` itself. Up to `T` the branch's trace equals the parent's (RFC 001 rule 52); after `T` it differs only by its own inputs.

**Rule 11.** The current run ends when its branch starts (one run at a time, external design section 3): its record is kept in origin storage (rule 16) and offered for export. The branch starts from the checkpoint at `T`, which the Host takes if `T` is not a kept checkpoint's instant.

**Rule 12.** A branch's run record is a run record (RFC 005 rule 29) whose `run.toml` carries a `[parent]` table: `chain`, `records`, `instant`. Replay of a branch needs nothing from the parent: the record's own inputs determine its trace. The parent entry is provenance, shown by the application in the world inspector and as a mark on the timeline, and compared by nothing.

## 5. Actor swapping

**Rule 13.** From 0.3 the instance pool (RFC 005 rule 17) may hold fewer resident instances than guest-backed actors. A non-resident actor is its exported state; before its activation the Host restores it into a fresh instance from the compiled cache, and when the pool is full it first exports and drops the least recently activated resident actor. The policy is the pool's and never affects the trace; the digest of a swapped run equals the digest of a resident run, and the conformance kit asserts it with a pool of one.

**Rule 14.** Swapping is asked for, never silent. `new-run` gains `residency`: `resident` (the default; a scenario beyond capacity is refused with `host.capacity-exceeded` naming the need and the capacity, as in 0.1 and 0.2) or `swap` (the capacity check applies to the resident set: the pool's size is the probed capacity less the Host's reserve, and the scenario is refused only when a single actor's memory limit exceeds what can be reserved). A swapping run reports `host.swapping` with the resident count, the actor count and the measured cost per kilobyte, and the status line says "swapping" while it does (NFR-004).

**Rule 15.** A checkpoint of a swapping run uses the exported state of non-resident actors as it is: nothing is restored to be exported again. A restore into a swapping run makes every actor non-resident and lets activations bring them in.

## 6. Modules and storage

**Rule 16.** Origin storage holds, as a cache, the scenarios, modules, run records and checkpoints the user opened, saved or imported in this browser (RFC 005 rule 36, WEB-012): each by its hash with the name the user gave, listed under "Files" with its size, reopened with one click, exported as a file at any time. The file is the durable form; the application says so where the list is shown, reports the cache's size (`storage.cache`), and shows an eviction the browser made as a plain fact, never as an error. Nothing is stored that the user did not open, save or import.

**Rule 17.** A module imported from a file (WEB-005, external design section 3.9) is hashed and admitted in the worker, its manifest shown with both hashes (source and admitted) before it can be placed, and then kept in origin storage and offered in the palette with the bundled modules. Journey J5 is this path end to end: import, read, place, connect, run, replay.

**Rule 18.** A WebAssembly Component is refused on import with `module.component`, naming the reason, in 0.3 (RFC 002 rule 6 left the question to the owner; open question 2). The reduction of a Component to its core module needs a component decoder in the worker and a second identity shown beside the first; no guest toolchain of the ladder produces a Component, and the refusal names the way to a core module.

## 7. The application

**Rule 19.** "Go to…" accepts an instant before now and performs rule 7; "Step to here" in the timeline moves backward to a row's instant the same way; both say what they will discard before they do it when interventions after the instant exist, and list those under "Interventions" as undone afterwards (rule 8). The world inspector shows the kept checkpoints with their instants and the memory they use, offers "Take a checkpoint now", "Export this checkpoint" and "Branch from here", and names the parent of a branch.

**Rule 20.** The application is installable (WEB-004): a web application manifest and icons served from the application's own origin, the service worker it already has, and nothing else. Installation changes no behaviour: the same files, the same version at the same immutable path, the same offline start.

**Rule 21.** The status line's "limited by" (NFR-004) gains `memory` and `swapping` beside the factors of 0.2, each from the Host's own figures: the memory counted by RFC 005 rule 18 against the capacity, and the swaps per second against the activations per second.

**Rule 22.** No address carries content (SEC-007): the application's URL names at most a bundled lab (`?lab=`) and a version path; a scenario, a run record or a checkpoint travels only as a file the user exports after the preview of RFC 005 rule 30. The conformance kit's one-origin check gains the assertion that the address bar holds nothing but the origin, the version path and a lab name after every journey.

## 8. The control interface

Additions within control version 1 (RFC 005 rule 25; a 0.2 Host refuses them as unknown, as the compatibility page says):

| Kind | Name | Content | Effect on the logical world |
|---|---|---|---|
| Command | `checkpoint` | — | None; a checkpoint is taken at the next boundary |
| Command | `advance { to }` with `to` before now | The instant | Rule 7: restore and execute forward; the inputs after `to` leave the run |
| Command | `restore { checkpoint }` | A kept checkpoint's index | Rule 7 to that checkpoint's instant |
| Command | `branch` | — | A new run per rule 10; the current run ends |
| Command | `load-checkpoint` | The file's bytes | A new run at the checkpoint's instant with its inputs |
| Command | `export { kind: checkpoint }` | — | None; previewed per RFC 005 rule 30 |
| Command | `export { kind: parent-record }` | — | None; the parent's record kept by rule 11, previewed per RFC 005 rule 30 (added at implementation, PR-26) |
| Command | `new-run { residency }` | `resident` or `swap` | Rule 14 |
| Query | `checkpoints` | — | The kept checkpoints: index, instant, record index, bytes |
| Query | `trace-window` | Any window of the run | Rule 9 when the window is before the kept records |
| Query | `support` | `features` gains `checkpoint`, `branch`, `swap`, `storage` | — |

Notifications, added to RFC 007 section 4.4 and 4.3:

| Code | Parameters | Severity | Scope | Ends |
|---|---|---|---|---|
| `checkpoint.taken` | `instant: tick`, `bytes: bytes`, `kept: count` | info | run | nothing |
| `checkpoint.budget` | `kept: count`, `dropped: count`, `bytes: bytes` | info | run | nothing |
| `run.rewound` | `to: tick`, `undone: count` | info | run | nothing |
| `run.branched` | `instant: tick`, `parent: text` | info | run | nothing |
| `module.component` | `module: name`, `reason: text` | error | file | command |
| `storage.evicted` | `kind: enum(file-kind)`, `name: text` | warning | file | nothing |

`host.swapping` and `checkpoint.mismatch` exist since 0.1 and are used as written; `enum(checkpoint-field)` gains `instant` and `actors` for a file whose counts disagree with the scenario.

The headless runner gains `florn resume <checkpoint> <scenario> [--module …] [--to TICK]`: a run from a checkpoint file to the scenario's end or to `--to`, printing the same lines as `run`. With `florn run --checkpoint-at TICK --checkpoint-out <file>`, a run writes one checkpoint file at the boundary before `TICK`; together the two commands are the kit's cross-process and cross-Host test of DATA-005.

## 9. Verification

| Property | Test |
|---|---|
| Rule 1, rule 5 | A checkpoint encodes and decodes to itself; a checkpoint taken natively restores in the browser and the reverse, with the digest of the uninterrupted run, for every scenario of the ladder (AC-006 extended) |
| Rule 2, rule 3 | The checkpoint count and size follow the interval and the budget; the newest eight survive any budget; the kept set after a long run is dense near the end and sparse at the start |
| Rule 4 | The digest of a run with checkpoints every 100 events, exports and three restores equals the digest without any |
| Rule 7 | Moving back to every stored chain point of every ladder scenario and running to the end gives the recorded digest; moving back to an instant between checkpoints executes forward from the earlier one |
| Rule 8 | A run with an intervention at 1,500 moved back to 1,000 continues to the digest of the run without the intervention; re-applied at 1,500, to the digest with it |
| Rule 9 | A trace window before the last checkpoint equals the same window of a run that kept every record, payloads included when asked; the run's instant and digest are unchanged afterwards |
| Rules 10 to 12 | A branch at `T` has the parent's chain at `T`; its record names the parent; its replay matches without the parent; two branches of one run with different interventions differ after `T` only |
| Rules 13 to 15 | The ladder's scenarios with a pool of one, of two and of the full capacity give one digest; a swapping run's `host.swapping` figures; a checkpoint of a swapping run restores in a resident run |
| Rule 6, RFC 006 section 7 | A checkpoint file one byte over each maximum is refused in bounded memory; one below is accepted; a corrupt file fails `file.unreadable` |
| Rule 16, rule 17 | Journey J5 in Chromium, Firefox and WebKit: a module imported from a file is shown, placed, run and replayed; the cache lists it afterwards; an export reproduces the imported bytes |
| Rule 18 | A Component is refused with `module.component` |
| Rules 19 to 22 | The journeys: "go to" backward and "step to here" backward with the undone list; "branch from here" and the parent shown; the installable manifest served from the origin; the address bar after every journey |
| Every control of RFC 006 section 5 | C6 gains the checkpoint file; C12 is unchanged by installation; C14 covers the new commands with invalid parameters |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Checkpoints at a fixed logical interval (every N ticks) | A tick is a scenario's unit; one scenario's 1,000 ticks is a second of work and another's a microsecond. Handled events measure work, so memory grows with what was computed |
| Keep every checkpoint; refuse when memory runs out | The user would lose backward navigation exactly on long runs, where it matters most; thinning keeps every instant reachable at a cost |
| A second instance pool for detail by replay | The browser's instance ceiling is the pool; a second one halves the scenario the browser can hold. Re-execution through the live pool costs two restores and nothing else |
| Branching as a fork that keeps both runs live | Two runs at once in one page double the pool and the display path and contradict the external design's one run at a time. A parent's record is kept, so nothing is lost |
| Swapping on by default | A silent 10× slowdown is the external design's "nothing silent" broken; the user asks for it with the numbers shown |
| Accepting Components on import in 0.3 | No producer in the ladder; a decoder in the worker; two identities to explain. Refused with the reason until a toolchain needs it |
| A `goto` command beside `advance` | One command for one instant: `advance { to }` already names an instant and says which way to go |

## Prior art

| Topic | Precedent |
|---|---|
| Snapshot and restore of WebAssembly instances for time travel | The proof of concept of 2026-10-14; Wizer's pre-initialisation of instances; record-and-replay debuggers (rr) restoring from periodic checkpoints and executing forward |
| Thinning a checkpoint set under a memory budget | Hierarchical and logarithmic checkpointing in reverse-mode automatic differentiation and in interactive emulators' rewind buffers |
| Branching a deterministic simulation from a point | Discrete-event simulators' cloning of runs; version control's notion of a parent commit |
| Paging actors beyond the resident set | Virtual memory; the proof of concept's measured 0.4 µs per KB |

## Consequences for other RFCs and the requirements

| RFC or document | Consequence |
|---|---|
| RFC 001 | Rules 51 to 55 implemented; the phasing table's "Later" row becomes 0.3.0 |
| RFC 003 | Rule 20's swapping sentence and rule 23's checkpoint sentence implemented; open question 5 decided by open question 1 here |
| RFC 005 | Rule 17 implemented with the policy stated; rule 29 gains `[parent]`; rule 36 implemented; the 0.3 row of the command table expanded by section 8; open question 5 decided here; rule 31 gains `resume` |
| RFC 002 | Rule 6 and open question 4 decided by rule 18 |
| RFC 006 | Section 7 gains the checkpoint file's maxima; section 5 C6 covers it; the release review names the new flows (the checkpoint file, origin storage, installation) |
| RFC 007 | Section 4.4 gains the codes of section 8 |
| External design | Sections 3.3, 3.7, 3.9 and 7 updated by the implementing pull requests; J5 added to section 2 |
| Requirements | DATA-005, DES-008, OBS-010, DATA-008, NFR-004, WEB-004, WEB-005, WEB-012, SEC-007 at 0.3 |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | ~~Residency's default: `resident` with swapping on request (this RFC, rule 14), or swapping whenever a scenario exceeds the capacity, with the notice~~ Decided 2026-10-29 at acceptance: `resident`; swapping on request. | — |
| 2 | ~~Components on import: refused in 0.3 with the reason (rule 18), or reduced to a core module in 0.3~~ Decided 2026-10-29 at acceptance: refused with the reason (RFC 002 open question 4 closed the same way). | — |
| 3 | The checkpoint interval's default (rule 2) and the memory budgets (rule 3), set here from the proof of concept's figures | The implementing pull request, with the kit's figures |
| 4 | Whether a checkpoint file carries the scenario text and the module bytes for convenience (a package and a checkpoint in one file) | Not in 0.3: a checkpoint references them by hash as a run record does (RFC 005 rule 29) |
