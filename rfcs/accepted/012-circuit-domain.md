# RFC 012 — Circuit domain: nets, signals, the plant in the Host, device models and sampled controllers, the `florn:circuit` contract, the time coupling

**Status.** Accepted 2026-10-31, the owner's word on the day it was proposed, as proposed; for release 0.4.0 (Phase 4 of the roadmap). **Amended 2026-10-31 (AD-25)** after the architect's independent entry review, on the owner's decisions: **D1** release 0.4.0 admits at most one guest device model per scenario (rule 19; finding F-8 is deferred); **D3** the Tier 2 comparison is specified over three kinds of value and needs both runs' payload bytes, and what the tolerance does not cover is controlled exactly (RFC 013 rules 19 and 20); **D4** the plant advances on demand and the inputs that change it are recorded as breakpoints, which replaces the free-running step events of the first text (rules 7 to 14). The amendment also corrects what the review found: the findings table is F-1 to F-16 (F-4 struck, F-8 to F-15 added by the entry review and F-16 by the development team's reading of the code; section 11), `accept-step` carries the solution, the built-ins `comparator`, `constant` and `dc-motor`, the units of rule 5, the ladder and the code table are completed, and the drafting errors are corrected. A second reader then checked the amended text against the code and a model of the rules, and its corrections are in it: the plant's one pending advance and the bound on pending breakpoints (rules 8 and 9), the units of signals (rules 4, 5 and 23), the device contract's limiting flag (rule 19), the start hook's order and keys (rules 21 and 30), and the findings F-1, F-3, F-5, F-9, F-12 and F-13. Rule numbers of the first text are kept; new rules are numbered from 30. The second domain RFC; it enters the Host by the interface of RFC 008 section 2 **with the additions of section 11**, and every place where it could not is a finding against the core. Companions: RFC 013 (the solver) and RFC 014 (waveforms and the circuit workspace). The handoff package is `rfcs/handoffs/012-circuit-domain/` (2026-10-31, on the owner's authorisation of the schedule).

**Tracks.** Requirements PHY-001, PHY-002, PHY-008, PHY-009, PHY-012, BUS-009, ARCH-006, ARCH-010, NUM-005, NUM-006, UI-009, AUTO-005, DATA-001, DATA-010, DES-001, DES-006, ACT-011. Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** The circuit domain's types, attributes, classes and interventions; its registered units and their dimensions; the contracts `florn:circuit@0.1.0` for a sampled controller and for a device model and the composed world `florn:cps@0.1.0`, the first use of the compound activation of RFC 002 rules 44 and 45 and of a two-domain world (rule 48); the built-ins that make a plant; how solver steps and the event queue advance together (PHY-009); the domain's checkpoint state; the `[circuit]` tables of the scenario and of the module manifest; the codes of a new RFC 007 table 4.7.

## 1. What the circuit domain is for

A network scenario emulates the logic in the actors and simulates the wire between them. A circuit scenario does the same with the roles turned round: the executable logic under test is a **controller**, a guest that runs at sample instants and sees the world through sampled signals; the world is a **plant**, an electrical circuit solved by the Host between the samples. The domain therefore has two kinds of guest-backed actor (PHY-002) and one Host-side solver (RFC 013), and its whole design is the rule by which the three advance on one logical clock (PHY-009).

The roadmap gives Phase 4 a purpose beyond the feature: to test whether the core is neutral. This RFC is written against the core of 0.3.0 as it is. Where the core suffices, the rule says so by citing it; where it does not, section 11 records a finding and the change it needs, so that the owner can see the cost of the second domain in one table. The entry review found that the core lacks more than the first text of this RFC said (fifteen findings, not seven, the last found by the development team); the kernel itself changes in one place only (F-2), and the one change that would touch it more is refused for 0.4 (rule 19, F-8).

## 2. The domain and its types

**Rule 1.** The circuit domain is a crate of the Host core, `florn-domain-circuit`, implementing the `Domain` trait of RFC 008 rule 1 with the name `circuit`, declared by a scenario as `domains = ["core", "circuit"]`. Its class (RFC 001 section 1) is **continuous**: the Host computes with floating point, and Tier 2 promises what RFC 013 rule 20 states. The kernel's time is ticks, its queue holds events, and no floating-point value reaches the kernel's event key (ARCH-006).

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
| `net` | `ground` | bool | `false` | The reference node, voltage 0. Exactly one net of a scenario has it; none, in a scenario that has at least one net, is `circuit.no-ground`, two is `circuit.two-grounds` |
| `signal` | `unit` | text | the unit of a built-in producer's port | The unit the signal holds its value in (`V`, `A`, `rad/s`, any unit of rule 5). Absent, it is the unit of a built-in producer's port (a sensor's unit, the unit a `constant`'s `value` is written in); a signal produced by a guest, and any other, is dimensionless. A built-in producer or consumer of another registered unit of the same dimension is converted (rule 5); of a different dimension is `circuit.signal-dimension`. A guest's port declares no unit and is not dimension-checked: it reads and sets the signal's own unit |
| `signal` | `initial` | a value in the signal's unit | `0` | The held value before the producer's first output (a built-in producer's first output is at start, rule 30: a `constant`'s value, a comparator's 0; a sensor's is its first sample). Written as a quantity of the signal's dimension (`150rad/s`), or as a plain number when the signal is dimensionless (RFC 004 rule 28; DATA-010) |

A pin that is on no net is `circuit.pin-open`, an error: an open pin is almost always a mistake, and a terminal left open on purpose is connected through a `resistor` to a net (a connection joins two or more ports, BUS-001; a one-pin net does not exist). A built-in's `signal-in` on no signal reads `0` and is a warning, `circuit.signal-unconnected`; a guest's `read-input` on a port on no signal returns `none` (rule 18); a `signal-out` on no signal is discarded.

**Rule 5.** Units the domain registers (RFC 004 rule 27), **each with its dimension** (finding F-13); the Host checks that a quantity written for a parameter or an attribute is of the dimension declared, and refuses a mismatch with `quantity.dimension`:

| Symbol | Dimension | Prefixes |
|---|---|---|
| `V`, `A`, `ohm` (alias `Ω`), `S`, `F`, `H`, `W`, `Hz`, `Nm` | voltage, current, resistance, conductance, capacitance, inductance, power, frequency, torque | `p`, `n`, `u` (alias `µ`), `m`, `k`, `M`, `G` |
| `rad/s` | angular-speed | none |
| `kgm2` | inertia | none |
| `Nms` | friction (N·m·s per radian) | none |
| `Nm/A` | torque-per-current | none |
| `Vs/rad` | voltage-per-speed | none |

Physical values cross the contract as f64 in the SI unit (RFC 001 rule 58). A quantity in a scenario is converted by **exact decimal arithmetic** (the number, the prefix and the unit's factor) and rounded **once** to the nearest f64 (RFC 001 rule 63); never by multiplying two floats (finding F-13). A dimensionless value has no unit: it is a plain number (RFC 004 rule 28), and a signal that carries one has no `unit`. **A signal holds its value in its own unit.** A built-in producer converts its SI result to the signal's unit by one division by the unit's factor, and a built-in consumer converts back by one multiplication; the factor is the unit's exact decimal (prefix and symbol) rounded once to the nearest f64 at composition. A guest reads and sets the value in the signal's unit, with no conversion by the Host. A quantity written for a value held on a signal (a `constant`'s `value`, a signal's `initial`) is converted by the same exact decimal arithmetic directly to the signal's unit and rounded once.

**Rule 6.** The scenario's `[circuit]` table (RFC 004 rule 22) is the **solver profile** of DATA-001, read by RFC 013 (its fields are rule 2 there), and it carries the domain's `faulted-policy` (rule 17). A scenario that declares the domain without the table runs with every default.

## 3. Time: how the plant and the queue advance together

This section is PHY-009. Three rules of the first text could not hold together and are replaced: a step event at every step end made a located crossing lie in the past (RFC 001 rule 5), made the instant of a change ambiguous, and needed a mark for stale events that a checkpoint would have to carry. The plant now advances **on demand**.

**Rule 7.** The domain keeps the plant's state valid at one tick, `solved`, starting at tick 0 with the initial solution that RFC 013 rule 9 computes in the start hook (rule 30). The state at `solved` is the solution at the end of the last step, **before** any breakpoint at `solved` acts (rule 9). The plant is advanced from `solved` to a later tick by **steps**, each a Host computation of RFC 013 over a whole number of ticks (RFC 001 rule 8); its internal arithmetic is in seconds.

**Rule 8.** The plant is advanced only by an event of class 6, `advance`, addressed to the world, and by nothing else: not a commit, not an intervention, not a query, not a checkpoint. (Observation and checkpoints must not change the step sequence, RFC 001 rule 39 and RFC 009 rule 4; no guest may be activated while a commit is in progress, so nothing in a commit can run a device model.) **One `advance` is pending at a time**, at the tick `min(the next tick at which a sensor is due, the last advance's tick + detect-interval)`, the second term only when the scenario has a **detector** (rule 13; RFC 013 rule 2) and the event only when that tick is before the scenario's end; the start hook schedules the first, counting the last advance as tick 0. Handling it, the domain (1) advances the plant from `solved` to the event's tick, by as many steps as the step controller takes, each ending on a tick and shortened to end at every breakpoint (rule 9) and at the event's tick; sets `solved`; (2) schedules the class-7 `sample` event of every sensor due at this tick on the next microstep, with the sensor as origin (rule 10); (3) schedules a class-8 `condition` event for each detector whose output changed during the advance (rule 13); (4) schedules the next `advance` by the formula above. An `advance` calls no actor and leaves no trace record (RFC 005 section 5); its effects are the records of the activations it causes. The plant is advanced at sensor ticks and detector wake-ups and at no other tick: a scenario with neither a sensor nor a detector would never advance it, and is refused with `circuit.plant-unobserved` (rule 28).

**Rule 9.** An **input** of the plant changes when a held signal that drives a built-in changes (a controller's output committed, rule 11; a sensor's output), a settable value is set (a `source` intervention or a scheduled change, rule 15), or a `signal` is frozen or released. At the instant the domain handles the change it records a **breakpoint** `(tick, input, value)` in the order of handling, **without advancing the plant**, which is why it can happen inside a commit; a change that sets an input to the value it already holds (equal bits) records nothing and cuts nothing, and a breakpoint replaces a pending one for the same input at the same tick. The plant sees a breakpoint from the step that **starts** at its tick: a step that ends at T uses the inputs in force before the breakpoints at T; the state at `solved = T` is the limit from the left; and a sensor sampling at T reads it. The next advance applies the breakpoints with tick not later than `solved` first, then splits every step at the later ones; breakpoints at one tick apply in the order recorded. Each applied breakpoint cuts the integration history (RFC 013 rule 7). **Pending breakpoints are bounded** by the profile's `max-breakpoints` (RFC 013 rule 2, default 65,536): more pending ends the run, as the kernel's limits do (RFC 001 rule 13): most breakpoints are recorded inside a commit, where a delivery returns no action, so the Host reads the domain's end request after every commit and ends the run there with `run.limit-exceeded` (`what` is `pending-breakpoints`, `origins` the actors whose outputs or targets hold the pending breakpoints) and the end reason `host-fault`; the run stays reproducible, as it does after the kernel's limits. The cause is a controller that changes an output far more often than any sensor samples, and the remedy a shorter sensor `period` or a larger bound. While the plant is held (RFC 013 rule 17) nothing is recorded and the pending ones are dropped. Pending breakpoints and the inputs in force are domain state (rule 29).

**Rule 10.** Sampling. A sampled controller and every sampling built-in (`voltmeter`, `ammeter`, the motor's `speed`) has a `period` (a whole number of ticks, at least 1: `quantity.not-whole-ticks`, `circuit.period-zero`) and a `phase` (default 0). The start hook schedules each controller's first class-7 `sample` event at its `phase` and the first `advance` (rule 8); handling a controller's sample (the `Domain::event` of RFC 008 rule 1) calls its `on-sample` and schedules its next sample `period` ticks later; handling a sensor's sample is the built-in's activation (RFC 008 rule 19), which reports on `probe` and sends its value on its `signal-out` port. Samples at or after the scenario's end are not scheduled (`end` is exclusive, RFC 004 section 3.3). The **origin** of a sample event is the sampling actor, and its **sequence number** is the next value of the world's counter for that origin (the Host's `Composite::next_seq`, which a checkpoint carries), not the kernel's count of the actor's committed effects; the class in the key keeps a sample event and a message from the same actor apart (RFC 001 rule 17 counts the effects of an origin, and an event the domain schedules is such an effect of the domain). **Order at a tick**: a scheduled change is class 0 at microstep 0 and only records breakpoints; then class 6 (the plant is made current); then the controllers' class-7 events, ordered by the tie policy and identity; then the sensors' class-7 events one microstep later, so **at a tick where a controller and a sensor both sample, the controller reads the sensor's previous sample**: the delay of a digital loop, stated and the same whatever the identities. A controller that must see a sensor's sample of tick S samples at S + 1 or later (`phase`). At tick 0 everything the start hook schedules with delay zero lands one microstep later, in the same order. A user's intervention is class 0 too, but unless it names a later tick it lands at the current tick and the next microstep (RFC 001 rule 34), after the samples of that tick that have already run; it records its breakpoint there and acts from the step that starts at that tick.

**Rule 11.** A controller's `set-output` is a recorded effect (RFC 001 rule 22). The Host records it as a **`send` effect on the `signal-out` port** whose payload is the value's eight bytes, canonical little-endian f64 (finding F-11): no new effect kind and no change to the trace encoding; the payload digest is in the trace. At commit the world's delivery for that port (RFC 008 rule 1, `world`) sets the signal's held value, records a breakpoint at the commit's tick for every plant input the signal drives whose value changes (rule 9), and returns no delivery. Readers (`read-input`) see the new value from their next activation. The plant never sees an output before the tick it was committed (rule 9).

**Rule 12.** A **device model** (rule 19) is not an actor of the queue: it is called by the solver inside an advance, with one compound activation of RFC 002 rule 44 per attempted step (rule 20). Its calls are not events. At most one actor of a scenario is a guest device (rule 19).

**Rule 13.** Solver-detected conditions as events (PHY-009). A built-in marked as a **detector** (the `comparator`, which has pins and measures `v_p − v_n`) asks the solver to check its condition at the end of every accepted step of an advance (RFC 013 rule 15). The comparator's **published output** starts at 0 (rule 30). When the output its hysteresis gives at the end of the advance differs from the one the detector last found (the published one, except between an advance and the condition event it schedules), the domain schedules a class-8 `condition` event for it on the next microstep after the advance, and the comparator's activation sends the new output on its `signal-out` port and reports the **located crossing** on channel `condition` (a whole tick inside the step that crossed, computed by RFC 013 rule 15; informational). A condition is therefore a sampled signal that changes **at the advance that found it**, at most `detect-interval` ticks after the crossing; chatter inside one advance is not published. A controller reads the new output at its next sample (a sample at the same tick has already run: class 7 precedes the condition event of the next microstep). The located crossing is data, never the tick of the event: a crossing cannot be scheduled into the past (RFC 001 rule 5). A comparator already beyond its threshold at start reports it at the first advance that takes a step, with the start of that step as its located crossing.

**Rule 14.** Classes of the circuit domain (RFC 001 rule 15), beside the core's 0 to 2 and the network's 3 to 5:

| Class | Name | Target | Origin | Meaning |
|---|---|---|---|---|
| 6 | `advance` | The world | The world | The plant is made current at this tick (rule 8). Calls no actor, leaves no trace record |
| 7 | `sample` | A controller, or a sensor built-in | The sampling actor | The controller's `on-sample` activation, or the sensor's report and send (rule 10) |
| 8 | `condition` | A detector built-in | The detector | A detected condition changed (rule 13) |

There is no class for the solver's observations. A rejected step, a step at the minimum size and a non-convergence are notifications (RFC 008 rule 3; RFC 013 rule 17), which take no sequence number, so that how many attempts a Host made cannot change a key (RFC 001 rule 39).

Default tie policy (RFC 001 rule 18): `by-identity`, as for the network; the scenario may choose `by-seed`.

## 4. Changes and interventions

**Rule 15.** A scheduled change (RFC 004 rule 20) and an intervention of kind `source` are addressed to the world with the content `actor` (a built-in with a settable value) and the value's field, typed as the built-in's parameter: `v` of a `voltage-source`, `i` of a `current-source`, `value` of a `constant`, `threshold` of a `comparator`. At its instant the domain records a breakpoint (rule 9) and, for a `constant`, sets the held value of its signal. The circuit domain's outage of RFC 001 rule 37 is the kind `signal` with `connection` and `held`: it freezes a signal's held value until a `signal` intervention with `held = false`. While a signal is frozen its producer's outputs are ignored, and on release the signal keeps the frozen value until the producer next sets one; a controller keeps running and reading the frozen value, which is the sensor failure a learner provokes.

**Rule 16.** The core's `message` intervention addressed to a controller is delivered as `on-message` through its `network/interface` port (RFC 008 rule 14) if its world has one (a two-domain controller, rule 22); a controller of the plain `controller` world has no such port and the intervention is refused as `control.invalid` naming the port. A `message` intervention addressed to a device model or to a built-in is refused in the same way, naming the target.

**Rule 17.** A fault's policy (RFC 001 rule 26; ACT-011) is declared in the scenario as `faulted-policy = "drop" | "stop-run"` in the `[circuit]` table, default `drop`, and reaches the Host through the domain (finding F-14). A faulted controller's `sample` events are dropped at delivery and it is not sampled again; its signals keep their held values, which is what a crashed controller does to a plant. A guest device that faults inside a compound activation (a trap, the budget, a contract violation) ends in a **Fault** (RFC 001 rule 24): a Fault record keyed by the `advance` event that caused it, or by the device's `init` event in the start hook (rule 21), and the actor dead. The solver discards the attempted step and makes it again without the device, which is absent from then on, and `circuit.device-faulted` is recorded once; the plant continues without it under `drop`. Under `stop-run` the advance is abandoned where it stands, the plant keeps the state of its last accepted step, and the run ends with the end reason `actor-fault` (finding F-7).

## 5. The contract `florn:circuit@0.1.0`

**Rule 18.** The sampled controller. Its world includes the core world and adds one import and one export; it has no timers of its own beyond the core's (a controller that wants a slower loop declares a longer `period`).

```wit
package florn:circuit@0.1.0;

/// What a controller reads and writes: its signal ports, as sampled values.
interface signals {
    use florn:core/types@0.1.0.{port-id};

    /// The value held on the signal connected to this `signal-in` port, as it was when the
    /// activation began, in the unit of that signal (rule 4). `none` when the port is on no
    /// signal.
    read-input: func(port: port-id) -> option<f64>;

    /// Set the value of the signal connected to this `signal-out` port, in the unit of that
    /// signal. Recorded as a send effect (rule 11), applied at commit, held until the next
    /// `set-output` on the same port. A non-finite value is a contract violation.
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

The module manifest's `[circuit]` table (the domain table of RFC 008 rule 16; the Host's `validate_module`) declares `kind = "controller"` and names the parameters the domain reads: `period = "<parameter>"` (type `ticks`, required or with a default) and optionally `phase = "<parameter>"` (type `ticks`, default `0`), and may declare `control-units = { input = "rad/s", error = "rad/s" }`, the units of its `control` series (RFC 014 rule 5), a key left out being dimensionless. A controller without a period parameter is `circuit.no-period`. Everything else the guest declares and receives as in RFC 002 section 3: parameters such as gains and a set point, telemetry channels, resources.

**Rule 19.** The device model. Its world includes the core world (for `init`, parameters, telemetry and the diagnostic log) and adds the compound activation. Because the world includes `florn:core/guest`, a device module still **exports the core's `init`, `on-message` and `on-timer`** (the SDK macro writes stubs; the Host never calls the last two) and imports the core `host`; its `send` and `set-timer` are contract violations, there being no queue presence.

```wit
/// A device model: evaluated by the solver inside a step (RFC 002 rules 44 and 45).
interface device {
    use florn:core/types@0.1.0.{tick};

    /// Opens a step. `start` is the tick at which the step begins; `h` is its length in
    /// seconds, `n × tick length` computed once (RFC 013 rule 14), or `0.0` for the initial
    /// solution (a DC analysis: no history). The guest may prepare but changes nothing that
    /// outlives a `reject-step`.
    begin-step: func(start: tick, h: f64);

    /// Evaluate at a trial point: for each of the device's `n` pins, the voltage of its net
    /// in volts, in ascending port identity. Returns `n` currents into the pins in amperes,
    /// then the `n × n` Jacobian entries, row-major, `∂i_k/∂v_j` in siemens (RFC 013 rule 5),
    /// then one entry, `1.0` when the device limited a junction in this evaluation and `0.0`
    /// when it did not (RFC 013 rule 8): `n + n² + 1` values. A deterministic function of the
    /// sequence of trials since `begin-step`.
    evaluate: func(trial: list<f64>) -> list<f64>;

    /// The step is accepted: `solution` holds the converged pin voltages, and the guest
    /// commits whatever state it carries (charges, counters) as of `start + h`.
    accept-step: func(solution: list<f64>);

    /// The step is rejected: the guest discards everything since `begin-step`.
    reject-step: func();
}

/// A device model: the core world with the solver's calls.
world device {
    include florn:core/guest@0.1.0;
    export device;
}
```

Two exceptions to the core's rules are made here, and only here, and RFC 001 and RFC 002 note them when this RFC is implemented. `h` is a **duration in seconds as an f64**: RFC 001 rule 3 says no floating-point value crosses a contract as a time; `h` is a physical value (rule 58) derived exactly from whole ticks and the tick length (RFC 013 rule 14), used by the guest for arithmetic and never as a time of the kernel. And `start` is the **step's start**, not the current instant (RFC 002 rule 22): a device's activation is driven by the solver inside an advance, not by an event.

A scenario may declare **at most one actor whose module is of kind `device`**; a second is `circuit.device-limit`, a validation error. The reasons are the kernel's, not the domain's: it enforces one open activation at a time (RFC 001 rule 21, `Kernel::begin` asserts it), and the browser engine has one budget global shared by every guest instance (RFC 003 rule 12 sets the budget at the opening call and reads it at the closing one, which cannot hold for overlapping activations on a shared global). Relaxing the limit is the deferred finding F-8 (section 11); built-in devices are not activations and are not limited. The module manifest's `[circuit]` table declares `kind = "device"`. A device module declares exactly one port kind of type `circuit/pin` with `min` and `max` (a two-terminal device: `min = 2, max = 2`); the number of pins is the `n` of `evaluate`, at most **16** (the Host maximum, a row of RFC 006 section 7; a larger `max` is `circuit.device-ports`), so that the result is at most 273 values.

**Rule 20.** The compound activation (RFC 002 rules 44 and 45), as the solver uses it for the device at each attempted step: `begin-step` opens; `evaluate` is called once per Newton iteration (RFC 013 rule 8); `accept-step` is the committing close and `reject-step` the discarding close. One budget and one set of quotas cover the whole; a trap, a budget exhaustion or a contract violation in any call ends the activation as a fault (rule 17). The Host checks the length of `evaluate`'s result (`n + n² + 1`) **before** it lifts the list from guest memory (SEC-005), and a result of the wrong length, with a non-finite value, or whose last entry is neither `0.0` nor `1.0` is the contract violation `circuit.stamp-invalid` and the device faults. The guest's `now()` inside the activation is the step's `start`; a guest timer set inside a device activation is a contract violation.

**Rule 21.** What the trace records (RFC 001 rules 22 to 24). An **accepted** attempted step's compound activation is one activation record for the device (a `Handled` record whose key and target are those of the `advance` event that caused it and the device, as the activations a domain event causes are today), with its consumed work and its committed effects (telemetry reports, log lines); several accepted steps in one advance give several such records, with the same key, in the order of the steps. The initial solve's calls (RFC 013 rule 9), made in the start hook where no event is handled, are keyed as the device's own `init` event (tick 0, microstep 0, class control, origin and target the device, sequence number 0): the solve that gives the result leaves one record, the others none. A **rejected** step's compound activation is **discarded** whole: not recorded, the device's random stream restored, exactly as a fault discards (RFC 002 rule 40) but without a fault and with the actor alive (finding F-2). The rejection itself is observation: a notification (RFC 013 rule 17) that takes no sequence number. The total work rejections can cause is bounded (RFC 013 rule 12).

**Rule 22.** Two domains in one actor (RFC 002 rule 48). The composed world is a contract of record of its own, package `florn:cps@0.1.0`, kept equal to this text by the test of RFC 002 rule 19 (`contracts/florn-cps/wit/cps.wit`):

```wit
package florn:cps@0.1.0;

/// A controller on a network acting on a plant: the core world, the network's interfaces
/// and the circuit's signals (ARCH-010).
world networked-controller {
    include florn:core/guest@0.1.0;
    import florn:network/link@0.1.0;
    export florn:network/router-events@0.1.0;
    import florn:circuit/signals@0.1.0;
    export florn:circuit/controller-events@0.1.0;
}
```

The scenario declares both domains; each validates the ports of its types; the network delivers frames to `on-message`, the circuit domain delivers samples to `on-sample`. Neither domain crate owns the world: the Host registers it with the domains' own worlds when it holds both domains (finding F-9), the scenario's use of it is checked at validation (RFC 002 rule 48), and the module's manifest names it as `world = "florn:cps/networked-controller@0.1.0"`. This is the two-domain test of RFC 002's verification table and the cyber-physical destination of ARCH-010. A `set-output` committed from `on-message` or `on-timer` is an input change at a tick that is not a sample; rule 9 covers it.

## 6. Built-ins: the plant

**Rule 23.** The plant is made of built-ins (BUS-009, RFC 008 rule 19): actors of kind `builtin = "circuit/<name>"` run by the domain with no instance, no budget and no quotas. Each has fixed ports and typed parameters in `config` (RFC 004 rule 17), validated by the domain (a quantity of the dimension declared; rule 5); a settable value (rule 15) is marked. Each is a device of RFC 013 rule 5 implemented in Host code; its stamps are the textbook ones. A built-in's `signal-in` has the unit its row gives: `cmd` and `ctl` are dimensionless and scaled by a parameter with a unit, `load` is in Nm; a signal that a sensor produces is in the sensor's unit (rule 4).

| Built-in | Pins and signals | Parameters | Settable | Model |
|---|---|---|---|---|
| `resistor` | `a`, `b` | `r` (ohm) | — | `i = (v_a − v_b) / r` |
| `capacitor` | `a`, `b` | `c` (F), `ic` (V, optional) | — | `i = c · dv/dt`, integrated by RFC 013 rule 6; without an `ic` the initial solve takes it open (RFC 013 rule 9) |
| `inductor` | `a`, `b` | `l` (H), `ic` (A, optional) | — | `v = l · di/dt`; its current is an unknown (modified nodal analysis); without an `ic` the initial solve takes it short |
| `voltage-source` | `p`, `n`; optional `signal-in` `cmd` (dimensionless) | `v` (V, default 0V), `gain` (V, default 1V) | `v` | `v_p − v_n = v + gain · cmd` when `cmd` is connected; its current is an unknown |
| `current-source` | `p`, `n`; optional `signal-in` `cmd` (dimensionless) | `i` (A, default 0A), `gain` (A, default 1A) | `i` | `i` from `p` to `n`, plus `gain · cmd` |
| `diode` | `a`, `k` | `is` (A, default 1e-14A), `n` (number, default 1), `gmin` (S, default 1e-12S) | — | Shockley with the limiting of RFC 013 rule 8, inside the device |
| `switch` | `a`, `b`; `signal-in` `ctl` (dimensionless) | `ron` (ohm), `roff` (ohm), `threshold` (number, default 0.5) | — | `r = ctl > threshold ? ron : roff`, read at the start of each step |
| `voltmeter` | `p`, `n`; `signal-out` `v` (V) | `period` (ticks), `phase` (ticks, default 0) | — | A sampled sensor (rule 10): sets `v = v_p − v_n` at the last accepted state; draws no current; reports on `probe` |
| `ammeter` | `a`, `b`; `signal-out` `i` (A) | `period`, `phase` | — | A zero-volt source whose current is an unknown; samples it as the voltmeter does |
| `constant` | `signal-out` `y` | `value` (a quantity in the unit of its signal, or a plain number when the signal is dimensionless; default 0) | `value` | A held signal set at start and by the `source` intervention: a set point, a load torque. A `value` written with a unit gives an undeclared signal that unit; one of another dimension than a declared `unit` is `circuit.signal-dimension` |
| `comparator` | `p`, `n`; `signal-out` `y` (dimensionless) | `threshold` (V, default 0V), `hysteresis` (V, default 0V) | `threshold` | A detector (rule 13); high impedance; `y` becomes 1 when `v_p − v_n` rises above `threshold + hysteresis/2` and 0 when it falls below `threshold − hysteresis/2`; its output starts at 0 (rule 30) |
| `dc-motor` | `a+`, `a−`; `signal-out` `speed` (rad/s), `signal-in` `load` (Nm; unconnected reads 0) | `r` (ohm), `l` (H), `kt` (Nm/A), `ke` (Vs/rad), `j` (kgm2), `b` (Nms), `period`, `phase` | — | Armature `v = r·i + l·di/dt + ke·ω`, shaft `j·dω/dt = kt·i − b·ω − load`; `ω` is a solver unknown integrated with the circuit; it starts in its DC steady state (RFC 013 rule 9); `speed` sampled at `period` |

The sampled sensors (`voltmeter`, `ammeter`, `dc-motor`'s `speed`) are what a controller can read and what the waveform view shows (RFC 014): a sensor's `period` bounds the data a scenario produces (OBS-003), and its samples are telemetry reports on channel `probe` with the layout of rule 24 **and** sends on its signal, so that they are in the trace as the domain's **results** (AUTO-004; RFC 013 rule 20 compares them). A load step is a scheduled change of a `constant` that drives the motor's `load`.

**Rule 24.** Telemetry the view reads (RFC 008 rule 17's form). A sensor built-in reports on `probe`, a controller guest may report on `control`, a detector reports on `condition`. The numbers are part of the trace, so they are fixed: `probe` is channel 1 and `condition` is channel 2 for every built-in kind that has them (`voltmeter`, `ammeter` and the `dc-motor` report on 1, the `comparator` on 2; channel 0 is the log, RFC 002), frozen by a format test of the kit; a guest's `control` has the number its module declares and is found by name and layout (RFC 008 rule 17). A guest that declares the layout gets its series drawn (RFC 014), one that does not is shown by its events alone.

| Channel name | Fields | Meaning |
|---|---|---|
| `probe` | `value: f64` | A sensor's sample at the report's instant, in the signal's unit |
| `control` | `input: f64`, `output: f64`, `error: f64` | A controller's sample: what it read, what it set, and the error it computed; units from `control-units` (rule 18) |
| `condition` | `output: f64`, `located: u64` | A detector's new output and the tick of the located crossing (rule 13) |

## 7. The guest SDK and the ladder

**Rule 25.** One crate per world (RFC 008's recorded constraint: two worlds in one crate would generate the included core interface twice): `florn-guest-circuit` for `controller`, `florn-guest-device` for `device` (it exports `cabi_post_evaluate`, F-16), `florn-guest-cps` for `networked-controller` (it depends on the network and circuit WIT sets). The traits `Controller` (`on_sample(now, index)`), `Device` (`begin_step(start, h)`, `evaluate(&[f64]) -> Vec<f64>`, `accept_step(&[f64])`, `reject_step`) and the composed pair; `read_input(port) -> Option<f64>` and `set_output(port, value)`; the macros `controller!`, `device!`, `networked_controller!`. A `Device` implementation is given a `Trial` helper that keeps the trial state apart from the committed state so that `reject_step` is a drop, and `pnjlim` with the solver's literals (RFC 013 rule 8); `evaluate` returns the `n + n² + 1` values of rule 19. The four new public crates of 0.4 (`florn-domain-circuit` and the three SDKs) are inside the five-crate burst of crates.io.

**Rule 26.** The ladder of the circuit domain, shipped as SDK examples and conformance fixtures with recorded results:

| Guest | World | What it does | What it proves |
|---|---|---|---|
| `pid` | `controller` | A PID controller with `kp`, `ki`, `kd`, output limits and a `setpoint` parameter that is the default of its second input; reads two inputs (`measurement`, and `setpoint` when that port is on a signal), sets one output; reports `control` | The sample loop, the held signals, the closed loop of the exit criterion, a set point changed by an intervention on a `constant` |
| `guest-diode` | `device` | The diode of rule 23 as a guest, with the SDK's limiter | The compound activation; a guest device and the built-in agree within tolerance (RFC 013 rule 21) |
| `networked-pid` | `networked-controller` | `pid` whose set point arrives as a frame on its interface | Two domains in one actor (rule 22) |
| `bad-device` | `device` | Returns a NaN stamp, or the wrong length, or sets a timer, or returns stamps that never converge (`diverge`), as its parameter says | The contract violations of rule 20 and non-convergence (AUTO-005) |

**Rule 27.** The scenarios of the kit, each with its recorded results under both Hosts and on every engine: `rc-step` (a 1 kohm `resistor` and a 1 uF `capacitor` with `ic = 0V` on a `voltage-source` of 1 V stepped to 0 V at 2 ms by a scheduled change, a `voltmeter` across the capacitor; the analytic solution is the golden, RFC 013 rule 21), `diode-clip` (a `voltage-source` stepped between +5 V and −5 V by scheduled changes, a `resistor`, a `diode`, a `voltmeter` across the diode; nonlinear iteration and restarts), `guest-diode` (the same with the guest device in the diode's place), `motor-loop` (`pid` driving a `voltage-source` into a `dc-motor`, the `speed` back to the controller, the set point and the load torque each a `constant`; a load step scheduled at one second: the exit criterion's closed loop), `cps-loop` (`networked-pid` with its set point sent as a `message` intervention), `comparator-latch` (a `comparator` on an RC waveform and a `voltmeter` beside it: the condition's event and its located crossing), `hostile-stamp`, `hostile-diverge` (RFC 013 rule 17's non-convergence), `two-devices` is not a shipped scenario but a fixture of the kit's tests, since a refused scenario has no digest (it is refused with `circuit.device-limit`). The learning lab of 0.4 is `motor-lab`, the `motor-loop` scenario with its story in both languages (RFC 014 rule 11).

## 8. Validation and codes

**Rule 28.** The domain validates at scenario load and at module admission (RFC 008 rule 1), each with a code in a new RFC 007 table 4.7. Parameters are fixed per code (RFC 007 rule on codes); severity, scope and `ends` as listed; each code has a test that produces it (RFC 007 section 8).

| Code | Parameters | Severity, scope, ends | When |
|---|---|---|---|
| `circuit.no-ground` | — | error, file, command | Rule 4 |
| `circuit.two-grounds` | `connection: name` (the second) | error, connection, command | Rule 4 |
| `circuit.pin-open` | `actor: name`, `port: name` | error, port, command | A pin on no net |
| `circuit.port-type` | `connection: name`, `port: name`, `type: text` | error, field, command | A port on a `net` that is not a `circuit/pin`, or on a `signal` that is not a `circuit/signal-out` or `circuit/signal-in` (rule 3); the form of `network.port-type` |
| `circuit.signal-ports` | `connection: name`, `outs: count` | error, connection, command | A signal without exactly one `signal-out` |
| `circuit.signal-dimension` | `connection: name`, `port: name`, `expected: enum(dimension)` | error, port, command | A producer's or consumer's unit of another dimension |
| `circuit.signal-unconnected` | `actor: name`, `port: name` | warning, port, nothing | A built-in's `signal-in` on no signal |
| `circuit.attr-unknown`, `circuit.attr-type` | `connection: name`, `key: name`, `expected: text` | error, field, command | A connection attribute |
| `circuit.param-unknown`, `circuit.param-type`, `circuit.param-missing` | `actor: name`, `key: name`, `expected: text` | error, field, command | A built-in's `config` |
| `circuit.profile-unknown`, `circuit.profile-value` | `key: name`, `expected: text` | error, field, command | A `[circuit]` key (RFC 013 rule 1) |
| `circuit.no-period` | `actor: name` | error, actor, command | A controller or sensor without a period (rules 18, 23) |
| `circuit.period-zero` | `actor: name` | error, actor, command | A period of zero ticks |
| `circuit.kind-unknown` | `module: name`, `kind: text` | error, file, command | A `[circuit]` module table with a kind the domain lacks |
| `circuit.device-limit` | `found: count`, `maximum: count` | error, file, command | More than one guest device (rule 19) |
| `circuit.device-ports` | `module: name`, `found: count` | error, file, command | A device module that does not declare exactly one port kind of type `circuit/pin` with `min` and `max` of at least 2 and at most 16 (rule 19) |
| `circuit.plant-unobserved` | — | error, file, command | A scenario with a controller or a settable value and neither a sensor nor a detector (rule 8) |
| `circuit.floating-node` | `connection: name` | warning, connection, nothing | A net, or a set of nets joined only by elements that conduct no direct current, with no direct-current path to ground, by the list of conducting elements in RFC 013 rule 9: its initial voltage is set by `gmin` and means nothing; give a capacitor an `ic` or add a resistor |
| `circuit.too-large` | `unknowns: count`, `maximum: count`, `bytes: bytes` | error, file, command | The estimate of RFC 013 rule 18 above the profile's bound (NUM-003) |
| `circuit.no-operating-point` | `unknown: name`, `kind: enum(unknown-kind)`, `iterations: count` | error, run, run | The start hook's initial solve fails or meets a singular matrix (RFC 013 rule 9), naming the unknown (a net's voltage, an element's current or state); the run does not start |
| `circuit.stamp-invalid` | `actor: name`, `reason: text` | error, actor, activation | Rule 20 (a fault of kind `contract`) |
| `circuit.device-faulted` | `actor: name` | warning, actor, nothing | Rule 17, once |
| `circuit.non-convergence` | `actor: name`, `tick: tick`, `iterations: count` | error, run, nothing | RFC 013 rule 17; under `stop-run` the run then ends with `non-convergence` |
| `circuit.step-rejected` | `tick: tick`, `attempts: count`, `cause: enum(rejection-cause)` | info, run, nothing | RFC 013 rule 17: once per advance in which attempts were rejected, the cause being that of the first |
| `circuit.step-minimum` | `tick: tick`, `step: count` | warning, run, nothing | RFC 013 rule 17 |

A period that is not a whole number of ticks is the core's `quantity.not-whole-ticks`; a quantity of the wrong dimension is the core's `quantity.dimension` (`field: text`, `value: quantity`, `expected: enum(dimension)`; error, field, command; finding F-13). The enumeration `dimension` (RFC 007 section 4.5) gains the dimensions of rule 5; new enumerations are `rejection-cause` (`error`, `convergence`) and `unknown-kind` (`voltage`, `current`, `state`), and `run-limit` gains `pending-breakpoints` (rule 9); `end-reason` gains `non-convergence` and `actor-fault` (finding F-7); `faulted-policy` gains nothing.

## 9. Checkpoint state

**Rule 29.** The domain's `export` (RFC 008 rule 1; RFC 009 rule 1) is its whole state in the canonical encoding, with a version byte first: `solved`; the solution vector and the integration history of RFC 013 rule 6 with the count of steps since the last cut, and the step controller's state, including the unconstrained next step; the **input values in force**, as the last step used them; the pending **breakpoints** in order of recording; every signal's held value and its frozen flag; each sensor's next due tick and each controller's next sample tick and index; the tick of the pending `advance`; each detector's published output and the output it last found; the state of every built-in (the motor's speed and current are in the solution vector, a comparator's output is its detector's); the plant's status, running or held (RFC 013 rule 17), with the once-only marks of the non-convergence notification and of the faulted-device flag; every f64 as its canonical bits (RFC 001 rule 59); and last the **inspection summary** of finding F-12 (the last advance's proposal, steps, iterations and rejections, and the rejections so far), which is observation, deterministic like the rest, and exported so that a restore, a scroll-back included, shows the state as of the checkpoint. A checkpoint written by one Host restores in the other and the run continues to the same results as an uninterrupted one on the same Host and within the comparison of RFC 013 rule 20 on the other (RFC 009 verification, extended to the circuit scenarios). The device module's own state is its actor state, taken at the boundary where no compound activation is open.

## 10. Verification

| Property | Test |
|---|---|
| Breakpoints | A change committed or recorded at tick T acts on the step that starts at T: in `rc-step` a source step at T leaves the voltmeter's sample at T unchanged and changes the next; the same for an intervention, a scheduled change and a controller output |
| Sample order | At a tick where `pid` and the `voltmeter` both sample, `pid` reads the previous sample; with `phase = 1` on `pid` it reads the current one |
| Observation | A run with every inspector open and checkpoints taken at every boundary has the digest of a run with none (RFC 001 rule 39): the step sequence is unchanged |
| Input outside a sample | `cps-loop`: a set point frame handled between two samples changes the plant from the tick it was committed, not before (rule 9) |
| Compound activation | `guest-diode`: a rejected step leaves the guest's committed state unchanged (the guest reports its charge on accept only; the kit asserts the report sequence against the step record); `bad-device` faults with `circuit.stamp-invalid` for each of its modes (a Fault record keyed by the advance), `diverge` ends with `non-convergence`, and the plant continues under `drop`; the initial solve of a guest device leaves one record, keyed as its `init` |
| One guest device | `two-devices` is refused with `circuit.device-limit` before anything is instantiated |
| Two domains | `cps-loop`: the set point frame changes the controller's behaviour at the next sample; the scenario validates with both domains and refuses with one |
| Detector | `comparator-latch`: the condition fires at the advance that finds the crossing, at most `detect-interval` after it; the located crossing is within one internal step of the analytic crossing; a comparator already above its threshold at start reports it at the first advance that takes a step |
| Breakpoint bound | A scenario with a controller and no sensor is refused with `circuit.plant-unobserved`; a controller that outputs a changing value at every tick under a sensor of a long period ends the run with `run.limit-exceeded` (`pending-breakpoints`) at the bound, the same on both Hosts; two changes of one input at one tick leave one breakpoint; a `set-output` of the value already held records nothing and leaves the step sequence unchanged |
| Interventions | `source` on a `constant` at a tick between samples acts from that tick; `signal` freezes a sensor and the controller keeps running; on release the old value stays until the next sample |
| Faults | A controller that traps keeps its last outputs held; under `faulted-policy = "stop-run"` the run ends with `actor-fault` |
| Validation | Each code of rule 28 produced by a scenario built to provoke it, in one pass |
| Units | A parameter written in the wrong dimension is `quantity.dimension`; `1.2kohm` becomes 1200 by exact decimal arithmetic; a `voltmeter` on a signal with no `unit` holds volts, and on a signal of unit `mV` the value divided by the factor of `mV` |
| Checkpoint | Every circuit scenario of rule 27 restored at its half under the other Host continues within the comparison of RFC 013 rule 20; restored under the same Host it continues with the same digest |
| Interface | `florn-domain-circuit` implements RFC 008 section 2 with the Host changes of section 11 and no other: the dependency check of RFC 005 rule 1 and a review against the findings table |

## 11. Findings against the core

The roadmap's purpose for Phase 4. Each row is a change the core needs for this domain, made by the handoff's first units and recorded here when done; a row that turns out unnecessary is struck, not deleted. The first text listed F-1 to F-7 and said "Nothing in the kernel changes"; the entry review (`AD-25`) verified F-8 to F-15 in the code and corrected F-2 to F-7, and the development team added F-16 on reading the code.

| # | Finding | Where | Change |
|---|---|---|---|
| F-1 | The flat call shapes know `i32` and `i64` only: there is no `f64`, and the rows the circuit needs are absent — `on-sample` `(i64, i64)`, `begin-step` `(i64, f64)`, `reject-step` `()`, `evaluate`'s export `(i32, i32) → i32` (a list result comes back through a pointer), `set-output` `(i32, f64)`; `read-input` returns `option<f64>` through memory and fits an existing row. `flat_signature` applies the import convention (the return pointer is a last parameter) to exports. The browser adapter has one hand-written trampoline per shape and import slot with `i32`/`i64` arms only; Wasmtime ignores the shape and accepts what the browser cannot call | `florn-abi`'s shape table and `flat_signature`; both engine adapters (RFC 008 rule 6) | `f64` among the flat types, the five rows above, the export convention in `flat_signature`, `list<f64>` lift and lower with the length checked before the lift; the trampolines; an adapter conformance case with floats and lists, which also checks the canonical ABI's way of returning a list from an export (F-16); a test of `module.export-unsupported` |
| F-2 | An activation ends by commit or by fault; a rejected step needs a third outcome that discards without a fault (rule 21). `Kernel::fault` writes a Fault record and kills the actor | `florn-kernel` (`begin`, `commit`, `fault`), `florn-host`'s `Outcome` | A kernel `discard`: nothing recorded, stream restored, actor alive, `in_activation` cleared |
| F-3 | A compound activation spans several calls of one export set; the Host activates one export per event, and the results of an export are discarded (`WorldSigs.exports` keep no result type); `Domain::event` returns actions that run after it returns, so a solver cannot call a guest and use the answer inside the event | `florn-host`'s `Run::apply`, `activate`, `activate_slot`, the `Domain` trait | A call-in handle given to the domain while it handles an `advance` or the start hook: open, call an export with lowered arguments and lifted results, close with commit or discard; the budget set at the opening call and read at the closing (RFC 003 rule 12), one activation open at a time. The domain sits inside the `Composite` that `commit` borrows, so the handle commits with a world that refuses sends (a device sends nothing, rule 19) or the Host lends the domain out for the call. When a device's fault under `stop-run` ends the run, the advance is abandoned and the plant keeps its last accepted step (rule 17) |
| F-4 | **Struck.** `Domain::answer` returns `Option<Val>` and `Val` has `Option` and `F64` (`florn-abi/src/val.rs`), so `read-input` needs no change; `None` still means "refuse the call and fault" | — | — |
| F-5 | The replay comparison knows `matched`, `diverged` and an internal `Incomparable` (stored points at different positions, reported as `replay.diverged`); a continuous domain adds `within-tolerance` and `needs-payloads` (RFC 013 rule 20). RFC 007 has `replay.matched` and `replay.diverged` only; `run.toml` has no comparison field and formats are frozen | `florn-host`'s comparison, the control interface, the runner's text and exit codes, RFC 007 | Four codes with sentences in both languages, all with the scope run and ending nothing: `replay.within-tolerance` (info; `compared: count`, `differing: count`, `worst: name`, `largest: quantity`, `at: tick`, where `worst` is the field that uses the largest share of its tolerance and `largest` its absolute difference in that field's unit; a `quantity` parameter of a dimensionless field has an empty unit text, which RFC 007 rule 6 gains), `replay.value-differs` (error; `record: count`, `field: name`, `recorded: quantity`, `replayed: quantity`), `replay.record-differs` (error; `record: count`, `what: enum(record-difference)`) and `replay.needs-payloads` (error; `side: enum(replay-side)`), with the enumerations `record-difference` (key, structure, payload, length) and `replay-side` (recorded, replayed, both); a tolerance comparison that is `diverged` is accompanied by the first differing record's `replay.value-differs` or `replay.record-differs` in place of `replay.diverged`; `florn replay` reads the record's `trace/` and `payloads/` and keeps its own run's (a circuit scenario implies `--trace --payloads` for the replay); the outcome is a result of `replay`, not a field of the record; AUTO-006 and RFC 001 rule 50 read "equal digests or the comparison of the domain's tier" |
| F-6 | The display stream's record kinds are `instant`, `flight`, `activity`; `decode_all` returns `None` on an unknown kind and the shell drops the batch; `display::VERSION` is read nowhere | RFC 005 rule 14; `florn-control/src/display.rs`; the shell | RFC 014 rule 3: kind 3 `sample`; readers skip an unknown kind by its fixed size; the stream's version becomes 2 and is carried in the ring header and each message batch and checked by the readers |
| F-7 | `end-reason` is `end-reached`, `user`, `host-fault`; a domain can neither end a run nor refuse one; the Host ends a `stop-run` guest fault as `user` | RFC 007 section 4.5; `Action`; `run.rs` | `non-convergence` and `actor-fault`; an end-run request from the domain, an `Action` for what it handles in `event`, `change`, `intervene` and `start` and a flag the Host reads after every commit for what `deliver` finds, since a delivery returns no `Action`; a domain's limit uses `host-fault` with `run.limit-exceeded` and leaves the run reproducible, as the kernel's limits do; the runner's exit code 3 for every fault ending, also with `--to` |
| F-8 | **Deferred (AD-25a).** The kernel permits one open activation (RFC 001 rule 21, asserted), and the browser engine shares one budget global; two guest devices would need overlapping compound activations. 0.4 limits a scenario to one guest device (rule 19) | `Kernel::begin`; the browser adapter | A later RFC: groups of compound activations under one event committed in a canonical order, and a save-and-restore of the shared budget around every call or a global per instance |
| F-9 | The Host registers at most one guest world per domain (`Domain::world() -> Option<DomainWorld>`) and no world that spans two domains; admission matches a module's world by name against a registry built when the controller is created, before any scenario | `florn-host`'s `Domain`, `Domains::worlds`, `Controller::new`; `florn-admission` | `worlds() -> Vec<DomainWorld>`; composed worlds registered in `Controller::new`, where the domains' own worlds are, when the Host holds every domain they span, and their use checked at validation (RFC 002 rule 48); the contract of record of rule 22 and its equality test (multi-block WIT) |
| F-10 | **Falls away** under F-8's deferral: with one guest device no other instance is activated while its compound activation is open, so swapping cannot evict it | — | — |
| F-11 | A domain import cannot record an effect: `Handler::call` sends every non-core import to `Domain::answer`, which returns a value; only the five core imports reach the activation's sink | `florn-host`'s `Handler::call` | A domain declares an import as effect-recording (`set-output`: record `send(port, canonical bytes of the value)`); no new effect kind (rule 11) |
| F-12 | The control interface cannot describe a domain's state: `ActorSummary` has `module: None` and no kind or state for a built-in, `Query` has nothing for a connection or the world, `RunStateInfo` has no domain fields, and `Domain` has no method that answers for any of them | `florn-control/src/message.rs` (`Query`, `ActorInfo`); `Domain` | A `Domain` method `inspect(target)` answering a list of named fields (text, and for a real its value and unit), and a query `inspect { id }` for a built-in actor, a connection or the world; `actor` keeps answering for guests; the answer reads the last accepted state (rule 31). Additions to an answer of version 1 (this query, the report payloads of `trace-window`) are a trailing optional section: an old Host's answer lacks it and a front end reads that, an old front end ignores it, and the format tests record both forms |
| F-13 | The unit registry has no dimensions (`Domain::units` returns `(symbol, factor)`), the config encoder looks the declared unit up and ignores it, and a `real` is `f64(number) × f64(factor)`: three roundings, of the number, of the factor and of their product | `Domain::units`; `florn-admission/src/config.rs`; RFC 004 rule 27 | Units carry a dimension; the encoder checks it and converts exactly, once rounded; `quantity.dimension` with its sentences; non-finite `f64` and `real` values are refused |
| F-14 | The faulted-actor policy has no route from a domain to the Host and the scenario has no field for it; `FaultPolicy::StopRun` is unreachable | `run.rs`; the scenario format | The domain's `faulted-policy` (rule 17) read through a `Domain` method; the policy of a composed actor is the more severe of its domains' |
| F-15 | A domain has no start hook, cannot refuse a run at start and cannot end one: `Domain::compose` returns nothing and is called once at composition | `Domain::compose`; `Action` | A start hook after the guests' `init` at tick 0, in identity order, that returns actions and may refuse (rule 30) |
| F-16 | The Host never calls `cabi_post_*` after lifting a list or string result of an export, and admission never requires the export. RFC 002 rule 2 makes it part of the contract and RFC 003's admission table lists it; `lift_results` documents the call and nothing makes it. No world has had a list-returning export so far. The device's `evaluate` is the first, so each Newton iteration would leave its result buffer (`n + n² + 1` f64) allocated in the guest's memory until the memory limit traps the device | `florn-abi` (`lift_results`); `florn-admission` (`validate.rs`, the required exports); the export-call path of both engine adapters; `florn-host`'s call-in handle (F-3) | Admission requires `cabi_post_<export>`, of type `(i32) → ()`, for every export whose result is a list or a string (`module.export-missing`); after lifting such a result the Host calls it once with the result pointer, inside the same compound activation and so under its budget (the shape is row 7 of the table, and the adapters' export path is checked by the case below); a conformance case with a list-returning export asserts that the guest's memory does not grow over a thousand calls; the SDK's `device!` macro exports it |

If the implementation finds a seventeenth, it is added here before the code is written (RFC 000: the RFC is amended first, the handoff after).

**Rule 30.** The start hook (finding F-15). After every guest's `init` has run at tick 0, in ascending identity, and before the first event is handled, the Host calls the domain's `start` with the call-in handle of F-3. In this order the circuit domain: (1) sets the held value of every `constant` to its value and of every comparator to 0, so that the signals in force at tick 0 are known; (2) builds the plant and takes the inputs in force from the signals' held values, which include the outputs the controllers' `init` set (rule 11), consuming the breakpoints those recorded at tick 0; (3) performs the initial solve of RFC 013 rule 9, its device calls, if any, in DC form and keyed as the device's `init` (rule 21); (4) schedules the first samples and the first `advance` (rules 8 and 10). It may refuse the run with `circuit.no-operating-point`. A change or an intervention at tick 0 is an event handled after the hook: it records a breakpoint at tick 0 and acts from the first step.

**Rule 31.** Inspection (finding F-12). The domain answers the `inspect` query for a built-in actor with its kind, its parameters as written and, from the last accepted step, its pin voltages and current; for a net with its voltage; for a signal with its held value, its unit and its producer; and for the world with the solver's proposal, method and status, the steps, iterations and rejections of the last advance and the rejections so far. The answer is a read of what was solved and never advances the plant (rule 8).

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| The solver as a loop beside the kernel, advancing between events | Two sources of time; the trace could not say when the plant was where. Advancing inside events keeps one clock |
| A step event at every step end (the first text) | A located crossing lies in the past when found, which RFC 001 rule 5 forbids; the instant of a change at a step end is ambiguous; stale events need a mark that a checkpoint must carry; an input committed at a tick the domain did not schedule must advance the plant inside a commit, where no guest may be activated |
| Class 9 `solver` events for observations (the first text) | They would take sequence numbers from the world's counter, so the number of attempts a Host made would shift every later key of the trace: observation changing the trace (RFC 001 rule 39). Notifications take none |
| Evaluate ahead and roll back | Computes steps that an unscheduled input may invalidate, and needs the pre-step state kept; on-demand advance with breakpoints computes each step once |
| Advancing the plant inside a commit or an intervention | A commit must not activate a guest (the kernel allows one open activation); an observation must not change the step sequence |
| Two or more guest device models in 0.4 | Overlapping compound activations need a kernel change and a per-activation budget in the browser, in the phase that tests whether the kernel needs to change; refused for 0.4 (F-8), built-in devices are not limited |
| A controller as a device model with state (one kind of actor) | PHY-002: a controller called inside an iteration is corrupted, and its state could not be committed per step without a contract for rollback it has no reason to have |
| Device models as built-ins only (no device world in 0.4) | The compound activation is the one rule of RFC 002 never exercised, and the `guest-diode` against the built-in is the cheapest strong test of it; the exit criterion needs only the controller, so the device world is small and its ladder entry is one example |
| `set-output` as a new effect kind | Changes the trace encoding and needs a format version; a `send` on the signal port is the same information, already recorded and digested |
| Mechanical ports (`circuit/shaft`) in 0.4 | PHY-007 is `later`; a motor with its mechanics inside is enough for the controller lab. A second continuous subdomain doubles the solver's unknown kinds for no 0.4 purpose |
| Streaming every solver step to the view | OBS-003: the Host reduces first. Sampled sensors give the scenario author the data rate, and make the waveform the same thing a controller reads |
| Floating point in the kernel's events (a step size in seconds) | RFC 001 rule 56; a step is whole ticks and the arithmetic in seconds stays inside the domain |

## Prior art

SPICE's `.TRAN` with breakpoints at source discontinuities is rule 9; Modelica's synchronous sampled blocks (`sample`, `hold`) are the signal of rule 3; Simulink's zero-crossing detection is rule 13, here reported at the advance that finds it because the clock belongs to the kernel; software-in-the-loop controllers in motor drives sample at a fixed period and act through a held command, which is rules 10 and 11, with the one-sample delay of a digital loop stated.

## Consequences for other RFCs and the requirements

Amendments to the RFCs of `rfcs/done/` are made by the units that implement them, as AD-19 decided for the network domain.

| RFC | Must provide |
|---|---|
| 001 | Classes 6 to 8 as rule 14 assigns them; section 4.1's "Total" read as "the key is unique": the sequence number of an actor's class-7 and class-8 events is the world's counter (rule 10), so origin and sequence number are unique per class; the `discarded` outcome beside commit and fault (rules 22 to 24, F-2); rule 3 and RFC 002 rule 22 noted for the device model (rule 19); rule 21 unchanged and F-8 deferred; rule 37's outage as the `signal` intervention; rule 50's "faithful" read with RFC 013 rule 20 |
| 002 | The `[circuit]` table of the module manifest (`kind`, `period`, `phase`, `control-units`) in the domain table of RFC 008 rule 16; the contracts of record `florn:circuit@0.1.0` and `florn:cps@0.1.0` and their equality tests; the compound-activation and two-domain rows of its verification table, written at last; rule 47: an import that records an effect (F-11); rule 2: the `cabi_post_*` export of a list or string result, first needed here (F-16) |
| 003 | Rule 12 (one budget per compound activation) exercised with one open activation; the estimate of RFC 013 rule 18 is made in the domain's `validate`; the admission table's `cabi_post_*` row, never implemented (F-16) |
| 004 | The units of rule 5 with dimensions (rule 27); the `[circuit]` solver profile; single-pin nets do not exist |
| 005 | The findings F-1 to F-3, F-5 to F-7, F-9 to F-16; `florn-domain-circuit` and the three SDK crates in the layout of rule 42; the kit's circuit rows; rule 14's display kinds |
| 006 | Section 7's maxima gain the plant's unknowns and breakpoints (RFC 013 rule 18) and the pins of a guest device (rule 19), each tested one above and one below; **a new capability by rule 3's definition**: `read-input` and `set-output` are Host functions a guest can call, and a device's `evaluate` is a place where the Host lifts a guest-sized list; the threat model and the review of rule 5 name them in the 0.4 release |
| 007 | Table 4.7 (rule 28); `dimension` gains the domain's dimensions; `rejection-cause` and `unknown-kind`; `run-limit` gains `pending-breakpoints`; `end-reason` gains `non-convergence` and `actor-fault`; `quantity.dimension`; the four `replay` codes of F-5 with their enumerations; rule 6: a `quantity` parameter may have an empty unit text, for a dimensionless field |
| 008 | The interface additions of section 11: several and composed worlds, the start hook, an action that ends a run, built-in inspection, the policy, units with dimensions, effect-recording imports; the promise "where they would need more, section 10 says so" (§1) is answered by section 11 here |
| 009 | The domain's state in a checkpoint (rule 29) |
| Requirements | PHY-001, 002, 008, 009 are rules 2, 18 to 20, 7 to 14; PHY-007 stays `later`; the acceptance sentence of section 22 for 0.4 is verified by `motor-loop` under both Hosts; ACT-011's declared policy is met for the first time (rule 17) |

## Open questions

All four of the first text were decided at acceptance (2026-10-31, as proposed) and are recorded here.

| # | Question | Decision |
|---|---|---|
| 1 | Whether the device contract carries the Jacobian from the guest | Decided: it does (rule 19); a guest that cannot write one returns finite differences itself |
| 2 | Whether a `signal`'s unit conversion is wanted in 0.4 | Decided: it is (rule 4) |
| 3 | Whether `comparator` and the detector mechanism ship in 0.4 | Decided: they do; the mechanism is amended by D4 (rule 13) |
| 4 | The sample period's place | Decided: a parameter the manifest names for guests (rule 18) and a built-in's own `period` (rule 23); both validate alike |
