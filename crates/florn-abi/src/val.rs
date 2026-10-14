//! Values of the type subset, as the Host holds them.

/// A value of a [`crate::Ty`].
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    S8(i8),
    S16(i16),
    S32(i32),
    S64(i64),
    F32(f32),
    F64(f64),
    String(String),
    List(Vec<Val>),
    Enum(u32),
    Record(Vec<Val>),
    Option(Option<Box<Val>>),
    /// `Ok(payload)` or `Err(payload)`, each payload optional as the type says.
    Result(Result<Option<Box<Val>>, Option<Box<Val>>>),
    Variant(u32, Option<Box<Val>>),
}

impl Val {
    pub fn bytes(b: &[u8]) -> Val {
        Val::List(b.iter().map(|x| Val::U8(*x)).collect())
    }
    pub fn str(s: &str) -> Val {
        Val::String(s.to_string())
    }
    /// The bytes of a `list<u8>`, if this is one.
    pub fn as_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Val::List(items) => items
                .iter()
                .map(|v| if let Val::U8(b) = v { Some(*b) } else { None })
                .collect(),
            _ => None,
        }
    }
}

/// Canonical NaN (RFC 001 rule 47; RFC 002 rule 9): every NaN becomes one fixed NaN.
pub fn canonical_f32(x: f32) -> f32 {
    if x.is_nan() {
        f32::from_bits(0x7fc0_0000)
    } else {
        x
    }
}
pub fn canonical_f64(x: f64) -> f64 {
    if x.is_nan() {
        f64::from_bits(0x7ff8_0000_0000_0000)
    } else {
        x
    }
}
