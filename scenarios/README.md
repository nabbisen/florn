# Scenarios

Example scenarios of the conformance kit with recorded digests (requirement AUTO-003; RFC 005 rule 42). Each file is a scenario manifest of RFC 004 over the SDK's example guests; `digests.toml` records, for every scenario, the seed it was run with, the trace chain, the record count and how the run ended, together with the hash of every guest and of its admitted form.

The files and the digests are written by the kit:

```sh
cargo run --release -p florn-conformance -- scenarios
```

Run one with the headless runner, after building the guests:

```sh
cargo build --release --target wasm32-unknown-unknown -p florn-guest --examples
cargo run --release -p florn-cli -- run scenarios/echo-pair.toml \
    --modules target/wasm32-unknown-unknown/release/examples --seed 42
```

The `chain` line printed must equal the scenario's `chain` in `digests.toml`. The whole set, as a loop (checklist C9):

```sh
for f in scenarios/*.toml; do
  n=$(basename "$f" .toml); [ "$n" = digests ] && continue
  seed=$(sed -n "/^\[runs.\"$n\"\]/,/^$/p" scenarios/digests.toml | sed -n 's/^seed = //p')
  want=$(sed -n "/^\[runs.\"$n\"\]/,/^$/p" scenarios/digests.toml | sed -n 's/^chain = "\(.*\)"/\1/p')
  got=$(cargo run --release -q -p florn-cli -- run "$f" --modules target/wasm32-unknown-unknown/release/examples --seed "$seed" --out /tmp/"$n".florn-run 2>/dev/null | sed -n 's/^chain = //p')
  [ "$got" = "$want" ] && echo "$n: matched" || echo "$n: DIFFERS ($got)"
done
```

The `zero-delay` scenario ends with the run fault of RFC 001 rule 13 by design; the runner exits with 0 and prints `ended = host-fault`, and its digest is recorded like the others.

| Scenario | What it shows |
|---|---|
| `echo-pair` | AC-001: a pinger and an echo, 10,000 ticks |
| `race-by-identity`, `race-by-seed`, and their `-reversed` forms | AC-004: two origins race 10,000 times; the declaration order changes nothing |
| `neighbour` | The echo alone: the reference the hostile scenarios' neighbour is compared with |
| `hostile-loop`, `hostile-recurse`, `hostile-grow`, `hostile-flood`, `hostile-bad-pointer` | AC-002: one fault each; the neighbour continues |
| `zero-delay` | A chain without delay ends the run with the fault of RFC 001 rule 13 |
| `heap` | AC-006: the export and restore reference, 10,000 activations |
| `bench-protocol`, `bench-compute` | The kernel-path benchmarks of RFC 005 section 11 |
