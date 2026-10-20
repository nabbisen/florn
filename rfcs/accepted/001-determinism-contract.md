# RFC 001 — Determinism contract

**Status.** Accepted (2026-10-13). Open questions 1 to 5 are deferred as stated in their table; none blocks implementation. Open questions 3 and 4 resolved 2026-10-20 by PR-13.
**Tracks.** Requirements DES-001 to DES-010, BUS-002, BUS-004, BUS-007, FIN-004, FIN-006, NUM-002, NUM-006, NFR-001, NFR-011, DATA-002, DATA-005, AUTO-003, AUTO-004, AUTO-006. Requirement IDs refer to the florn requirements specification, which will be published under `docs/`.
**Touches.** The kernel, every guest contract, the scenario format, replay, and the conformance tests.

## Summary

florn promises that a run can be reproduced. This RFC defines what the promise covers and the rules that make it hold: how time is represented, how events are ordered, what an activation is, where randomness comes from, how a user's actions are recorded, and what replay means.

Every other contract inherits these rules. They are settled first for that reason.

## Background

The specifications call florn deterministic and reproducible throughout, and define neither word. Four gaps follow from that.

1. No rule orders two events that carry the same time.
2. Nothing records what a user does during a run, so a run that contains such an action cannot be reproduced.
3. "Replay" may mean executing the guests again or showing a stored record. These are different features with different guarantees.
4. florn has two Hosts, one in the browser and one native. Nothing states what must be equal between them.

A rule added after a contract is accepted changes the contract. The rules therefore come before the contracts.

## Terms

One term names one concept. These definitions bind every later RFC, the user interface, and the public interfaces.

| Term | Meaning |
|---|---|
| **Logical world** | All state that the run inputs determine: logical time, events, actors, and the state of the domain. |
| **Host** | The part of florn that owns the logical world and executes guests. |
| **Guest** | A WebAssembly module supplied to the Host. |
| **Actor** | One participant in a scenario with an identity of its own. An actor is backed by an instance of a guest, or is a built-in of the Host. |
| **Origin** | Whatever causes an event: an actor, a connection, a component of the domain, or the source of interventions. Every origin has a stable identity. |
| **Event** | Something scheduled to happen at one instant, addressed to one target. |
| **Activation** | One call from the Host into a guest-backed actor, to handle one event. |
| **Effect** | A request that an actor makes during an activation: to send, to set or cancel a timer, to report telemetry. |
| **Fault** | The abnormal end of an activation, or of a run. |
| **Intervention** | A change to the logical world that comes from outside it during a run: from a user or from a script. |
| **Tick** | The unit of logical time. |
| **Instant** | A pair: a tick count and a microstep. |
| **Scenario** | The definition of a logical world before it runs. |
| **Run** | One execution of a scenario. |
| **Run inputs** | Everything that determines a run (rule 40). |
| **Trace** | The ordered record of what happened in the logical world during a run. |
| **Digest** | A hash over the trace, used to compare runs. |
| **Replay** | Execution of a run again, from its inputs. |
| **Playback** | Display of a stored trace. Nothing is executed. Playback is not replay. |
| **Checkpoint** | The stored state of the logical world at the boundary between two instants. |
| **Pacing** | The relation between logical time and the wall clock while a run is displayed. Pacing is not part of the logical world. |

## 1. What florn promises

### 1.1 Tiers

| Tier | Compared runs differ in | Promise |
|---|---|---|
| **1 — Same build** | Nothing. Same florn build, same engine and version, same CPU architecture. | The traces are identical, bit for bit. Every domain. |
| **2 — Cross-host** | Host (browser or native), engine, or CPU architecture. Same florn version. | Discrete domains: the traces are identical, bit for bit. Continuous domains: the results agree within a tolerance that the domain documents. |
| **3 — Cross-version** | florn version. | Not promised before 1.0.0. Every released version remains available, so that a run can be reproduced under the version that produced it. |

Each domain RFC declares its class.

| Class | Definition | Examples |
|---|---|---|
| Discrete | Every Host computation that influences the trace uses integers only. | Network, market |
| Continuous | The Host computes with floating point. | Circuit, physics |

### 1.2 What is compared

The digest of the trace.

Not compared: the memory of a guest, data taken from the wall clock, display output, and performance.

## 2. Time

**Rule 1.** Logical time is an unsigned 64-bit count of ticks. A run starts at tick zero.

**Rule 2.** The scenario declares the tick length. It is 1 ns, 1 ps, or 1 fs. The default is 1 ns. It does not change during a run.

