# Scenarios

Example scenarios of the conformance kit with recorded digests (requirement AUTO-003; RFC 005 rule 42). Each file is a scenario manifest of RFC 004 over the SDK's example guests, whose built modules are shipped in `modules/`; `digests.toml` records, for every scenario, the seed it was run with, the trace chain, the record count and how the run ended, together with the hash of every shipped module and of its admitted form.

The modules are shipped as bytes, not only as source, because the same source built with the same toolchain gives different bytes on an AArch64 host than on an x86-64 host (every module, measured in CI), and one digest across machines needs one set of bytes. The kit's `figures` command reports how the machine's own build compares with the shipped modules, in bytes and in the digests its runs produce.

The modules, the files and the digests are written by the kit, which builds the guests first:

```sh
cargo run --release -p florn-conformance -- scenarios
```

Run one with the headless runner:

```sh
cargo run --release -p florn -- run scenarios/echo-pair.toml --modules scenarios/modules --seed 42
```

The `chain` line printed must equal the scenario's `chain` in `digests.toml`. The whole set, as a loop (checklist C9):

```sh
for f in scenarios/*.toml; do
  n=$(basename "$f" .toml); [ "$n" = digests ] && continue
  seed=$(sed -n "/^\[runs.\"$n\"\]/,/^$/p" scenarios/digests.toml | sed -n 's/^seed = //p')
  want=$(sed -n "/^\[runs.\"$n\"\]/,/^$/p" scenarios/digests.toml | sed -n 's/^chain = "\(.*\)"/\1/p')
  got=$(cargo run --release -q -p florn -- run "$f" --modules scenarios/modules --seed "$seed" --out /tmp/"$n".florn-run 2>/dev/null | sed -n 's/^chain = //p')
  [ "$got" = "$want" ] && echo "$n: matched" || echo "$n: DIFFERS ($got)"
done
```

The `zero-delay` scenario ends with the run fault of RFC 001 rule 13 by design; the runner prints `ended = host-fault` and exits with 3, and its digest is recorded like the others.

