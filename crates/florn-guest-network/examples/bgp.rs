//! The BGP subset of RFC 011 (NET-005): external sessions over point-to-point links, the
//! four-state session machine (`idle`, `open-sent`, `open-confirm`, `established`) with a
//! keepalive and a hold timer, per-peer Adj-RIB-In and Adj-RIB-Out, a Loc-RIB decided per
//! prefix by local preference, AS-path length, MED and the advertising peer's router id, and
//! advertisement of the winner to every established peer but the one it came from, with this
//! router's AS prepended. It reports `peer` and `route` in the layouts of RFC 011 rule 8.
//!
//! Everything outside the subset (rule 6) is refused with a logged reason and never
//! approximated: a peer in this router's own autonomous system (internal BGP), another
//! protocol version, another hold time, a second router on an interface, a message type other
//! than the four. An update whose AS path holds this router's AS is dropped as a loop and
//! counted.
//!
//! Parameters: `as` (this router's autonomous system; the actor identity when 0),
//! `router-id` (the actor identity when 0), `hold` and `keepalive` (ticks; the keepalive is a
//! third of the hold when 0), `networks` (the prefixes originated, comma-separated texts
//! carried opaquely), `local-pref` and `med` (a number for every port, or `port=value` pairs
//! separated by commas: the contract knows no per-port parameter beyond the cost, so a port's
//! preference and MED travel here; `local-pref` defaults to 100, `med` to 0).
//!
//! The frames (protocol `bgp`, version 1), little-endian: `1`, the type (1 open, 2 update,
//! 3 keepalive, 4 notification), the sender's AS (u32), the sender's router id (u64); then for
//! an open the hold time (u32); for an update the withdrawn prefixes (u16 count, each a u8
//! length and the bytes) and the advertised prefixes (u16 count; each the prefix, u16 path
//! length, u32 per AS, u32 MED); for a notification the code (u8) and a text (u16 length).
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host, now};
use florn_guest_network::{Router, link_info};

florn_guest::declare! {
    world: "florn:network/router@0.1.0",
    kind: "bgp",
    description: "The BGP subset of RFC 011: external sessions over links, the four-state session machine with keepalive and hold timers, Adj-RIB-In, Loc-RIB and Adj-RIB-Out, the decision by local preference, AS-path length, MED and router id",
    ports: [("eth", "network/interface", 1, 8)],
    params: [
        (autonomous_system, "as", u64, false, default = 0),
        (router_id, "router-id", u64, false, default = 0),
        (hold, "hold", ticks, false, default = "90s"),
        (keepalive, "keepalive", ticks, false, default = "0s"),
        (networks, "networks", text, false, default = ""),
        (local_pref, "local-pref", text, false, default = "100"),
        (med, "med", text, false, default = "0"),
    ],
    telemetry: [
        (1, "peer", Peer, [(port, "port", u64), (state, "state", text), (routes, "routes", u64)]),
        (2, "route", RouteReport, [(prefix, "prefix", text), (next_hop, "next-hop", u64), (path_length, "path-length", u64), (local_pref, "local-pref", u64), (med, "med", u64), (winner, "winner", text)]),
    ],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
    tables: [("network", [("protocol", "bgp")])],
}

const VERSION: u8 = 1;
const OPEN: u8 = 1;
const UPDATE: u8 = 2;
const KEEPALIVE: u8 = 3;
const NOTIFICATION: u8 = 4;
/// The header: version, type, sender AS, router id.
const HEADER: usize = 14;
const BAD_VERSION: u8 = 1;
const OWN_AS: u8 = 2;
const HOLD_TIME: u8 = 3;
const OUTSIDE: u8 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    OpenSent,
    OpenConfirm,
    Established,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::OpenSent => "open-sent",
            Self::OpenConfirm => "open-confirm",
            Self::Established => "established",
        }
    }
}

/// A path as a peer advertised it: the AS path and the MED.
#[derive(Clone, PartialEq, Eq)]
struct Path {
    as_path: Vec<u32>,
    med: u32,
}

struct Session {
    state: State,
    peer_as: u32,
    peer_id: u64,
    last_heard: u64,
    /// What the peer advertised, as received (Adj-RIB-In).
    adj_in: BTreeMap<Vec<u8>, Path>,
    /// What this router advertised to the peer (Adj-RIB-Out).
    adj_out: BTreeMap<Vec<u8>, Path>,
}

