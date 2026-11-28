# florn-guest-cps

The guest SDK for the composed world of RFC 012 rule 22 (`florn:cps@0.1.0`, world `networked-controller`): the core world of `florn-guest` with the network's `link-info` import and `on-link` export and the circuit's `read-input` and `set-output` imports and `on-sample` export. A controller that is on a network and acts on a plant is one actor of one world: it implements `florn_guest::Actor` (its `on_message` takes the frames of its `network/interface` ports) and `florn_guest_cps::NetworkedController` (`on_link` and `on_sample`), declares itself with `florn_guest::declare!` under `world: "florn:cps/networked-controller@0.1.0"`, and exports with `florn_guest_cps::networked_controller!`. The scenario declares both domains (`domains = ["core", "network", "circuit"]`); each validates the ports of its own types. The example is the ladder's `networked_pid` (RFC 012 rule 26): `pid` whose set point arrives as a frame.

```toml
[dependencies]
florn-guest = "0.3.1"
florn-guest-cps = "0.3.1"
```
