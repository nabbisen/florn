//! Exported state (RFC 003 §6; RFC 001 rule 51): an actor's state between activations, and a
//! checkpoint of the whole run, with one canonical binary encoding so that a checkpoint written
//! by one Host restores in another.

use std::collections::BTreeMap;

use florn_admission::GlobalKind;
use florn_kernel::{
    ActorKernelState, AppliedIntervention, Event, EventKey, EventKind, Identity, Instant,
    KernelSnapshot, Stream, StreamKey, TraceSnapshot,
};
use florn_model::canonical::{Decoder, Encoder};

/// A memory's bytes held sparse: the 4 KiB chunks that hold a non-zero byte, by offset, and the
/// full length. A guest's memory is mostly zeros (the stack not reached, the heap not touched),
/// and a checkpoint of thousands of actors, or their swapped state (RFC 009 rules 13 and 15),
/// must fit the Host's own memory. The file form is the dense bytes: the layout is unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SparseBytes {
    len: usize,
    chunks: Vec<(usize, Vec<u8>)>,
}

impl SparseBytes {
    pub const CHUNK: usize = 4096;

    pub fn from_dense(bytes: &[u8]) -> Self {
        Self {
            len: bytes.len(),
            chunks: bytes
                .chunks(Self::CHUNK)
                .enumerate()
                .filter(|(_, c)| c.iter().any(|b| *b != 0))
                .map(|(i, c)| (i * Self::CHUNK, c.to_vec()))
                .collect(),
        }
    }
    /// The bytes again, zeros where no chunk is held.
    pub fn to_dense(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.len];
        for (offset, chunk) in &self.chunks {
            out[*offset..*offset + chunk.len()].copy_from_slice(chunk);
        }
        out
    }
    /// The memory's size.
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// The bytes held.
    pub fn stored(&self) -> usize {
        self.chunks.iter().map(|(_, c)| c.len()).sum()
    }
}

/// An actor's state (RFC 003 rule 21): the memory up to its size, every mutable global, every
/// table's size, and the kernel's own state for the actor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorState {
    pub memory: SparseBytes,
    /// Name, type, and the value's bits.
    pub globals: Vec<(String, GlobalKind, u64)>,
    /// Name and size: tables never change (RFC 003 rule 5), so the size is verified on restore.
    pub tables: Vec<(String, u64)>,
    pub kernel: ActorKernelState,
}

/// What a checkpoint must agree on before it restores (RFC 003 rule 22; DATA-005).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointIdentity {
    pub scenario_sha256: [u8; 32],
    /// By `[modules]` name: the source hash.
    pub modules: Vec<(String, [u8; 32])>,
    pub admission_version: u32,
    pub cost_model_version: u32,
    pub root_seed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub identity: CheckpointIdentity,
    pub kernel: KernelSnapshot,
    pub actors: BTreeMap<Identity, ActorState>,
    /// The sequence counters of the domain origins (RFC 001 rule 17; connections and worlds),
    /// from the composed world: version 2 added them, 0.1 and 0.2 held none and never wrote a
    /// checkpoint to a file.
    pub origins: Vec<(Identity, u64)>,
    /// Each domain's own state (RFC 008 rule 1, `export`), by domain name: a trailing section,
    /// absent when no domain has state.
    pub domains: BTreeMap<String, Vec<u8>>,
}

/// Version 2 since 0.3.0 (RFC 009 rule 1): the origins' sequence counters joined the layout.
pub const CHECKPOINT_VERSION: u8 = 2;
/// A checkpoint file's maximum (RFC 006 section 7, RFC 009 rule 6): 2 GiB. Every count inside
/// is checked against the bytes left before anything is allocated (the canonical decoder).
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

impl Checkpoint {
    /// A checkpoint from a file under the maxima: `file.too-large` above `max`, `file.unreadable`
    /// for bytes that are no checkpoint of this version.
    pub fn decode_file(bytes: &[u8], max: u64) -> Result<Self, &'static str> {
        if bytes.len() as u64 > max {
            return Err("file.too-large");
        }
        Self::decode(bytes).ok_or("file.unreadable")
    }
}

fn kind_tag(k: GlobalKind) -> u8 {
    match k {
        GlobalKind::I32 => 0,
        GlobalKind::I64 => 1,
        GlobalKind::F32 => 2,
        GlobalKind::F64 => 3,
    }
}

