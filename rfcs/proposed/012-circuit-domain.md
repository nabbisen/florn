# RFC 012 — Circuit domain: nets, signals, the plant in the Host, device models and sampled controllers, the `florn:circuit` contract, the time coupling

**Status.** Proposed 2026-10-31, for release 0.4.0 (Phase 4 of the roadmap). The second domain RFC; it enters the Host by the interface of RFC 008 section 2 unchanged, and every place where it could not is a finding against the core, listed in section 11. Companions: RFC 013 (the solver) and RFC 014 (waveforms and the circuit workspace).

**Tracks.** Requirements PHY-001, PHY-002, PHY-008, PHY-009, PHY-012, BUS-009, ARCH-006, ARCH-010, NUM-005, NUM-006, UI-009, AUTO-005, DATA-001, DATA-010, DES-001, DES-006. Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** The circuit domain's types, attributes, classes and interventions; its registered units; the contracts `florn:circuit@0.1.0` for a sampled controller and for a device model, the first use of the compound activation of RFC 002 rules 44 and 45 and of a two-domain world (rule 48); the built-ins that make a plant; how solver steps and the event queue advance together (PHY-009); the domain's checkpoint state; the `[circuit]` tables of the scenario and of the module manifest; the codes of a new RFC 007 table 4.7.

## 1. What the circuit domain is for

A network scenario emulates the logic in the actors and simulates the wire between them. A circuit scenario does the same with the roles turned round: the executable logic under test is a **controller**, a guest that runs at sample instants and sees the world through sampled signals; the world is a **plant**, an electrical circuit solved by the Host between the samples. The domain therefore has two kinds of guest-backed actor (PHY-002) and one Host-side solver (RFC 013), and its whole design is the rule by which the three advance on one logical clock (PHY-009).

The roadmap gives Phase 4 a purpose beyond the feature: to test whether the core is neutral. This RFC is written against the core of 0.3.0 as it is. Where the core suffices, the rule says so by citing it; where it does not, section 11 records a finding and the change it needs, so that the owner can see the cost of the second domain in one table.

## 2. The domain and its types

**Rule 1.** The circuit domain is a crate of the Host core, `florn-domain-circuit`, implementing the `Domain` trait of RFC 008 rule 1 with the name `circuit`, declared by a scenario as `domains = ["core", "circuit"]`. Its class (RFC 001 section 1) is **continuous**: the Host computes with floating point, and Tier 2 promises agreement within the tolerance RFC 013 rule 20 states. The kernel is unchanged (ARCH-006): time is ticks, the queue holds events, and no floating-point value reaches the kernel.

**Rule 2.** Port types, each `circuit/<name>`:

| Type | Direction | Meaning |
|---|---|---|
| `circuit/pin` | — | An electrical terminal. Its net's voltage and the current into it are solver unknowns |
| `circuit/signal-out` | out | A sampled real value the actor produces: a sensor's reading, a controller's output |
| `circuit/signal-in` | in | A sampled real value the actor consumes: a controller's input, a driven source's command |

A module declares its port kinds with these types (RFC 002 rule 15); a built-in's ports are fixed by its definition (section 6). Mechanical terminals (PHY-007, a shaft) are not a type of 0.4; the one mechanical element of 0.4, the motor of rule 23, keeps its mechanics inside the built-in and exposes them as signals.

**Rule 3.** Connection types (RFC 004 rule 19 as RFC 008 rule 8 amended it: `type` routes to the domain):

| Type | Ports | Meaning |
|---|---|---|
| `circuit/net` | Two or more `pin` | One electrical node: every pin shares the voltage, the currents sum to zero (Kirchhoff). The hyperedge of RFC 004 section 1.1 |
| `circuit/signal` | Exactly one `signal-out`, one or more `signal-in` | A sampled value path. The value a producer sets is held until it sets another; every consumer reads the held value |

**Rule 4.** Attributes, all optional:

