//! Lowering: Host values into the guest's memory and flat arguments (the Host calls an export, or
//! answers an import).

use crate::ty::{Flat, FlatType, MAX_FLAT_PARAMS, MAX_FLAT_RESULTS, Ty};
use crate::val::{Val, canonical_f32, canonical_f64};
use crate::{AbiError, Memory};

/// Store a value at `addr` in the layout of `ty`. Lists and strings are allocated with the guest's
/// allocator and referenced by pointer and length.
pub fn store(mem: &mut dyn Memory, ty: &Ty, val: &Val, addr: u32) -> Result<(), AbiError> {
    match (ty, val) {
        (Ty::Bool, Val::Bool(b)) => mem.write(addr, &[*b as u8]),
        (Ty::U8, Val::U8(x)) => mem.write(addr, &[*x]),
        (Ty::S8, Val::S8(x)) => mem.write(addr, &[*x as u8]),
        (Ty::U16, Val::U16(x)) => mem.write(addr, &x.to_le_bytes()),
        (Ty::S16, Val::S16(x)) => mem.write(addr, &x.to_le_bytes()),
        (Ty::U32, Val::U32(x)) => mem.write(addr, &x.to_le_bytes()),
        (Ty::S32, Val::S32(x)) => mem.write(addr, &x.to_le_bytes()),
        (Ty::U64, Val::U64(x)) => mem.write(addr, &x.to_le_bytes()),
        (Ty::S64, Val::S64(x)) => mem.write(addr, &x.to_le_bytes()),
        (Ty::F32, Val::F32(x)) => mem.write(addr, &canonical_f32(*x).to_bits().to_le_bytes()),
        (Ty::F64, Val::F64(x)) => mem.write(addr, &canonical_f64(*x).to_bits().to_le_bytes()),
        (Ty::Enum(cases), Val::Enum(d)) => {
            if d >= cases {
                return Err(AbiError::Discriminant {
                    found: *d,
                    cases: *cases,
                });
            }
            store_discriminant(mem, addr, *d, *cases)
        }
        (Ty::String, Val::String(s)) => {
            let (ptr, len) = lower_bytes(mem, s.as_bytes(), 1)?;
            mem.write(addr, &ptr.to_le_bytes())?;
            mem.write(addr + 4, &len.to_le_bytes())
        }
        (Ty::List(elem), Val::Bytes(b)) if matches!(elem.as_ref(), Ty::U8) => {
            let (ptr, len) = lower_bytes(mem, b, 1)?;
            mem.write(addr, &ptr.to_le_bytes())?;
            mem.write(addr + 4, &len.to_le_bytes())
        }
        (Ty::List(elem), Val::List(items)) => {
            let (ptr, len) = lower_list(mem, elem, items)?;
            mem.write(addr, &ptr.to_le_bytes())?;
            mem.write(addr + 4, &len.to_le_bytes())
        }
        (Ty::Record(fields), Val::Record(vals)) => {
            if fields.len() != vals.len() {
                return Err(AbiError::Shape {
                    expected: "record with the declared fields",
                });
            }
            let offsets = ty.field_offsets();
            for (((_, fty), v), off) in fields.iter().zip(vals).zip(offsets) {
                store(mem, fty, v, addr + off)?;
            }
            Ok(())
        }
        (Ty::Option(inner), Val::Option(v)) => {
            store_discriminant(mem, addr, v.is_some() as u32, 2)?;
            if let Some(v) = v {
                store(mem, inner, v, addr + ty.payload_offset())?;
            }
            Ok(())
        }
        (Ty::Result(ok, err), Val::Result(r)) => {
            let off = ty.payload_offset();
            match r {
                Ok(p) => {
                    store_discriminant(mem, addr, 0, 2)?;
                    match (ok, p) {
                        (Some(t), Some(v)) => store(mem, t, v, addr + off),
                        (None, None) => Ok(()),
                        _ => Err(AbiError::Shape {
                            expected: "result payload as declared",
                        }),
                    }
                }
                Err(p) => {
                    store_discriminant(mem, addr, 1, 2)?;
                    match (err, p) {
                        (Some(t), Some(v)) => store(mem, t, v, addr + off),
                        (None, None) => Ok(()),
                        _ => Err(AbiError::Shape {
                            expected: "result payload as declared",
                        }),
                    }
                }
            }
        }
        (Ty::Variant(cases), Val::Variant(d, p)) => {
            let n = cases.len() as u32;
            if *d >= n {
                return Err(AbiError::Discriminant {
                    found: *d,
                    cases: n,
                });
            }
            store_discriminant(mem, addr, *d, n)?;
            match (&cases[*d as usize].1, p) {
                (Some(t), Some(v)) => store(mem, t, v, addr + ty.payload_offset()),
                (None, None) => Ok(()),
                _ => Err(AbiError::Shape {
                    expected: "variant payload as declared",
                }),
            }
        }
        _ => Err(AbiError::Shape {
            expected: "value of the declared type",
        }),
    }
}

