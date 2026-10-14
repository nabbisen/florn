//! The type subset of RFC 002 rule 7, with the canonical ABI's layout: size and alignment in
//! memory, and the flat (core WebAssembly) representation across a call.

/// A type of the subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Bool,
    U8,
    U16,
    U32,
    U64,
    S8,
    S16,
    S32,
    S64,
    F32,
    F64,
    String,
    List(Box<Ty>),
    /// An enumeration with this many cases.
    Enum(u32),
    /// Fields in declared order.
    Record(Vec<(String, Ty)>),
    Option(Box<Ty>),
    /// `result<T, E>`; either side may carry no payload.
    Result(Option<Box<Ty>>, Option<Box<Ty>>),
    /// Cases in declared order, each with an optional payload.
    Variant(Vec<(String, Option<Ty>)>),
}

/// A flat core type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatType {
    I32,
    I64,
    F32,
    F64,
}

/// A flat value as it crosses a call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flat {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl Flat {
    pub fn ty(&self) -> FlatType {
        match self {
            Flat::I32(_) => FlatType::I32,
            Flat::I64(_) => FlatType::I64,
            Flat::F32(_) => FlatType::F32,
            Flat::F64(_) => FlatType::F64,
        }
    }
}

/// The canonical ABI's limits: beyond them, parameters are passed through memory and results
/// through a return pointer.
pub const MAX_FLAT_PARAMS: usize = 16;
pub const MAX_FLAT_RESULTS: usize = 1;

impl Ty {
    pub fn list(t: Ty) -> Ty {
        Ty::List(Box::new(t))
    }
    pub fn option(t: Ty) -> Ty {
        Ty::Option(Box::new(t))
    }
    pub fn result(ok: Option<Ty>, err: Option<Ty>) -> Ty {
        Ty::Result(ok.map(Box::new), err.map(Box::new))
    }
    pub fn record(fields: &[(&str, Ty)]) -> Ty {
        Ty::Record(
            fields
                .iter()
                .map(|(n, t)| (n.to_string(), t.clone()))
                .collect(),
        )
    }
    pub fn variant(cases: &[(&str, Option<Ty>)]) -> Ty {
        Ty::Variant(
            cases
                .iter()
                .map(|(n, t)| (n.to_string(), t.clone()))
                .collect(),
        )
    }

    /// Alignment in memory.
    pub fn align(&self) -> u32 {
        match self {
            Ty::Bool | Ty::U8 | Ty::S8 => 1,
            Ty::U16 | Ty::S16 => 2,
            Ty::U32 | Ty::S32 | Ty::F32 => 4,
            Ty::U64 | Ty::S64 | Ty::F64 => 8,
            Ty::String | Ty::List(_) => 4,
            Ty::Enum(n) => discriminant_size(*n),
            Ty::Record(fields) => fields.iter().map(|(_, t)| t.align()).max().unwrap_or(1),
            Ty::Option(t) => t.align().max(1),
            Ty::Result(ok, err) => discriminant_size(2)
                .max(ok.as_ref().map_or(1, |t| t.align()))
                .max(err.as_ref().map_or(1, |t| t.align())),
            Ty::Variant(cases) => discriminant_size(cases.len() as u32).max(
                cases
                    .iter()
                    .filter_map(|(_, t)| t.as_ref().map(|t| t.align()))
                    .max()
                    .unwrap_or(1),
            ),
        }
    }

    /// Size in memory, padded to the alignment.
    pub fn size(&self) -> u32 {
        match self {
            Ty::Bool | Ty::U8 | Ty::S8 => 1,
            Ty::U16 | Ty::S16 => 2,
            Ty::U32 | Ty::S32 | Ty::F32 => 4,
            Ty::U64 | Ty::S64 | Ty::F64 => 8,
            Ty::String | Ty::List(_) => 8,
            Ty::Enum(n) => discriminant_size(*n),
            Ty::Record(fields) => {
                let mut offset = 0;
                for (_, t) in fields {
                    offset = align_to(offset, t.align()) + t.size();
                }
                align_to(offset, self.align())
            }
            Ty::Option(t) => align_to(align_to(1, t.align()) + t.size(), self.align()),
            Ty::Result(ok, err) => {
                let payload = ok
                    .as_ref()
                    .map_or(0, |t| t.size())
                    .max(err.as_ref().map_or(0, |t| t.size()));
                let payload_align = ok
                    .as_ref()
                    .map_or(1, |t| t.align())
                    .max(err.as_ref().map_or(1, |t| t.align()));
                align_to(
                    align_to(discriminant_size(2), payload_align) + payload,
                    self.align(),
                )
            }
            Ty::Variant(cases) => {
                let payload = cases
                    .iter()
                    .filter_map(|(_, t)| t.as_ref().map(|t| t.size()))
                    .max()
                    .unwrap_or(0);
                let payload_align = cases
                    .iter()
                    .filter_map(|(_, t)| t.as_ref().map(|t| t.align()))
                    .max()
                    .unwrap_or(1);
                align_to(
                    align_to(discriminant_size(cases.len() as u32), payload_align) + payload,
                    self.align(),
                )
            }
        }
    }