| Tick length | Longest run |
|---|---|
| 1 ns | 584 years |
| 1 ps | 213 days |
| 1 fs | 5.1 hours |

**Rule 3.** The kernel and every contract represent time in ticks. No floating-point value represents time in the kernel, and none crosses a contract as a time.

**Rule 4.** A duration written in a scenario carries its unit. A duration that is not a whole number of ticks is a validation error. The Host does not round.

**Rule 5.** Logical time does not decrease. No event is scheduled before the current instant.

**Rule 6.** An event scheduled beyond the largest tick is refused. The refusal is a fault of the origin that asked for it.

**Rule 7.** An activation takes zero logical time. The work it consumed is measured and recorded (rule 46). It does not advance time.

**Rule 8.** A solver of a continuous domain may compute in floating-point seconds internally. It exchanges time with the kernel in whole ticks, and its step boundaries fall on ticks.

**Rule 9.** Pacing cannot influence the trace. Pausing, stepping, and changing speed change when the Host works, never what it computes.

### 2.1 Advancing a run

The kernel offers these units. The user interface chooses which to show and how to name them.

| Unit | The kernel handles |
|---|---|
| One event | The next event in order |
| One instant | Every event of the current instant |
| One tick | Every instant of the current tick |
| To a tick | Everything before the named tick |

## 3. Instants

**Rule 10.** An instant is a pair of a tick count and a microstep. For every tick the microstep starts at zero.

**Rule 11.** An effect with a delay of zero produces an event at the same tick and the next microstep. An effect with a positive delay `d` produces an event at tick + `d`, microstep zero.

**Rule 12.** Every event of one instant is handled before any event of a later instant.

**Rule 13.** The number of microsteps in one tick is limited. So is the number of events in one instant. Exceeding either limit ends the run with a fault that names the origins involved. A scenario may lower the limits. It cannot raise them above the maximum of the Host.

### 3.1 Why microsteps

An actor that handles an event at tick `t` may cause another event at tick `t`. That event must come after its cause. With time alone, "after" cannot be expressed. The microstep expresses it. The technique is known from hardware simulation as the delta cycle.

Rule 13 turns an endless chain of zero-delay events into a reported fault. Without it the run would stop advancing and give no reason.

## 4. Order of events

**Rule 14.** Events are handled in ascending order of this key:

| Position | Part | Type | Source |
|---|---|---|---|
| 1 | Tick | Unsigned 64-bit | Rule 11 |
| 2 | Microstep | Unsigned 32-bit | Rule 11 |
| 3 | Class | Unsigned 8-bit | Rule 15 |
| 4 | Tie value | Unsigned 64-bit | Rule 18 |
| 5 | Origin | Unsigned 32-bit | Rule 16 |
| 6 | Sequence number | Unsigned 64-bit | Rule 17 |

**Rule 15.** Class 0 is reserved for interventions and for changes that the scenario schedules. A domain assigns the other classes to its kinds of events and documents the assignment in its RFC.

**Rule 16.** The origin is the stable identity of whatever caused the event. The scenario gives it, or the Host reserves it. It never depends on the order in which actors were created.

**Rule 17.** Every origin counts the effects it has committed. The count at the moment of commit is the sequence number of the effect and of the event it produces.

**Rule 18.** The scenario selects one tie policy.

| Policy | Tie value | Equal-time events of one class are ordered |
|---|---|---|
| `by-identity` | Zero | By origin. Always the same way. |
| `by-seed` | A keyed hash of the root seed, tick, microstep, class, and origin | In an order that differs from instant to instant and is fixed by the seed. |

**Rule 19.** No part of the key depends on the order in which events entered the queue, on the wall clock, or on a thread.

### 4.1 Properties

| Property | Reason |
|---|---|
| Total | Origin and sequence number together are unique. No two events share a key. |
| Causal | An event sorts after its cause (rules 5 and 11). |
| Local | An origin computes the key of its events from its own state and the current instant. |
| Independent of arrival | Rule 19. This keeps the order valid if execution is ever distributed over threads. |

### 4.2 Choosing a tie policy

| Policy | Strength | Weakness | Default for |
|---|---|---|---|
| `by-identity` | A person can predict it. | The origin with the lower identity wins every race. | Network |
| `by-seed` | Over many races, no origin is favoured by its identity. A single race is decided by the seed. | A person cannot predict it without computing the hash. | Market |

## 5. Activation