| Connection | Attribute | Type | Default | Meaning |
|---|---|---|---|---|
| `net` | `ground` | bool | `false` | The reference node, voltage 0. Exactly one net of a scenario has it; none is `circuit.no-ground`, two is `circuit.two-grounds` |
| `net` | `name` | — | the connection's name | For the waveform's label (RFC 014) |
| `signal` | `initial` | real | `0` | The held value before the producer's first output |
| `signal` | `unit` | text | `1` | The unit the value is read in (`V`, `A`, `rad/s`, `1`); a producer and consumers of another registered unit of the same dimension are converted; a different dimension is `circuit.signal-dimension` |

A pin that is on no net is `circuit.pin-open` (an error, not a warning: an open pin is almost always a mistake, and a deliberately open terminal is written as a net of one pin with `open = true`). A `signal-in` on no signal reads `0` and is a warning, `circuit.signal-unconnected`; a `signal-out` on no signal is discarded.

**Rule 5.** Units the domain registers (RFC 004 rule 27), each with the SI prefixes `p`, `n`, `u` (alias `µ`), `m`, `k`, `M`, `G` where the dimension takes them: `V` (voltage), `A` (current), `ohm` (alias `Ω`, resistance), `S` (conductance), `F` (capacitance), `H` (inductance), `W` (power), `Hz` (frequency), `rad/s` (angular speed), `Nm` (torque), `kgm2` (inertia), `Nms` (viscous friction), and the dimensionless `1`. Physical values cross the contract as f64 in the SI unit (RFC 001 rule 58); a quantity in a scenario is converted with exact decimal arithmetic before it becomes a float (RFC 001 rule 63).

**Rule 6.** The scenario's `[circuit]` table (RFC 004 rule 22) is the **solver profile** of DATA-001, read by RFC 013 (its fields are rule 2 there). A scenario that declares the domain without the table runs with every default.

## 3. Time: how the plant and the queue advance together

This section is PHY-009 and the resolution of finding F-5 of the Stage 0 paper test: solver steps end on ticks and on event ticks.

**Rule 7.** The domain keeps the plant's state valid at one tick, `solved`, starting at tick 0 with the operating point RFC 013 rule 9 computes at init. Between `solved` and the next tick at which anything of the circuit is read or changed, the plant is advanced by **steps**, each a Host computation of RFC 013 from one tick to a later tick. A step is a whole number of ticks (RFC 001 rule 8); its internal arithmetic is in seconds.

**Rule 8.** Steps are driven by the queue, not by a loop beside it. The domain schedules an event of class 6, `solver-step`, addressed to the world (RFC 008 rule 1, `event`), at the tick where the next step ends. Handling it, the domain advances the plant from `solved` to that tick (one step, or several if the step controller shortens them, all ending at that tick), sets `solved`, and schedules the next `solver-step` at `solved + h`, where `h` is the step controller's next size, shortened so that it ends at or before the **next horizon**: the earliest of the next `sample` event (rule 10), the next scheduled change addressed to the domain (rule 15), and the scenario's end. Nothing of the circuit happens at a tick that is not a step end, so every reader sees a state the solver has reached.

**Rule 9.** The horizon is known from the queue the domain itself fills: `sample` events are scheduled by the domain (rule 10), changes are in the scenario (RFC 004 rule 20), and an intervention arrives at a boundary (RFC 001 rule 34) and is applied at the next step end at or after it; the domain never needs to look into the kernel's queue. An intervention that must take effect at a tick that is not a step end shortens the step in flight: the domain handles the intervention at its instant by setting a new horizon and, if a `solver-step` is already scheduled beyond it, replacing it (the kernel has no cancel; the domain marks the stale event and ignores it when it arrives, which the trace records as an event with no effect).

**Rule 10.** A **sampled controller** (rule 18) runs at its sample instants only. At init the domain schedules an event of class 7, `sample`, for each controller at `phase`, then every `period` ticks, until the end (both from its parameters, rule 18). Within one instant the key of RFC 001 rule 14 orders class 6 before class 7, so the plant is at the sample's tick before any controller reads it; and class 0 (a scheduled change or an intervention) before both, so a change at tick `T` is seen by the step that ends at `T` and by the sample at `T`. Two controllers sampling at the same tick are ordered by the tie policy like any two activations.