fn store_discriminant(mem: &mut dyn Memory, addr: u32, d: u32, cases: u32) -> Result<(), AbiError> {
    match crate::ty::discriminant_size(cases) {
        1 => mem.write(addr, &[d as u8]),
        2 => mem.write(addr, &(d as u16).to_le_bytes()),
        _ => mem.write(addr, &d.to_le_bytes()),
    }
}

/// Allocate and write bytes; returns (pointer, length). An empty list needs no allocation.
pub fn lower_bytes(mem: &mut dyn Memory, bytes: &[u8], align: u32) -> Result<(u32, u32), AbiError> {
    let len = bytes.len() as u32;
    if len == 0 {
        return Ok((align, 0));
    }
    let ptr = mem.realloc(0, 0, align, len)?;
    if ptr.checked_add(len).is_none_or(|end| end > mem.size()) {
        return Err(AbiError::Realloc { requested: len });
    }
    mem.write(ptr, bytes)?;
    Ok((ptr, len))
}

/// Allocate and write a list of elements; returns (pointer, length in elements).
pub fn lower_list(mem: &mut dyn Memory, elem: &Ty, items: &[Val]) -> Result<(u32, u32), AbiError> {
    if let Ty::U8 = elem {
        let bytes: Vec<u8> = items
            .iter()
            .map(|v| {
                if let Val::U8(b) = v {
                    Ok(*b)
                } else {
                    Err(AbiError::Shape { expected: "u8" })
                }
            })
            .collect::<Result<_, _>>()?;
        return lower_bytes(mem, &bytes, 1);
    }
    let len = items.len() as u32;
    if len == 0 {
        return Ok((elem.align(), 0));
    }
    let bytes = len
        .checked_mul(elem.size())
        .ok_or(AbiError::Length { len })?;
    let ptr = mem.realloc(0, 0, elem.align(), bytes)?;
    if ptr.checked_add(bytes).is_none_or(|end| end > mem.size()) {
        return Err(AbiError::Realloc { requested: bytes });
    }
    for (i, v) in items.iter().enumerate() {
        store(mem, elem, v, ptr + i as u32 * elem.size())?;
    }
    Ok((ptr, len))
}

/// Lower a value to its flat values, allocating lists and strings in memory.
pub fn lower(
    mem: &mut dyn Memory,
    ty: &Ty,
    val: &Val,
    out: &mut Vec<Flat>,
) -> Result<(), AbiError> {
    match (ty, val) {
        (Ty::Bool, Val::Bool(b)) => out.push(Flat::I32(*b as i32)),
        (Ty::U8, Val::U8(x)) => out.push(Flat::I32(*x as i32)),
        (Ty::U16, Val::U16(x)) => out.push(Flat::I32(*x as i32)),
        (Ty::U32, Val::U32(x)) => out.push(Flat::I32(*x as i32)),
        (Ty::S8, Val::S8(x)) => out.push(Flat::I32(*x as i32)),
        (Ty::S16, Val::S16(x)) => out.push(Flat::I32(*x as i32)),
        (Ty::S32, Val::S32(x)) => out.push(Flat::I32(*x)),
        (Ty::U64, Val::U64(x)) => out.push(Flat::I64(*x as i64)),
        (Ty::S64, Val::S64(x)) => out.push(Flat::I64(*x)),
        (Ty::F32, Val::F32(x)) => out.push(Flat::F32(canonical_f32(*x))),
        (Ty::F64, Val::F64(x)) => out.push(Flat::F64(canonical_f64(*x))),
        (Ty::Enum(cases), Val::Enum(d)) => {
            if d >= cases {
                return Err(AbiError::Discriminant {
                    found: *d,
                    cases: *cases,
                });
            }
            out.push(Flat::I32(*d as i32));
        }
        (Ty::String, Val::String(s)) => {
            let (ptr, len) = lower_bytes(mem, s.as_bytes(), 1)?;
            out.push(Flat::I32(ptr as i32));
            out.push(Flat::I32(len as i32));
        }
        (Ty::List(elem), Val::Bytes(b)) if matches!(elem.as_ref(), Ty::U8) => {
            let (ptr, len) = lower_bytes(mem, b, 1)?;
            out.push(Flat::I32(ptr as i32));
            out.push(Flat::I32(len as i32));
        }
        (Ty::List(elem), Val::List(items)) => {
            let (ptr, len) = lower_list(mem, elem, items)?;
            out.push(Flat::I32(ptr as i32));
            out.push(Flat::I32(len as i32));
        }
        (Ty::Record(fields), Val::Record(vals)) => {
            if fields.len() != vals.len() {
                return Err(AbiError::Shape {
                    expected: "record with the declared fields",
                });
            }
            for ((_, fty), v) in fields.iter().zip(vals) {
                lower(mem, fty, v, out)?;
            }
        }
        (Ty::Option(inner), Val::Option(v)) => {
            out.push(Flat::I32(v.is_some() as i32));
            let flats = inner.flat_types();
            match v {
                Some(v) => lower(mem, inner, v, out)?,
                None => out.extend(flats.iter().map(zero_of)),
            }
        }
        (Ty::Result(..), Val::Result(r)) => {
            let (d, payload_ty, payload) = match (ty, r) {
                (Ty::Result(ok, _), Ok(p)) => (0, ok.as_deref(), p.as_deref()),
                (Ty::Result(_, err), Err(p)) => (1, err.as_deref(), p.as_deref()),
                _ => unreachable!(),
            };
            out.push(Flat::I32(d));
            lower_joined_payload(mem, ty, payload_ty, payload, out)?;
        }
        (Ty::Variant(cases), Val::Variant(d, p)) => {
            let n = cases.len() as u32;
            if *d >= n {
                return Err(AbiError::Discriminant {
                    found: *d,
                    cases: n,
                });
            }
            out.push(Flat::I32(*d as i32));
            lower_joined_payload(mem, ty, cases[*d as usize].1.as_ref(), p.as_deref(), out)?;
        }
        _ => {
            return Err(AbiError::Shape {
                expected: "value of the declared type",
            });
        }
    }
    Ok(())
}

