//! The heap guest (RFC 003 §10, the export row): a heap that grows with every activation, a
//! counter, and a call through a function pointer chosen by parity, so that an exported and
//! restored instance must carry its memory and its table to continue identically.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::vec::Vec;
use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "heap",
    description: "Grows its heap every activation and reports a checksum through a function pointer",
    ports: [],
    params: [
        (grow, "grow", u64, false, default = 64),
        (period, "period", u64, false, default = 1),
    ],
    telemetry: [(1, "sum", Sum, [(len, "len", u64), (checksum, "checksum", u64)])],
    resources: [("memory", "4MiB"), ("pending-timers", 1)],
}

fn fold_add(acc: u64, b: u8) -> u64 {
    acc.wrapping_mul(31).wrapping_add(u64::from(b))
}

fn fold_xor(acc: u64, b: u8) -> u64 {
    (acc.rotate_left(5)) ^ u64::from(b)
}

struct Heap {
    grow: u64,
    period: u64,
    data: Vec<u8>,
    counter: u64,
    checksum: u64,
    folds: [fn(u64, u8) -> u64; 2],
}

impl Actor for Heap {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        let c = Config::decode(config);
        host::set_timer(0, c.period);
        Self {
            grow: c.grow,
            period: c.period,
            data: Vec::new(),
            counter: 0,
            checksum: 0,
            folds: [fold_add, fold_xor],
        }
    }
    fn on_message(&mut self, _: PortId, _: &[u8]) {}
    fn on_timer(&mut self, _: TimerId) {
        let start = self.data.len();
        for i in 0..self.grow {
            self.data.push((self.counter.wrapping_add(i)) as u8);
        }
        self.counter += 1;
        // The checksum carries on over the new bytes only, so that an activation's work does
        // not grow with the heap.
        let fold = self.folds[(self.counter & 1) as usize];
        self.checksum = self.data[start..]
            .iter()
            .fold(self.checksum, |acc, b| fold(acc, *b));
        Sum {
            len: self.data.len() as u64,
            checksum: self.checksum,
        }
        .report();
        host::set_timer(0, self.period);
    }
}

florn_guest::actor!(Heap);
