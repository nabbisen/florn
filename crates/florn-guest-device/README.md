# florn-guest-device

The guest SDK for the device world of RFC 012 rule 19 (`florn:circuit@0.1.0`, world `device`): the core world of `florn-guest` with the compound activation the solver plays into the guest inside a step, `begin-step`, `evaluate` once per Newton iteration, then `accept-step` or `reject-step`. A device model implements `florn_guest::Actor` and `florn_guest_device::Device`, declares itself with `florn_guest::declare!` under `world: "florn:circuit/device@0.1.0"` with one port kind of type `circuit/pin` (a diode is `min = 2, max = 2`) and a `[circuit]` table `kind = "device"`, and exports with `florn_guest_device::device!`.

`evaluate` is given the voltages of the pins in ascending port identity and returns `n + n² + 1` values: the currents into the pins, the Jacobian row-major, and `1.0` when the device limited a junction in this evaluation and `0.0` when it did not. `Trial` keeps what the device carries between steps apart from what a step tries, so that `reject_step` is a drop; `pnjlim` is SPICE3's junction limiter with the solver's literals (RFC 013 rule 8), and `math::{exp, ln}` are the solver's own `libm` functions: a guest and a built-in diode compute the same bits. The examples are the ladder's `guest_diode` and the kit's `bad_device`.

```toml
[dependencies]
florn-guest = "0.3.1"
florn-guest-device = "0.3.1"
```
