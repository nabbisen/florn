//! The chatter guest (RFC 002 §10, the parameter, effect, randomness, and telemetry rows; RFC 003
//! §10, the quota rows): on its timer it logs text on channel 0, draws random values and reports
//! them, sends, reports a blob, and traps at the tick it is told to. Every amount is a parameter,
//! so one guest stands one below and one above each quota.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use florn_guest::{Actor, ActorId, Port, PortId, Random, TimerId, host, now};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "chatter",
    description: "Logs, draws, sends, reports, and traps as its parameters say",
    ports: [("p", "core/port", 0, 8)],
    params: [
        (label, "label", text, true),
        (period, "period", u64, false, default = 5),
        (draws, "draws", u64, false, default = 2),
        (trap_at, "trap-at", u64, false, default = 0),
        (sends, "sends", u64, false, default = 0),
        (payload, "payload", u64, false, default = 1),
        (telemetry, "telemetry", u64, false, default = 0),
        (channel, "channel", u64, false, default = 1),
        (log, "log", text, false, default = ""),
    ],
    telemetry: [
        (1, "draw", Draw, [(value, "value", u64), (at, "at", u64)]),
        (2, "config", ConfigReport, [(label, "label", text), (period, "period", u64), (draws, "draws", u64), (trap_at, "trap-at", u64)]),
        (3, "blob", Blob, [(data, "data", bytes)]),
    ],
    resources: [("memory", "1MiB"), ("pending-timers", 1)],
}

struct Chatter {
    config: Config,
    random: Random,
    log: String,
}

impl Actor for Chatter {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        // The decoded values, in declaration order with the defaults filled (RFC 002 rule 14).
        ConfigReport {
            label: c.label.clone(),
            period: c.period,
            draws: c.draws,
            trap_at: c.trap_at,
        }
        .report();
        host::set_timer(0, c.period);
        let log = c.log.clone();
        Self {
            config: c,
            random: Random::host(),
            log,
        }
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, _: TimerId) {
        let c = &self.config;
        if !self.log.is_empty() {
            host::log(&self.log);
        }
        for _ in 0..c.draws {
            let value = self.random.u64();
            if c.channel == 1 {
                Draw { value, at: now() }.report();
            } else {
                host::report(c.channel as u32, &value.to_le_bytes());
            }
        }
        for _ in 0..c.sends {
            host::send(0, &vec![0xAB; c.payload as usize]);
        }
        if c.telemetry > 0 {
            Blob {
                data: vec![1; c.telemetry as usize],
            }
            .report();
        }
        if c.trap_at != 0 && now() == c.trap_at {
            florn_guest::actor::trap();
        }
        host::set_timer(0, c.period);
    }
}

florn_guest::actor!(Chatter);
