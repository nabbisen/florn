# florn-conformance

The conformance kit (requirement AUTO-007): it checks a guest against its contract and a Host against RFC 001, and it carries the benchmarks whose figures the release notes publish.

## Running it

```sh
cargo test -p florn-conformance                        # every test, under the native Host
cargo run --release -p florn-conformance -- figures    # the benchmarks' figures, as a table
cargo run --release -p florn-conformance -- scenarios  # rewrite scenarios/ and its digests
```

The tests build the SDK's example guests for `wasm32-unknown-unknown` with the Rust toolchain alone, into `target/guests`, and run the behaviour tests over that build. Set `FLORN_GUESTS` to a directory of built modules to use those instead. The shipped scenarios run over the modules shipped in `scenarios/modules`, which `scenarios` rewrites from a fresh build; the same source built on an AArch64 host gives other bytes than on x86-64 (measured in CI), so the digests are recorded over shipped bytes and `figures` reports how the machine's own build compares.

The browser Host runs the same scenarios through its test page: `ci/browser/run.js` (see `crates/florn-ui-web/README.md`).

## The guests

| Guest | What it does | Used by |
|---|---|---|
| `echo` | Echoes every message; reports a random value on its timer | AC-001, AC-002, AC-004 |
| `pinger` | Sends a payload on a timer, or once at init; echoes when told to | The initiator of every pair |
| `probe` | Reports NaN bit patterns computed inside the guest | RFC 003 section 9 |
| `hostile_loop`, `hostile_recurse`, `hostile_grow`, `hostile_flood`, `hostile_bad_pointer` | One fault kind each | AC-002 |
| `hostile_wasi` | Imports a WASI function | Refused at admission |
| `timers` | Sets timers in init as its mode says; reports every expiry | The init and timer rows |
| `chatter` | Logs, draws, sends, reports and traps as its parameters say | The parameter, effect, randomness, telemetry and quota rows |
| `heap` | Grows its heap every activation; calls through a function pointer | AC-006 |
| `clock` | A `std` guest that reads the system clock | No ambient access |
| `bench_protocol` | Parses a 1,500-byte buffer, updates a 1,000-entry map, replies | The cost benchmark, B1 |
| `bench_compute` | Multiplies two 64×64 matrices | The cost benchmark, B2 |
| `realloc_loop` (text, in the kit) | A `cabi_realloc` that loops | Budget before lowering |
| `crossing` (text, in the kit) | Returns at once; echoes a payload | The crossing benchmark |

## The tables

Where a row is proven by another crate's own test, the row names it; the kit does not repeat it.

### RFC 001 section 14

| Property | Test |
|---|---|
| Tier 1 | `rfc_001::tier_1_one_scenario_twice_under_one_host`; `ac::ac_001_…` |
| Tier 2, discrete | `ac::ac_001_…`: the recorded digests of `scenarios/` compared in CI on x86-64 and AArch64, and by `ci/browser/run.js` under the browser Host in Chromium, Firefox and WebKit |
| Order | `ac::ac_004_…` |
| Fairness of `by-seed` | `ac::ac_004_…` |
| Zero delay | `rfc_001::zero_delay_ends_the_run_with_the_fault_of_rule_13` |
| Activation | `rfc_001::an_activation_that_faults_commits_nothing` |
| Streams | `rfc_001::a_third_actor_changes_no_draw_of_the_first_two` |
| Interventions | `rfc_001::a_run_with_an_intervention_replays_to_the_same_digest` |
| Observation | `rfc_001::observation_changes_nothing` |
| Machine conditions | `rfc_001::memory_within_the_limit_is_never_refused` |
| Checkpoint | `ac::ac_006_…` |

### RFC 002 section 10

