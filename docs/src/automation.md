# For automation users

The headless runner `florn` runs a scenario, writes a run record, replays one, admits a module, and reports the machine's capacity. Everything it prints is a `key = value` line on standard output or a coded message in English on standard error, and its exit code says what happened. This chapter is the reference for scripts.

## Getting the runner

```sh
cargo install florn                                                          # from crates.io
cargo install --git https://github.com/nabbisen/florn --tag 0.2.0 florn      # pinned to the tag
cargo install --path crates/florn                                            # from a checkout
```

Each release also carries prebuilt binaries for x86-64 Linux, AArch64 Linux and Apple Silicon, and the browser application as a bundle to serve, with their SHA-256 in the release notes.

## Commands

```text
florn run <scenario.toml|package.florn> [--module <file>]... [--modules <dir>] [--seed N]
          [--out <file>] [--trace] [--payloads] [--cost-model N]
florn resume <run.florn-checkpoint> <scenario.toml|package.florn> [--module <file>]... [--modules <dir>]
          [--to TICK] [--out <file>] [--trace] [--payloads]
florn replay <run.florn-run> <scenario.toml|package.florn> [--module <file>]... [--modules <dir>]
florn admit <module.wasm>
florn probe [--instances N] [--memory BYTES]
florn help
```

| Command | What it does | Prints |
|---|---|---|
| `run` | Admits the modules, starts a run with the seed given (or one the machine chooses, which is then printed), runs to the scenario's end, writes the run record | `state`, `seed`, `handled`, `records`, `chain`, `ended`, `run-record` |
| `resume` | Starts a run from a checkpoint file (RFC 009 rule 6) with the scenario and its modules, to the scenario's end or to `--to`, and writes the run record | the same as `run`, with `handled` and `records` counted from the start of the run the checkpoint came from |
| `replay` | Starts a run from the record's inputs, applies the recorded interventions, compares the chain at every stored point | the same, plus `replay = matched` or `diverged` |
| `admit` | The admission step alone | `module`, `source`, `admitted`, `contract`, `kind`, `description`, `port-kind`, `param`, `channel`, `needs` lines |
| `probe` | The capacity and the build | `instances`, `memory`, `engine`, `arch`, `florn`, `control` |

Modules are found by the hash the scenario names: give each file with `--module` or a directory with `--modules`; a module the scenario does not reference is reported, and a missing one is an error. A replay needs the scenario and the modules again, since a run record carries neither (it carries their hashes, and refuses a different module).

`run --cost-model N` admits the modules under cost-model version `N` (1, the library's pass; 2, florn's own, RFC 010, the default): the version is a run input, recorded in the run record, and a replay or a resume uses its file's version whatever the default. The two versions count the same units, so a run gives the same digest under both; they differ in speed. `run --checkpoint-at TICK --checkpoint-out <file>` writes one checkpoint at the boundary before the tick and goes on to the end; `resume` of that file on the other Host, or in another process, ends with the same `chain` as the uninterrupted run (DATA-005). A checkpoint file, `.florn-checkpoint`, holds the kernel's state, every actor's state and every domain's, with the identity it must agree on (the scenario's hash, the modules' hashes, the admission and cost-model versions, the seed); a mismatch is refused with the field named.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Done: the run reached the scenario's end, or the replay matched, or the module was admitted |
| 1 | Refused or failed: a coded message on standard error says why |
| 2 | The replay diverged: the first divergent interval is in the message |
| 3 | The run ended with a run fault: the `ended` line says `host-fault` (a limit of RFC 001 rule 13, or a Host fault that made the run not reproducible) |

## The run record

`<name>.florn-run` is a ZIP archive (RFC 005 rule 29):

| Entry | Content |
|---|---|
| `run.toml` | The run identity and inputs: florn version, Host build, engine, architecture, scenario hash, module hashes (source and admitted), admission and cost-model versions, root seed, the interventions applied, the stored chain points, how the run ended, whether it is reproducible; for a branch, a `[parent]` table with the parent's chain value, record count and instant at the branching (RFC 009 rule 12: provenance, needed by nothing) |
| `trace/N.bin` | The trace records, with `--trace`: length-prefixed records in the canonical encoding |
| `payloads/<digest>` | The payload bytes by digest, with `--payloads` |
| `diagnostic.log`, `diagnostic.bin` | The diagnostic log as JSON lines and in the canonical encoding |
| `README.md` | What the archive is |

The `chain` is the digest of the whole trace: two runs with the same inputs give the same chain, on every supported engine. A record is enough to replay; it is not enough to display payloads unless they were kept.

A branch (RFC 009 section 4) is a new run from the current instant of the current run, started by the `branch` command of the control interface: its inputs are the parent's up to that instant, its own after; the parent ends there and its record, with the trace it kept, is held by the Host until the next run starts (`export { kind: parent-record }`). The branch's record carries the `[parent]` table and the chain point of the branching; `florn replay` of either record needs nothing of the other. A branch begins at its instant: moving back stops there.

## Scenarios with recorded digests

`scenarios/` holds fifteen scenarios over the SDK's guests, the built guest modules under `scenarios/modules`, and `digests.toml` with the chain, the record count and the end of each run, plus every module's source and admitted hash. To check a machine:

```sh
florn run scenarios/echo-pair.toml --modules scenarios/modules --seed 42
```

and compare `chain` with the file; `scenarios/README.md` carries a loop over all of them. The conformance kit does the same in CI on x86-64, AArch64 and Apple Silicon, and the test page does it in Chromium, Firefox and WebKit.

## The conformance kit and the test page

`cargo test -p florn-conformance` runs the acceptance criteria and the verification rows of the RFCs natively; `cargo run --release -p florn-conformance -- figures` prints the benchmarks. The browser Host is driven by `ci/browser/run.js` over the test page (`crates/florn-ui-web/README.md`).

## Identity of a run

A run is identified by its inputs (RFC 001 §9): the scenario's canonical hash, the modules' hashes, the admission and cost-model versions, the contract versions, the root seed, and the interventions. The environment (engine, architecture, Host build) is recorded beside them so that a comparison knows which tier it is making: the same engine, two engines, or two architectures.
