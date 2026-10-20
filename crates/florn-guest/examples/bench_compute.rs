//! The compute-style benchmark guest B2 (RFC 003 §7): one activation multiplies two 64×64
//! matrices filled from the message's seed and replies with a checksum.
// The example guests are WebAssembly modules; on another target the file is empty.
#![cfg(target_arch = "wasm32")]
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use florn_guest::{Actor, ActorId, Port, PortId, TimerId, host};

florn_guest::declare! {
    world: "florn:core/guest@0.1.0",
    kind: "bench-compute",
    description: "Multiplies two square matrices filled from the message and replies with a checksum",
    ports: [("p", "core/port", 0, 8)],
    params: [(n, "n", u64, false, default = 64)],
    telemetry: [],
    resources: [("memory", "2MiB"), ("pending-timers", 1)],
}

struct Compute {
    n: usize,
    a: Vec<f64>,
    b: Vec<f64>,
    c: Vec<f64>,
}

impl Actor for Compute {
    fn init(_: ActorId, _: &[Port], config: &[u8]) -> Self {
        let n = Config::decode(config).n.clamp(1, 256) as usize;
        Self {
            n,
            a: vec![0.0; n * n],
            b: vec![0.0; n * n],
            c: vec![0.0; n * n],
        }
    }
    fn on_message(&mut self, port: PortId, payload: &[u8]) {
        let mut seed = payload
            .get(..8)
            .map_or(1, |s| u64::from_le_bytes(s.try_into().unwrap()));
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
        };
        for v in self.a.iter_mut().chain(self.b.iter_mut()) {
            *v = next();
        }
        let n = self.n;
        for i in 0..n {
            for j in 0..n {
                let mut s = 0.0;
                for k in 0..n {
                    s += self.a[i * n + k] * self.b[k * n + j];
                }
                self.c[i * n + j] = s;
            }
        }
        let checksum: f64 = self.c.iter().sum();
        host::send(port, &checksum.to_bits().to_le_bytes());
    }
    fn on_timer(&mut self, _: TimerId) {}
}

florn_guest::actor!(Compute);
