# Phase 3 implementation handoff

Entry point: [`README.md`](./README.md). This document says what each crate must do for release 0.3.0 and when it is done. Rule numbers refer to RFC 009 unless another RFC is named.

## 1. Summary

Release 0.2.0 gave florn its first world and its first users. Release 0.3.0 gives the run a past and alternatives, the browser more actors than it has instances, the metering its planned cost, and the ladder a second protocol family. The core does not change in meaning: every operation of RFC 009 is an observation or the start of a run whose inputs are stated; RFC 010 changes the admitted bytes and nothing a guest or a front end sees; RFC 011 adds a guest and a decoder.

## 2. Toolchain additions, pinned

None. The metering pass of RFC 010 is written over `wasmparser` and `wasm-encoder`, already pinned for admission; `fvm-wasm-instrument` stays for cost-model version 1. The installable application needs a manifest file and icons, no tool. A version is changed by a pull request that says why.

## 3. The crates

### 3.1 `florn-host`: checkpoints (RFC 009 section 2)

The automatic checkpoint at every `checkpoint-interval` handled events, at the next boundary (rule 2); the kept set under `checkpoint-memory` with the thinning of rule 3; the `checkpoints` query; the `checkpoint` command; restore as rule 5 with the identity check first; the `.florn-checkpoint` file (rule 6) as the file kind the control interface reserved, read under the maxima added to RFC 006 section 7 and written after the export preview; `load-checkpoint`; the notifications `checkpoint.taken` and `checkpoint.budget`. The run's `Options` gain the interval and the budget; the browser Host sets its budget from the probed capacity.

Done when: a checkpoint taken natively restores in the browser and the reverse with the digest of the uninterrupted run for every scenario of the ladder; a run with checkpoints every 100 events, three exports and three restores has the digest of a run without; the kept set follows the interval and the budget; a file one byte over each maximum is refused in bounded memory.

### 3.2 `florn-cli`: resume and the checkpoint options (RFC 005 rule 31)

`florn resume <checkpoint> <scenario> [--module …] [--modules <dir>] [--to TICK]`; `florn run --checkpoint-at TICK --checkpoint-out <file>`; `--cost-model N` on `run` (RFC 010 rule 9). The same lines and exit codes as `run`.

Done when: `run --checkpoint-at` then `resume` on the other Host gives the recorded digest for every ladder scenario, in CI's native matrix and in the browser driver through the kit's page.

### 3.3 `florn-admission`: cost-model version 2 (RFC 010)

The pass of RFC 010 rules 2 to 7 as pass 2 and 3 of RFC 003 section 3, selected by the cost-model version; the table recorded with the version; the longest check-free path per function in the estimate and the refusal `module.segment-too-long`; version 1 unchanged beside it. The `Admission` value names its version; the Host passes the version of the run.

Done when: every recorded digest of the shipped scenarios is reproduced under version 2 except the hostile `budget` scenario's, which is recorded for version 2; AC-002 passes under version 2 on every engine; the admitted hashes are identical natively and in the browser; the fuzz target covers the pass; the cost benchmark's figures for both versions on every engine are in the pull request, and the roadmap's step 4 is answered by them or by the owner.

### 3.4 `florn-host`, `florn-kernel`: backward navigation and detail (RFC 009 section 3)

`advance { to }` with `to` before now as rule 7: restore the nearest kept checkpoint, execute forward, discard the records, chain points, telemetry and display records after `to`; the interventions after `to` leave the inputs and are reported by `run.rewound` (rule 8); the `trace-window` query answered by re-execution through the live pool around a checkpoint of the present (rule 9); the browser Host keeps only the records since the last checkpoint (DATA-008).

Done when: moving back to every stored chain point of every ladder scenario and running to the end gives the recorded digest; an intervention undone and re-applied gives the two expected digests; a window before the last checkpoint equals the same window of a run that kept every record, and the run's instant and digest are unchanged afterwards.

### 3.5 `florn-host`, `florn-control`: branching (RFC 009 section 4)

The `branch` command as rules 10 and 11; `[parent]` in `run.toml` with `chain`, `records`, `instant` (rule 12); the parent's record kept for the front end; `run.branched`.

Done when: a branch at `T` has the parent's chain at `T`, names the parent, replays to a match without it, and two branches with different interventions differ after `T` only.

### 3.6 `florn-host`, `florn-engine-web`, `florn-engine-wasmtime`: actor swapping (RFC 009 section 5)

