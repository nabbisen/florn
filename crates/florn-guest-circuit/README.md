# florn-guest-circuit

The guest SDK for the controller world of RFC 012 (`florn:circuit@0.1.0`): the core world of `florn-guest` with the circuit's `read-input` and `set-output` imports and the `on-sample` export. A sampled controller implements `florn_guest::Actor` and `florn_guest_circuit::Controller`, declares itself with `florn_guest::declare!` under `world: "florn:circuit/controller@0.1.0"`, a `[circuit]` table that names its `period` parameter, and exports with `florn_guest_circuit::controller!`. It reads the signals its `signal-in` ports are on with `read_input(port)`, as they were when the activation began, and sets the signals its `signal-out` ports are on with `set_output(port, value)`, which is recorded and applied at commit and held until the next. The example is the ladder's `pid` (RFC 012 rule 26).

```toml
[dependencies]
florn-guest = "0.3.1"
florn-guest-circuit = "0.3.1"
```