fn kind_of(tag: u8) -> Option<GlobalKind> {
    Some(match tag {
        0 => GlobalKind::I32,
        1 => GlobalKind::I64,
        2 => GlobalKind::F32,
        3 => GlobalKind::F64,
        _ => return None,
    })
}

fn encode_actor_state(e: &mut Encoder, s: &ActorState) {
    e.bytes(&s.memory.to_dense());
    e.list(&s.globals, |e, (n, k, v)| {
        e.text(n);
        e.u8(kind_tag(*k));
        e.u64(*v);
    });
    e.list(&s.tables, |e, (n, size)| {
        e.text(n);
        e.u64(*size);
    });
    encode_kernel_actor(e, &s.kernel);
}

fn encode_kernel_actor(e: &mut Encoder, a: &ActorKernelState) {
    e.u64(a.seq);
    e.bytes(&a.stream.key().0);
    e.u64(a.stream.drawn());
    e.u32(a.timers.len() as u32);
    for (t, g) in &a.timers {
        e.u32(*t);
        e.u64(*g);
    }
    e.u8(a.alive as u8);
}

fn encode_event(e: &mut Encoder, ev: &Event) {
    e.u64(ev.key.tick);
    e.u32(ev.key.microstep);
    e.u8(ev.key.class);
    e.u64(ev.key.tie);
    e.u32(ev.key.origin);
    e.u64(ev.key.seq);
    e.u32(ev.target);
    match &ev.kind {
        EventKind::Timer { timer } => {
            e.u8(0);
            e.u32(*timer);
        }
        EventKind::Message { port, payload } => {
            e.u8(1);
            e.u32(*port);
            e.bytes(payload);
        }
        EventKind::ScheduledChange { payload } => {
            e.u8(2);
            e.bytes(payload);
        }
        EventKind::Intervention { payload } => {
            e.u8(3);
            e.bytes(payload);
        }
        EventKind::Domain { payload } => {
            e.u8(4);
            e.bytes(payload);
        }
    }
}

impl ActorState {
    pub fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::default();
        e.u8(CHECKPOINT_VERSION);
        encode_actor_state(&mut e, self);
        e.finish()
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut d = Decoder::new(bytes);
        if d.u8()? != CHECKPOINT_VERSION {
            return None;
        }
        let s = d.actor_state()?;
        d.done().then_some(s)
    }
}