/// The Loc-RIB's entry for a prefix: where the winner came from and its attributes.
#[derive(Clone, PartialEq, Eq)]
struct Winner {
    /// The port the path arrived on; `None` for a prefix this router originates.
    port: Option<PortId>,
    path: Path,
    local_pref: u32,
    peer_id: u64,
}

struct Bgp {
    autonomous_system: u32,
    id: u64,
    hold: u64,
    keepalive: u64,
    networks: Vec<Vec<u8>>,
    local_pref: BTreeMap<PortId, u32>,
    med: BTreeMap<PortId, u32>,
    default_local_pref: u32,
    default_med: u32,
    sessions: BTreeMap<PortId, Session>,
    /// Interfaces on which a second router was heard and refused, once each.
    refused_ports: BTreeSet<PortId>,
    loc_rib: BTreeMap<Vec<u8>, Winner>,
    loops: u64,
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

/// A prefix: a length byte and the bytes; the position after it.
fn prefix_at(p: &[u8], at: usize) -> Option<(Vec<u8>, usize)> {
    let n = usize::from(*p.get(at)?);
    let bytes = p.get(at + 1..at + 1 + n)?.to_vec();
    Some((bytes, at + 1 + n))
}

fn put_prefix(out: &mut Vec<u8>, prefix: &[u8]) {
    out.push(prefix.len().min(255) as u8);
    out.extend_from_slice(&prefix[..prefix.len().min(255)]);
}

/// `port=value` pairs separated by commas, or one value for every port.
fn per_port(text: &str, ports: &[PortId]) -> (BTreeMap<PortId, u32>, Option<u32>) {
    let mut map = BTreeMap::new();
    let mut default = None;
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.split_once('=') {
            Some((p, v)) => {
                if let (Ok(p), Ok(v)) = (p.trim().parse::<u32>(), v.trim().parse::<u32>())
                    && ports.contains(&p)
                {
                    map.insert(p, v);
                }
            }
            None => {
                if let Ok(v) = part.parse::<u32>() {
                    default = Some(v);
                }
            }
        }
    }
    (map, default)
}

impl Bgp {
    fn header(&self, kind: u8) -> Vec<u8> {
        let mut p = Vec::with_capacity(64);
        p.push(VERSION);
        p.push(kind);
        p.extend_from_slice(&self.autonomous_system.to_le_bytes());
        p.extend_from_slice(&self.id.to_le_bytes());
        p
    }

    fn local_pref_of(&self, port: PortId) -> u32 {
        *self
            .local_pref
            .get(&port)
            .unwrap_or(&self.default_local_pref)
    }

    fn med_of(&self, port: PortId) -> u32 {
        *self.med.get(&port).unwrap_or(&self.default_med)
    }

    fn send_open(&self, port: PortId) {
        let mut p = self.header(OPEN);
        p.extend_from_slice(&(self.hold.min(u64::from(u32::MAX)) as u32).to_le_bytes());
        host::send(port, &p);
    }

    fn send_keepalive(&self, port: PortId) {
        host::send(port, &self.header(KEEPALIVE));
    }

    fn send_notification(&self, port: PortId, code: u8, text: &str) {
        let mut p = self.header(NOTIFICATION);
        p.push(code);
        let bytes = text.as_bytes();
        let n = bytes.len().min(usize::from(u16::MAX));
        p.extend_from_slice(&(n as u16).to_le_bytes());
        p.extend_from_slice(&bytes[..n]);
        host::send(port, &p);
    }

    /// An update to the peer on `port`: withdrawals, then advertisements with this router's AS
    /// prepended and the MED of this port.
    fn send_update(&self, port: PortId, withdrawn: &[Vec<u8>], advertised: &[(Vec<u8>, Path)]) {
        if withdrawn.is_empty() && advertised.is_empty() {
            return;
        }
        let mut p = self.header(UPDATE);
        p.extend_from_slice(&(withdrawn.len() as u16).to_le_bytes());
        for w in withdrawn {
            put_prefix(&mut p, w);
        }
        p.extend_from_slice(&(advertised.len() as u16).to_le_bytes());
        for (prefix, path) in advertised {
            put_prefix(&mut p, prefix);
            p.extend_from_slice(&(path.as_path.len() as u16).to_le_bytes());
            for a in &path.as_path {
                p.extend_from_slice(&a.to_le_bytes());
            }
            p.extend_from_slice(&path.med.to_le_bytes());
        }
        host::send(port, &p);
    }

    fn report_peer(&self, port: PortId) {
        let s = &self.sessions[&port];
        Peer {
            port: u64::from(port),
            state: s.state.name().to_string(),
            routes: s.adj_in.len() as u64,
        }
        .report();
    }