**Rule 20.** One event addressed to a guest-backed actor causes one activation of that actor.

**Rule 21.** The kernel handles one event at a time. It starts no activation while another is in progress. A later RFC may relax this rule only if the trace stays identical.

**Rule 22.** During an activation the Host records the effects of the actor. It does not apply them.

**Rule 23.** When an activation returns normally, its effects are committed in the order in which the actor made them.

**Rule 24.** When an activation ends in a fault, its effects are discarded, and so is every change to the actor's state made during the activation, including the position of its random stream. The fault is committed to the trace in their place. The actor handles no further event until it is reset or restored.

The faults of an activation are: a trap; exhaustion of the execution budget, the stack limit, the memory limit, or a quota; and a violation of the contract.

**Rule 25.** A Host function that returns a value answers from two sources only: the state of the logical world at the start of the activation, and the effects that this activation has recorded so far.

**Rule 26.** What happens to an event addressed to a faulted actor is defined by the domain and declared in the scenario.

### 5.1 Why effects are recorded first

| Reason | Explanation |
|---|---|
| A fault leaves nothing behind | An activation happens entirely or not at all. No half-sent message exists. |
| The Host is not changed while a guest runs | A guest calls the Host while the Host waits for the guest. Recording keeps that call from touching state the Host is using. |
| The trace has a natural unit | One handled event and what it committed. |

## 6. Randomness

**Rule 27.** A run has one root seed, an unsigned 64-bit integer. It is a run input.

**Rule 28.** Every consumer of randomness has a stream of its own: each actor, each connection, each component of the domain. A stream is identified by the identity of its owner and a purpose label.

**Rule 29.** The key of a stream is derived from the root seed, the identity of the owner, and the purpose label. It depends on no other stream and not on the order of creation.

**Rule 30.** The Host supplies uniformly distributed unsigned 64-bit integers and nothing else. Ranges, floating-point values, and distributions are computed from them by algorithms that florn specifies.

**Rule 31.** A guest has no other source of randomness.

**Rule 32.** The generator and the derivation are named in the run identity. Changing either is a change of version.

### 6.1 Consequences

| Consequence | From |
|---|---|
| Adding or removing an actor leaves every other stream unchanged. | Rule 29 |
| Two variants of a scenario can be compared under the same random conditions. | Rule 29 |
| An update of a library cannot change results without notice. | Rule 30 |

## 7. Interventions

**Rule 33.** An intervention is applied at the boundary between two instants: after every event of the current instant, and before any event of the next.

**Rule 34.** An applied intervention becomes an event of class 0. If the requester names a later tick, the event is placed at that tick, microstep zero. Otherwise it is placed at the current tick and the next microstep.

**Rule 35.** Every applied intervention is added to the run inputs, with its instant and its content.

**Rule 36.** Replay applies the recorded interventions at their recorded instants.

**Rule 37.** The core offers no operation that suspends one actor while time advances for the others. A domain may define the outage of a node as an intervention, with stated consequences for the events of that node.

**Rule 38.** An intervention requested while a run advances is applied at the first boundary after the Host receives it. The user interface shows the instant at which it was applied.

### 7.1 Interventions and the scenario

| | Part of | Example |
|---|---|---|
| Scheduled change | The scenario | "At 10 s, take this link down." |
| Intervention | The run inputs | The user takes the link down while watching. |

Both are events of class 0. A run with interventions can be saved as a new scenario in which they are scheduled changes.

## 8. Observation

**Rule 39.** Observation does not change the logical world. A run that is observed and a run that is not have the same digest.

This rule has three parts.

| Part | Statement |
|---|---|
| a | Telemetry from a guest is an effect, made during an activation. |
| b | The Host does not call into a live guest for inspection. Inspection that needs guest code to run uses a copy restored from a checkpoint. |
| c | The display path may drop or reduce telemetry. The trace is unaffected. |

## 9. Run inputs and run identity

**Rule 40.** These are the run inputs. Together they determine the trace.

| # | Input |
|---|---|
| 1 | The scenario, identified by the hash of its canonical form. It includes the tick length, the tie policy, the limits, and the scheduled changes. |
| 2 | Every guest module, identified by the hash of its bytes. |
| 3 | The version and parameters of module admission. |
| 4 | The versions of the contracts. |
| 5 | The root seed. |
| 6 | The interventions, in order. |
| 7 | The names of the generator, the derivation, and the digest. |

**Rule 41.** The run identity is the run inputs together with the environment: florn version, Host build, engine and version, and CPU architecture. The environment decides which tier applies when two runs are compared.