The instance pool with a resident set smaller than the actor count; the least recently activated actor exported and dropped when the pool is full, the next actor restored from the compiled cache before its activation (rule 13); `new-run { residency }` with `resident` the default and `swap` on request (rule 14); `host.swapping` with the resident count, the actor count and the measured cost per kilobyte; a checkpoint of a swapping run from the exported state as it is (rule 15).

Done when: the ladder's scenarios with a pool of one, of two and of the full capacity give one digest on every engine; a scenario beyond capacity is refused by default with the numbers and runs with `swap`; a checkpoint of a swapping run restores in a resident run.

### 3.7 `florn-ui-model`, `florn-ui-scene`, `florn-ui-web`: the application's depth (RFC 009 sections 6 and 7)

The view model's checkpoint list, undone interventions, parent and residency state over the control additions of section 8; "Go to…" backward and "Step to here" backward with the discard notice (rule 19); "Take a checkpoint now", "Export this checkpoint", "Branch from here" and the parent in the world inspector; origin storage as a cache with the list under "Files", the size, the eviction notice (rule 16); journey J5: a module imported from a file, shown with both hashes, placed from the palette, run and replayed (rule 17); `module.component` for a Component (rule 18); the web application manifest and icons from the origin (rule 20); `limited by` with `memory` and `swapping` (rule 21); the address bar holds only the origin, the version path and a lab name (rule 22). The catalogues gain every new string in both languages.

Done when: the journeys cover J5, going back with the undone list, branching with the parent shown, the installable manifest and the address check in Chromium, Firefox and WebKit; the view model's tests run natively; the dependency check still finds no framework crate below the shell; the forbidden words pass.

### 3.8 `florn-guest-network`, `florn-domain-network`: the BGP subset (RFC 011)

The `bgp` example with the session machine, the RIBs, the decision and the advertisement rules (RFC 011 rules 1 to 6), its frames (rule 7) and the decoder in the domain, the `peer` and `route` layouts (rule 8), the scenarios `bgp-lab` and `bgp-outside` with their stories in both languages (rules 9 and 10) and their digests; the ladder's table in RFC 008 rule 20 amended.

Done when: the verification table of RFC 011 passes natively and in the three browsers; the lab's story holds through the journeys' lab checks; every refusal of rule 6 is logged and the router keeps running.

### 3.9 Release 0.3.0

Documentation (the lab's guide and the interface's guide extended with going back, branching, checkpoints, storage and the BGP lab, in both languages; the module author's chapter on the cost model and the longest path; the automation chapter's `resume` and `--cost-model`), the security review of RFC 006 rule 5 for the added flows (the checkpoint file, origin storage, installation, the second transformation), release notes with the figures of both cost-model versions, RFC 009, 010 and 011 to `rfcs/done/` with the 0.3 rules of RFC 001, 003 and 005 marked implemented, the tag, the crates published. The usability test of the checklist's section D runs at the owner's pace; its findings are recorded as issues for 0.3.x or for Phase 4, and the release does not wait for it unless the owner says so.

## 4. Cross-cutting rules

| Rule | Source | How it is checked |
|---|---|---|
| No operation of RFC 009 changes a digest; every 0.1 and 0.2 digest stays as recorded | RFC 001 rule 39; RFC 009 rule 4 | The kit's shipped scenarios, unchanged, in every CI run; the observation test of 3.1 |
| Cost-model version 2 counts what version 1 counts | RFC 010 rule 1 | Every recorded digest reproduced under version 2 in CI, the hostile `budget` scenario excepted and recorded for version 2 |
| One instance pool in the browser; detail and branching use it | RFC 009 rules 9 and 11 | The browser driver's instance count never exceeds the pool during the kit's run |
| Nothing silent: swapping, discarding, eviction and refusals are said | External design section 2; RFC 009 rules 8, 14, 16 | The journeys read every notice; the catalogues hold every key |
| Control additions stay within version 1 | RFC 005 rule 25 | The format tests of the control crate; a 0.2 record replays on a 0.3 Host |
| The kernel knows no checkpoint policy, no swapping policy and no cost model | RFC 005 rule 3; RFC 003 rule 15 | `ci/check-deps.py`; the policies live in the Host's run loop and the admission crate |
| Every string a person sees is a catalogue key in both languages | RFC 007 rules 14 to 17 | The build fails on a missing key; `ci/forbidden-words.sh` |
