//! Encoders for declared telemetry channels (RFC 002 rule 16): the field types of rule 13, each
//! encoded as the config record encodes them.

use alloc::vec::Vec;

/// A record being encoded, field by field in the declared order.
#[derive(Default)]
pub struct Encoder {
    out: Vec<u8>,
}

impl Encoder {
    pub fn bool(&mut self, v: bool) {
        self.out.push(v as u8);
    }
    pub fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn s64(&mut self, v: i64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    /// Canonical NaN (RFC 002 rule 9), so that a record has one encoding.
    pub fn f64(&mut self, v: f64) {
        let v = if v.is_nan() {
            f64::from_bits(0x7ff8_0000_0000_0000)
        } else {
            v
        };
        self.out.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        self.out.extend_from_slice(b);
    }
    pub fn text(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    pub fn finish(self) -> Vec<u8> {
        self.out
    }
}

#[cfg(test)]
mod tests;
