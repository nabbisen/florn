//! The canonical encoding of the parsed model and the scenario hash (RFC 004 §5). One value has one
//! encoding: formatting, comments, key order, and `views` never change it. Actors and connections
//! are encoded in identity order, so that the order in which a file lists them does not matter
//! either; scheduled changes keep their written order, which is content.

use sha2::{Digest, Sha256};

use crate::manifest::*;

/// The byte writer of RFC 004 rule 30.
#[derive(Default)]
pub struct Encoder {
    out: Vec<u8>,
}

impl Encoder {
    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    pub fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.out.extend_from_slice(b);
    }
    pub fn text(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    /// A quantity: the number in shortest form as text, then the unit as text.
    pub fn quantity(&mut self, number: &str, unit: &str) {
        self.text(number);
        self.text(unit);
    }
    pub fn option<T>(&mut self, v: Option<T>, f: impl FnOnce(&mut Self, T)) {
        match v {
            None => self.u8(0),
            Some(x) => {
                self.u8(1);
                f(self, x);
            }
        }
    }
    pub fn list<T>(&mut self, items: &[T], mut f: impl FnMut(&mut Self, &T)) {
        self.u32(items.len() as u32);
        for i in items {
            f(self, i);
        }
    }
    pub fn finish(self) -> Vec<u8> {
        self.out
    }
}

/// A reader of the canonical encoding (the inverse of [`Encoder`]); every method returns `None`
/// at the first malformed byte, and a list's length is bounded by the input.
pub struct Decoder<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    pub fn done(&self) -> bool {
        self.pos == self.bytes.len()
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    pub fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    pub fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u32()? as usize;
        Some(self.take(n)?.to_vec())
    }
    pub fn text(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?).ok()
    }
    pub fn quantity(&mut self) -> Option<(String, String)> {
        Some((self.text()?, self.text()?))
    }
    pub fn digest(&mut self) -> Option<[u8; 32]> {
        self.bytes()?.try_into().ok()
    }
    pub fn option<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<Option<T>> {
        match self.u8()? {
            0 => Some(None),
            1 => Some(Some(f(self)?)),
            _ => None,
        }
    }
    pub fn list<T>(&mut self, mut f: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let n = self.u32()? as usize;
        if n > self.remaining() {
            return None;
        }
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(f(self)?);
        }
        Some(v)
    }
    pub fn domain_value(&mut self) -> Option<DomainValue> {
        Some(match self.u8()? {
            TAG_INTEGER => DomainValue::Integer(self.i64()?),
            TAG_DECIMAL => DomainValue::Decimal(self.text()?),
            TAG_TEXT => DomainValue::Text(self.text()?),
            TAG_BOOLEAN => DomainValue::Boolean(match self.u8()? {
                0 => false,
                1 => true,
                _ => return None,
            }),
            TAG_LIST => DomainValue::List(self.list(Self::domain_value)?),
            TAG_MAP => DomainValue::Map(self.domain_map()?),
            _ => return None,
        })
    }
    pub fn domain_map(&mut self) -> Option<DomainMap> {
        let n = self.u32()? as usize;
        if n > self.remaining() {
            return None;
        }
        let mut m = DomainMap::new();
        for _ in 0..n {
            let k = self.text()?;
            let v = self.domain_value()?;
            m.insert(k, v);
        }
        Some(m)
    }
}

/// Tags of a domain value (rule 32), so that one value has one encoding.
const TAG_INTEGER: u8 = 0;
const TAG_DECIMAL: u8 = 1;
const TAG_TEXT: u8 = 2;
const TAG_BOOLEAN: u8 = 3;
const TAG_LIST: u8 = 4;
const TAG_MAP: u8 = 5;

pub fn domain_value(e: &mut Encoder, v: &DomainValue) {
    match v {
        DomainValue::Integer(i) => {
            e.u8(TAG_INTEGER);
            e.i64(*i);
        }
        DomainValue::Decimal(d) => {
            e.u8(TAG_DECIMAL);
            e.text(d);
        }
        DomainValue::Text(t) => {
            e.u8(TAG_TEXT);
            e.text(t);
        }
        DomainValue::Boolean(b) => {
            e.u8(TAG_BOOLEAN);
            e.u8(*b as u8);
        }
        DomainValue::List(l) => {
            e.u8(TAG_LIST);
            e.list(l, domain_value);
        }
        DomainValue::Map(m) => {
            e.u8(TAG_MAP);
            domain_map(e, m);
        }
    }
}

