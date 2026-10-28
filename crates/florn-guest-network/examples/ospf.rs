//! The OSPF subset of RFC 008 rule 21 (NET-004): a single area; `hello` and `dead` intervals;
//! a two-state neighbour machine (down, full) over a point-to-point adjacency, with no
//! designated-router election; a link-state database of router-LSAs only, flooded with
//! sequence numbers and acknowledgements; Dijkstra over the database; a routing table of
//! prefixes (router identities) to next hops (interfaces). It reports `neighbour` and
//! `route` in the layouts the network view reads (rule 17).
//!
//! Everything outside the subset is refused with a logged reason and never approximated: a
//! router in an area other than 0 stays idle; a second router heard on one interface (a
//! multi-access network, which would need an election) is ignored; a neighbour whose hello
//! and dead intervals differ from this router's is not adjacent.
//!
//! The packets (protocol `ospf`, version 1), little-endian: `1`, the type (1 hello, 2 update,
//! 3 acknowledgement), the sender's id (u64), the area (u64); then for a hello the hello and
//! dead intervals (u32 each) and the ids heard on the interface (u16 count, u64 each); for an
//! update the LSAs (u16 count; each: origin u64, sequence u64, u16 link count, (neighbour
//! u64, cost u16) pairs); for an acknowledgement the (origin u64, sequence u64) pairs it
//! acknowledges (u16 count).
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;
use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host, now};
use florn_guest_network::{Router, link_info};

florn_guest::declare! {
    world: "florn:network/router@0.1.0",
    kind: "ospf",
    description: "The OSPF subset of RFC 008 rule 21: one area, hello and dead intervals, point-to-point adjacencies, router-LSAs flooded with acknowledgements, Dijkstra, a routing table",
    ports: [("eth", "network/interface", 1, 8)],
    params: [
        (router_id, "router-id", u64, false, default = 0),
        (hello, "hello", ticks, false, default = "10s"),
        (dead, "dead", ticks, false, default = "40s"),
        (area, "area", u64, false, default = 0),
    ],
    telemetry: [
        (1, "neighbour", Neighbour, [(port, "port", u64), (neighbour, "neighbour", u64), (state, "state", text)]),
        (2, "route", RouteReport, [(prefix, "prefix", text), (next_hop, "next-hop", u64), (metric, "metric", u64), (action, "action", text)]),
    ],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
    tables: [("network", [("protocol", "ospf")])],
}

const VERSION: u8 = 1;
const HELLO: u8 = 1;
const UPDATE: u8 = 2;
const ACK: u8 = 3;
/// The header: version, type, sender, area.
const HEADER: usize = 18;

struct Adjacency {
    id: u64,
    full: bool,
    last_heard: u64,
    /// The LSAs sent and not yet acknowledged, by origin: the sequence sent.
    unacked: BTreeMap<u64, u64>,
}

#[derive(Clone)]
struct Lsa {
    seq: u64,
    links: Vec<(u64, u16)>,
}

struct Ospf {
    id: u64,
    hello: u64,
    dead: u64,
    area: u64,
    ports: Vec<PortId>,
    /// Point to point: one neighbour per interface.
    neighbours: BTreeMap<PortId, Adjacency>,
    /// Interfaces on which a second router was heard and refused, once each.
    refused_ports: BTreeSet<PortId>,
    lsdb: BTreeMap<u64, Lsa>,
    /// (prefix, next hop interface, metric)
    routes: BTreeMap<u64, (PortId, u64)>,
}

fn u64_at(p: &[u8], at: usize) -> Option<u64> {
    p.get(at..at + 8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
}
fn u32_at(p: &[u8], at: usize) -> Option<u32> {
    p.get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap_or([0; 4])))
}
fn u16_at(p: &[u8], at: usize) -> Option<u16> {
    p.get(at..at + 2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap_or([0; 2])))
}

impl Ospf {
    fn header(&self, kind: u8) -> Vec<u8> {
        let mut p = Vec::with_capacity(64);
        p.push(VERSION);
        p.push(kind);
        p.extend_from_slice(&self.id.to_le_bytes());
        p.extend_from_slice(&self.area.to_le_bytes());
        p
    }