**Rule 42.** Data from the wall clock and identifiers of the run record are metadata. They are excluded from every comparison.

**Rule 43.** An identifier inside the logical world is derived from an origin and a sequence number. It is never random and never taken from a clock.

## 10. Trace and digest

**Rule 44.** The trace consists of records in the order of handling. A record states what was handled, at which instant and with which key, and what was committed.

| Kind of record | Content |
|---|---|
| Handled event | Key, target, committed effects with their sequence numbers |
| Fault | Key, actor, kind of fault |
| Intervention | Key, content |

**Rule 45.** A record has one canonical binary encoding. One value has one encoding. The encoding is defined together with the record formats.

**Rule 46.** The record of a handled event states the work that the activation consumed.

**Rule 47.** A floating-point value in a record is canonical: every NaN is replaced by one fixed NaN.

**Rule 48.** The digest is a chain. Each link covers the previous link and the next record. A run stores the digest at fixed intervals of records and at its end.

## 11. Replay, checkpoint, and branch

**Rule 49.** Replay is execution of the run again, from the run inputs. The stored trace is never an input.

**Rule 50.** A replay is faithful when its digest equals the stored digest at every stored point. A difference is reported together with the interval in which it first appears.

**Rule 51.** A checkpoint is taken at the boundary between two instants. It contains the state of the kernel, of the domain, and of every actor.

**Rule 52.** Execution that continues from a checkpoint produces the same trace as execution that was never interrupted.

**Rule 53.** Moving backward restores the nearest earlier checkpoint, or the start, and executes forward.

**Rule 54.** A branch is a new run. Its inputs are those of its parent up to the instant of branching, followed by its own. Its identity names the parent and that instant.

**Rule 55.** Detail for a window of time is produced by replay with recording switched on for that window. The full trace need not be stored.

### 11.1 Phasing

| Release | Implements |
|---|---|
| 0.1.0 | Rules 49, 50, 55. Moving backward by execution from the start. |
| Later | Rules 51 to 54. |

Nothing in 0.1.0 may prevent the later rules. Rule 52 is the reason that guest state must be exportable from the first release, even though no checkpoint is taken yet.

## 12. Numbers

**Rule 56.** The kernel uses integers only.

**Rule 57.** Money and quantities cross a contract as integers in declared units.

**Rule 58.** Physical values cross a contract as 64-bit floating-point numbers in SI units.

**Rule 59.** A floating-point value that crosses a contract is made canonical as in rule 47.

**Rule 60.** Host code uses one implementation of the mathematical functions on every target. It does not call the math library of the platform.

**Rule 61.** A discrete domain uses no floating point in any Host computation that influences the trace. A probability is an integer fraction.

**Rule 62.** A continuous domain states, in its RFC, the tolerance and the method by which two results are compared under Tier 2.

**Rule 63.** A floating-point value in a scenario is read with correct rounding and written in the shortest form that reads back as the same value. Formatting for display happens outside the logical world.

## 13. Conditions that depend on the machine

**Rule 64.** A condition that depends on the machine is never visible to a guest, and never changes the trace without notice.

**Rule 65.** When such a condition occurs, the run ends with a Host fault, and the run record states that the fault is not reproducible. Two such conditions are known: the machine cannot provide memory that the limits allow, and the stack of the engine itself is exhausted.

**Rule 66.** The limits on execution, on stack depth, and on memory are counted by florn, in the same way under every engine. The mechanism is the subject of RFC 003.

### 13.1 Sources of non-determinism and their treatment

Every source is removed, made virtual, recorded, or turned into a Host fault. None is tolerated without notice.

| Source | Treatment | Rule |
|---|---|---|
| Wall clock | Removed. Guests and kernel logic cannot read it. | 9 |
| Arrival order of events | Removed. | 19 |
| Threads and shared memory in a guest | Removed at admission. | RFC 003 |
| Instructions with results that differ by platform | Removed at admission. | RFC 003 |
| Imports outside the contract | Removed at admission. | RFC 003 |
| Randomness | Virtual. Seeded streams. | 27 to 32 |
| Actions of a user | Recorded. | 33 to 38 |
| Observation | Without effect. | 39 |
| Identifiers | Derived. | 43 |
| NaN bit patterns | Canonical at the contract and in the trace. Inside a guest: open question 1. | 47, 59 |
| Mathematical functions in the Host | One implementation. | 60 |
| Vector width and order of summation in the Host | Discrete domains: not used. Continuous domains: within tolerance under Tier 2. | 61, 62 |
| Parallel execution in the Host | Not used. Permitted later only if the trace is identical. | 19 |
| Iteration over unordered collections in the Host | Forbidden where the order can influence the trace. | — |
| Execution budget, stack depth, memory limit | Counted by florn. | 66 |
| Memory the machine cannot provide; stack of the engine | Host fault. | 65 |
| Language, region, and time zone of the machine | Removed. Formatting happens in the user interface. | 63 |
| Version of engine or compiler | Recorded in the run identity. | 41 |