    fn report_route(&self, prefix: &[u8], w: Option<&Winner>) {
        RouteReport {
            prefix: String::from_utf8_lossy(prefix).into_owned(),
            next_hop: w.and_then(|w| w.port).map_or(0, u64::from),
            path_length: w.map_or(0, |w| w.path.as_path.len() as u64),
            local_pref: w
                .filter(|w| w.port.is_some())
                .map_or(0, |w| u64::from(w.local_pref)),
            med: w.map_or(0, |w| u64::from(w.path.med)),
            winner: match w {
                Some(w) if w.port.is_none() => "originated",
                Some(_) => "yes",
                None => "withdrawn",
            }
            .to_string(),
        }
        .report();
    }

    fn set_state(&mut self, port: PortId, state: State) {
        if let Some(s) = self.sessions.get_mut(&port)
            && s.state != state
        {
            s.state = state;
            self.report_peer(port);
        }
    }

    /// Back to idle (rule 2's last row): every route learned on the session withdrawn.
    fn drop_session(&mut self, port: PortId) {
        let Some(s) = self.sessions.get_mut(&port) else {
            return;
        };
        let was = s.state;
        s.adj_in.clear();
        s.adj_out.clear();
        s.peer_as = 0;
        s.peer_id = 0;
        if was != State::Idle {
            s.state = State::Idle;
            self.report_peer(port);
            self.decide();
        }
    }

    /// Rule 4, per prefix, over every established session's Adj-RIB-In and this router's own
    /// networks: the highest local preference, the shortest AS path, the lowest MED, the
    /// lowest router id of the advertising peer. Changes are reported and advertised (rule 5).
    fn decide(&mut self) {
        let mut next: BTreeMap<Vec<u8>, Winner> = BTreeMap::new();
        for n in &self.networks {
            next.insert(
                n.clone(),
                Winner {
                    port: None,
                    path: Path {
                        as_path: Vec::new(),
                        med: 0,
                    },
                    local_pref: u32::MAX,
                    peer_id: 0,
                },
            );
        }
        for (port, s) in &self.sessions {
            if s.state != State::Established {
                continue;
            }
            let local_pref = self.local_pref_of(*port);
            for (prefix, path) in &s.adj_in {
                let candidate = Winner {
                    port: Some(*port),
                    path: path.clone(),
                    local_pref,
                    peer_id: s.peer_id,
                };
                let better = match next.get(prefix) {
                    None => true,
                    Some(cur) => {
                        (
                            core::cmp::Reverse(candidate.local_pref),
                            candidate.path.as_path.len(),
                            candidate.path.med,
                            candidate.peer_id,
                        ) < (
                            core::cmp::Reverse(cur.local_pref),
                            cur.path.as_path.len(),
                            cur.path.med,
                            cur.peer_id,
                        )
                    }
                };
                if better {
                    next.insert(prefix.clone(), candidate);
                }
            }
        }
        let mut changed = false;
        for (prefix, w) in &next {
            if self.loc_rib.get(prefix) != Some(w) {
                self.report_route(prefix, Some(w));
                changed = true;
            }
        }
        let gone: Vec<Vec<u8>> = self
            .loc_rib
            .keys()
            .filter(|p| !next.contains_key(*p))
            .cloned()
            .collect();
        for prefix in &gone {
            self.report_route(prefix, None);
            changed = true;
        }
        self.loc_rib = next;
        if changed {
            self.sync_out();
        }
    }