/// A map: length, then entries sorted by key (a `BTreeMap` already is), each key then value.
pub fn domain_map(e: &mut Encoder, m: &DomainMap) {
    e.u32(m.len() as u32);
    for (k, v) in m {
        e.text(k);
        domain_value(e, v);
    }
}

fn limits(e: &mut Encoder, l: &Limits) {
    e.option(l.budget, Encoder::u64);
    e.option(l.stack, Encoder::u64);
    e.option(l.memory_bytes, Encoder::u64);
    e.option(l.effects_per_activation, Encoder::u64);
    e.option(l.payload_bytes, Encoder::u64);
    e.option(l.pending_timers, Encoder::u64);
    e.option(l.telemetry_per_activation_bytes, Encoder::u64);
}

impl Manifest {
    /// The canonical encoding of rule 30: the core fields in the order RFC 004 §3 lists them, every
    /// reference by identity, domain fields as tagged maps, `views` excluded.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut e = Encoder::default();
        // §3.2 header
        e.u64(self.schema);
        e.text(&self.name);
        e.list(&self.domains, |e, d| e.text(d));
        e.option(self.description.as_deref(), Encoder::text);
        e.option(self.next_identity, Encoder::u32);
        // §3.3 time, order, seed
        e.text(self.time.tick.as_str());
        e.u64(self.time.end);
        e.text(self.order.tie.as_str());
        e.option(self.order.microsteps_per_tick, Encoder::u64);
        e.option(self.order.events_per_instant, Encoder::u64);
        e.u64(self.seed);
        // §3.4 limits: map by class
        e.u32(self.limits.len() as u32);
        for (class, l) in &self.limits {
            e.text(class);
            limits(&mut e, l);
        }
        // §3.5 modules: map by name
        e.u32(self.modules.len() as u32);
        for (name, m) in &self.modules {
            e.text(name);
            e.bytes(&m.sha256);
            e.text(&m.contract);
        }
        // §3.6 actors, by identity
        e.list(&self.actors, |e, a| {
            e.u32(a.id);
            e.text(&a.name);
            match &a.kind {
                ActorKind::Module(m) => {
                    e.u8(0);
                    e.text(m);
                }
                ActorKind::Builtin(b) => {
                    e.u8(1);
                    e.text(b);
                }
            }
            e.text(&a.class);
            e.list(&a.ports, |e, p| {
                e.u32(p.id);
                e.text(&p.name);
                e.text(&p.r#type);
            });
            domain_map(e, &a.config);
        });
        // §3.7 connections, by identity; port references by identities
        e.list(&self.connections, |e, c| {
            e.u32(c.id);
            e.text(&c.name);
            e.list(&c.ports, |e, p| {
                e.u32(p.actor);
                e.u32(p.port);
            });
            domain_map(e, &c.attrs);
            e.u32(c.per_port.len() as u32);
            for (p, m) in &c.per_port {
                e.u32(p.actor);
                e.u32(p.port);
                domain_map(e, m);
            }
        });
        // §3.8 scheduled changes, in written order
        e.list(&self.changes, |e, c| {
            e.u64(c.at);
            match c.target {
                ChangeTarget::Actor(i) => {
                    e.u8(0);
                    e.u32(i);
                }
                ChangeTarget::Connection(i) => {
                    e.u8(1);
                    e.u32(i);
                }
                ChangeTarget::World(n) => {
                    e.u8(2);
                    e.u32(n);
                }
            }
            domain_map(e, &c.payload);
        });
        // §3.10 domain tables, by domain name (views are excluded, rule 21)
        e.u32(self.domain_tables.len() as u32);
        for (d, m) in &self.domain_tables {
            e.text(d);
            domain_map(&mut e, m);
        }
        e.finish()
    }

    /// The scenario hash: SHA-256 over the canonical encoding (rule 31).
    pub fn scenario_hash(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_bytes()).into()
    }

    pub fn scenario_hash_hex(&self) -> String {
        hex(&self.scenario_hash())
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
