//! The first rung of the ladder (RFC 008 rule 20): echoes every frame through the interface it
//! arrived on. With a `period` it also sends a frame through every interface each period, so
//! that a pair of them drives itself; link state and `link-info` are reported when a link
//! changes. The kit's network scenarios are built on it.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::vec::Vec;
use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};
use florn_guest_network::{Router, link_info};

florn_guest::declare! {
    world: "florn:network/router@0.1.0",
    kind: "net-echo",
    description: "Echoes every frame through its interface; with a period, sends a frame through every interface",
    ports: [("eth", "network/interface", 1, 8)],
    params: [
        (period, "period", u64, false, default = 0),
        (payload, "payload", u64, false, default = 64),
        (echo, "echo", bool, false, default = true),
        (count, "count", u64, false, default = 0),
    ],
    telemetry: [
        (1, "link", Link, [(port, "port", u64), (up, "up", bool)]),
        (2, "info", Info, [(port, "port", u64), (cost, "cost", u64), (mtu, "mtu", u64), (bandwidth, "bandwidth", u64)]),
    ],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
    tables: [("network", [("protocol", "net-echo")])],
}

struct NetEcho {
    period: u64,
    payload: usize,
    echo: bool,
    count: u64,
    sent: u64,
    ports: Vec<PortId>,
}

impl NetEcho {
    /// The frame: the send's ordinal in the first eight bytes, then a pattern.
    fn frame(&self) -> Vec<u8> {
        let mut p: Vec<u8> = Vec::with_capacity(self.payload);
        p.extend_from_slice(&self.sent.to_le_bytes());
        while p.len() < self.payload {
            p.push((p.len() as u8).wrapping_mul(31));
        }
        p.truncate(self.payload);
        p
    }
}

impl Actor for NetEcho {
    fn init(_: ActorId, ports: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        let me = Self {
            period: c.period,
            payload: c.payload as usize,
            echo: c.echo,
            count: c.count,
            sent: 0,
            ports: ports.iter().map(|p| p.id).collect(),
        };
        if me.period > 0 {
            host::set_timer(0, me.period);
        }
        me
    }

    fn on_message(&mut self, port: PortId, payload: &[u8]) {
        if self.echo {
            host::send(port, payload);
        }
    }

    fn on_timer(&mut self, _: TimerId) {
        let frame = self.frame();
        for p in &self.ports {
            host::send(*p, &frame);
        }
        self.sent += 1;
        if self.count == 0 || self.sent < self.count {
            host::set_timer(0, self.period);
        }
    }
}

impl Router for NetEcho {
    fn on_link(&mut self, port: PortId, up: bool) {
        Link {
            port: u64::from(port),
            up,
        }
        .report();
        let i = link_info(port);
        Info {
            port: u64::from(port),
            cost: u64::from(i.cost),
            mtu: u64::from(i.mtu),
            bandwidth: i.bandwidth,
        }
        .report();
    }
}

florn_guest_network::router!(NetEcho);