**Rule 11.** A controller's `set-output` is a recorded effect (RFC 001 rule 22); at commit the domain sets the signal's held value and, when the signal drives a source or a device (rule 23), the next step from that tick sees the new value. An output is never seen by the step that ended at the sample's tick: the plant's past is not rewritten.

**Rule 12.** A **device model** (rule 19) is not an actor of the queue: it is called by the solver inside a step, within one compound activation of RFC 002 rule 44 per attempted step, as rule 20 defines. Its calls are not events and leave no record of their own; the record is the step's.

**Rule 13.** Solver-detected conditions as events (PHY-009). A built-in of rule 23 marked as a **detector** (the `comparator`) asks the solver to find the tick at which its condition changes; the solver shortens its steps so that a step ends at the first tick at or after the crossing (RFC 013 rule 15), and at that tick the domain schedules an event of class 8, `condition`, addressed to the detector, whose handling sets the detector's `signal-out`. A condition is therefore a sampled signal that changes at the tick the solver located, and a controller reads it like any other.

**Rule 14.** Classes of the circuit domain (RFC 001 rule 15), beside the core's 0 to 2 and the network's 3 to 5:

| Class | Name | Target | Meaning |
|---|---|---|---|
| 6 | `solver-step` | The world | The plant advances to this tick (rule 8). An event with no effect when stale (rule 9) |
| 7 | `sample` | A controller | The controller's `on-sample` activation (rule 18) |
| 8 | `condition` | A detector built-in | A solver-detected condition changed at this tick (rule 13) |
| 9 | `solver` | The world | Observation: a step accepted after rejections, a step at the minimum size, non-convergence (RFC 013 rule 17); never a guest input |

Default tie policy (RFC 001 rule 19): `by-identity`, as for the network; the scenario may choose `by-seed`.

## 4. Changes and interventions

**Rule 15.** A scheduled change (RFC 004 rule 20) and an intervention of kind `source` are addressed to the world with the content `actor` (a built-in of rule 23 with a settable value) and the value's field (`v`, `i`, `load`, `threshold` as the built-in defines), typed as the built-in's parameter. At its instant the domain sets the value and the step from that tick sees it (rule 10). The circuit domain's outage of RFC 001 rule 37 is the kind `signal` with `connection` and `held`: it freezes a signal's held value until a `signal` intervention with `held = false`; a controller keeps running and reading the frozen value, which is the sensor failure a learner provokes.

**Rule 16.** The core's `message` intervention addressed to a controller is delivered as `on-message` through a core port if the controller's world has one (a two-domain controller, rule 22); a controller of the plain `controller` world has no core port and the intervention is refused as `control.invalid` naming the port.

**Rule 17.** A fault's policy (RFC 001 rule 26) stays the scenario's. A faulted controller's `sample` events are dropped at delivery and its signals keep their held values, which is what a crashed controller does to a plant; a faulted device model (a fault inside its compound activation, RFC 002 rule 44) ends the step as rejected and the solver treats the device as absent from then on, recording `circuit.device-faulted` once; the plant continues without it under `drop`, or the run ends under `stop-run`.

## 5. The contract `florn:circuit@0.1.0`

**Rule 18.** The sampled controller. Its world includes the core world and adds one import and one export; it has no timers of its own beyond the core's (a controller that wants a slower loop declares a longer `period`).

```wit
package florn:circuit@0.1.0;

/// What a controller reads and writes: its signal ports, as sampled values.
interface signals {
    use florn:core/types@0.1.0.{port-id};

    /// The value held on the signal connected to this `signal-in` port, as it was when the
    /// activation began, in the port's unit (RFC 002 rule 47). `none` when the port is on no
    /// signal.
    read-input: func(port: port-id) -> option<f64>;

    /// Set the value of the signal connected to this `signal-out` port. Recorded; applied at
    /// commit; held until the next `set-output` on the same port. A non-finite value is a
    /// contract violation.
    set-output: func(port: port-id, value: f64);
}

/// The controller's events beyond the core's.
interface controller-events {
    use florn:core/types@0.1.0.{tick};

    /// A sample instant: `index` counts the samples of this actor from 0.
    on-sample: func(now: tick, index: u64);
}

/// A sampled controller: the core world with signals.
world controller {
    include florn:core/guest@0.1.0;
    import signals;
    export controller-events;
}
```

