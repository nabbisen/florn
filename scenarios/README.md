# Scenarios

Example scenarios of the conformance kit with recorded digests (requirement AUTO-003; RFC 005 rule 42). Each file is a scenario manifest of RFC 004 over the SDK's example guests, whose built modules are shipped in `modules/`; `digests.toml` records, for every scenario, the seed it was run with, the trace chain, the record count and how the run ended, together with the hash of every shipped module and of its admitted form.

The modules are shipped as bytes, not only as source, because the same source built with the same toolchain gives different bytes on an AArch64 host than on an x86-64 host (every module, measured in CI), and one digest across machines needs one set of bytes. The kit's `figures` command reports how the machine's own build compares with the shipped modules, in bytes and in the digests its runs produce.

The modules, the files and the digests are written by the kit, which builds the guests first:

```sh
cargo run --release -p florn-conformance -- scenarios
```

Run one with the headless runner:

```sh
cargo run --release -p florn-cli -- run scenarios/echo-pair.toml --modules scenarios/modules --seed 42
```

The `chain` line printed must equal the scenario's `chain` in `digests.toml`. The whole set, as a loop (checklist C9):

```sh
for f in scenarios/*.toml; do
  n=$(basename "$f" .toml); [ "$n" = digests ] && continue
  seed=$(sed -n "/^\[runs.\"$n\"\]/,/^$/p" scenarios/digests.toml | sed -n 's/^seed = //p')
  want=$(sed -n "/^\[runs.\"$n\"\]/,/^$/p" scenarios/digests.toml | sed -n 's/^chain = "\(.*\)"/\1/p')
  got=$(cargo run --release -q -p florn-cli -- run "$f" --modules scenarios/modules --seed "$seed" --out /tmp/"$n".florn-run 2>/dev/null | sed -n 's/^chain = //p')
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
