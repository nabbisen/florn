//! Lifting: the guest's flat arguments and memory into Host values (the guest calls an import, or
//! an export returns). Strict: a range outside memory, invalid UTF-8, or a discriminant out of
//! range is an [`AbiError`], which the Host turns into the `contract` fault (RFC 002 rule 8).

use crate::ty::{Flat, FlatType, MAX_FLAT_PARAMS, MAX_FLAT_RESULTS, Ty};
use crate::val::{Val, canonical_f32, canonical_f64};
use crate::{AbiError, Memory};

/// Load a value of `ty` at `addr`.
pub fn load(mem: &dyn Memory, ty: &Ty, addr: u32) -> Result<Val, AbiError> {
    let read = |len: u32| mem.read(addr, len);
    Ok(match ty {
        Ty::Bool => Val::Bool(read(1)?[0] != 0),
        Ty::U8 => Val::U8(read(1)?[0]),
        Ty::S8 => Val::S8(read(1)?[0] as i8),
        Ty::U16 => Val::U16(u16::from_le_bytes(read(2)?.try_into().unwrap())),
        Ty::S16 => Val::S16(i16::from_le_bytes(read(2)?.try_into().unwrap())),
        Ty::U32 => Val::U32(u32::from_le_bytes(read(4)?.try_into().unwrap())),
        Ty::S32 => Val::S32(i32::from_le_bytes(read(4)?.try_into().unwrap())),
        Ty::U64 => Val::U64(u64::from_le_bytes(read(8)?.try_into().unwrap())),
        Ty::S64 => Val::S64(i64::from_le_bytes(read(8)?.try_into().unwrap())),
        Ty::F32 => Val::F32(canonical_f32(f32::from_bits(u32::from_le_bytes(
            read(4)?.try_into().unwrap(),
        )))),
        Ty::F64 => Val::F64(canonical_f64(f64::from_bits(u64::from_le_bytes(
            read(8)?.try_into().unwrap(),
        )))),
        Ty::Enum(cases) => {
            let d = load_discriminant(mem, addr, *cases)?;
            if d >= *cases {
                return Err(AbiError::Discriminant {
                    found: d,
                    cases: *cases,
                });
            }
            Val::Enum(d)
        }
        Ty::String => {
            let ptr = u32::from_le_bytes(mem.read(addr, 4)?.try_into().unwrap());
            let len = u32::from_le_bytes(mem.read(addr + 4, 4)?.try_into().unwrap());
            lift_string(mem, ptr, len)?
        }
        Ty::List(elem) => {
            let ptr = u32::from_le_bytes(mem.read(addr, 4)?.try_into().unwrap());
            let len = u32::from_le_bytes(mem.read(addr + 4, 4)?.try_into().unwrap());
            lift_list(mem, elem, ptr, len)?
        }
        Ty::Record(fields) => {
            let offsets = ty.field_offsets();
            let mut vals = Vec::with_capacity(fields.len());
            for ((_, fty), off) in fields.iter().zip(offsets) {
                vals.push(load(mem, fty, addr + off)?);
            }
            Val::Record(vals)
        }
        Ty::Option(inner) => {
            let d = load_discriminant(mem, addr, 2)?;
            match d {
                0 => Val::Option(None),
                1 => Val::Option(Some(Box::new(load(
                    mem,
                    inner,
                    addr + ty.payload_offset(),
                )?))),
                _ => return Err(AbiError::Discriminant { found: d, cases: 2 }),
            }
        }
        Ty::Result(ok, err) => {
            let d = load_discriminant(mem, addr, 2)?;
            let off = addr + ty.payload_offset();
            match d {
                0 => Val::Result(Ok(match ok {
                    Some(t) => Some(Box::new(load(mem, t, off)?)),
                    None => None,
                })),
                1 => Val::Result(Err(match err {
                    Some(t) => Some(Box::new(load(mem, t, off)?)),
                    None => None,
                })),
                _ => return Err(AbiError::Discriminant { found: d, cases: 2 }),
            }
        }
        Ty::Variant(cases) => {
            let n = cases.len() as u32;
            let d = load_discriminant(mem, addr, n)?;
            if d >= n {
                return Err(AbiError::Discriminant { found: d, cases: n });
            }
            let payload = match &cases[d as usize].1 {
                Some(t) => Some(Box::new(load(mem, t, addr + ty.payload_offset())?)),
                None => None,
            };
            Val::Variant(d, payload)
        }
    })
}

fn load_discriminant(mem: &dyn Memory, addr: u32, cases: u32) -> Result<u32, AbiError> {
    Ok(match crate::ty::discriminant_size(cases) {
        1 => mem.read(addr, 1)?[0] as u32,
        2 => u16::from_le_bytes(mem.read(addr, 2)?.try_into().unwrap()) as u32,
        _ => u32::from_le_bytes(mem.read(addr, 4)?.try_into().unwrap()),
    })
}