    /// Every established peer's Adj-RIB-Out follows the Loc-RIB: the winners not learned from
    /// that peer, this router's AS prepended and the originator's MED kept (rule 3: set by the
    /// originator per port, compared by every receiver), this router's own MED for the
    /// prefixes it originates; one update per peer with what changed.
    fn sync_out(&mut self) {
        let ports: Vec<PortId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.state == State::Established)
            .map(|(p, _)| *p)
            .collect();
        for port in ports {
            let med = self.med_of(port);
            let desired: BTreeMap<Vec<u8>, Path> = self
                .loc_rib
                .iter()
                .filter(|(_, w)| w.port != Some(port))
                .map(|(prefix, w)| {
                    let mut as_path = Vec::with_capacity(w.path.as_path.len() + 1);
                    as_path.push(self.autonomous_system);
                    as_path.extend_from_slice(&w.path.as_path);
                    let med = if w.port.is_none() { med } else { w.path.med };
                    (prefix.clone(), Path { as_path, med })
                })
                .collect();
            let current = &self.sessions[&port].adj_out;
            let withdrawn: Vec<Vec<u8>> = current
                .keys()
                .filter(|p| !desired.contains_key(*p))
                .cloned()
                .collect();
            let advertised: Vec<(Vec<u8>, Path)> = desired
                .iter()
                .filter(|(p, path)| current.get(*p) != Some(path))
                .map(|(p, path)| (p.clone(), path.clone()))
                .collect();
            self.send_update(port, &withdrawn, &advertised);
            if let Some(s) = self.sessions.get_mut(&port) {
                s.adj_out = desired;
            }
        }
    }

    fn on_open(&mut self, port: PortId, sender_as: u32, sender_id: u64, payload: &[u8]) {
        let Some(hold) = u32_at(payload, HEADER) else {
            return;
        };
        if sender_as == self.autonomous_system {
            host::log(&format!(
                "router {sender_id} on interface {port} is in my own AS {sender_as}: internal BGP is outside the subset; the session is refused"
            ));
            self.send_notification(port, OWN_AS, "own AS");
            self.drop_session(port);
            return;
        }
        if u64::from(hold) != self.hold {
            host::log(&format!(
                "router {sender_id} on interface {port} opens with hold {hold}, not {}: the subset does not negotiate the hold time; the session is refused",
                self.hold
            ));
            self.send_notification(port, HOLD_TIME, "hold time");
            self.drop_session(port);
            return;
        }
        let Some(s) = self.sessions.get_mut(&port) else {
            return;
        };
        s.peer_as = sender_as;
        s.peer_id = sender_id;
        s.last_heard = now();
        match s.state {
            // A peer that opened first: answer with an open too, then confirm.
            State::Idle => {
                self.send_open(port);
                self.send_keepalive(port);
                self.set_state(port, State::OpenConfirm);
            }
            State::OpenSent => {
                self.send_keepalive(port);
                self.set_state(port, State::OpenConfirm);
            }
            State::OpenConfirm | State::Established => {}
        }
    }

    fn on_keepalive(&mut self, port: PortId) {
        let Some(s) = self.sessions.get_mut(&port) else {
            return;
        };
        s.last_heard = now();
        if s.state == State::OpenConfirm {
            self.set_state(port, State::Established);
            // The whole Adj-RIB-Out, as updates (rule 2).
            self.sync_out();
            self.decide();
        }
    }

    fn on_update(&mut self, port: PortId, sender_as: u32, payload: &[u8]) {
        let state = self.sessions.get(&port).map(|s| s.state);
        if state != Some(State::Established) {
            return;
        }
        let Some(nw) = u16_at(payload, HEADER) else {
            return;
        };
        let mut at = HEADER + 2;
        let mut withdrawn = Vec::with_capacity(usize::from(nw));
        for _ in 0..nw {
            let Some((prefix, next)) = prefix_at(payload, at) else {
                return;
            };
            withdrawn.push(prefix);
            at = next;
        }
        let Some(na) = u16_at(payload, at) else {
            return;
        };
        at += 2;
        let mut advertised = Vec::with_capacity(usize::from(na));
        for _ in 0..na {
            let Some((prefix, next)) = prefix_at(payload, at) else {
                return;
            };
            at = next;
            let Some(n) = u16_at(payload, at) else {
                return;
            };
            at += 2;
            let mut as_path = Vec::with_capacity(usize::from(n));
            for _ in 0..n {
                let Some(a) = u32_at(payload, at) else {
                    return;
                };
                as_path.push(a);
                at += 4;
            }
            let Some(med) = u32_at(payload, at) else {
                return;
            };
            at += 4;
            advertised.push((prefix, Path { as_path, med }));
        }
        let me = self.autonomous_system;
        let Some(s) = self.sessions.get_mut(&port) else {
            return;
        };
        s.last_heard = now();
        for w in &withdrawn {
            s.adj_in.remove(w);
        }
        let mut looped = 0u64;
        for (prefix, path) in advertised {
            if path.as_path.contains(&me) {
                looped += 1;
                s.adj_in.remove(&prefix);
                continue;
            }
            s.adj_in.insert(prefix, path);
        }
        if looped > 0 {
            self.loops += looped;
            host::log(&format!(
                "{looped} paths from AS {sender_as} on interface {port} carried my own AS {me}: dropped as loops ({} so far)",
                self.loops
            ));
        }
        self.report_peer(port);
        self.decide();
    }

    fn on_notification(&mut self, port: PortId, sender_id: u64, payload: &[u8]) {
        let code = payload.get(HEADER).copied().unwrap_or(0);
        host::log(&format!(
            "router {sender_id} on interface {port} closed the session with notification {code}"
        ));
        self.drop_session(port);
    }
}

