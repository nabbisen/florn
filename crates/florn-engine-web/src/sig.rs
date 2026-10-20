//! The call signatures the adapter calls from WebAssembly to WebAssembly (RFC 005 rule 7): the
//! three exports of the core world (RFC 002 rule 20) and `cabi_realloc`. A flat argument travels
//! in one 64-bit slot of the Host's memory; this module encodes and decodes the slots, so that
//! the part the worker and the trampoline agree on is testable natively.

use florn_abi::core_world;
use florn_engine::{Flat, FlatType};

/// A signature the trampoline knows, by number (the worker passes it through unchanged).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Sig {
    /// `init(now: i64, actor: i32, ports.ptr, ports.len, config.ptr, config.len)`
    Init = 1,
    /// `on-message(now: i64, port: i32, payload.ptr, payload.len)`
    Message = 2,
    /// `on-timer(now: i64, timer: i32)`
    Timer = 3,
    /// `cabi_realloc(old_ptr, old_size, align, new_size) -> ptr`
    Realloc = 4,
}

impl Sig {
    /// The signature of an export, by its canonical name.
    pub fn of_export(name: &str) -> Option<Sig> {
        match name {
            core_world::EXPORT_INIT => Some(Sig::Init),
            core_world::EXPORT_ON_MESSAGE => Some(Sig::Message),
            core_world::EXPORT_ON_TIMER => Some(Sig::Timer),
            core_world::EXPORT_REALLOC => Some(Sig::Realloc),
            _ => None,
        }
    }

    pub fn params(self) -> &'static [FlatType] {
        use FlatType::*;
        match self {
            Sig::Init => &[I64, I32, I32, I32, I32, I32],
            Sig::Message => &[I64, I32, I32, I32],
            Sig::Timer => &[I64, I32],
            Sig::Realloc => &[I32, I32, I32, I32],
        }
    }

    pub fn results(self) -> &'static [FlatType] {
        match self {
            Sig::Realloc => &[FlatType::I32],
            _ => &[],
        }
    }
}

/// The most slots a call carries.
pub const MAX_SLOTS: usize = 8;

/// Encode flat values into 64-bit slots: an `i32` sign-extended, an `i64` as is, floats as
/// their bits. Returns the number of slots, or `None` if the values do not fit the signature.
pub fn encode(sig: Sig, flats: &[Flat], out: &mut [u64; MAX_SLOTS]) -> Option<usize> {
    let params = sig.params();
    if flats.len() != params.len() {
        return None;
    }
    for (i, (f, t)) in flats.iter().zip(params).enumerate() {
        out[i] = match (f, t) {
            (Flat::I32(x), FlatType::I32) => *x as i64 as u64,
            (Flat::I64(x), FlatType::I64) => *x as u64,
            (Flat::F32(x), FlatType::F32) => x.to_bits() as u64,
            (Flat::F64(x), FlatType::F64) => x.to_bits(),
            _ => return None,
        };
    }
    Some(flats.len())
}

/// Decode a call's results from slots.
pub fn decode(sig: Sig, slots: &[u64]) -> Vec<Flat> {
    sig.results()
        .iter()
        .zip(slots)
        .map(|(t, s)| match t {
            FlatType::I32 => Flat::I32(*s as i32),
            FlatType::I64 => Flat::I64(*s as i64),
            FlatType::F32 => Flat::F32(f32::from_bits(*s as u32)),
            FlatType::F64 => Flat::F64(f64::from_bits(*s)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_map_to_their_signatures_and_slots_round_trip() {
        assert_eq!(
            Sig::of_export(core_world::EXPORT_ON_MESSAGE),
            Some(Sig::Message)
        );
        assert_eq!(Sig::of_export("other"), None);
        let mut slots = [0u64; MAX_SLOTS];
        let n = encode(
            Sig::Message,
            &[Flat::I64(-5), Flat::I32(-1), Flat::I32(4096), Flat::I32(64)],
            &mut slots,
        )
        .unwrap();
        assert_eq!(n, 4);
        assert_eq!(slots[0] as i64, -5);
        assert_eq!(slots[1] as i32, -1);
        assert_eq!(slots[2], 4096);
        assert_eq!(encode(Sig::Timer, &[Flat::I32(1)], &mut slots), None);
        assert_eq!(decode(Sig::Realloc, &[1024]), vec![Flat::I32(1024)]);
        assert!(decode(Sig::Timer, &[]).is_empty());
    }
}
