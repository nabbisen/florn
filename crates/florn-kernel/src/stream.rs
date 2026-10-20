//! Randomness (RFC 001 §6): one stream per consumer, keyed by the root seed, the owner's identity,
//! and a purpose label (rule 29), producing uniformly distributed unsigned 64-bit integers and
//! nothing else (rule 30). The generator is ChaCha with 8 rounds, implemented here so that the
//! stream depends on no library's choices; the derivation is BLAKE3's key derivation (RFC 005
//! rule 22). A stream's position can be saved and restored, which is how a fault rolls the stream
//! back to the start of the activation (RFC 001 rule 24; RFC 002 rule 31).

use crate::key::Identity;

/// The 32-byte key of a stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamKey(pub [u8; 32]);

impl StreamKey {
    pub const CONTEXT: &'static str = "florn:core/stream@0.1.0";

    /// Rule 29: derived from the root seed, the owner's identity, and the purpose label; dependent
    /// on no other stream and not on the order of creation.
    pub fn derive(root_seed: u64, owner: Identity, purpose: &str) -> Self {
        let mut material = Vec::with_capacity(12 + purpose.len());
        material.extend_from_slice(&root_seed.to_le_bytes());
        material.extend_from_slice(&owner.to_le_bytes());
        material.extend_from_slice(purpose.as_bytes());
        Self(blake3::derive_key(Self::CONTEXT, &material))
    }
}

/// A stream: ChaCha8 keystream, consumed as 64-bit words in order. Cheap to copy, so that the
/// position at the start of an activation can be kept and restored.
#[derive(Clone, Copy, Debug)]
pub struct Stream {
    key: [u32; 8],
    /// The block counter of the next block to generate.
    counter: u64,
    block: [u32; 16],
    /// The next word of `block` to hand out; 16 means the block is spent.
    index: usize,
    /// Words drawn so far: the stream's position, for the trace and for diagnostics.
    drawn: u64,
}

/// Two streams are equal when they have the same key and the same position; the block cache is
/// derived from those.
impl PartialEq for Stream {
    fn eq(&self, o: &Self) -> bool {
        self.key == o.key && self.drawn == o.drawn
    }
}
impl Eq for Stream {}

impl Stream {
    pub fn new(key: StreamKey) -> Self {
        let mut k = [0u32; 8];
        for (i, w) in k.iter_mut().enumerate() {
            *w = u32::from_le_bytes(key.0[i * 4..i * 4 + 4].try_into().unwrap());
        }
        Self {
            key: k,
            counter: 0,
            block: [0; 16],
            index: 16,
            drawn: 0,
        }
    }

    pub fn for_actor(root_seed: u64, owner: Identity) -> Self {
        Self::new(StreamKey::derive(root_seed, owner, "actor"))
    }

    /// The next uniformly distributed unsigned 64-bit integer (rule 30).
    pub fn next_u64(&mut self) -> u64 {
        let lo = self.next_u32() as u64;
        let hi = self.next_u32() as u64;
        self.drawn += 1;
        lo | (hi << 32)
    }

    /// Words of 64 bits drawn since creation.
    pub fn drawn(&self) -> u64 {
        self.drawn
    }

