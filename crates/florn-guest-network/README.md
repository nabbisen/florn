# florn-guest-network

The guest SDK for the router world of RFC 008 (`florn:network@0.1.0`): the core world of `florn-guest` with the network's `link-info` import and `on-link` export. A router implements `florn_guest::Actor` and `florn_guest_network::Router`, declares itself with `florn_guest::declare!` under `world: "florn:network/router@0.1.0"`, and exports with `florn_guest_network::router!`. The examples are the ladder of RFC 008 rule 20 and the subset of rule 21: `net_echo` echoes every frame; `distance_vector` advertises its routes with split horizon and counts to infinity; `ospf` runs the declared OSPF subset, one area with hello and dead intervals, point-to-point adjacencies, router-LSAs flooded with acknowledgements, Dijkstra and a routing table, and refuses what lies outside the subset with a logged reason rather than approximating it.

```toml
[dependencies]
florn-guest = "0.2.0"
florn-guest-network = "0.2.0"
```

The module author's chapter of the documentation has the network section.