The module manifest's `[circuit]` table (RFC 002 rule 12's domain table) declares `kind = "controller"` and names the parameters the domain reads: `period = "period"` (type `ticks`, required or with a default) and optionally `phase = "phase"` (type `ticks`, default `0`). The domain validates them in `validate_module` (RFC 008 rule 1): a controller without a period parameter is `circuit.no-period`. Everything else the guest declares and receives as in RFC 002 section 3: parameters such as gains and a set point, telemetry channels, resources.

**Rule 19.** The device model. Its world includes the core world (for `init`, parameters, telemetry and the diagnostic log) and adds the compound activation; it has no `on-message`, no timers and no `send` that reaches anyone (the core's `send` on a device is a contract violation, there being no core port).

```wit
/// A device model: evaluated by the solver inside a step (RFC 002 rules 44 and 45).
interface device {
    use florn:core/types@0.1.0.{tick};

    /// Opens a step from `now` of `h` seconds. The guest may prepare but changes nothing
    /// that outlives a `reject-step`.
    begin-step: func(now: tick, h: f64);

    /// Evaluate at a trial point: for each of the device's `n` pins, the voltage of its net
    /// in volts, in port order. Returns `n` currents into the pins in amperes followed by
    /// the `n × n` Jacobian entries, row-major, `∂i_k/∂v_j` in siemens (RFC 013 rule 5).
    /// Pure: the same trial gives the same result within one step.
    evaluate: func(trial: list<f64>) -> list<f64>;

    /// The step is accepted: the trial point last evaluated is the solution, and the guest
    /// commits whatever state it carries (charges, fluxes, counters) to `now + h`.
    accept-step: func();

    /// The step is rejected: the guest discards everything since `begin-step`.
    reject-step: func();
}

/// A device model: the core world with the solver's calls.
world device {
    include florn:core/guest@0.1.0;
    export device;
}
```

The module manifest's `[circuit]` table declares `kind = "device"`. A device module declares exactly one port kind of type `circuit/pin` with `min` and `max` (a two-terminal device: `min = 2, max = 2`); the number of pins is the `n` of `evaluate`.

**Rule 20.** The compound activation (RFC 002 rules 44 and 45), as the solver uses it for each device at each attempted step: `begin-step` opens; `evaluate` is called once per Newton iteration (RFC 013 rule 8); `accept-step` is the committing close and `reject-step` the discarding close. One budget and one set of quotas cover the whole; a trap, a budget exhaustion or a contract violation in any call ends the activation as a fault (rule 17). An `evaluate` result of the wrong length, or with a non-finite value, is the contract violation `circuit.stamp-invalid` and the device faults. The guest's `now()` inside the activation is the step's `now`; a guest timer set inside a device activation is a contract violation (the device has no queue presence).

**Rule 21.** What the trace records (RFC 001 rules 22 to 24). An **accepted** step's compound activation is one activation record per device, with its consumed work and its committed effects (telemetry reports, log lines), keyed by the `solver-step` event that caused it. A **rejected** step's compound activations are discarded whole: not recorded, their work not charged beyond the activation, the device's random stream restored, exactly as a fault discards (RFC 002 rule 40) but without a fault — this is the **discarded** outcome, finding F-2 of section 11. The rejection itself is observation: one `solver` event (class 9) with the step's size and the reason (RFC 013 rule 17).

**Rule 22.** Two domains in one actor (RFC 002 rule 48, finding F-6 of the paper test). A module may declare a world of its own that includes `florn:core/guest`, `florn:network/link` and `router-events`, and `florn:circuit/signals` and `controller-events`: a controller with an interface and signal ports. The scenario declares both domains; each validates the ports of its types; the network delivers frames to `on-message`, the circuit domain delivers samples to `on-sample`. The SDK crate `florn-guest-circuit` (rule 25) provides the world `networked-controller` so that no author writes the WIT by hand. This is the two-domain test of RFC 002's verification table, and the cyber-physical destination of ARCH-010.

## 6. Built-ins: the plant

**Rule 23.** The plant is made of built-ins (BUS-009, RFC 008 rule 19): actors of kind `builtin = "circuit/<name>"` run by the domain with no instance, no budget and no quotas. Each has fixed ports and typed parameters in `config` (RFC 004 rule 17), validated by the domain; a settable value (rule 15) is marked. Each is a device of RFC 013 rule 5 implemented in Host code; its stamps are the textbook ones.

| Built-in | Pins and signals | Parameters | Settable | Model |
|---|---|---|---|---|
| `resistor` | `a`, `b` | `r` (ohm) | — | `i = (v_a − v_b) / r` |
| `capacitor` | `a`, `b` | `c` (F), `ic` (V, default 0) | — | `i = c · dv/dt`, integrated by RFC 013 rule 6 |
| `inductor` | `a`, `b` | `l` (H), `ic` (A, default 0) | — | `v = l · di/dt`; its current is an unknown (modified nodal analysis) |
| `voltage-source` | `p`, `n`; optional `signal-in` `cmd` | `v` (V), `gain` (V per unit, default 1) | `v` | `v_p − v_n = v + gain · cmd` when `cmd` is connected; its current is an unknown |
| `current-source` | `p`, `n`; optional `signal-in` `cmd` | `i` (A), `gain` | `i` | `i` from `p` to `n`, plus `gain · cmd` |
| `diode` | `a`, `k` | `is` (A, default 1e-14), `n` (1, default 1), `gmin` (S, default 1e-12) | — | Shockley with the limiting of RFC 013 rule 8 |
| `switch` | `a`, `b`; `signal-in` `ctl` | `ron` (ohm), `roff` (ohm), `threshold` (1, default 0.5) | — | `r = ctl > threshold ? ron : roff`, read at step start |
| `voltmeter` | `p`, `n`; `signal-out` `v` | `period` (ticks), `phase` (ticks, default 0) | — | A sampled sensor: at each sample tick sets `v = v_p − v_n`; draws no current; reports on channel `probe` |
| `ammeter` | `a`, `b`; `signal-out` `i` | `period`, `phase` | — | A zero-volt source whose current is an unknown; samples it as the voltmeter does |
| `constant` | `signal-out` `y` | `value` (1) | `value` | A held signal set at init and by the `source` intervention: a set point, a reference |
| `comparator` | `signal-in` `x`; `signal-out` `y` | `threshold` (1), `hysteresis` (1, default 0) | `threshold` | A detector (rule 13): `y` becomes 1 when `x` rises past `threshold + hysteresis/2`, 0 when it falls below `threshold − hysteresis/2`; `x` must be a signal a probe produces or the solver can evaluate continuously (a voltmeter's) |
| `dc-motor` | `a+`, `a−`; `signal-out` `speed` (rad/s), `signal-in` `load` (Nm) | `r` (ohm), `l` (H), `kt` (Nm/A), `ke` (V/(rad/s)), `j` (kgm2), `b` (Nms), `period`, `phase` | `load` | Armature `v = r·i + l·di/dt + ke·ω`, shaft `j·dω/dt = kt·i − b·ω − load`; `ω` is a solver unknown integrated with the circuit; `speed` sampled at `period` |

The sampled sensors (`voltmeter`, `ammeter`, `dc-motor`'s `speed`) are what a controller can read and what the waveform view shows (RFC 014): a sensor's `period` bounds the data a scenario produces (OBS-003), and its samples are its telemetry reports on channel `probe` with the layout of rule 24, so that they are in the trace as the domain's **results** (AUTO-004; RFC 013 rule 20 compares them).

**Rule 24.** Telemetry the view reads (RFC 008 rule 17's form). A sensor built-in reports on `probe`; a controller guest may report on `control`; a guest that declares the layout gets its series drawn (RFC 014), one that does not is shown by its events alone.

| Channel name | Fields | Meaning |
|---|---|---|
| `probe` | `value: f64` | A sensor's sample at the report's instant, in the signal's unit |
| `control` | `input: f64`, `output: f64`, `error: f64` | A controller's sample: what it read, what it set, and the error it computed |

## 7. The guest SDK and the ladder

**Rule 25.** The SDK crate `florn-guest-circuit` (one crate per world, as `florn-guest-network` is): the bindings of `controller`, `device` and `networked-controller`; the traits `Controller` (`on_sample(now, index)`) and `Device` (`begin_step`, `evaluate(&[f64]) -> Vec<f64>`, `accept_step`, `reject_step`); `read_input(port) -> Option<f64>` and `set_output(port, value)`; the macros `controller!`, `device!`, `networked_controller!`. A `Device` implementation is given a `Trial` helper that keeps the trial state apart from the committed state so that `reject_step` is a drop.

**Rule 26.** The ladder of the circuit domain, shipped as SDK examples and conformance fixtures with recorded results:

| Guest | World | What it does | What it proves |
|---|---|---|---|
| `pid` | `controller` | A PID controller with `kp`, `ki`, `kd`, `setpoint`, output limits; reads one input, sets one output; reports `control` | The sample loop, the held signals, the closed loop of the exit criterion |
| `guest-diode` | `device` | The diode of rule 23 as a guest | The compound activation; a guest device and the built-in agree within tolerance (RFC 013 rule 21) |
| `networked-pid` | `networked-controller` | `pid` whose set point arrives as a frame on its interface | Two domains in one actor (rule 22) |
| `bad-device` | `device` | Returns a NaN stamp, or the wrong length, or sets a timer, as its parameter says | The contract violations of rule 20 (AUTO-005) |

**Rule 27.** The scenarios of the kit, each with its recorded results under both Hosts and on every engine: `rc-step` (a source step into `resistor` and `capacitor`, a `voltmeter`; the analytic solution is the golden, RFC 013 rule 21), `diode-clip` (a sine-driven source, a `resistor`, a `diode`; nonlinear iteration), `guest-diode` (the same with the guest device), `motor-loop` (`pid` driving a `voltage-source` into a `dc-motor`, the `speed` back to the controller; a `load` step scheduled at one second: the exit criterion's closed loop), `cps-loop` (`networked-pid` with its set point sent as a `message` intervention), `comparator-latch` (a `comparator` on a `voltmeter`, the located crossing), `hostile-stamp`, `hostile-diverge` (RFC 013 rule 17's non-convergence). The learning lab of 0.4 is `motor-lab`, the `motor-loop` scenario with its story in both languages (RFC 014 rule 10).

## 8. Validation and codes

**Rule 28.** The domain validates at scenario load and at module admission (RFC 008 rule 1), each with a code in a new RFC 007 table 4.7:

| Code | Parameters | When |
|---|---|---|
| `circuit.no-ground`, `circuit.two-grounds` | `connection: name` (the second) | Rule 4 |
| `circuit.pin-open` | `actor: name`, `port: name` | A pin on no net |
| `circuit.signal-ports` | `connection: name`, `outs: count` | A signal without exactly one `signal-out` |
| `circuit.signal-dimension` | `connection: name`, `port: name`, `expected: text` | A consumer's unit of another dimension |
| `circuit.signal-unconnected` | `actor: name`, `port: name` | A `signal-in` on no signal (warning) |
| `circuit.attr-unknown`, `circuit.attr-type` | `connection: name`, `key: name`, `expected: text` | As the network's |
| `circuit.param-unknown`, `circuit.param-type`, `circuit.param-missing` | `actor: name`, `key: name`, `expected: text` | A built-in's `config` |
| `circuit.no-period` | `module: name` | A controller or sensor without a period (rule 18, 23) |
| `circuit.period-not-ticks` | `actor: name`, `value: text` | A period that is not a whole number of ticks |
| `circuit.too-large` | `unknowns: count`, `maximum: count`, `bytes: bytes` | The estimate of RFC 013 rule 18 above the profile's bound (NUM-003) |
| `circuit.stamp-invalid` | `actor: name`, `reason: text` | Rule 20 (a fault of kind `contract`) |
| `circuit.device-faulted` | `actor: name` | Rule 17 |
| `circuit.kind-unknown` | `module: name`, `kind: text` | A `[circuit]` table with a kind the domain lacks |

The enumeration `dimension` (RFC 007 section 4.5) gains the domain's dimensions; `faulted-policy` gains nothing (the circuit uses `drop` and `stop-run`).

## 9. Checkpoint state

**Rule 29.** The domain's `export` (RFC 008 rule 1; RFC 009 rule 1) is its whole state in the canonical encoding: `solved`, the solution vector and the integration history of RFC 013 rule 6, the step controller's state, every signal's held value, every sensor's next sample index, every built-in's internal state (the motor's speed, the comparator's output), each f64 made canonical (RFC 001 rule 59). A checkpoint written by one Host restores in the other and the run continues to the same results as an uninterrupted one (RFC 009 verification, extended to the circuit scenarios).

## 10. Verification

| Property | Test |
|---|---|
| Step ends | In `motor-loop`, every `solver-step` event's tick is at or before the next `sample` tick and at every scheduled change's tick; no sample reads a plant older than its tick |
| Held signals | A controller's output set at sample `k` is read by the plant from that tick and by a second controller at its next sample, never earlier |
| Compound activation | `guest-diode`: a rejected step leaves the guest's committed state unchanged (the guest reports its charge on accept only; the kit asserts the report sequence against the step record); `bad-device` faults with `circuit.stamp-invalid` for each of its modes, and the plant continues under `drop` |
| Two domains | `cps-loop`: the set point frame changes the controller's behaviour at the next sample; the scenario validates with both domains and refuses with one |
| Detector | `comparator-latch`: the `condition` event's tick is the first tick at or after the analytic crossing of the RC waveform |
| Interventions | `source` on the motor's `load` at a tick between steps takes effect at that tick (a step ends there); `signal` freezes a sensor and the controller keeps running |
| Faults | A controller that traps keeps its last outputs held; under `stop-run` the run ends with `host-fault` |
| Validation | Each code of rule 28 produced by a scenario built to provoke it, in one pass |
| Checkpoint | Every circuit scenario of rule 27 restored at its half under the other Host continues to the same results (RFC 013 rule 20) |
| Interface | `florn-domain-circuit` implements RFC 008 section 2 with the Host changes of section 11 and no other: the dependency check of RFC 005 rule 1 and a review against the findings table |

## 11. Findings against the core

The roadmap's purpose for Phase 4. Each row is a change the core needs for this domain, to be made by the handoff's first pull requests and recorded here when done; a row that turns out unnecessary is struck, not deleted.

| # | Finding | Where | Change |
|---|---|---|---|
| F-1 | The flat call shapes know `i32` and `i64` only; `read-input`, `set-output`, `begin-step` and the probes' values cross as `f64` (RFC 001 rule 58) | `florn-abi`'s shape table; both engine adapters' trampolines (RFC 008 rule 6) | The table gains `f64` flat types and the rows this contract needs; the browser adapter's trampolines and the Wasmtime adapter's `func` handle `f64` params and results |
| F-2 | An activation ends by commit or by fault; a rejected step needs a third outcome that discards without a fault (rule 21) | `florn-host`'s activation (RFC 005 section 4), the trace (RFC 001 rules 22 to 24) | The outcome `discarded`: nothing recorded, the stream restored, no fault; the domain asks for it by returning the discarding close |
| F-3 | A compound activation spans several calls of one export set; the Host activates one export per event | `florn-host`'s run loop and `Action::Call` (RFC 008 rule 1) | The domain opens an activation, calls exports inside it and closes it; the budget global is set at the opening call and read at the closing (RFC 003 rule 12, already written) |
| F-4 | `Domain::answer` returns one `Val`; `read-input` returns `option<f64>` | The domain trait | `answer` may return a lifted `option`; no trait change if `Val` carries it, else a variant |
| F-5 | The replay comparison knows `matched` and `diverged`; a continuous domain adds `within-tolerance` (RFC 013 rule 20) | `florn-host`'s comparison, the runner's exit codes, `run.toml`'s `comparison` | RFC 013 rule 20 |
| F-6 | The display stream's record kinds are `instant`, `flight`, `activity` | RFC 005 rule 14 | RFC 014 rule 3 |
| F-7 | `end-reason` is `end-reached`, `user`, `host-fault` | RFC 007 section 4.5 | Gains `non-convergence` (RFC 013 rule 17) |

Nothing in the kernel changes. If the implementation finds an eighth, it is added here before the code is written (RFC 000: the RFC is amended first, the handoff after).

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| The solver as a loop beside the kernel, advancing between events | Two sources of time; the trace could not say when the plant was where. Steps as queue events keep one clock and make every step a record |
| A controller as a device model with state (one kind of actor) | PHY-002: a controller called inside an iteration is corrupted, and its state could not be committed per step without a contract for rollback it has no reason to have |
| Device models as built-ins only (no device world in 0.4) | The compound activation is the one rule of RFC 002 never exercised, and the `guest-diode` against the built-in is the cheapest strong test of it; the exit criterion needs only the controller, so the device world is small and its ladder entry is one example |
| Mechanical ports (`circuit/shaft`) in 0.4 | PHY-007 is `later`; a motor with its mechanics inside is enough for the controller lab. A second continuous subdomain doubles the solver's unknown kinds for no 0.4 purpose |
| Streaming every solver step to the view | OBS-003: the Host reduces first. Sampled sensors give the scenario author the data rate, and make the waveform the same thing a controller reads |
| Floating point in the kernel's events (a step size in seconds) | RFC 001 rule 56; a step is whole ticks and the arithmetic in seconds stays inside the domain |

## Prior art

SPICE's `.TRAN` with breakpoints at source discontinuities is rule 8's horizon; Modelica's synchronous sampled blocks (`sample`, `hold`) are the signal of rule 3; Simulink's zero-crossing detection is rule 13; software-in-the-loop controllers in motor drives sample at a fixed period and act through a held command, which is rules 10 and 11.

## Consequences for other RFCs and the requirements

| RFC | Must provide |
|---|---|
| 001 | Classes 6 to 9 as rule 14 assigns them; the `discarded` outcome beside commit and fault (rules 22 to 24, F-2); rule 37's outage as the `signal` intervention |
| 002 | The `[circuit]` table of the module manifest (`kind`, `period`, `phase`); the contracts of record `florn:circuit@0.1.0` and their equality tests; the compound-activation and two-domain rows of its verification table, written at last |
| 003 | Rule 12 (one budget per compound activation) exercised; the estimate of RFC 013 rule 18 at admission |
| 004 | The units of rule 5; the `[circuit]` solver profile |
| 005 | The findings F-1 to F-5; `florn-domain-circuit` and `florn-guest-circuit` in the layout of rule 42; the kit's circuit rows |
| 006 | No new origin, no new capability: a device's `evaluate` is a guest function the Host calls, inside the sandbox as any export; the shape table's `f64` rows are validated at admission like the others |
| 007 | Table 4.7 (rule 28); `dimension` gains the domain's dimensions; `end-reason` gains `non-convergence` |
| 008 | Section 10's promise ("where they would need more, section 10 says so") is answered by section 11 here |
| 009 | The domain's state in a checkpoint (rule 29) |
| Requirements | PHY-001, 002, 008, 009 are rules 2, 18 to 20, 10 to 13; PHY-007 stays `later`; the acceptance sentence of section 22 for 0.4 is verified by `motor-loop` under both Hosts |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | Whether the device contract carries the Jacobian from the guest (rule 19) or the solver differences it numerically from `evaluate`'s currents alone, which halves the contract and doubles the calls | The owner at acceptance; the RFC proposes the analytic Jacobian, because a guest that cannot write one can return finite differences itself |
| 2 | Whether a `signal`'s unit conversion (rule 4) is wanted in 0.4 or every signal is in the producer's unit | The owner at acceptance; the RFC proposes conversion, because a controller written in volts should not care that the sensor is a motor |
| 3 | Whether `comparator` and the detector mechanism (rule 13) ship in 0.4 or wait for the first lab that needs an event from the plant | The owner at acceptance; the RFC proposes shipping it, because PHY-009 names solver-detected conditions and the mechanism is small once steps end on ticks |
| 4 | The sample period's place: a parameter the manifest names (rule 18) or a per-actor attribute of the scenario | The implementing pull request, if the parameter form proves awkward for built-ins; both must give the same validation |
