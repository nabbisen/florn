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
| `pid`, `bad_controller` (the `florn-guest-circuit` examples) | The sampled controller of RFC 012 and a controller that traps, sets a NaN and reports on two channels | `circuit_motor_loop.rs`; the shipped `motor-loop` |
| `networked_pid` (the `florn-guest-cps` example) | `pid` on a network: its set point arrives as a frame on an interface, in the composed world of RFC 012 rule 22 | `circuit_cps.rs`; the shipped `cps-loop` |
| `guest_diode`, `bad_device` (the `florn-guest-device` examples) | A diode as a device model, and a conductance that returns a NaN, a wrong length, a flag that is no flag, sets a timer or a trap, or never converges | `circuit_device.rs`; the shipped `diode-clip`'s twin `guest-diode`, `hostile-stamp`, `hostile-diverge` |
| `net_echo`, `distance_vector`, `ospf`, `bgp` (the `florn-guest-network` examples) | The network ladder of RFC 008 and RFC 011 | `network.rs`, `ladder.rs`, `ospf.rs`, `bgp.rs`; the shipped `net-*`, `dv-*`, `ospf-*` and `bgp-*` scenarios |

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
| Compound activation | `circuit_device` (the circuit's device model, RFC 012 rules 19 to 21): a step the solver rejects leaves the guest's committed state and its reports as they were (`a_rejected_step_leaves_what_the_guest_reported_as_it_was`), every attempt is one activation closed once (`florn-domain-circuit`'s `transient::tests::device`), a result that is no stamp, a timer, a send and a trap are faults of the device and the plant continues |
| Two domains | `circuit_cps` (RFC 012 rule 22, RFC 002 rule 48): the module of the composed world is admitted when the Host holds both domains and refused with one, a scenario that uses it declares both, each domain validates its own ports, the network delivers the frame and the circuit the samples; before it, `composed_worlds` over the test domains |
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
| Export and restore (AC-006) | `ac::ac_006_…` natively; in the browser, `ci/browser/run.js` restores a native checkpoint in every browser and a browser checkpoint natively (RFC 009 rule 6) |
| Versions | `rfc_003::a_checkpoint_of_another_version_is_refused_naming_the_field`; `cost_model_2.rs` for version 2 against version 1 (RFC 010) |
| Cost benchmark | `rfc_003::the_cost_benchmark_produces_its_figures`; the figures from `figures` |
| NaN probe | `rfc_003::the_nan_probe_reports_five_patterns`; the bits per architecture from `figures` in CI |

### RFC 008 to 011

The later RFCs have their tests by file rather than by table: `rfc_008.rs` and `network.rs` (the domain interface and the link model), `ladder.rs`, `ospf.rs` and `bgp.rs` (the guests), `display.rs` (the display stream), `checkpoints.rs`, `navigation.rs`, `branching.rs`, `swapping.rs` and `retention.rs` (RFC 009), `cost_model_2.rs` (RFC 010), `ui.rs` (the view model over the real Host). Each test names the rule it verifies in its name or its comment.

### RFC 013 rules 19 to 21 and section 11, results across engines (PR-48)

The circuit's results are compared across engines through files the kit records on x86-64 with `cargo run --release -p florn-conformance -- scenarios`: `scenarios/results/<name>.results` for each circuit scenario (RFC 013 rule 21) and `scenarios/results/bit-identity.table` (rule 19, control 4). Every other Host and engine is compared to them by rule 20 as a second run, through `florn_host::compare::golden`, the one function native tests, the runner's kit and the browsers' worker all call. `florn-conformance cross-engine` prints this machine's row of the table (the browsers' rows come from `ci/browser/run.js`, in the same form); `figures` prints it with the figures.

| Property | Test |
|---|---|
| Every circuit scenario against its file, on every architecture | `circuit_results::every_circuit_scenario_agrees_with_its_results_file_and_matches_where_it_was_recorded`; the three Rust jobs of CI (x86-64, AArch64 Linux, AArch64 macOS) and the browser jobs (Chromium, Firefox, WebKit) print their rows |
| A scenario with no file, a file with no scenario | `circuit_results::every_circuit_scenario_has_a_results_file_and_every_file_has_a_scenario` |
| The file is the run it records, and stays so | `circuit_results::a_results_file_is_the_trace_of_its_digests_run`, `…the_recorded_results_are_what_the_native_host_gives_here_…` (it fails on a stale file) |
| The comparison fails on a planted difference | `circuit_results::the_controls_of_the_comparison_…` (a last bit: `within-tolerance`; another value, one record fewer: `diverged`); `florn-host`'s `compare::golden` tests; a sensor's send on a port connected to nothing is a number (`a_sensors_send_is_a_number_…`) |
| `bit_identity` | `circuit_results::bit_identity_agrees_with_the_recorded_table`; `…_fails_on_a_doctored_file_and_on_a_doctored_function`; the crate's `bits` tests; in the browsers `ci/browser/run.js` |
| It fails when the crate calls a `libm` function the table does not list | `circuit_results::every_libm_call_goes_through_crate_math_and_names_a_function_of_the_table` (reads the crate's sources with a lexer that knows comments and strings; every `libm` call is in `src/math.rs`), its own control `…_fails_on_every_form_a_call_can_take`, and the compiler's check: `clippy.toml` disallows every other `libm` function |
| The lint list | `ci/check-float-methods.py` (the list holds every stable `f64` and `f32` method of the pinned toolchain outside the allowed operations, and every `libm` function the table does not name; a toolchain that adds a method fails it); `ci/test-gates.sh`: the crate's `clippy.toml` in a scratch crate with every path it lists used, the allowed operations and the table's three functions passing, and an `allow` failing under the crate's `#![forbid(clippy::disallowed_methods)]` |
| The dependency allow-list | `ci/test-gates.sh` (PR-44): a second external dependency, a range, a development dependency; `libm` with a feature or its default features (PR-49) |
| The flag script | `ci/check-float-flags.sh`; `ci/test-gates.sh`: `target-cpu`, `target-feature`, `-Z`, an LLVM argument, fast-math and a nightly toolchain, each in the file a build reads it from; `[unstable]`, `RUSTC_BOOTSTRAP`, `rustc` wrappers and a code-generation backend |
| `florn replay` across Hosts | `crates/florn/tests/circuit_replay.rs`: a circuit record without payloads whose chain differs prints `needs-payloads` and exits 1; on one build it is `diverged`, exit 2 |

### RFC 012 rules 9 and 22, two domains in one actor (PR-50)

| Property | Test |
|---|---|
| The contract of record `florn:cps@0.1.0` equals the RFC's block and parses over the three packages it uses | `florn-abi`'s `contract_of_record::cps_wit_equals_rfc_012_block_three`, `…_parses_as_package_florn_cps_over_the_three_packages`; the SDK's copy `florn-guest-cps`'s `contract_copy` |
| The module is admitted with both domains and refused with one; a scenario that uses it declares both | `circuit_cps::the_module_is_admitted_on_a_host_that_holds_both_domains_and_refused_on_one`, `…a_scenario_that_uses_the_world_declares_both_domains` |
| Each domain validates the ports of its own types | `circuit_cps::each_domain_validates_the_ports_of_its_own_types` |
| The set-point frame changes the plant from the tick it was committed and not before | `circuit_cps::a_frame_between_two_samples_changes_the_plant_from_the_tick_it_was_committed_and_not_before` (the samples up to the frame are bit-identical to a run without it, the next differs), `…the_frame_answers_at_once_and_the_next_sample_carries_on_from_its_set_point` |
| The network delivers the frame, the circuit the samples | `circuit_cps::the_frame_is_delivered_by_the_network_domain_and_the_samples_by_the_circuit_domain`; `…a_frame_from_a_peer_over_a_link_reaches_the_controller_through_the_network_domain` (a `net_echo` router on a link sends the frames; the loop follows them) |
| A frame that is no set point is ignored and reported | `circuit_cps::a_frame_that_is_not_a_set_point_is_ignored_and_says_so` |
| The loop with the controller on a network is the loop it derives from | `circuit_cps::the_loop_with_its_controller_on_a_network_settles_as_the_loop_it_derives_from` (the speeds are bit-identical to `motor-loop`'s) |
| The frame is an input: a replay matches, a restore across it continues | `circuit_cps::a_run_with_the_frame_replays_…`, `…a_run_restored_across_the_frame_…` |

### RFC 012 rules 17 and 19 to 21 and RFC 013 rules 8, 9, 16 and 17, the device model (PR-49)

| Property | Test |
|---|---|
| `guest-diode` against `diode-clip` on the voltmeters' samples | `circuit_device::guest_diode_and_the_built_in_diode_agree_on_the_voltmeters_samples` (within the profile's tolerance, pair by pair; the largest difference is printed: 0 V, the two are bit-identical); `florn-domain-circuit`'s `a_guest_diode_is_the_built_in_diode_to_the_last_bit` (the same stamps in the same order); the shipped scenarios' digests and results files in every engine |
| Each mode of `bad-device` is a Fault keyed by the advance, with `circuit.stamp-invalid` where the result was no stamp | `circuit_device::each_result_that_is_no_stamp_…` (`nan`, `short`, `long`, `flag`: parameter 12020), `…a_timer_and_a_send_in_a_device…` (12019), `…a_trap_in_a_device…`, `…the_fault_record_is_keyed_by_the_advance…`, `…a_fault_in_the_initial_solve_is_keyed_as_the_devices_init…` |
| The plant continues without the device; under `stop-run` the run ends with `actor-fault` | the same tests; `circuit_device::under_stop_run_a_device_fault_ends_the_run_with_actor_fault` |
| `diverge` ends with `non-convergence`, and under `hold` continues frozen | `circuit_device::stamps_that_never_converge_…`, `…under_hold_the_plant_freezes_…`; the attempts of the step never exceed `max-rejections + 1` |
| The initial solve of a guest device leaves one record, keyed as its `init`; the solves that do not give the result leave none | `circuit_device::the_device_leaves_one_record_per_accepted_step_…`; `florn-domain-circuit`'s `the_dc_solve_opens_the_device_in_dc_form_…`, `a_failed_stage_is_rejected_and_only_the_solve_that_gives_the_result_is_accepted` |
| The device's records carry the advance's key and the device as target | `circuit_device::the_device_leaves_one_record_per_accepted_step_…` |
| A result of the wrong length is a Fault with the parameter 12020, and one over the bound is never lifted | `circuit_device::each_result_that_is_no_stamp_…` (`short` and `long`); `florn-domain-circuit`'s `device::tests` (the handle's refusal) |
| `two-devices` is refused with `circuit.device-limit` from the start hook | `circuit_device::two_devices_are_refused_…` |
| A guest device that reports limiting while the criteria hold takes one more iteration | `florn-domain-circuit`'s `a_device_that_limits_takes_one_more_iteration_than_one_that_does_not` |
| A checkpoint restored before and after the loss of the device, and a replay | `circuit_device::a_run_restored_before_and_after_the_loss_of_the_device_…`, `…a_replay_of_a_run_with_a_device_…` |

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