impl Checkpoint {
    pub fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::default();
        e.u8(CHECKPOINT_VERSION);
        e.bytes(&self.identity.scenario_sha256);
        e.list(&self.identity.modules, |e, (n, h)| {
            e.text(n);
            e.bytes(h);
        });
        e.u32(self.identity.admission_version);
        e.u32(self.identity.cost_model_version);
        e.u64(self.identity.root_seed);
        let k = &self.kernel;
        e.list(&k.queue, encode_event);
        e.u32(k.actors.len() as u32);
        for (id, a) in &k.actors {
            e.u32(*id);
            encode_kernel_actor(&mut e, a);
        }
        e.u64(k.now.tick);
        e.u32(k.now.microstep);
        e.u64(k.instant_events);
        e.list(&k.instant_origins, |e, o| e.u32(*o));
        e.u32(k.last_tick_microsteps);
        e.list(&k.applied_interventions, |e, i| {
            e.u64(i.instant.tick);
            e.u32(i.instant.microstep);
            e.bytes(&i.content);
        });
        e.u64(k.intervention_seq);
        e.u64(k.change_seq);
        e.u64(k.handled);
        e.bytes(&k.trace.chain);
        e.u64(k.trace.records);
        e.list(&k.trace.stored, |e, (n, d)| {
            e.u64(*n);
            e.bytes(d);
        });
        e.u32(self.actors.len() as u32);
        for (id, a) in &self.actors {
            e.u32(*id);
            encode_actor_state(&mut e, a);
        }
        e.list(&self.origins, |e, (o, s)| {
            e.u32(*o);
            e.u64(*s);
        });
        if !self.domains.is_empty() {
            e.u32(self.domains.len() as u32);
            for (name, state) in &self.domains {
                e.text(name);
                e.bytes(state);
            }
        }
        e.finish()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut d = Decoder::new(bytes);
        if d.u8()? != CHECKPOINT_VERSION {
            return None;
        }
        let scenario_sha256 = d.digest()?;
        let modules = d.list(|d| Some((d.text()?, d.digest()?)))?;
        let admission_version = d.u32()?;
        let cost_model_version = d.u32()?;
        let root_seed = d.u64()?;
        let queue = d.list(|d| d.event())?;
        let n = d.u32()?;
        let mut actors = BTreeMap::new();
        for _ in 0..n {
            let id = d.u32()?;
            actors.insert(id, d.kernel_actor()?);
        }
        let now = Instant::new(d.u64()?, d.u32()?);
        let instant_events = d.u64()?;
        let instant_origins = d.list(Decoder::u32)?;
        let last_tick_microsteps = d.u32()?;
        let applied_interventions = d.list(|d| {
            Some(AppliedIntervention {
                instant: Instant::new(d.u64()?, d.u32()?),
                content: d.bytes()?,
            })
        })?;
        let intervention_seq = d.u64()?;
        let change_seq = d.u64()?;
        let handled = d.u64()?;
        let chain = d.digest()?;
        let records = d.u64()?;
        let stored = d.list(|d| Some((d.u64()?, d.digest()?)))?;
        let n = d.u32()?;
        let mut states = BTreeMap::new();
        for _ in 0..n {
            let id = d.u32()?;
            states.insert(id, d.actor_state()?);
        }
        let origins = d.list(|d| Some((d.u32()?, d.u64()?)))?;
        let mut domains = BTreeMap::new();
        if !d.done() {
            let n = d.u32()?;
            if n == 0 {
                return None;
            }
            for _ in 0..n {
                let name = d.text()?;
                domains.insert(name, d.bytes()?);
            }
        }
        if !d.done() {
            return None;
        }
        Some(Self {
            identity: CheckpointIdentity {
                scenario_sha256,
                modules,
                admission_version,
                cost_model_version,
                root_seed,
            },
            kernel: KernelSnapshot {
                queue,
                actors,
                now,
                instant_events,
                instant_origins,
                last_tick_microsteps,
                applied_interventions,
                intervention_seq,
                change_seq,
                handled,
                trace: TraceSnapshot {
                    chain,
                    records,
                    stored,
                },
            },
            actors: states,
            origins,
            domains,
        })
    }
}

/// The snapshot-specific readers over the model's decoder.
pub trait SnapshotDecoder {
    fn event(&mut self) -> Option<Event>;
    fn kernel_actor(&mut self) -> Option<ActorKernelState>;
    fn actor_state(&mut self) -> Option<ActorState>;
}

impl SnapshotDecoder for Decoder<'_> {
    fn event(&mut self) -> Option<Event> {
        let key = EventKey {
            tick: self.u64()?,
            microstep: self.u32()?,
            class: self.u8()?,
            tie: self.u64()?,
            origin: self.u32()?,
            seq: self.u64()?,
        };
        let target = self.u32()?;
        let kind = match self.u8()? {
            0 => EventKind::Timer { timer: self.u32()? },
            1 => EventKind::Message {
                port: self.u32()?,
                payload: self.bytes()?,
            },
            2 => EventKind::ScheduledChange {
                payload: self.bytes()?,
            },
            3 => EventKind::Intervention {
                payload: self.bytes()?,
            },
            4 => EventKind::Domain {
                payload: self.bytes()?,
            },
            _ => return None,
        };
        Some(Event { key, target, kind })
    }
    fn kernel_actor(&mut self) -> Option<ActorKernelState> {
        let seq = self.u64()?;
        let key = StreamKey(self.digest()?);
        let drawn = self.u64()?;
        let n = self.u32()?;
        let mut timers = BTreeMap::new();
        for _ in 0..n {
            timers.insert(self.u32()?, self.u64()?);
        }
        let alive = self.u8()? != 0;
        Some(ActorKernelState {
            seq,
            stream: Stream::at(key, drawn),
            timers,
            alive,
        })
    }
    fn actor_state(&mut self) -> Option<ActorState> {
        let memory = SparseBytes::from_dense(&self.bytes()?);
        let globals = self.list(|d| Some((d.text()?, kind_of(d.u8()?)?, d.u64()?)))?;
        let tables = self.list(|d| Some((d.text()?, d.u64()?)))?;
        let kernel = self.kernel_actor()?;
        Some(ActorState {
            memory,
            globals,
            tables,
            kernel,
        })
    }
}
