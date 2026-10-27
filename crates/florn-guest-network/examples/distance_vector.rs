//! The second rung of the ladder (RFC 008 rule 20): a distance-vector router. It advertises its
//! routes to every neighbour every `period` with split horizon, learns neighbours from what it
//! hears, counts to `infinity`, withdraws a route or a neighbour unheard for three periods,
//! sends a triggered update when its table changes, and reports `neighbour` and `route` in the
//! layouts the network view reads (rule 17). Destinations are router identities; the metric of
//! a hop is the interface's cost from `link-info`.
//!
//! The frame (protocol `distance-vector`, version 1): `1`, the sender's id (u64), the entry
//! count (u16), then (destination u64, metric u16) pairs.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec::Vec;
use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host, now};
use florn_guest_network::{Router, link_info};

florn_guest::declare! {
    world: "florn:network/router@0.1.0",
    kind: "distance-vector",
    description: "A distance-vector router: periodic and triggered advertisements with split horizon, count to infinity, expiry after three periods",
    ports: [("eth", "network/interface", 1, 8)],
    params: [
        (router_id, "router-id", u64, false, default = 0),
        (period, "period", ticks, false, default = "30s"),
        (infinity, "infinity", u64, false, default = 16),
    ],
    telemetry: [
        (1, "neighbour", Neighbour, [(port, "port", u64), (neighbour, "neighbour", u64), (state, "state", text)]),
        (2, "route", RouteReport, [(prefix, "prefix", text), (next_hop, "next-hop", u64), (metric, "metric", u64), (action, "action", text)]),
    ],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
    tables: [("network", [("protocol", "distance-vector")])],
}

const VERSION: u8 = 1;

struct NeighbourState {
    last_heard: u64,
    up: bool,
}

struct Route {
    /// The interface the route was learned through; `None` for the router itself.
    port: Option<PortId>,
    metric: u64,
    last_heard: u64,
}

struct DistanceVector {
    id: u64,
    period: u64,
    infinity: u64,
    ports: Vec<PortId>,
    /// By interface and router id: a lan has several neighbours on one interface.
    neighbours: BTreeMap<(PortId, u64), NeighbourState>,
    routes: BTreeMap<u64, Route>,
}

impl DistanceVector {
    fn report_route(&self, dest: u64, port: Option<PortId>, metric: u64, action: &str) {
        RouteReport {
            prefix: dest.to_string(),
            next_hop: port.map_or(0, u64::from),
            metric,
            action: action.to_string(),
        }
        .report();
    }

    /// Advertise to every neighbour with split horizon: a route is not sent back through the
    /// interface it was learned from.
    fn advertise(&self) {
        for p in &self.ports {
            let entries: Vec<(u64, u16)> = self
                .routes
                .iter()
                .filter(|(_, r)| r.port != Some(*p) && r.metric < self.infinity)
                .map(|(d, r)| (*d, r.metric.min(u64::from(u16::MAX)) as u16))
                .collect();
            let mut frame = Vec::with_capacity(11 + entries.len() * 10);
            frame.push(VERSION);
            frame.extend_from_slice(&self.id.to_le_bytes());
            frame.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            for (d, m) in entries {
                frame.extend_from_slice(&d.to_le_bytes());
                frame.extend_from_slice(&m.to_le_bytes());
            }
            host::send(*p, &frame);
        }
    }

    /// Withdraw every route through `port`; returns whether the table changed.
    fn withdraw_port(&mut self, port: PortId) -> bool {
        let gone: Vec<u64> = self
            .routes
            .iter()
            .filter(|(_, r)| r.port == Some(port))
            .map(|(d, _)| *d)
            .collect();
        for d in &gone {
            self.routes.remove(d);
            self.report_route(*d, Some(port), self.infinity, "withdraw");
        }
        !gone.is_empty()
    }

    fn set_neighbour(&mut self, port: PortId, id: u64, up: bool) {
        let t = now();
        let changed = match self.neighbours.get_mut(&(port, id)) {
            Some(n) => {
                let changed = n.up != up;
                n.up = up;
                n.last_heard = t;
                changed
            }
            None => {
                self.neighbours
                    .insert((port, id), NeighbourState { last_heard: t, up });
                true
            }
        };
        if changed {
            Neighbour {
                port: u64::from(port),
                neighbour: id,
                state: if up { "up" } else { "down" }.to_string(),
            }
            .report();
        }
    }

