//! Canonical ABI lifting and lowering of the type subset of RFC 002 §2, shared by both engine
//! adapters. The adapters give this crate a [`Memory`] (the instance's linear memory and its
//! `cabi_realloc`); this crate turns values into flat arguments and back, with the strict checks of
//! RFC 002 rule 8 (every failure is the `contract` fault) and the canonical NaN of rule 9.
//!
//! The one place besides the adapters where `unsafe` may appear, with the invariant stated at each
//! use — none is needed so far.
//!
//! Implemented by PR-04 of `rfcs/handoffs/005-host-architecture/task-breakdown-pr-plan.md`.

pub mod core_world;
mod lift;
mod lower;
mod ty;
mod val;

pub use lift::{lift_flat_params, lift_results, load};
pub use lower::{lower_flat_params, lower_result, store};
pub use ty::{Flat, FlatType, MAX_FLAT_PARAMS, MAX_FLAT_RESULTS, Ty};
pub use val::{Val, canonical_f32, canonical_f64};

use std::fmt;

/// The guest's linear memory and allocator, as the adapter exposes them. Every address and length
/// is checked against `size()` before use; a failure is never a panic (RFC 005 rule 5).
pub trait Memory {
    /// The current size in bytes.
    fn size(&self) -> u32;
    fn read(&self, addr: u32, len: u32) -> Result<Vec<u8>, AbiError>;
    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), AbiError>;
    /// `cabi_realloc(old_ptr, old_size, align, new_size)` of the guest (RFC 002 rule 2).
    fn realloc(
        &mut self,
        old_ptr: u32,
        old_size: u32,
        align: u32,
        new_size: u32,
    ) -> Result<u32, AbiError>;
}

/// Why lifting or lowering failed: each is a contract violation (RFC 002 rule 8), with the rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbiError {
    /// A range lies outside the memory.
    OutOfRange { addr: u32, len: u32, size: u32 },
    /// A string's bytes are not valid UTF-8.
    InvalidUtf8 { addr: u32, len: u32 },
    /// An enumeration or discriminant is out of range.
    Discriminant { found: u32, cases: u32 },
    /// The guest's allocator failed or returned an address outside memory.
    Realloc { requested: u32 },
    /// The flat values do not match the type (an adapter or binding defect, not a guest's).
    Shape { expected: &'static str },
    /// A list is longer than the memory could hold.
    Length { len: u32 },
}

impl AbiError {
    /// The rule a violation breaks, for `fault.contract`'s parameter.
    pub fn rule(&self) -> &'static str {
        "RFC 002 rule 8"
    }
}

impl fmt::Display for AbiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbiError::OutOfRange { addr, len, size } => {
                write!(f, "range {addr}+{len} outside memory of {size} bytes")
            }
            AbiError::InvalidUtf8 { addr, len } => write!(f, "invalid UTF-8 at {addr}+{len}"),
            AbiError::Discriminant { found, cases } => {
                write!(f, "discriminant {found} of {cases} cases")
            }
            AbiError::Realloc { requested } => {
                write!(f, "cabi_realloc failed for {requested} bytes")
            }
            AbiError::Shape { expected } => {
                write!(f, "flat values do not match: expected {expected}")
            }
            AbiError::Length { len } => write!(f, "list length {len} exceeds memory"),
        }
    }
}

impl std::error::Error for AbiError {}

/// A memory in a `Vec<u8>` with a bump allocator: for tests, and for the ABI's own fixtures.
#[derive(Clone, Debug)]
pub struct VecMemory {
    pub bytes: Vec<u8>,
    next: u32,
}

impl VecMemory {
    pub fn new(size: u32) -> Self {
        Self {
            bytes: vec![0; size as usize],
            next: 8,
        }
    }
}

impl Memory for VecMemory {
    fn size(&self) -> u32 {
        self.bytes.len() as u32
    }
    fn read(&self, addr: u32, len: u32) -> Result<Vec<u8>, AbiError> {
        let end = addr.checked_add(len).ok_or(AbiError::OutOfRange {
            addr,
            len,
            size: self.size(),
        })?;
        if end > self.size() {
            return Err(AbiError::OutOfRange {
                addr,
                len,
                size: self.size(),
            });
        }
        Ok(self.bytes[addr as usize..end as usize].to_vec())
    }
    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), AbiError> {
        let len = data.len() as u32;
        let end = addr.checked_add(len).ok_or(AbiError::OutOfRange {
            addr,
            len,
            size: self.size(),
        })?;
        if end > self.size() {
            return Err(AbiError::OutOfRange {
                addr,
                len,
                size: self.size(),
            });
        }
        self.bytes[addr as usize..end as usize].copy_from_slice(data);
        Ok(())
    }
    fn realloc(
        &mut self,
        _old_ptr: u32,
        _old_size: u32,
        align: u32,
        new_size: u32,
    ) -> Result<u32, AbiError> {
        let align = align.max(1);
        let start = self.next.div_ceil(align) * align;
        let end = start.checked_add(new_size).ok_or(AbiError::Realloc {
            requested: new_size,
        })?;
        if end > self.size() {
            return Err(AbiError::Realloc {
                requested: new_size,
            });
        }
        self.next = end;
        Ok(start)
    }
}
