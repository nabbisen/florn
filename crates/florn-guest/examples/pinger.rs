//! The pinger: sends a payload through port 0 on a timer, or once at `init`; echoes every
//! message when told to. The kit's initiator: it drives the echo pair, the races, the zero-delay
//! chain, and the Host-path benchmarks.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::vec::Vec;
use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "pinger",
    description: "Sends a payload through port 0 on a timer or once at init; echoes when told to",
    ports: [("p", "core/port", 0, 8)],
    params: [
        (period, "period", u64, false, default = 1),
        (payload, "payload", u64, false, default = 16),
        (at_init, "at-init", bool, false, default = false),
        (echo, "echo", bool, false, default = false),
        (count, "count", u64, false, default = 0),
    ],
    telemetry: [],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
}

struct Pinger {
    period: u64,
    payload: usize,
    echo: bool,
    count: u64,
    sent: u64,
}

impl Pinger {
    /// The payload: the send's ordinal in the first eight bytes, then a pattern.
    fn ping(&mut self) {
        let mut p: Vec<u8> = Vec::with_capacity(self.payload);
        p.extend_from_slice(&self.sent.to_le_bytes());
        while p.len() < self.payload {
            p.push((p.len() as u8).wrapping_mul(31));
        }
        p.truncate(self.payload);
        host::send(0, &p);
        self.sent += 1;
    }
}

impl Actor for Pinger {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        let mut me = Self {
            period: c.period,
            payload: c.payload as usize,
            echo: c.echo,
            count: c.count,
            sent: 0,
        };
        if c.at_init {
            me.ping();
        }
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
        if self.count == 0 || self.sent < self.count {
            self.ping();
        }
        if self.period > 0 {
            host::set_timer(0, self.period);
        }
    }
}

florn_guest::actor!(Pinger);