/// A payload of a result or variant, lowered into the joined flat slots: each flat is widened to
/// the joined type, and unused slots are zero.
fn lower_joined_payload(
    mem: &mut dyn Memory,
    ty: &Ty,
    payload_ty: Option<&Ty>,
    payload: Option<&Val>,
    out: &mut Vec<Flat>,
) -> Result<(), AbiError> {
    let joined = &ty.flat_types()[1..];
    let mut own = Vec::new();
    match (payload_ty, payload) {
        (Some(t), Some(v)) => lower(mem, t, v, &mut own)?,
        (None, None) => {}
        _ => {
            return Err(AbiError::Shape {
                expected: "payload as declared",
            });
        }
    }
    for (i, jt) in joined.iter().enumerate() {
        out.push(match own.get(i) {
            Some(f) => widen(*f, *jt),
            None => zero_of(jt),
        });
    }
    Ok(())
}

fn zero_of(t: &FlatType) -> Flat {
    match t {
        FlatType::I32 => Flat::I32(0),
        FlatType::I64 => Flat::I64(0),
        FlatType::F32 => Flat::F32(0.0),
        FlatType::F64 => Flat::F64(0.0),
    }
}

/// Widen a flat value to the joined type (the canonical ABI's rules: f32→i32 by bits, i32→i64
/// zero-extended, f64→i64 by bits, f32→i64 by bits of the i32).
pub fn widen(f: Flat, to: FlatType) -> Flat {
    match (f, to) {
        (Flat::I32(x), FlatType::I32) => Flat::I32(x),
        (Flat::F32(x), FlatType::I32) => Flat::I32(x.to_bits() as i32),
        (Flat::I32(x), FlatType::I64) => Flat::I64(x as u32 as i64),
        (Flat::F32(x), FlatType::I64) => Flat::I64(x.to_bits() as i64),
        (Flat::F64(x), FlatType::I64) => Flat::I64(x.to_bits() as i64),
        (Flat::I64(x), FlatType::I64) => Flat::I64(x),
        (f, _) => f,
    }
}

/// Lower a call's parameters: flat when at most `MAX_FLAT_PARAMS` flats, else spilled into one
/// record in memory and passed as a single pointer.
pub fn lower_flat_params(
    mem: &mut dyn Memory,
    tys: &[Ty],
    vals: &[Val],
) -> Result<Vec<Flat>, AbiError> {
    if tys.len() != vals.len() {
        return Err(AbiError::Shape {
            expected: "one value per parameter",
        });
    }
    let count: usize = tys.iter().map(Ty::flat_count).sum();
    if count > MAX_FLAT_PARAMS {
        let record = Ty::Record(
            tys.iter()
                .enumerate()
                .map(|(i, t)| (format!("p{i}"), t.clone()))
                .collect(),
        );
        let ptr = mem.realloc(0, 0, record.align(), record.size())?;
        store(mem, &record, &Val::Record(vals.to_vec()), ptr)?;
        return Ok(vec![Flat::I32(ptr as i32)]);
    }
    let mut out = Vec::with_capacity(count);
    for (t, v) in tys.iter().zip(vals) {
        lower(mem, t, v, &mut out)?;
    }
    Ok(out)
}

/// Lower the result of an import the Host answers: one flat value, or `None` for no result. A
/// result of more than one flat (a string, a list) would need a return area, which no core
/// import has; it is a shape error here until a contract needs it.
pub fn lower_result(
    mem: &mut dyn Memory,
    ty: Option<&Ty>,
    val: Option<&Val>,
) -> Result<Option<Flat>, AbiError> {
    match (ty, val) {
        (None, None) => Ok(None),
        (Some(t), Some(v)) => {
            if t.flat_count() > MAX_FLAT_RESULTS {
                return Err(AbiError::Shape {
                    expected: "a result of one flat value",
                });
            }
            let mut out = Vec::with_capacity(1);
            lower(mem, t, v, &mut out)?;
            Ok(out.pop())
        }
        _ => Err(AbiError::Shape {
            expected: "result as declared",
        }),
    }
}
