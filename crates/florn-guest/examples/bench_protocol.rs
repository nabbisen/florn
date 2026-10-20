//! The protocol-style benchmark guest B1 (RFC 003 §7): one activation parses a 1,500-byte buffer
//! of records, updates a 1,000-entry ordered map, and builds a reply of the same shape.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "bench-protocol",
    description: "Parses a record buffer, updates an ordered map, and replies with the current values",
    ports: [("p", "core/port", 0, 8)],
    params: [(entries, "entries", u64, false, default = 1000)],
    telemetry: [],
    resources: [("memory", "2MiB"), ("pending-timers", 1)],
}

/// A buffer: a record count, then records of a key and a value, then a trailer of eight bytes.
const RECORD: usize = 12;

struct Protocol {
    entries: u64,
    map: BTreeMap<u32, u64>,
    reply: Vec<u8>,
}

impl Actor for Protocol {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        let map = (0..c.entries as u32)
            .map(|k| (k, u64::from(k) * 7))
            .collect();
        Self {
            entries: c.entries.max(1),
            map,
            reply: Vec::with_capacity(1500),
        }
    }
    fn on_message(&mut self, port: PortId, payload: &[u8]) {
        if payload.len() < 12 {
            return;
        }
        let declared = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
        let count = declared.min((payload.len() - 12) / RECORD);
        self.reply.clear();
        self.reply.extend_from_slice(&(count as u32).to_le_bytes());
        let mut sum = 0u64;
        for i in 0..count {
            let at = 4 + i * RECORD;
            let key = u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
            let value = u64::from_le_bytes(payload[at + 4..at + 12].try_into().unwrap());
            let key = (u64::from(key) % self.entries) as u32;
            let now = self
                .map
                .entry(key)
                .and_modify(|v| *v = v.wrapping_add(value))
                .or_insert(value);
            sum = sum.wrapping_add(*now);
            self.reply.extend_from_slice(&key.to_le_bytes());
            self.reply.extend_from_slice(&now.to_le_bytes());
        }
        self.reply.extend_from_slice(&sum.to_le_bytes());
        host::send(port, &self.reply);
    }
    fn on_timer(&mut self, _: TimerId) {}
}

florn_guest::actor!(Protocol);