    fn send_hello(&self, port: PortId) {
        let mut p = self.header(HELLO);
        p.extend_from_slice(&(self.hello.min(u64::from(u32::MAX)) as u32).to_le_bytes());
        p.extend_from_slice(&(self.dead.min(u64::from(u32::MAX)) as u32).to_le_bytes());
        let heard: Vec<u64> = self
            .neighbours
            .get(&port)
            .map(|n| n.id)
            .into_iter()
            .collect();
        p.extend_from_slice(&(heard.len() as u16).to_le_bytes());
        for id in heard {
            p.extend_from_slice(&id.to_le_bytes());
        }
        host::send(port, &p);
    }

    /// An update with these LSAs to the neighbour on `port`, remembered until acknowledged.
    fn send_update(&mut self, port: PortId, origins: &[u64]) {
        let mut p = self.header(UPDATE);
        let lsas: Vec<(u64, Lsa)> = origins
            .iter()
            .filter_map(|o| self.lsdb.get(o).map(|l| (*o, l.clone())))
            .collect();
        if lsas.is_empty() {
            return;
        }
        p.extend_from_slice(&(lsas.len() as u16).to_le_bytes());
        for (origin, lsa) in &lsas {
            p.extend_from_slice(&origin.to_le_bytes());
            p.extend_from_slice(&lsa.seq.to_le_bytes());
            p.extend_from_slice(&(lsa.links.len() as u16).to_le_bytes());
            for (n, c) in &lsa.links {
                p.extend_from_slice(&n.to_le_bytes());
                p.extend_from_slice(&c.to_le_bytes());
            }
        }
        if let Some(n) = self.neighbours.get_mut(&port) {
            for (origin, lsa) in &lsas {
                n.unacked.insert(*origin, lsa.seq);
            }
        }
        host::send(port, &p);
    }

    fn send_ack(&self, port: PortId, acks: &[(u64, u64)]) {
        let mut p = self.header(ACK);
        p.extend_from_slice(&(acks.len() as u16).to_le_bytes());
        for (origin, seq) in acks {
            p.extend_from_slice(&origin.to_le_bytes());
            p.extend_from_slice(&seq.to_le_bytes());
        }
        host::send(port, &p);
    }

    /// Flood LSAs to every full neighbour but the one on `except`.
    fn flood(&mut self, origins: &[u64], except: Option<PortId>) {
        let targets: Vec<PortId> = self
            .neighbours
            .iter()
            .filter(|(p, n)| n.full && Some(**p) != except)
            .map(|(p, _)| *p)
            .collect();
        for p in targets {
            self.send_update(p, origins);
        }
    }

    /// This router's LSA from its full adjacencies, a new sequence, flooded.
    fn originate(&mut self) {
        let links: Vec<(u64, u16)> = self
            .neighbours
            .iter()
            .filter(|(_, n)| n.full)
            .map(|(p, n)| {
                (
                    n.id,
                    link_info(*p).cost.max(1).min(u32::from(u16::MAX)) as u16,
                )
            })
            .collect();
        let seq = self.lsdb.get(&self.id).map_or(0, |l| l.seq) + 1;
        self.lsdb.insert(self.id, Lsa { seq, links });
        let me = self.id;
        self.flood(&[me], None);
        self.recompute();
    }

    fn report_neighbour(&self, port: PortId, id: u64, full: bool) {
        Neighbour {
            port: u64::from(port),
            neighbour: id,
            state: if full { "full" } else { "down" }.to_string(),
        }
        .report();
    }

