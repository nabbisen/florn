# Phase 1 — Acceptance and QA checklist

Entry point: [`README.md`](./README.md). For the verifier. Each item is a procedure with an expected result; the result is recorded, with the commit it was run against, in `.git-exclude/reviewed/phase1/<item>/README.md`. An item passes only when it passes on every engine it names.

## A. The acceptance criteria of release 0.1 (requirements §22.1)

| ID | Procedure | Expected | Engines |
|---|---|---|---|
| AC-001 | `florn run scenarios/echo-pair.toml --seed 42` natively on x86-64 and AArch64; the same scenario through the browser test page in Chromium, Firefox, and WebKit (CI jobs). Compare the printed digests. | One digest, identical in all five. | Wasmtime ×2, three browsers |
| AC-002 | Run the five hostile scenarios of the kit (loop, recursion, memory growth, flood, bad pointer) under each Host. Read the fault kind, the metered work at the fault, and the benign neighbour's digest. | Same fault kind and metered work under every engine; the neighbour's digest equals its digest in the scenario without the hostile actor; the Host prints a result and exits normally. | Both Hosts, every engine |
| AC-003 | `florn run` a scenario, then `florn replay` its run record. Then alter one byte of a module file and run the scenario again. | Replay reports "matched at every stored point". The altered module is refused with `module.hash-mismatch` naming both hashes. | Native |
| AC-004 | Run the two-origin race scenario of the kit with actors declared in one order, then in the reverse order, under `by-identity` and under `by-seed`. | Same digest for both orders under each policy; the `by-seed` fairness test reports each origin winning 45–55% of 10,000 races. | Native and one browser |
| AC-005 | `florn admit` on a module without a manifest; on the echo guest. | The first prints `module.manifest-missing` with its parameters; the second prints the manifest: contract, kind, port kinds, parameters with types and units, channels, needs, and the admitted hash. | Native |
| AC-006 | The kit's export test: 10,000 activations uninterrupted, against export after 5,000 into a fresh instance, against export to bytes and restore in a new process. | Equal digests over the corresponding records; export and restore each under 5 ms per MB. | Both Hosts |
| AC-007 | Run the conformance kit entirely: `cargo test -p florn-conformance` natively and the browser job in CI. | Every test passes; the published figures are produced. | Both Hosts, every engine |

## B. Per-crate checks (the PR plan's "verifier checks")

| ID | Crate | Procedure | Expected |
|---|---|---|---|
| C1 | workspace | Inspect CI for the PR: three architectures, licence gate, docs, dependency direction, WIT equality. Change one character in a WIT file locally and run the test. | All green; the WIT test fails on the change. |
| C2 | `florn-model` | Run the crate's tests. Feed the worked example of RFC 004 §8 and its variations from §9 (comment, key order, `views`; one identity changed; `delay = 5`; `tick = "1ns"` with `1.5ns`). Feed a manifest declaring 10⁷ actors. | Same hash for formatting changes; different hash for content; each refusal with its code and parameters; the large manifest refused in bounded memory. |
| C3 | `florn-kernel` | Run the crate's tests; read the test names against RFC 001 §14's rows that need no engine. | Order, fairness, zero delay, streams: present and passing. |
| C4 | `florn-abi` | Run the crate's tests; inspect that invalid UTF-8, out-of-range list, and out-of-range enum each produce `fault.contract` with the rule named. | Present and passing. |
| C5 | engine adapters | Run the adapter conformance file under Wasmtime (PR-05) and in the browser (PR-12). Run the crossing benchmark. | Same results; figures within 1.5× of the Stage 0 baseline per engine. |
| C6 | `florn-admission` | Admit every kit module natively and (from PR-12) in the browser; compare the admitted hashes. Feed one module each with a start section, a second memory, a `wasi:` import, a SIMD instruction, a `florn.stack` import, `table.set`, initial memory above the limit. | Hashes identical; each refusal with its code. |
| C7 | `florn-host` | AC-002, AC-003, AC-004, AC-006 natively. Inspect the dependency tree of `florn-host`. | Pass; no platform or engine crate in the tree. |
| C8 | `florn-control` | Send each command with one invalid parameter; send with a wrong interface version. | `control.invalid` naming the field; `control.version`; no state change. |
| C9 | `florn-cli` | AC-005; run every kit scenario by a shell loop and compare with recorded digests. | Pass. |
| C10 | `florn-guest` | On a clean machine or container with only the Rust toolchain and the `wasm32-unknown-unknown` target: create a guest from the template, build, `florn admit` it, run it in the echo scenario. | Builds, admits, runs; its manifest shows the declared parameters. |
| C11 | kit, native | AC-001 (native part), AC-002, AC-003, AC-004, AC-006, AC-007 natively on x86-64 and AArch64 (CI). | Pass on both. |
| C12 | browser Host | AC-001 (browsers), AC-002, AC-007 in the browser; the network log of the test page. | Pass; the network log holds only the page's origin; the page is cross-origin isolated. |
| C13 | figures | Read the benchmark outputs that set the defaults, the maxima, the interval, and the hash choice; check that RFC 003 rule 19, RFC 006 §7, and RFC 005 rules 22 and 23 were amended with the figures. | Figures and amendments agree. |

## C. The RFC 006 security review for the release

Run the seven steps of RFC 006 rule 5 and record each: data flows added (none expected in 0.1 beyond the run record), the controls' tests, `cargo deny` and `cargo audit` output, the fuzzing run of rule 7 with its duration, the forbidden-words check, the file hashes in the release notes.

## D. How to record

One file per item, `.git-exclude/reviewed/phase1/<ID>/README.md`, with: the commit hash, the command lines run, the output or a pointer to the CI run, pass or fail per engine, and any question for the architect with the RFC rule it concerns. A failed item is a question, not a fix: the developer and the architect answer it in the PR.