pub fn lift_string(mem: &dyn Memory, ptr: u32, len: u32) -> Result<Val, AbiError> {
    let bytes = mem.read(ptr, len)?;
    String::from_utf8(bytes)
        .map(Val::String)
        .map_err(|_| AbiError::InvalidUtf8 { addr: ptr, len })
}

pub fn lift_list(mem: &dyn Memory, elem: &Ty, ptr: u32, len: u32) -> Result<Val, AbiError> {
    if let Ty::U8 = elem {
        return Ok(Val::Bytes(mem.read(ptr, len)?));
    }
    let bytes = len
        .checked_mul(elem.size())
        .ok_or(AbiError::Length { len })?;
    if ptr.checked_add(bytes).is_none_or(|end| end > mem.size()) {
        return Err(AbiError::OutOfRange {
            addr: ptr,
            len: bytes,
            size: mem.size(),
        });
    }
    let mut items = Vec::with_capacity(len.min(1 << 20) as usize);
    for i in 0..len {
        items.push(load(mem, elem, ptr + i * elem.size())?);
    }
    Ok(Val::List(items))
}

/// Lift one value of `ty` from flat values starting at `flats[*pos]`.
pub fn lift(mem: &dyn Memory, ty: &Ty, flats: &[Flat], pos: &mut usize) -> Result<Val, AbiError> {
    fn next(flats: &[Flat], pos: &mut usize) -> Result<Flat, AbiError> {
        let f = *flats.get(*pos).ok_or(AbiError::Shape {
            expected: "enough flat values",
        })?;
        *pos += 1;
        Ok(f)
    }
    fn i32_of(f: Flat) -> Result<i32, AbiError> {
        match f {
            Flat::I32(x) => Ok(x),
            _ => Err(AbiError::Shape { expected: "i32" }),
        }
    }
    fn i64_of(f: Flat) -> Result<i64, AbiError> {
        match f {
            Flat::I64(x) => Ok(x),
            _ => Err(AbiError::Shape { expected: "i64" }),
        }
    }
    Ok(match ty {
        Ty::Bool => Val::Bool(i32_of(next(flats, pos)?)? != 0),
        Ty::U8 => Val::U8(i32_of(next(flats, pos)?)? as u8),
        Ty::U16 => Val::U16(i32_of(next(flats, pos)?)? as u16),
        Ty::U32 => Val::U32(i32_of(next(flats, pos)?)? as u32),
        Ty::S8 => Val::S8(i32_of(next(flats, pos)?)? as i8),
        Ty::S16 => Val::S16(i32_of(next(flats, pos)?)? as i16),
        Ty::S32 => Val::S32(i32_of(next(flats, pos)?)?),
        Ty::U64 => Val::U64(i64_of(next(flats, pos)?)? as u64),
        Ty::S64 => Val::S64(i64_of(next(flats, pos)?)?),
        Ty::F32 => match next(flats, pos)? {
            Flat::F32(x) => Val::F32(canonical_f32(x)),
            _ => return Err(AbiError::Shape { expected: "f32" }),
        },
        Ty::F64 => match next(flats, pos)? {
            Flat::F64(x) => Val::F64(canonical_f64(x)),
            _ => return Err(AbiError::Shape { expected: "f64" }),
        },
        Ty::Enum(cases) => {
            let d = i32_of(next(flats, pos)?)? as u32;
            if d >= *cases {
                return Err(AbiError::Discriminant {
                    found: d,
                    cases: *cases,
                });
            }
            Val::Enum(d)
        }
        Ty::String => {
            let ptr = i32_of(next(flats, pos)?)? as u32;
            let len = i32_of(next(flats, pos)?)? as u32;
            lift_string(mem, ptr, len)?
        }
        Ty::List(elem) => {
            let ptr = i32_of(next(flats, pos)?)? as u32;
            let len = i32_of(next(flats, pos)?)? as u32;
            lift_list(mem, elem, ptr, len)?
        }
        Ty::Record(fields) => {
            let mut vals = Vec::with_capacity(fields.len());
            for (_, fty) in fields {
                vals.push(lift(mem, fty, flats, pos)?);
            }
            Val::Record(vals)
        }
        Ty::Option(inner) => {
            let d = i32_of(next(flats, pos)?)? as u32;
            let n = inner.flat_count();
            match d {
                0 => {
                    *pos += n;
                    Val::Option(None)
                }
                1 => Val::Option(Some(Box::new(lift(mem, inner, flats, pos)?))),
                _ => return Err(AbiError::Discriminant { found: d, cases: 2 }),
            }
        }
        Ty::Result(ok, err) => {
            let d = i32_of(next(flats, pos)?)? as u32;
            let joined = ty.flat_count() - 1;
            let start = *pos;
            let v = match d {
                0 => Val::Result(Ok(lift_joined_payload(
                    mem,
                    ok.as_deref(),
                    flats,
                    start,
                    joined,
                )?)),
                1 => Val::Result(Err(lift_joined_payload(
                    mem,
                    err.as_deref(),
                    flats,
                    start,
                    joined,
                )?)),
                _ => return Err(AbiError::Discriminant { found: d, cases: 2 }),
            };
            *pos = start + joined;
            v
        }
        Ty::Variant(cases) => {
            let d = i32_of(next(flats, pos)?)? as u32;
            let n = cases.len() as u32;
            if d >= n {
                return Err(AbiError::Discriminant { found: d, cases: n });
            }
            let joined = ty.flat_count() - 1;
            let start = *pos;
            let payload =
                lift_joined_payload(mem, cases[d as usize].1.as_ref(), flats, start, joined)?;
            *pos = start + joined;
            Val::Variant(d, payload)
        }
    })
}