| Scenario | What it shows |
|---|---|
| `echo-pair` | AC-001: a pinger and an echo, 10,000 ticks |
| `race-by-identity`, `race-by-seed`, and their `-reversed` forms | AC-004: two origins race 10,000 times; the declaration order changes nothing |
| `neighbour` | The echo alone: the reference the hostile scenarios' neighbour is compared with |
| `hostile-loop`, `hostile-recurse`, `hostile-grow`, `hostile-flood`, `hostile-bad-pointer` | AC-002: one fault each; the neighbour continues |
| `zero-delay` | A chain without delay ends the run with the fault of RFC 001 rule 13 |
| `heap` | AC-006: the export and restore reference, 10,000 activations |
| `net-delay` | RFC 008 §12: a 1,500-byte frame over a 20 ns link at 1000 Gbit/s arrives 32 ticks after it was sent, and the echo 32 later |
| `net-loss` | RFC 008 §12: 10,000 frames at `loss = 0.1`, drawn from the link's own stream; between 900 and 1,100 are dropped |
| `net-jitter` | RFC 008 §12: every arrival within `[delay, delay + jitter]` |
| `net-link-down` | RFC 008 §12: the link goes down at 300 and up at 600; `on-link` at both ends, frames in flight arrive, sends while down are dropped |
| `net-lan` | RFC 008 §12: a lan of three; a frame reaches the two others, each with its own loss draw |
| `net-blackhole` | RFC 008 §12: the `network/blackhole` built-in drops every frame and reports the count, with `metered = 0` |
| `dv-triangle` | RFC 008 rule 20 (NET-012): three distance-vector routers; the link r1-r2 down at 1000 and up at 2000; the routes converge, reroute through r3 and converge back within three periods |
| `dv-chain` | RFC 008 rule 20: four distance-vector routers in a line with `infinity = 3`; the far ends never install each other |
| `ospf-lab` | RFC 008 rule 21 (NET-004, NET-010), the learning lab: four OSPF routers in a ring with a costly chord r1-r3, hellos every 50 ticks and a dead interval of 200; the link r2-r3 down at 1500 and up at 3000; the adjacencies go full, the LSAs flood, the routes take the short way, reroute after the dead interval and come back |
| `ospf-outside` | RFC 008 rule 21: r2 is in area 1, outside the subset; it logs its refusal and stays idle while r1 keeps saying hello |
| `bgp-lab` | RFC 011 rule 9 (NET-005), the BGP learning lab: four routers in four autonomous systems, r1 with sessions to r2 and r3, both with sessions to r4, which originates 10.4.0.0/16; local preference 200 on r1's port to r2, MED 10 and 20 from r4; hold 90 ticks, keepalive 30; the link r1-r2 down at 1000 and up at 2000 |
| `bgp-outside` | RFC 011 rule 10: two routers of one autonomous system on a link; internal BGP is outside the subset, so both refuse the session with the reason logged and no route forms |
| `motor-loop` | RFC 012 rule 27 (PHY-001, PHY-009), the closed loop of Phase 4's exit criterion: a `pid` controller (period 1 ms, one tick after the motor's sensor) drives a 12 V source into a DC motor; the set point (100 rad/s) and the load torque are `constant`s; a load step of 0.02 Nm at one second, to 1.5 s. The speed settles at the set point by 0.8 s, dips under the step and recovers |
| `cps-loop` | RFC 012 rules 22 and 27, two domains in one actor: `motor-loop` with the controller `networked_pid`, on a network interface as well as on the plant's signals; the set point is its parameter (100 rad/s) until a frame says another, and a frame is a `message` intervention to its interface, which the kit's tests send |
| `rc-step` | RFC 013 rule 21 (AUTO-004), the solver's analytic golden: a 1 kohm resistor and a 1 uF capacitor with `ic = 0V` on a 1 V source stepped to 0 V at 2 ms, a voltmeter across the capacitor every 100 us, to 5 ms, at the default profile; its samples stay within `4·ltol·V` of `V·(1 − exp(−t/RC))` and then of the decay |
| `diode-clip` | RFC 012 rule 27, the nonlinear plant: a 5 V source through 1 kohm into a built-in diode, a voltmeter across the diode every 10 us, the source stepped to −5 V at 0.4 ms, back at 0.8 ms and to −5 V at 1.2 ms (the limiter and the restarts) |
| `guest-diode` | RFC 012 rules 19 to 21 and 27: `diode-clip` with the guest device `guest_diode` in the diode's place; the two voltmeters' samples agree (to the last bit, measured) |
| `hostile-stamp` | RFC 012 rules 17 and 20: a `bad_device` that answers a NaN stamp at its fifth evaluation; a Fault of kind contract (12020) keyed by the advance, `circuit.stamp-invalid` and `circuit.device-faulted`, and the plant goes on without it |
| `hostile-diverge` | RFC 013 rule 17: a `bad_device` whose stamps never converge; the run ends with `circuit.non-convergence` and the end reason `non-convergence` |
| `bench-protocol`, `bench-compute` | The kernel-path benchmarks of RFC 005 section 11 |

## The circuit's results across engines

`results/` holds what the circuit scenarios (`rc-step`, `motor-loop`, `cps-loop`, `diode-clip`, `guest-diode`, `hostile-stamp`, `hostile-diverge`) did on x86-64 (RFC 013 rules 19 to 21; `florn-conformance scenarios` writes it with the rest): `<name>.results`, one line per trace record carrying everything the comparison of Tier 2 reads (the key and target, the effects, each numeric field as the bits of its `f64`, every other payload as its digest, the consumed work), and `bit-identity.table`, the arithmetic of the solver run over a fixed table of inputs, with the bits of each answer. Every other Host and engine is compared to them as a second run: `matched` when every line is the same (what the kit expects on every engine), `within-tolerance` when a number differs within the scenario's `atol` and `rtol`, `diverged` otherwise. `cargo run --release -p florn-conformance -- cross-engine` prints this machine's row; `ci/browser/run.js` prints the browsers'.