| Property | Test |
|---|---|
| One artifact | `rfc_002::one_artifact_admits_to_the_recorded_hash`; the browser run compares every module's admitted hash with the same record |
| Strict lifting | `rfc_002::a_list_outside_memory_faults_as_contract`; invalid UTF-8 and an enumeration out of range, which the core world never carries from a guest, in `florn-abi`'s `strict_lifting_produces_the_three_contract_faults` |
| Manifest | `rfc_002::a_module_without_a_manifest_or_with_disagreeing_sections_is_refused` (no manifest; a world the Host does not have; a scenario naming another contract); two sections of one module that disagree in `florn-admission`'s `the_manifest_sections_are_required_and_checked`; the runner's listing in `ac::ac_005_…` |
| Parameters | `rfc_002::parameters_are_checked_and_reach_the_guest_in_order_with_defaults` |
| Port kinds | `rfc_002::port_kinds_are_checked_against_the_declaration` |
| Init | `rfc_002::init_sets_a_timer_that_fires_at_its_delay_and_runs_in_identity_order`; an actor created by an intervention waits for that intervention kind |
| Timers | `rfc_002::timers_cancel_replace_and_count_against_the_quota` |
| Effects | `rfc_001::an_activation_that_faults_commits_nothing` |
| Randomness | `rfc_002::a_fault_restores_the_stream_to_the_start_of_the_activation`; `rfc_001::a_third_actor_changes_no_draw_of_the_first_two` |
| Telemetry | `rfc_002::telemetry_channels_and_quota_are_enforced_and_text_reaches_the_log` |
| Compound activation | Waits for a domain world (Phase 2) |
| Two domains | Waits for a domain world (Phase 2) |
| No ambient access | `rfc_002::no_ambient_access` |
| SDK | Every test: the guests are built from the SDK with the toolchain alone |

### RFC 003 section 10

| Property | Test |
|---|---|
| Same fault point everywhere (AC-002) | `ac::ac_002_…`; the browser run checks each hostile scenario's fault kind and digest in every browser |
| Transformed bytes | `rfc_003::every_kit_module_admits_to_its_recorded_hash`; `ac::ac_001_…`. The admitted bytes depend on the class limits (the memory maximum of RFC 003 section 3, pass 1); the recorded hashes are for the kit's scenarios' limits |
| Budget before lowering | `rfc_003::a_looping_realloc_faults_as_budget_not_as_a_host_error` |
| Classification | `ac::ac_002_…`; `florn-admission`'s `classification_reads_globals_only` |
| Custom sections | `florn-admission`'s `custom_sections_are_preserved_byte_for_byte` |
| Validation | `florn-admission`'s `every_refusal_of_section_2_has_its_code` |
| Quotas | `rfc_003::each_quota_faults_one_above_and_passes_one_below`; `rfc_002::timers_…`; `rfc_002::telemetry_…` |
| Memory | `rfc_001::memory_within_the_limit_is_never_refused` |
| Estimate before instantiate | `rfc_003::an_estimate_above_capacity_is_refused_before_anything_is_instantiated` |
| Capacity probe | the runner's `probe_prints_the_capacity_and_bad_arguments_print_usage` (`crates/florn/tests/cli.rs`) |
| Export and restore (AC-006) | `ac::ac_006_…` natively; the browser half waits for the restore command of release 0.3, since the control interface of 0.1 carries no restore |
| Versions | `rfc_003::a_checkpoint_of_another_version_is_refused_naming_the_field`; cost-model version 2 does not exist yet |
| Cost benchmark | `rfc_003::the_cost_benchmark_produces_its_figures`; the figures from `figures` |
| NaN probe | `rfc_003::the_nan_probe_reports_five_patterns`; the bits per architecture from `figures` in CI |

### RFC 005 section 11, the rows that apply in 0.1

| Property | Test |
|---|---|
| One core | `ci/check-deps.py` |
| Tiers | `ac::ac_001_…`, `rfc_001::observation_changes_nothing` (through the control interface) |
| Engine adapter rules | `rfc_005::the_crossing_benchmark_produces_its_figures`; the figures from `figures`; in the browser, the driver's crossing figures and its check that the worker made no view per lowering |
| Payload digests | `ac::ac_003_…` (the record's payload digests); the kernel-path figure from `figures` |
| Replay | `ac::ac_003_…` (matched, and a divergent record names its interval); the runner's exit code 2 test; the browser driver's replay check |
| Interventions | `rfc_001::a_run_with_an_intervention_replays_to_the_same_digest` |
| Control interface | `florn-host`'s `tests/control.rs` |
| Run record | `ac::ac_003_…` |
| Capacity | `rfc_003::an_estimate_…`; the runner's probe test |
| Native cache | `rfc_005::the_second_run_of_a_scenario_does_not_compile_the_module` |
| Instance failure | the browser driver's overflow check: `host.instance-failed`, the run ends, the page lives |

## The figures

`figures` prints, for the engine and architecture it runs on: the crossing benchmark (the call floor, 64 bytes, 1,500 bytes, 64 KiB; metered and unmetered) beside the Stage 0 baseline of RFC 005 rule 11; the cost benchmark of RFC 003 section 7 (B1 and B2, 10,000 activations, metered against unmetered); the kernel path (one activation of the Host-path scenarios); the NaN probe's bits; and the machine's own build of the guests against the shipped modules, in bytes and in the digests of the shipped scenarios. CI runs it on three architectures; the release notes copy the table.