    /// The key this stream was made from.
    pub fn key(&self) -> StreamKey {
        let mut k = [0u8; 32];
        for (i, w) in self.key.iter().enumerate() {
            k[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        StreamKey(k)
    }

    /// A stream at a position: `key` with `drawn` words already handed out (for restore,
    /// RFC 003 rule 22). Generates the blocks up to the position; linear in `drawn`.
    pub fn at(key: StreamKey, drawn: u64) -> Self {
        let mut s = Self::new(key);
        // Two words of 32 bits per draw: skip whole blocks of 16 words, then the remainder. A
        // position no run can reach (a hostile checkpoint) saturates rather than overflows;
        // the fuzz target of RFC 006 rule 7 found the overflow.
        let words = drawn.saturating_mul(2);
        s.counter = words / 16;
        s.index = 16;
        for _ in 0..(words % 16) {
            s.next_u32();
        }
        s.drawn = drawn;
        s
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= 16 {
            self.block = chacha_block(&self.key, self.counter, 8);
            self.counter = self.counter.wrapping_add(1);
            self.index = 0;
        }
        let w = self.block[self.index];
        self.index += 1;
        w
    }
}

/// One ChaCha block (RFC 7539 §2.3, with a 64-bit counter and a zero nonce), `rounds` rounds.
pub(crate) fn chacha_block(key: &[u32; 8], counter: u64, rounds: u32) -> [u32; 16] {
    let mut s = [
        0x6170_7865,
        0x3320_646e,
        0x7962_2d32,
        0x6b20_6574, // "expand 32-byte k"
        key[0],
        key[1],
        key[2],
        key[3],
        key[4],
        key[5],
        key[6],
        key[7],
        counter as u32,
        (counter >> 32) as u32,
        0,
        0,
    ];
    let init = s;
    for _ in 0..rounds / 2 {
        quarter(&mut s, 0, 4, 8, 12);
        quarter(&mut s, 1, 5, 9, 13);
        quarter(&mut s, 2, 6, 10, 14);
        quarter(&mut s, 3, 7, 11, 15);
        quarter(&mut s, 0, 5, 10, 15);
        quarter(&mut s, 1, 6, 11, 12);
        quarter(&mut s, 2, 7, 8, 13);
        quarter(&mut s, 3, 4, 9, 14);
    }
    for (w, i) in s.iter_mut().zip(init) {
        *w = w.wrapping_add(i);
    }
    s
}

#[inline]
fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_at_a_position_continues_like_the_original() {
        let key = StreamKey::derive(5, 9, "actor");
        for n in [0u64, 1, 7, 8, 15, 16, 17, 100] {
            let mut original = Stream::new(key);
            for _ in 0..n {
                original.next_u64();
            }
            let mut at = Stream::at(key, n);
            assert_eq!(at, original);
            for _ in 0..40 {
                assert_eq!(at.next_u64(), original.next_u64(), "after {n} draws");
            }
        }
    }

    /// RFC 7539 §2.3.2: the ChaCha20 block with the test key, counter 1, nonce 0 — restricted here
    /// to a zero nonce by using the keystream of §2.4.2's structure: we check the block function
    /// against the RFC's §2.3.2 vector by setting the nonce words to the vector's values.
    #[test]
    fn chacha20_block_matches_rfc_7539() {
        let key: [u32; 8] = [
            0x03020100, 0x07060504, 0x0b0a0908, 0x0f0e0d0c, 0x13121110, 0x17161514, 0x1b1a1918,
            0x1f1e1d1c,
        ];
        // The RFC vector uses counter 1 and nonce 00:00:00:09:00:00:00:4a:00:00:00:00; our block
        // function has a zero nonce, so build the state by hand to compare the round function.
        let mut s = [
            0x61707865, 0x3320646e, 0x79622d32, 0x6b206574, key[0], key[1], key[2], key[3], key[4],
            key[5], key[6], key[7], 1, 0x09000000, 0x4a000000, 0,
        ];
        let init = s;
        for _ in 0..10 {
            quarter(&mut s, 0, 4, 8, 12);
            quarter(&mut s, 1, 5, 9, 13);
            quarter(&mut s, 2, 6, 10, 14);
            quarter(&mut s, 3, 7, 11, 15);
            quarter(&mut s, 0, 5, 10, 15);
            quarter(&mut s, 1, 6, 11, 12);
            quarter(&mut s, 2, 7, 8, 13);
            quarter(&mut s, 3, 4, 9, 14);
        }
        for (w, i) in s.iter_mut().zip(init) {
            *w = w.wrapping_add(i);
        }
        let expected: [u32; 16] = [
            0xe4e7f110, 0x15593bd1, 0x1fdd0f50, 0xc47120a3, 0xc7f4d1c7, 0x0368c033, 0x9aaa2204,
            0x4e6cd4c3, 0x466482d2, 0x09aa9f07, 0x05d7c214, 0xa2028bd9, 0xd19c12b5, 0xb94e16de,
            0xe883d0cb, 0x4e3c50a2,
        ];
        assert_eq!(s, expected);
    }

    #[test]
    fn stream_is_reproducible_and_independent() {
        let mut a = Stream::for_actor(42, 1);
        let mut b = Stream::for_actor(42, 1);
        let mut c = Stream::for_actor(42, 2);
        let mut d = Stream::for_actor(43, 1);
        let xs: Vec<u64> = (0..40).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..40).map(|_| b.next_u64()).collect();
        assert_eq!(xs, ys);
        assert_ne!(xs[0], c.next_u64());
        assert_ne!(xs[0], d.next_u64());
        assert_eq!(a.drawn(), 40);
        // Rollback: a copy taken before draws restores the position.
        let saved = a;
        let after = a.next_u64();
        a = saved;
        assert_eq!(a.next_u64(), after);
    }

    #[test]
    fn purpose_labels_give_independent_streams() {
        assert_ne!(
            StreamKey::derive(1, 1, "actor"),
            StreamKey::derive(1, 1, "connection")
        );
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;

    #[test]
    fn a_position_beyond_any_run_does_not_overflow() {
        let key = StreamKey::derive(1, 2, "actor");
        let s = Stream::at(key, u64::MAX);
        assert_eq!(s.drawn(), u64::MAX);
        let t = Stream::at(key, 5);
        let mut u = Stream::new(key);
        for _ in 0..5 {
            u.next_u64();
        }
        assert_eq!(t, u);
    }
}