/// A payload stored in joined flat slots: narrow each joined flat back to the payload's own type.
fn lift_joined_payload(
    mem: &dyn Memory,
    payload_ty: Option<&Ty>,
    flats: &[Flat],
    start: usize,
    joined: usize,
) -> Result<Option<Box<Val>>, AbiError> {
    let Some(t) = payload_ty else { return Ok(None) };
    let own_types = t.flat_types();
    if own_types.len() > joined {
        return Err(AbiError::Shape {
            expected: "joined flats to hold the payload",
        });
    }
    let narrowed: Vec<Flat> = own_types
        .iter()
        .enumerate()
        .map(|(i, ot)| {
            let f = *flats.get(start + i).ok_or(AbiError::Shape {
                expected: "enough flat values",
            })?;
            Ok(narrow(f, *ot))
        })
        .collect::<Result<_, AbiError>>()?;
    let mut p = 0;
    Ok(Some(Box::new(lift(mem, t, &narrowed, &mut p)?)))
}

/// The inverse of `widen`.
fn narrow(f: Flat, to: FlatType) -> Flat {
    match (f, to) {
        (Flat::I32(x), FlatType::F32) => Flat::F32(f32::from_bits(x as u32)),
        (Flat::I64(x), FlatType::I32) => Flat::I32(x as u32 as i32),
        (Flat::I64(x), FlatType::F32) => Flat::F32(f32::from_bits(x as u32)),
        (Flat::I64(x), FlatType::F64) => Flat::F64(f64::from_bits(x as u64)),
        (f, _) => f,
    }
}

/// Lift a call's parameters as the guest passed them to an import: flat, or through one pointer
/// when they exceed `MAX_FLAT_PARAMS`.
pub fn lift_flat_params(
    mem: &dyn Memory,
    tys: &[Ty],
    flats: &[Flat],
) -> Result<Vec<Val>, AbiError> {
    let count: usize = tys.iter().map(Ty::flat_count).sum();
    if count > MAX_FLAT_PARAMS {
        let record = Ty::Record(
            tys.iter()
                .enumerate()
                .map(|(i, t)| (format!("p{i}"), t.clone()))
                .collect(),
        );
        let ptr = match flats {
            [Flat::I32(p)] => *p as u32,
            _ => {
                return Err(AbiError::Shape {
                    expected: "one pointer to the spilled parameters",
                });
            }
        };
        return match load(mem, &record, ptr)? {
            Val::Record(vals) => Ok(vals),
            _ => unreachable!(),
        };
    }
    if flats.len() != count {
        return Err(AbiError::Shape {
            expected: "the declared number of flat parameters",
        });
    }
    let mut pos = 0;
    let mut vals = Vec::with_capacity(tys.len());
    for t in tys {
        vals.push(lift(mem, t, flats, &mut pos)?);
    }
    Ok(vals)
}

/// Lift the results of an export: one flat value directly, or, when the result has more flats
/// than `MAX_FLAT_RESULTS`, a pointer to a return area in the guest's memory (the caller then calls
/// `cabi_post_*`, RFC 002 rule 2).
pub fn lift_results(
    mem: &dyn Memory,
    ty: Option<&Ty>,
    flats: &[Flat],
) -> Result<Option<Val>, AbiError> {
    let Some(t) = ty else {
        return if flats.is_empty() {
            Ok(None)
        } else {
            Err(AbiError::Shape {
                expected: "no result",
            })
        };
    };
    if t.flat_count() > MAX_FLAT_RESULTS {
        let ptr = match flats {
            [Flat::I32(p)] => *p as u32,
            _ => {
                return Err(AbiError::Shape {
                    expected: "one pointer to the return area",
                });
            }
        };
        return load(mem, t, ptr).map(Some);
    }
    let mut pos = 0;
    let v = lift(mem, t, flats, &mut pos)?;
    if pos != flats.len() {
        return Err(AbiError::Shape {
            expected: "the declared number of flat results",
        });
    }
    Ok(Some(v))
}