    /// Dijkstra over the database: an edge counts when both ends list each other (the
    /// adjacency is full both ways), with the cost the origin states.
    fn recompute(&mut self) {
        let mut dist: BTreeMap<u64, (u64, Option<PortId>)> = BTreeMap::new();
        let mut done: BTreeSet<u64> = BTreeSet::new();
        dist.insert(self.id, (0, None));
        loop {
            let Some((&u, &(du, via))) = dist
                .iter()
                .filter(|(n, _)| !done.contains(n))
                .min_by_key(|(n, (d, _))| (*d, **n))
            else {
                break;
            };
            done.insert(u);
            let Some(lsa) = self.lsdb.get(&u) else {
                continue;
            };
            for (v, c) in &lsa.links {
                let back = self
                    .lsdb
                    .get(v)
                    .is_some_and(|l| l.links.iter().any(|(x, _)| *x == u));
                if !back {
                    continue;
                }
                let nd = du.saturating_add(u64::from(*c));
                let first_hop = if u == self.id {
                    self.neighbours
                        .iter()
                        .find(|(_, n)| n.full && n.id == *v)
                        .map(|(p, _)| *p)
                } else {
                    via
                };
                let better = dist.get(v).is_none_or(|(d, _)| nd < *d);
                if better && first_hop.is_some() {
                    dist.insert(*v, (nd, first_hop));
                }
            }
        }
        let mut next: BTreeMap<u64, (PortId, u64)> = BTreeMap::new();
        for (n, (d, via)) in &dist {
            if let (true, Some(p)) = (*n != self.id, via) {
                next.insert(*n, (*p, *d));
            }
        }
        for (prefix, (port, metric)) in &next {
            match self.routes.get(prefix) {
                Some(old) if old == &(*port, *metric) => {}
                Some(_) => self.report_route(*prefix, Some(*port), *metric, "replace"),
                None => self.report_route(*prefix, Some(*port), *metric, "add"),
            }
        }
        for (prefix, (port, metric)) in &self.routes {
            if !next.contains_key(prefix) {
                self.report_route(*prefix, Some(*port), *metric, "withdraw");
            }
        }
        self.routes = next;
    }

    fn report_route(&self, prefix: u64, port: Option<PortId>, metric: u64, action: &str) {
        RouteReport {
            prefix: prefix.to_string(),
            next_hop: port.map_or(0, u64::from),
            metric,
            action: action.to_string(),
        }
        .report();
    }

    fn bring_down(&mut self, port: PortId) {
        let Some(n) = self.neighbours.get_mut(&port) else {
            return;
        };
        let was_full = n.full;
        let id = n.id;
        n.full = false;
        n.unacked.clear();
        if was_full {
            self.report_neighbour(port, id, false);
            self.originate();
        }
    }

    fn on_hello(&mut self, port: PortId, sender: u64, payload: &[u8]) {
        let (Some(hello), Some(dead), Some(count)) = (
            u32_at(payload, HEADER),
            u32_at(payload, HEADER + 4),
            u16_at(payload, HEADER + 8),
        ) else {
            return;
        };
        if u64::from(hello) != self.hello || u64::from(dead) != self.dead {
            host::log(&format!(
                "router {sender} on interface {port} uses hello {hello} and dead {dead}, not {} and {}: no adjacency",
                self.hello, self.dead
            ));
            return;
        }
        if let Some(n) = self.neighbours.get(&port)
            && n.id != sender
        {
            if self.refused_ports.insert(port) {
                host::log(&format!(
                    "a second router {sender} on interface {port}: a multi-access network needs designated-router election, outside the subset; ignored"
                ));
            }
            return;
        }
        let t = now();
        let heard_me = (0..usize::from(count))
            .filter_map(|i| u64_at(payload, HEADER + 10 + i * 8))
            .any(|id| id == self.id);
        let n = self.neighbours.entry(port).or_insert(Adjacency {
            id: sender,
            full: false,
            last_heard: t,
            unacked: BTreeMap::new(),
        });
        n.last_heard = t;
        if heard_me && !n.full {
            n.full = true;
            self.report_neighbour(port, sender, true);
            // The adjacency is full: the whole database goes to the neighbour, and this
            // router's LSA gains the link.
            let all: Vec<u64> = self.lsdb.keys().copied().collect();
            self.send_update(port, &all);
            self.originate();
            self.send_hello(port);
        } else if !heard_me {
            // Tell the neighbour it is heard, so that it comes up too.
            self.send_hello(port);
        }
    }