## 14. Verification

Each property has a test in the conformance kit. The tests run under both Hosts.

| Property | Test |
|---|---|
| Tier 1 | One scenario, run twice under one Host: equal digests. |
| Tier 2, discrete | One scenario under the browser Host and the native Host, on two CPU architectures: equal digests. |
| Order | Two origins produce events for the same instant. The order follows the tie policy and does not change when the origins are created in the opposite order. |
| Fairness of `by-seed` | Over 10,000 equal-time races between two origins, each wins between 45% and 55%. |
| Zero delay | An actor that answers itself without delay ends the run with the fault of rule 13. |
| Activation | An actor sends, then traps. The trace contains the fault and no message. |
| Streams | A third actor is added. The values drawn by the first two do not change. |
| Interventions | A run with an intervention is replayed: equal digests. |
| Observation | One run with every inspector open, one with none: equal digests. |
| Machine conditions | A guest that asks for memory within its limit never sees a refusal. |
| Checkpoint, when implemented | Interrupted and uninterrupted execution: equal digests. |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Time as floating-point seconds | Equality and order of times become inexact. Accumulated steps drift. |
| A fixed tick of 1 ns | Circuit transients need steps below 1 ns. |
| Order of insertion as the tie-break | It depends on the order of arrival. It does not survive distribution over threads. |
| A single fixed tie policy | A fixed order biases a market. A seeded order is hard to follow when learning a protocol. |
| Effects applied at once | A fault leaves partial effects. The Host is changed while a guest runs. |
| Replay by applying stored events | It verifies nothing about the guests. It remains useful for display and is named playback. |
| Limits by wall-clock timeout | Whether a fault occurs would depend on the speed of the machine. |
| Bit-identical results across Hosts for every domain | Host numerics would give up the vector width and the threads of the native machine. |
| Random floating-point values from the Host | The conversion is a place where libraries differ between versions. |

## Prior art

| Topic | Precedent |
|---|---|
| Integer time with a resolution chosen per model | Established discrete-event simulators for hardware and networks |
| Microsteps | Delta cycles of hardware description languages; superdense time in the theory of hybrid systems |
| Sources of non-determinism in WebAssembly | The guidance on deterministic execution published by the Wasmtime project |

## Consequences for other RFCs

| RFC | Must provide |
|---|---|
| 002 — Guest contract | Time in ticks. Effects as Host calls that are recorded. A randomness interface that returns unsigned 64-bit integers. Timers with cancel. No periodic tick. |
| 003 — Module admission | Counting of execution, stack, and memory that is equal under every engine. Rejection of threads, shared memory, and platform-dependent instructions. Export of guest state. |
| 004 — Core model | Stable identities for actors, connections, and components. Tick length, tie policy, and limits in the scenario. A canonical form of the scenario. |
| 005 — Host architecture | One kernel for both Hosts. Interventions as recorded commands of the control interface. Inspection without effect. |
| Domain RFCs | Class of the domain. Assignment of event classes. Default tie policy. Policy for events addressed to a faulted actor. For a continuous domain: tolerance and method of comparison. |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | Are NaN values made canonical inside a guest, at the cost of speed, or only at the contract and in the trace? | Measurement in the proof of concept on limits |
| 2 | Which generator, which derivation, which digest? Proposal: a ChaCha generator, keys derived with BLAKE3, digest with BLAKE3. | This RFC, before acceptance |
| 3 | ~~The default limits of rule 13.~~ The microstep and event limits stand at 1,000,000 per tick and 10,000,000 per instant (RFC 004 §3.4); the actor limits are RFC 003 rule 19, measured 2026-10-20. | — |
| 4 | ~~The interval of rule 48.~~ Resolved 2026-10-20: 1,000 records (RFC 005 rule 23). | — |
| 5 | Is the consumed work of rule 46 part of what is compared under Tier 2? It is equal under every engine if rule 66 holds. | After the proof of concept on limits |