    /// The field offsets of a record.
    pub fn field_offsets(&self) -> Vec<u32> {
        let Ty::Record(fields) = self else {
            return Vec::new();
        };
        let mut offset = 0;
        let mut out = Vec::with_capacity(fields.len());
        for (_, t) in fields {
            offset = align_to(offset, t.align());
            out.push(offset);
            offset += t.size();
        }
        out
    }

    /// The offset of a payload after a discriminant of `n` cases, aligned for the payload.
    pub fn payload_offset(&self) -> u32 {
        match self {
            Ty::Option(t) => align_to(1, t.align()),
            Ty::Result(ok, err) => {
                let a = ok
                    .as_ref()
                    .map_or(1, |t| t.align())
                    .max(err.as_ref().map_or(1, |t| t.align()));
                align_to(discriminant_size(2), a)
            }
            Ty::Variant(cases) => {
                let a = cases
                    .iter()
                    .filter_map(|(_, t)| t.as_ref().map(|t| t.align()))
                    .max()
                    .unwrap_or(1);
                align_to(discriminant_size(cases.len() as u32), a)
            }
            _ => 0,
        }
    }

    /// The flat types this type lowers to.
    pub fn flat_types(&self) -> Vec<FlatType> {
        match self {
            Ty::Bool | Ty::U8 | Ty::U16 | Ty::U32 | Ty::S8 | Ty::S16 | Ty::S32 | Ty::Enum(_) => {
                vec![FlatType::I32]
            }
            Ty::U64 | Ty::S64 => vec![FlatType::I64],
            Ty::F32 => vec![FlatType::F32],
            Ty::F64 => vec![FlatType::F64],
            Ty::String | Ty::List(_) => vec![FlatType::I32, FlatType::I32],
            Ty::Record(fields) => fields.iter().flat_map(|(_, t)| t.flat_types()).collect(),
            Ty::Option(t) => {
                let mut v = vec![FlatType::I32];
                v.extend(t.flat_types());
                v
            }
            Ty::Result(ok, err) => {
                let mut v = vec![FlatType::I32];
                v.extend(join_flats(&[
                    ok.as_deref().map_or(Vec::new(), Ty::flat_types),
                    err.as_deref().map_or(Vec::new(), Ty::flat_types),
                ]));
                v
            }
            Ty::Variant(cases) => {
                let mut v = vec![FlatType::I32];
                v.extend(join_flats(
                    &cases
                        .iter()
                        .map(|(_, t)| t.as_ref().map_or(Vec::new(), Ty::flat_types))
                        .collect::<Vec<_>>(),
                ));
                v
            }
        }
    }

    pub fn flat_count(&self) -> usize {
        self.flat_types().len()
    }
}

/// The canonical ABI's join of the flat types of several cases: position by position, equal types
/// stay, i32/f32 join to i32, anything else joins to i64.
pub fn join_flats(cases: &[Vec<FlatType>]) -> Vec<FlatType> {
    let n = cases.iter().map(Vec::len).max().unwrap_or(0);
    (0..n)
        .map(|i| {
            let mut acc: Option<FlatType> = None;
            for c in cases {
                if let Some(t) = c.get(i) {
                    acc = Some(match acc {
                        None => *t,
                        Some(a) if a == *t => a,
                        Some(FlatType::I32) | Some(FlatType::F32)
                            if matches!(t, FlatType::I32 | FlatType::F32) =>
                        {
                            FlatType::I32
                        }
                        _ => FlatType::I64,
                    });
                }
            }
            acc.unwrap_or(FlatType::I32)
        })
        .collect()
}

pub fn discriminant_size(cases: u32) -> u32 {
    if cases <= 256 {
        1
    } else if cases <= 65_536 {
        2
    } else {
        4
    }
}

pub fn align_to(offset: u32, align: u32) -> u32 {
    offset.div_ceil(align.max(1)) * align.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_follow_the_canonical_abi() {
        let port = Ty::record(&[("id", Ty::U32), ("kind", Ty::U32), ("name", Ty::String)]);
        assert_eq!(port.size(), 16);
        assert_eq!(port.align(), 4);
        assert_eq!(port.field_offsets(), vec![0, 4, 8]);
        assert_eq!(
            port.flat_types(),
            vec![FlatType::I32, FlatType::I32, FlatType::I32, FlatType::I32]
        );
        let mixed = Ty::record(&[("a", Ty::U8), ("b", Ty::U64), ("c", Ty::U16)]);
        assert_eq!(mixed.field_offsets(), vec![0, 8, 16]);
        assert_eq!(mixed.size(), 24);
        assert_eq!(Ty::option(Ty::U32).size(), 8);
        assert_eq!(Ty::option(Ty::U32).payload_offset(), 4);
        assert_eq!(Ty::result(Some(Ty::U64), Some(Ty::U8)).size(), 16);
        assert_eq!(
            Ty::result(Some(Ty::F32), Some(Ty::U32)).flat_types(),
            vec![FlatType::I32, FlatType::I32]
        );
        assert_eq!(
            Ty::result(Some(Ty::F32), Some(Ty::U64)).flat_types(),
            vec![FlatType::I32, FlatType::I64]
        );
        assert_eq!(Ty::Enum(3).size(), 1);
        assert_eq!(Ty::Enum(300).size(), 2);
        let v = Ty::variant(&[("none", None), ("some", Some(Ty::U64))]);
        assert_eq!(v.size(), 16);
        assert_eq!(v.flat_types(), vec![FlatType::I32, FlatType::I64]);
    }
}
