//! The call shapes the adapter calls from WebAssembly to WebAssembly (RFC 005 rule 7; RFC 008
//! rule 6): the table of `florn_abi::Shape`, shared with admission, so that a world's function
//! outside it is refused before any run. A flat argument travels in one 64-bit slot of the Host's
//! memory; this module encodes and decodes the slots, so that the part the worker and the
//! trampoline agree on is testable natively. The import binding text the adapter hands the
//! worker at instantiation is written and read here too.

use florn_abi::core_world;
use florn_abi::{Flat, FlatType, Shape};
use florn_engine::ImportSpec;

/// The most slots a call carries.
pub const MAX_SLOTS: usize = 8;

/// Import trampolines per shape beyond the core's named ones (`florn_import_s<shape>_<slot>`).
/// Admission keeps a world's imports of one shape within it (RFC 008 rule 6).
pub const SLOTS_PER_SHAPE: u32 = Shape::IMPORT_SLOTS;

/// The shape of a core export, by its canonical name.
pub fn of_core_export(name: &str) -> Option<Shape> {
    match name {
        core_world::EXPORT_INIT => Some(Shape::INIT),
        core_world::EXPORT_ON_MESSAGE => Some(Shape::MESSAGE),
        core_world::EXPORT_ON_TIMER => Some(Shape::TIMER),
        core_world::EXPORT_REALLOC => Some(Shape::REALLOC),
        _ => None,
    }
}

/// Encode flat values into 64-bit slots: an `i32` sign-extended, an `i64` as is, floats as
/// their bits. Returns the number of slots, or `None` if the values do not fit the shape.
pub fn encode(shape: Shape, flats: &[Flat], out: &mut [u64; MAX_SLOTS]) -> Option<usize> {
    let params = shape.params();
    if flats.len() != params.len() {
        return None;
    }
    for (i, (f, t)) in flats.iter().zip(params).enumerate() {
        out[i] = match (f, t) {
            (Flat::I32(v), FlatType::I32) => *v as i64 as u64,
            (Flat::I64(v), FlatType::I64) => *v as u64,
            (Flat::F32(v), FlatType::F32) => v.to_bits() as u64,
            (Flat::F64(v), FlatType::F64) => v.to_bits(),
            _ => return None,
        };
    }
    Some(flats.len())
}

/// Decode a call's results from the slots, by the shape's result types.
pub fn decode(shape: Shape, slots: &[u64]) -> Vec<Flat> {
    shape
        .results()
        .iter()
        .zip(slots)
        .map(|(t, s)| match t {
            FlatType::I32 => Flat::I32(*s as u32 as i32),
            FlatType::I64 => Flat::I64(*s as i64),
            FlatType::F32 => Flat::F32(f32::from_bits(*s as u32)),
            FlatType::F64 => Flat::F64(f64::from_bits(*s)),
        })
        .collect()
}

/// One import binding: the guest's import (module, name) and the Host export that answers it:
/// a core import by its own name, any other by the shape's slot (RFC 008 rule 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub module: String,
    pub name: String,
    pub shape: Shape,
    /// `None` for a core import bound to `florn_import_<name>`.
    pub slot: Option<u32>,
}

/// The bindings of a spec: core imports by name, the rest by shape and slot in spec order.
/// `None` when an import's shape is outside the table or a shape's slots are exhausted, which
/// admission prevents (`module.export-unsupported`).
pub fn bindings(spec: &ImportSpec) -> Option<Vec<Binding>> {
    let mut used: Vec<(Shape, u32)> = Vec::new();
    let mut out = Vec::with_capacity(spec.functions.len());
    for f in &spec.functions {
        let shape = Shape::of(&f.params, &f.results)?;
        let core = f.module == core_world::HOST_MODULE
            && core_world::host_functions()
                .iter()
                .any(|s| s.name == f.name);
        let slot = if core {
            None
        } else {
            let n = match used.iter_mut().find(|(s, _)| *s == shape) {
                Some((_, n)) => {
                    *n += 1;
                    *n - 1
                }
                None => {
                    used.push((shape, 1));
                    0
                }
            };
            if n >= SLOTS_PER_SHAPE {
                return None;
            }
            Some(n)
        };
        out.push(Binding {
            module: f.module.clone(),
            name: f.name.clone(),
            shape,
            slot,
        });
    }
    Some(out)
}

