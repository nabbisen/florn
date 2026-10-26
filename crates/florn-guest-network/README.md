# florn-guest-network

The guest SDK for the router world of RFC 008 (`florn:network@0.1.0`): the core world of `florn-guest` with the network's `link-info` import and `on-link` export. A router implements `florn_guest::Actor` and `florn_guest_network::Router`, declares itself with `florn_guest::declare!` under `world: "florn:network/router@0.1.0"`, and exports with `florn_guest_network::router!`. The example `net_echo` is the first rung of the ladder.

```toml
[dependencies]
florn-guest = "0.1.0"
florn-guest-network = "0.1.0"
```

The module author's chapter of the documentation has the network section.
