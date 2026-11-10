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
    use FlatType::{F64, I32, I64};
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
    assert!(bindings(&spec(&[("t:w/q@0.1.0", "x", &[F64, F64], &[])])).is_none());
    let many: Vec<(&str, &str, &[FlatType], &[FlatType])> = (0..=SLOTS_PER_SHAPE as usize)
        .map(|_| ("t:w/q@0.1.0", "x", &[I32][..], &[I32][..]))
        .collect();
    assert!(bindings(&spec(&many)).is_none());
}

#[test]
fn floats_travel_as_their_bits_in_a_slot() {
    // RFC 012 finding F-1: `begin-step(i64, f64)` and `set-output(i32, f64)`.
    let mut slots = [0u64; MAX_SLOTS];
    let begin = Shape::of(&[FlatType::I64, FlatType::F64], &[]).unwrap();
    assert_eq!(begin.number(), 13);
    let n = encode(begin, &[Flat::I64(-7), Flat::F64(-0.5)], &mut slots);
    assert_eq!(n, Some(2));
    assert_eq!(slots[0], (-7i64) as u64);
    assert_eq!(slots[1], (-0.5f64).to_bits());
    // A float where the shape wants an integer, or the other way, does not encode.
    assert_eq!(
        encode(begin, &[Flat::I64(1), Flat::I64(2)], &mut slots),
        None
    );
    assert_eq!(
        encode(begin, &[Flat::F64(1.0), Flat::F64(2.0)], &mut slots),
        None
    );
    let output = Shape::of(&[FlatType::I32, FlatType::F64], &[]).unwrap();
    assert_eq!(output.number(), 16);
    encode(output, &[Flat::I32(-1), Flat::F64(1e300)], &mut slots);
    assert_eq!(slots[0], u64::MAX, "an i32 is sign-extended");
    assert_eq!(slots[1], 1e300f64.to_bits());
    // No parameters: nothing to encode, and the shape of `reject-step` has no results to decode.
    let reject = Shape::of(&[], &[]).unwrap();
    assert_eq!(reject.number(), 14);
    assert_eq!(encode(reject, &[], &mut slots), Some(0));
    assert!(decode(reject, &[]).is_empty());
    // The pointer `evaluate` returns comes back in a slot.
    let evaluate = Shape::of(&[FlatType::I32, FlatType::I32], &[FlatType::I32]).unwrap();
    assert_eq!(evaluate.number(), 15);
    assert_eq!(decode(evaluate, &[4096]), vec![Flat::I32(4096)]);
}

#[test]
fn an_import_with_a_float_is_bound_to_its_shapes_trampoline() {
    let s = spec(&[(
        "t:w/h@0.1.0",
        "set-output",
        &[FlatType::I32, FlatType::F64],
        &[],
    )]);
    let b = bindings(&s).expect("in the table");
    let text = binding_text(&b);
    assert!(text.contains("t:w/h@0.1.0\tset-output\tflorn_import_s16_0\n"));
    // Four slots of a shape beyond the core's, as for every shape.
    let five: Vec<(&str, &str, &[FlatType], &[FlatType])> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|n| {
            (
                "t:w/h@0.1.0",
                *n,
                &[FlatType::I64, FlatType::F64][..],
                &[][..],
            )
        })
        .collect();
    assert_eq!(bindings(&spec(&five)), None);
}