/// The bindings as the text the worker reads: one line per import, `module\tname\texport`.
pub fn binding_text(bindings: &[Binding]) -> String {
    let mut t = String::new();
    for b in bindings {
        let export = match b.slot {
            None => format!("florn_import_{}", b.name.replace('-', "_")),
            Some(slot) => format!("florn_import_s{}_{}", b.shape.number(), slot),
        };
        t.push_str(&b.module);
        t.push('\t');
        t.push_str(&b.name);
        t.push('\t');
        t.push_str(&export);
        t.push('\n');
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use florn_engine::{HostGlobal, ImportFunction};

    fn spec(extra: &[(&str, &str, &[FlatType], &[FlatType])]) -> ImportSpec {
        let mut functions: Vec<ImportFunction> = core_world::host_functions()
            .iter()
            .map(|s| ImportFunction {
                module: core_world::HOST_MODULE.to_string(),
                name: s.name.to_string(),
                params: s
                    .param_types()
                    .iter()
                    .flat_map(florn_abi::Ty::flat_types)
                    .collect(),
                results: s
                    .result
                    .as_ref()
                    .map_or(Vec::new(), florn_abi::Ty::flat_types),
            })
            .collect();
        for (m, n, p, r) in extra {
            functions.push(ImportFunction {
                module: m.to_string(),
                name: n.to_string(),
                params: p.to_vec(),
                results: r.to_vec(),
            });
        }
        ImportSpec {
            functions,
            globals: vec![HostGlobal {
                module: "env".into(),
                name: "gas_counter".into(),
                initial: 0,
            }],
        }
    }

    #[test]
    fn slots_round_trip() {
        assert_eq!(
            of_core_export(core_world::EXPORT_ON_MESSAGE),
            Some(Shape::MESSAGE)
        );
        assert_eq!(of_core_export("other"), None);
        let mut slots = [0u64; MAX_SLOTS];
        let n = encode(
            Shape::MESSAGE,
            &[Flat::I64(-5), Flat::I32(-1), Flat::I32(16), Flat::I32(3)],
            &mut slots,
        );
        assert_eq!(n, Some(4));
        assert_eq!(slots[0], (-5i64) as u64);
        assert_eq!(slots[1], (-1i64) as u64);
        assert_eq!(encode(Shape::TIMER, &[Flat::I32(1)], &mut slots), None);
        assert_eq!(decode(Shape::REALLOC, &[1024]), vec![Flat::I32(1024)]);
        assert!(decode(Shape::TIMER, &[]).is_empty());
    }

    #[test]
    fn core_imports_bind_by_name_and_others_by_slot() {
        use FlatType::{I32, I64};
        let b = bindings(&spec(&[
            ("t:w/q@0.1.0", "probe", &[I32], &[I32]),
            ("t:w/q@0.1.0", "other", &[I32], &[I32]),
            ("t:w/e@0.1.0", "on-link", &[I64, I32, I32], &[]),
        ]))
        .unwrap();
        assert_eq!(b[0].slot, None);
        assert_eq!(b[5].slot, Some(0));
        assert_eq!(b[6].slot, Some(1));
        assert_eq!(b[7].slot, Some(0));
        let text = binding_text(&b);
        assert!(text.contains("florn:core/host@0.1.0\tset-timer\tflorn_import_set_timer\n"));
        assert!(text.contains("t:w/q@0.1.0\tother\tflorn_import_s11_1\n"));
        assert!(text.contains("t:w/e@0.1.0\ton-link\tflorn_import_s9_0\n"));
        // Outside the table, or too many of one shape: no binding.
        assert!(bindings(&spec(&[("t:w/q@0.1.0", "x", &[I64, I64], &[])])).is_none());
        let many: Vec<(&str, &str, &[FlatType], &[FlatType])> = (0..=SLOTS_PER_SHAPE as usize)
            .map(|_| ("t:w/q@0.1.0", "x", &[I32][..], &[I32][..]))
            .collect();
        assert!(bindings(&spec(&many)).is_none());
    }
}
