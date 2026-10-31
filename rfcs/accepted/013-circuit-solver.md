# RFC 013 — The circuit solver: modified nodal analysis, Newton iteration, transient integration, step control, and the tolerance of Tier 2

**Status.** Accepted 2026-10-31, the owner's word on the day it was proposed, as proposed; for release 0.4.0 (Phase 4 of the roadmap). Companion of RFC 012, which owns the domain's types, contract and time coupling; this RFC owns the numbers: how the plant is formulated, solved, stepped and compared across Hosts.

**Tracks.** Requirements PHY-003, PHY-004, PHY-005, PHY-006, PHY-010, PHY-012, NUM-001, NUM-002, NUM-003, NUM-005, NUM-006, AUTO-004, AUTO-005, DES-006, NFR-004. Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** The solver profile `[circuit]` of the scenario; the formulation and the device interface every built-in and every device guest stamps into; integration methods and their history; Newton iteration and its limiting; the step controller and its bounds in ticks; the location of a detector's crossing; failure and non-convergence; the memory estimate at admission; the arithmetic rules that make results identical across engines by construction, and the tolerance and comparison method promised when they are not (RFC 001 rule 62); the goldens of the kit.

## 1. What the solver promises

The solver is Host code of `florn-domain-circuit`. It advances the plant from one tick to a later one (RFC 012 rule 7) with the textbook methods of PHY-003 to PHY-005 and nothing cleverer: the point of 0.4 is a controller lab and a test of the core, not a competitor to a circuit simulator. Everything it does is a deterministic function of the run inputs and of the sequence of held signal values the queue hands it (RFC 012 rules 10 and 11); the only source of difference between two Hosts is floating-point arithmetic, and section 7 closes that gap as far as the engines allow and states the tolerance for the rest.

## 2. The solver profile

**Rule 1.** The scenario's `[circuit]` table (RFC 012 rule 6) is the solver profile of DATA-001: the only place a learner tunes the solver (NUM-005: "the display rate is never the step"). Every field is optional and has the default below; an unknown key is `circuit.attr-unknown`; a value of the wrong type or outside its range is `circuit.attr-type` naming the expectation. Time fields are whole numbers of ticks (RFC 004 rule 25).

**Rule 2.** The fields:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `method` | `"trapezoidal"` or `"gear2"` | `"trapezoidal"` | The integration method of rule 6 |
| `step` | time | `1us`, or the tick if larger | The initial and, under `fixed`, the only step |
| `control` | `"fixed"` or `"adaptive"` | `"adaptive"` | Rule 10 |
| `min-step` | time | the tick | The smallest step the controller may take (rule 13) |
| `max-step` | time | `100 × step` | The largest |
| `ltol` | real, `(0, 1]` | `1e-3` | The relative local truncation tolerance of rule 11 |
| `newton.max-iterations` | integer, `1..=1000` | `50` | Rule 8 |
| `newton.abstol` | real, A | `1e-12` | The current residual below which a node is converged |
| `newton.reltol` | real | `1e-6` | The relative voltage change below which an unknown is converged |
| `newton.vntol` | real, V | `1e-9` | The absolute voltage change below which an unknown is converged |
| `gmin` | real, S | `1e-12` | The conductance from every node to ground that keeps the matrix regular (rule 4) |
| `on-non-convergence` | `"stop-run"` or `"hold"` | `"stop-run"` | Rule 17 |
| `max-unknowns` | integer | `1024` | The bound of rule 18 (NUM-003) |
| `tolerance.atol` | real | `1e-9` | Rule 20: the absolute tolerance of the comparison, in the result's unit |
| `tolerance.rtol` | real | `1e-6` | Rule 20: the relative tolerance |

The profile is part of the scenario and so of the run identity (RFC 004: the canonical form hashes every table); two runs with different profiles are different runs.

## 3. Formulation

**Rule 3.** The unknowns `x` of modified nodal analysis (PHY-003), in a fixed order that depends on identities only (RFC 001 rule 16): the voltage of every `net` but the ground, in ascending connection id; then the current of every element whose current is an unknown (`voltage-source`, `inductor`, `ammeter`, the `dc-motor`'s armature), in ascending actor id; then every extra state an element declares (the `dc-motor`'s speed), in ascending actor id. A guest device contributes no unknown: its pins are on nets and its currents are stamped (rule 5).