    /// Expire neighbours and routes unheard for three periods.
    fn expire(&mut self) -> bool {
        let t = now();
        let limit = self.period.saturating_mul(3);
        let mut changed = false;
        let stale: Vec<(PortId, u64)> = self
            .neighbours
            .iter()
            .filter(|(_, n)| n.up && t.saturating_sub(n.last_heard) >= limit)
            .map(|(k, _)| *k)
            .collect();
        for (port, id) in stale {
            self.set_neighbour(port, id, false);
        }
        let old: Vec<(u64, Option<PortId>)> = self
            .routes
            .iter()
            .filter(|(_, r)| r.port.is_some() && t.saturating_sub(r.last_heard) >= limit)
            .map(|(d, r)| (*d, r.port))
            .collect();
        for (d, port) in old {
            self.routes.remove(&d);
            self.report_route(d, port, self.infinity, "withdraw");
            changed = true;
        }
        changed
    }
}

impl Actor for DistanceVector {
    fn init(actor: ActorId, ports: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        let id = if c.router_id == 0 {
            u64::from(actor)
        } else {
            c.router_id
        };
        let mut me = Self {
            id,
            period: c.period.max(1),
            infinity: c.infinity.max(1),
            ports: ports.iter().map(|p| p.id).collect(),
            neighbours: BTreeMap::new(),
            routes: BTreeMap::new(),
        };
        me.routes.insert(
            id,
            Route {
                port: None,
                metric: 0,
                last_heard: 0,
            },
        );
        me.report_route(id, None, 0, "add");
        me.advertise();
        host::set_timer(0, me.period);
        me
    }

    fn on_message(&mut self, port: PortId, payload: &[u8]) {
        if payload.first() != Some(&VERSION) || payload.len() < 11 {
            return;
        }
        let sender = u64::from_le_bytes(payload[1..9].try_into().unwrap_or([0; 8]));
        let n = u16::from_le_bytes(payload[9..11].try_into().unwrap_or([0; 2])) as usize;
        if payload.len() != 11 + n * 10 {
            return;
        }
        self.set_neighbour(port, sender, true);
        let cost = u64::from(link_info(port).cost.max(1));
        let t = now();
        let mut changed = false;
        for i in 0..n {
            let at = 11 + i * 10;
            let dest = u64::from_le_bytes(payload[at..at + 8].try_into().unwrap_or([0; 8]));
            let heard = u64::from(u16::from_le_bytes(
                payload[at + 8..at + 10].try_into().unwrap_or([0; 2]),
            ));
            if dest == self.id {
                continue;
            }
            let metric = heard.saturating_add(cost).min(self.infinity);
            match self.routes.get_mut(&dest) {
                Some(r) if r.port == Some(port) => {
                    r.last_heard = t;
                    if r.metric != metric {
                        if metric >= self.infinity {
                            self.routes.remove(&dest);
                            self.report_route(dest, Some(port), self.infinity, "withdraw");
                        } else {
                            r.metric = metric;
                            self.report_route(dest, Some(port), metric, "replace");
                        }
                        changed = true;
                    }
                }
                Some(r) => {
                    if metric < r.metric {
                        r.port = Some(port);
                        r.metric = metric;
                        r.last_heard = t;
                        self.report_route(dest, Some(port), metric, "replace");
                        changed = true;
                    }
                }
                None => {
                    if metric < self.infinity {
                        self.routes.insert(
                            dest,
                            Route {
                                port: Some(port),
                                metric,
                                last_heard: t,
                            },
                        );
                        self.report_route(dest, Some(port), metric, "add");
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.advertise();
        }
    }

    fn on_timer(&mut self, _: TimerId) {
        self.expire();
        self.advertise();
        host::set_timer(0, self.period);
    }
}

impl Router for DistanceVector {
    fn on_link(&mut self, port: PortId, up: bool) {
        if up {
            // A triggered advertisement lets the neighbour learn this router at once.
            self.advertise();
            return;
        }
        let on_port: Vec<u64> = self
            .neighbours
            .keys()
            .filter(|(p, _)| *p == port)
            .map(|(_, id)| *id)
            .collect();
        for id in on_port {
            self.set_neighbour(port, id, false);
        }
        if self.withdraw_port(port) {
            self.advertise();
        }
    }
}

florn_guest_network::router!(DistanceVector);