    fn on_update(&mut self, port: PortId, payload: &[u8]) {
        let Some(count) = u16_at(payload, HEADER) else {
            return;
        };
        let mut at = HEADER + 2;
        let mut acks = Vec::new();
        let mut installed = Vec::new();
        for _ in 0..count {
            let (Some(origin), Some(seq), Some(nlinks)) = (
                u64_at(payload, at),
                u64_at(payload, at + 8),
                u16_at(payload, at + 16),
            ) else {
                return;
            };
            at += 18;
            let mut links = Vec::with_capacity(usize::from(nlinks));
            for _ in 0..nlinks {
                let (Some(n), Some(c)) = (u64_at(payload, at), u16_at(payload, at + 8)) else {
                    return;
                };
                links.push((n, c));
                at += 10;
            }
            acks.push((origin, seq));
            if origin == self.id {
                continue;
            }
            let newer = self.lsdb.get(&origin).is_none_or(|l| seq > l.seq);
            if newer {
                self.lsdb.insert(origin, Lsa { seq, links });
                installed.push(origin);
            }
        }
        if !self.neighbours.get(&port).is_some_and(|n| n.full) {
            return;
        }
        self.send_ack(port, &acks);
        if !installed.is_empty() {
            self.flood(&installed, Some(port));
            self.recompute();
        }
    }

    fn on_ack(&mut self, port: PortId, payload: &[u8]) {
        let Some(count) = u16_at(payload, HEADER) else {
            return;
        };
        let Some(n) = self.neighbours.get_mut(&port) else {
            return;
        };
        for i in 0..usize::from(count) {
            let at = HEADER + 2 + i * 16;
            if let (Some(origin), Some(seq)) = (u64_at(payload, at), u64_at(payload, at + 8))
                && n.unacked.get(&origin) == Some(&seq)
            {
                n.unacked.remove(&origin);
            }
        }
    }
}

impl Actor for Ospf {
    fn init(actor: ActorId, ports: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        let id = if c.router_id == 0 {
            u64::from(actor)
        } else {
            c.router_id
        };
        let mut me = Self {
            id,
            hello: c.hello.max(1),
            dead: c.dead.max(1),
            area: c.area,
            ports: ports.iter().map(|p| p.id).collect(),
            neighbours: BTreeMap::new(),
            refused_ports: BTreeSet::new(),
            lsdb: BTreeMap::new(),
            routes: BTreeMap::new(),
        };
        if me.area != 0 {
            host::log(&format!(
                "area {} is outside the subset (a single area, 0); this router stays idle",
                me.area
            ));
        } else {
            me.lsdb.insert(
                id,
                Lsa {
                    seq: 1,
                    links: Vec::new(),
                },
            );
            for p in me.ports.clone() {
                me.send_hello(p);
            }
        }
        host::set_timer(0, me.hello);
        me
    }

    fn on_message(&mut self, port: PortId, payload: &[u8]) {
        if self.area != 0 || payload.len() < HEADER || payload[0] != VERSION {
            return;
        }
        let (Some(sender), Some(area)) = (u64_at(payload, 2), u64_at(payload, 10)) else {
            return;
        };
        if area != self.area {
            host::log(&format!(
                "router {sender} on interface {port} is in area {area}, outside the subset (area 0); ignored"
            ));
            return;
        }
        match payload[1] {
            HELLO => self.on_hello(port, sender, payload),
            UPDATE => self.on_update(port, payload),
            ACK => self.on_ack(port, payload),
            other => host::log(&format!(
                "packet type {other} from router {sender} is outside the subset; ignored"
            )),
        }
    }

    fn on_timer(&mut self, _: TimerId) {
        if self.area == 0 {
            let t = now();
            let dead: Vec<PortId> = self
                .neighbours
                .iter()
                .filter(|(_, n)| n.full && t.saturating_sub(n.last_heard) >= self.dead)
                .map(|(p, _)| *p)
                .collect();
            for p in dead {
                self.bring_down(p);
            }
            for p in self.ports.clone() {
                self.send_hello(p);
            }
            // Retransmit what a full neighbour has not acknowledged.
            let pending: Vec<(PortId, Vec<u64>)> = self
                .neighbours
                .iter()
                .filter(|(_, n)| n.full && !n.unacked.is_empty())
                .map(|(p, n)| (*p, n.unacked.keys().copied().collect()))
                .collect();
            for (p, origins) in pending {
                self.send_update(p, &origins);
            }
        }
        host::set_timer(0, self.hello);
    }
}

impl Router for Ospf {
    fn on_link(&mut self, port: PortId, up: bool) {
        if self.area != 0 {
            return;
        }
        if up {
            self.send_hello(port);
        } else {
            self.bring_down(port);
        }
    }
}

florn_guest_network::router!(Ospf);