**Rule 4.** The equations: Kirchhoff's current law at every non-ground net, the branch equation of every current unknown, and the state equation of every extra state. A conductance `gmin` from every net to ground is always present so that a floating net has a solution; it is in the profile because a learner who builds a floating subcircuit sees `gmin`'s effect and can turn it down to see the trouble.

**Rule 5.** The device interface, the same for a built-in in Host code and for a guest of the `device` world (RFC 012 rule 19): given the trial voltages of its pins (and, for an element with an unknown current or state, the trial values of those), a device returns the currents into its pins and the Jacobian of those currents with respect to the trial values, each in SI units, as the stamps the solver adds into the matrix and the right-hand side. A reactive element's stamps are those of its companion model under the integration method in force (rule 7), which the solver hands the device as the pair `(alpha, history)` it needs — a guest device receives `h` in `begin-step` and keeps its own history; a built-in asks the solver for the method's coefficients. A device stamps nothing but its own rows and columns (ARCH-006 within the domain: the matrix is the solver's).

## 4. Integration

**Rule 6.** Two methods (PHY-005), chosen by the profile: **trapezoidal** (second order, A-stable, the default, because learners' RC and motor plants are smooth) and **Gear's second-order backward differentiation** (`gear2`, stiffly stable, for switching plants where trapezoidal rings). Both are fixed-leading-coefficient forms with the history the method needs: the previous solution and its derivative for trapezoidal; the two previous solutions for Gear with the step-ratio correction when `h` changes. The history is state of the domain (RFC 012 rule 29) and part of a checkpoint.

**Rule 7.** The companion model: a capacitor's current at the end of a step is `c · (alpha · v_{n+1} − history)`, with `alpha = 2/h` and `history = 2/h · v_n + dv/dt_n` under trapezoidal, `alpha = 3/(2h)` and `history = (4 v_n − v_{n−1})/(2h)` under Gear 2 at constant `h`; an inductor's voltage likewise in its current; the motor's mechanical equation likewise in its speed. The first step after init, after a restore with no history and after a `reject` that halved below the history's step uses backward Euler for one step (first order; the history is rebuilt from it). The method's coefficients are computed once per step from `h` in seconds (rule 14) and are the same on every Host because `h` is an integer tick count times the tick length, both exact.

## 5. Nonlinear iteration

**Rule 8.** Newton-Raphson (PHY-004) at every step: starting from the predictor (the previous solution, or the linear extrapolation under `adaptive`), iterate `J · dx = −f`, `x += dx`, calling every device's `evaluate` once per iteration with the trial values in a fixed order (rule 3). Convergence: for every unknown the change satisfies `|dx| ≤ reltol · max(|x|, |x_prev|) + vntol` (currents use `abstol` as the absolute term), and every KCL residual satisfies `|f| ≤ abstol`. Limiting: a diode's junction voltage change per iteration is bounded by the critical-voltage rule of SPICE (`n · vt · ln(…)`), so that `exp` never overflows; the limited value is the trial value for every device at that iteration, so a guest device sees the same trial as a built-in would. More than `max-iterations` iterations is a convergence failure (rule 16).

**Rule 9.** The operating point at init (and after `load-checkpoint` with no state): a DC solution with every capacitor open, every inductor shorted and every state at its initial condition (`ic`; a signal's `initial`), by Newton from zero; on failure, `gmin` stepping (from `1e-3` S down to the profile's `gmin` by decades), then source stepping (every source scaled from 0 to 1 in ten steps); on failure of all three the scenario is refused at run start with `circuit.no-operating-point`, which is a validation of the plant a learner can act on (UI-009). The operating point is tick 0's state; the first `solver-step` is scheduled from it (RFC 012 rule 8).

## 6. Step control

**Rule 10.** Two policies (PHY-010): **fixed**, every step is `step` ticks, shortened only to end at a horizon (RFC 012 rule 8), and a convergence failure is a non-convergence at once (rule 17); **adaptive**, the default, in which the step controller chooses `h` by rules 11 to 13.

**Rule 11.** The local truncation error estimate per step: the difference between the predictor and the corrector for every unknown, scaled by the method's constant (`1/3` for trapezoidal, `2/9` for Gear 2, as the standard derivation gives for the differentiation-based estimate), divided by `ltol · max(|x|, floor) + atol_x`, where `floor` is `vntol` for voltages and `abstol` for currents; the step's error ratio `r` is the largest over the unknowns.

**Rule 12.** Accept and reject: a step with `r ≤ 1` and converged Newton is **accepted**, its devices' compound activations committed (RFC 012 rule 20), and the next `h` is `h · min(2, max(0.5, 0.9 · r^(−1/3)))` under trapezoidal (`−1/3` for a second-order local error), rounded down to whole ticks and clamped to `[min-step, max-step]`. A step with `r > 1` is **rejected**: the devices' activations are discarded (RFC 012 rule 21), `h` becomes `max(min-step, floor(h · 0.5 · r^(−1/3)))` ticks, and the step is attempted again from the same start. A convergence failure (rule 16) rejects with `h / 4`.

**Rule 13.** Bounds, in ticks. `h` is an integer number of ticks, never below `min-step` nor above `max-step`, and never beyond the horizon (RFC 012 rule 8): the last step before a horizon is shortened to end exactly there, and if the shortened step is below `min-step` the previous step is shortened instead so that both are at least `min-step`, unless the horizon is less than `min-step` from the start, in which case one step of that size is taken (the only case a step is below `min-step`, and it is recorded as such in the `solver` event of rule 17).

**Rule 14.** Time in the arithmetic: a step of `n` ticks is `h = n · tick` seconds, where the tick length is the scenario's exact decimal converted to the nearest f64 once at composition; `now` is never converted (the solver never needs absolute time, only `h` and the history). This keeps every step's coefficients identical across Hosts and keeps accumulated rounding out of the plant: the ticks accumulate, not the seconds.

**Rule 15.** Locating a detector's crossing (RFC 012 rule 13): after an accepted step from `t_0` to `t_1`, if a detector's condition differs between the two ends, the step is rejected as if by error (its devices discarded) and retried with `h` halved, repeatedly, until the step ends at the first tick at or after the crossing (the condition is false at the step's start and true at its end, and the step is one tick, or the crossing is bracketed to one tick by at most forty halvings — the bound a 64-bit tick count needs). The `condition` event is scheduled at that tick. A detector therefore costs steps around a crossing and nothing elsewhere.

## 7. Failure

**Rule 16.** A convergence failure is Newton reaching `max-iterations` without rule 8's criteria, or a device returning a non-finite stamp (which is the device's contract violation `circuit.stamp-invalid`, RFC 012 rule 20, and the device faults; the step is then rejected and retried without it, RFC 012 rule 17). Under `adaptive` the step is rejected with `h / 4` (rule 12); under `fixed` it is a non-convergence.

**Rule 17.** Non-convergence is a convergence failure at `h = min-step` (or under `fixed`). The domain records a `solver` event (class 9, RFC 012 rule 14) with the code `circuit.non-convergence` (`actor: name` of the device with the largest residual, `tick: tick`, `iterations: count`) and acts by the profile: `stop-run` ends the run with the end reason `non-convergence` (RFC 007's `end-reason` gains it; RFC 012 finding F-7), its state at the last accepted step, which is the actionable state of UI-009; `hold` freezes the plant at the last accepted step, keeps the sampled signals at their held values, lets every controller keep sampling, and records one `circuit.non-convergence` per further horizon, so that a learner can inspect what the controller does to a plant that stopped answering. Observation short of failure: a rejected step is `circuit.step-rejected` (info; `tick`, `step: ticks`, `ratio: text`), a step at `min-step` is `circuit.step-minimum` (warning); both are `solver` events, in the timeline and the log, never guest inputs. The world inspector of RFC 014 shows the current step, the last step's iterations and the rejection count from these.

## 8. Size and memory

**Rule 18.** The estimate at admission (NUM-003; RFC 003's estimate before anything is instantiated): with `n` unknowns of rule 3 and `d` devices, the solver needs `8 · n²` bytes for the dense matrix of rule 22, `8 · 6n` for the vectors and history, and `d` compound activations' bookkeeping; `n > max-unknowns` is refused at scenario load with `circuit.too-large` naming `n`, the bound and the bytes (RFC 012 rule 28). The default bound of 1,024 unknowns is 8 MiB of matrix, far below the browser's ceiling and far above a learner's plant; it is a profile field so that a larger plant on the native Host is one line.

## 9. Numbers across Hosts

**Rule 19.** Arithmetic rules, so that two Hosts compute the same bits wherever the engines allow (NUM-001, NUM-002; RFC 001 rules 56 to 63): the solver uses `f64` with IEEE 754 semantics only; no fused multiply-add is emitted (Rust emits none without the intrinsic, and WebAssembly has no scalar fused operation); no SIMD in the solver (NUM-001 keeps it to the domain's tier, and the tier is "within tolerance", so none is taken); every reduction (a residual norm, a stamp sum into a matrix entry) runs in the fixed order of rule 3; transcendental functions (`exp`, `ln` for the diode, `sqrt` nowhere needed) come from one pure-Rust implementation compiled into both Hosts, never from the platform's math library (RFC 001 rule 60) — the `libm` crate, MIT and Apache-2.0, already permitted by RFC 006 rule 6's list, pinned exactly; the LU of rule 22 pivots by the largest magnitude with the lowest index on ties, a rule that depends on values only; every f64 that leaves the solver (a held signal, a probe's report, a checkpoint) is made canonical (RFC 001 rule 59); a NaN anywhere in the solution is a convergence failure (rule 16), never a result.

Under these rules the kit's figures are expected to show **bit-identical results** on x86-64 and AArch64 natively and in Chromium, Firefox and WebKit, as POC-1 and RFC 003 section 9 found for the guests' arithmetic. Tier 2 nevertheless promises rule 20, not identity: the promise is what the project can hold if an engine's `f64` differs in a corner the kit does not cover.

**Rule 20.** The comparison method of Tier 2 (RFC 001 rule 62; PHY-012). The **results** of a circuit run are the `probe` reports of its sensors (RFC 012 rule 23) and the `control` reports of its controllers, in the trace with their instants. Two runs agree when: every event's tick, class, origin and target match exactly (the integer part of the trace is Tier 1); every `probe` and `control` value `a` of one run and `b` of the other at the same report satisfy `|a − b| ≤ atol + rtol · max(|a|, |b|)` with the profile's `tolerance`, where every NaN is one value (RFC 003 open question 4) and two infinities of the same sign are equal; and every other payload matches byte for byte. The runner's `replay` and the Host's comparison report `matched` when the traces are identical, `within-tolerance` when only reports of a continuous domain differ and all within tolerance, and `diverged` otherwise, with the first differing report named; `within-tolerance` is exit code 0 of the runner with the word printed (RFC 012 finding F-5). The digest of a run is still the digest of its trace: two runs that are `within-tolerance` may have different digests, and the kit's `digests.toml` records, for a circuit scenario, the digest **and** the results file the comparison reads.

**Rule 21.** Goldens (AUTO-004). The kit keeps, for each circuit scenario of RFC 012 rule 27, the `probe` series recorded natively on x86-64 as the golden, and compares every Host's run to it by rule 20. Two scenarios carry an analytic golden besides: `rc-step`'s voltmeter against `V · (1 − exp(−t/RC))` at every sample, within `atol = 1e-6` V and `rtol = 1e-4` at the default `ltol` (the integration error, not the arithmetic, sets this one); `guest-diode` against `diode-clip` (the same plant with the guest device in the built-in's place), within the profile's tolerance, which is the test that a guest device and a built-in are the same thing to the solver.

## 10. Linear algebra

**Rule 22.** In 0.4 the solver factors a dense matrix by LU with partial pivoting, written in the crate (about two hundred lines), because the plants of Phase 4 have tens of unknowns and a dense factorisation is the most deterministic thing available: no reordering heuristics, no fill-in decisions, one pivot rule. PHY-006's sparse storage and solving is **deferred** to the release that first needs a plant above the bound of rule 18 (open question 1); when it comes, the sparse library is chosen by a measurement against this dense baseline on the kit's largest scenario, and must be pure Rust under RFC 006 rule 6 with a pivot rule that depends on values only, or the domain keeps its own.

## 11. Verification

| Property | Test |
|---|---|
| Formulation | `rc-step`: the operating point and the transient match the analytic solution within rule 21's tolerance under trapezoidal and under Gear 2 |
| Order | Halving `step` under `fixed` reduces `rc-step`'s error against the analytic solution by about four under both methods (second order) |
| Newton | `diode-clip` converges at every step within `max-iterations`; the iteration count per step is in the `solver` events; a diode with `is` chosen to overflow `exp` without limiting converges with it |
| Step control | `motor-loop` under `adaptive` takes fewer steps than under `fixed` at `min-step` with the same error against the fixed run within tolerance; every rejected step appears as `circuit.step-rejected`; no step is below `min-step` except at a horizon (rule 13) |
| Horizons | Every `sample` tick and every scheduled change's tick is a step end (RFC 012 verification) |
| Failure | `hostile-diverge` (a `bad-device` whose stamps diverge) ends with `non-convergence` under `stop-run` and continues frozen under `hold`, controllers still sampling |
| Operating point | A plant that needs `gmin` stepping and one that needs source stepping start; a plant with no solution is refused with `circuit.no-operating-point` |
| Estimate | A scenario above `max-unknowns` is refused before anything is instantiated, naming the bytes |
| Bits | Every circuit scenario's results are compared across x86-64, AArch64, Chromium, Firefox and WebKit: the kit reports `matched` or `within-tolerance` per pair, and the release notes print the table; the expectation of rule 19 is a figure, not a promise |
| Tolerance | A run whose profile sets `tolerance.rtol = 0` and `atol = 0` against itself is `matched`; against a run under the other method is `diverged` with the first differing report named |
| Checkpoint | Restored at its half, every circuit scenario continues `matched` to the uninterrupted run under the same Host and `within-tolerance` under the other |
| Determinism of the kernel part | The event ticks and classes of every circuit scenario are byte-identical on every engine (Tier 1 for the discrete part of a continuous domain) |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| An external solver crate (nalgebra, faer, sprs) in 0.4 | Each adds a dependency tree under RFC 006 rule 6 for plants of tens of unknowns; none promises a value-only pivot rule across versions. The dense LU is small and auditable; the sparse question is deferred with a measurement attached |
| Higher-order Gear (up to 6) | Stability and step-ratio handling cost more than the lab gains; `gear2` covers the switching plants |
| A single fixed step and no error control | PHY-010 asks for both; and a fixed step is what a learner sets when the adaptive one surprises them, so both exist |
| Comparing the whole trace within tolerance | A tolerance on integers is meaningless and hides a real divergence; the integer part stays Tier 1 and only the floating-point reports take the tolerance |
| Streaming results as f32 to save space | The results are the domain's promise; they stay f64 in the trace. The display path (RFC 014) may use f32 because it is not compared |
| Treating a NaN result as a value to compare | A NaN is a failure of the solver, not a result; rule 16 catches it before it can be one |

## Consequences for other RFCs and the requirements

| RFC | Must provide |
|---|---|
| 001 | Rule 62's statement is rule 20 here; the `discarded` outcome of RFC 012 rule 21 for rejected steps |
| 003 | The estimate of rule 18 at admission |
| 005 | The comparison outcome `within-tolerance` in the Host, the runner and `run.toml` (RFC 012 finding F-5); the kit's figures table gains the cross-engine results comparison |
| 006 | `libm` pinned exactly in the domain crate; no other dependency |
| 007 | Table 4.7 gains `circuit.non-convergence`, `circuit.step-rejected`, `circuit.step-minimum`, `circuit.no-operating-point`; `end-reason` gains `non-convergence` |
| 012 | The profile's fields (rule 2), the device interface (rule 5), the detector's location (rule 15), the observation events (rule 17) |
| 014 | The solver's state for the world inspector from the `solver` events and the run state |
| Requirements | PHY-003 to 005 and 010 as written; PHY-006 met for storage and solving by the dense method with the sparse part deferred (the requirement's release should read `0.4` dense, `later` sparse, an amendment the owner accepts with this RFC); PHY-012 is rule 20; AUTO-004 is rule 21 |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | When the sparse solver arrives, and whether a pure-Rust library or the domain's own (rule 22) | The first plant above the bound; a measurement on the kit's largest scenario |
| 2 | The error constants and the growth bound of rule 12, before the goldens are recorded | The implementing pull request, against `rc-step`'s analytic solution; the constants are then written into this rule |
| 3 | Whether the default `method` stays `trapezoidal` once `motor-loop` with a `switch` is in the lab | The lab's pull request, with the step counts of both methods |
| 4 | Whether the kit records the goldens from x86-64 natively (rule 21) or from the dispatched CI run's three architectures agreeing | The first dispatched run with the circuit scenarios; if they are bit-identical, any one is the golden |