impl Actor for Bgp {
    fn init(actor: ActorId, ports: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        let port_ids: Vec<PortId> = ports.iter().map(|p| p.id).collect();
        let (local_pref, default_lp) = per_port(&c.local_pref, &port_ids);
        let (med, default_med) = per_port(&c.med, &port_ids);
        let hold = c.hold.max(3);
        let mut me = Self {
            autonomous_system: if c.autonomous_system == 0 {
                actor
            } else {
                c.autonomous_system.min(u64::from(u32::MAX)) as u32
            },
            id: if c.router_id == 0 {
                u64::from(actor)
            } else {
                c.router_id
            },
            hold,
            keepalive: if c.keepalive == 0 {
                (hold / 3).max(1)
            } else {
                c.keepalive
            },
            networks: c
                .networks
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.as_bytes().to_vec())
                .collect(),
            local_pref,
            med,
            default_local_pref: default_lp.unwrap_or(100),
            default_med: default_med.unwrap_or(0),
            sessions: BTreeMap::new(),
            refused_ports: BTreeSet::new(),
            loc_rib: BTreeMap::new(),
            loops: 0,
        };
        for p in &port_ids {
            me.sessions.insert(
                *p,
                Session {
                    state: State::Idle,
                    peer_as: 0,
                    peer_id: 0,
                    last_heard: 0,
                    adj_in: BTreeMap::new(),
                    adj_out: BTreeMap::new(),
                },
            );
        }
        // The networks originated are the Loc-RIB's first winners.
        me.decide();
        for p in port_ids {
            if link_info(p).up {
                me.send_open(p);
                me.set_state(p, State::OpenSent);
            }
        }
        host::set_timer(0, me.keepalive);
        me
    }

    fn on_message(&mut self, port: PortId, payload: &[u8]) {
        if payload.len() < HEADER {
            return;
        }
        let (Some(sender_as), Some(sender_id)) = (u32_at(payload, 2), u64_at(payload, 6)) else {
            return;
        };
        if payload[0] != VERSION {
            host::log(&format!(
                "router {sender_id} on interface {port} speaks version {}, not {VERSION}: outside the subset; the session is refused",
                payload[0]
            ));
            self.send_notification(port, BAD_VERSION, "version");
            self.drop_session(port);
            return;
        }
        // A second router on the interface (a lan) is outside the subset: refused once per port.
        if let Some(s) = self.sessions.get(&port)
            && s.state != State::Idle
            && s.peer_id != 0
            && s.peer_id != sender_id
        {
            if self.refused_ports.insert(port) {
                host::log(&format!(
                    "a second router {sender_id} on interface {port}: a session is one link between two routers; ignored"
                ));
            }
            return;
        }
        match payload[1] {
            OPEN => self.on_open(port, sender_as, sender_id, payload),
            UPDATE => self.on_update(port, sender_as, payload),
            KEEPALIVE => self.on_keepalive(port),
            NOTIFICATION => self.on_notification(port, sender_id, payload),
            other => {
                host::log(&format!(
                    "message type {other} from router {sender_id} is outside the subset; ignored"
                ));
                self.send_notification(port, OUTSIDE, "message type");
            }
        }
    }

    fn on_timer(&mut self, _: TimerId) {
        let t = now();
        // The hold timer: a session silent for `hold` ticks goes idle (rule 2).
        let expired: Vec<PortId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.state != State::Idle && t.saturating_sub(s.last_heard) >= self.hold)
            .map(|(p, _)| *p)
            .collect();
        for p in expired {
            host::log(&format!(
                "the hold timer of interface {p} expired after {} ticks of silence",
                self.hold
            ));
            self.drop_session(p);
        }
        let established: Vec<PortId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.state == State::Established)
            .map(|(p, _)| *p)
            .collect();
        for p in established {
            self.send_keepalive(p);
        }
        host::set_timer(0, self.keepalive);
    }
}

impl Router for Bgp {
    fn on_link(&mut self, port: PortId, up: bool) {
        if up {
            if self
                .sessions
                .get(&port)
                .is_some_and(|s| s.state == State::Idle)
            {
                if let Some(s) = self.sessions.get_mut(&port) {
                    s.last_heard = now();
                }
                self.send_open(port);
                self.set_state(port, State::OpenSent);
            }
        } else {
            self.drop_session(port);
        }
    }
}

florn_guest_network::router!(Bgp);
