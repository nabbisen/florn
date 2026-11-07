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
