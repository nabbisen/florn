//! RFC 012 section 11, the changes the core needs for the circuit domain, each with a test in the
//! kit's test domain and before any circuit code exists. PR-37: finding F-1 (floats among the flat
//! types, the five call shapes, the export convention of `flat_signature`) and finding F-16 (the
//! post-return export of a list or a string result, required at admission). The adapters' side of
//! both, a call that returns a list and the guest's memory not growing over a thousand calls, is
//! `florn_engine::cases`, run by each adapter.
use florn_abi::{FlatType, Shape, Ty};
use florn_admission::Admission;
use florn_admission::world::{FuncSig, WorldSpec};
use florn_conformance::host;
use florn_conformance::test_domain::{self as td, TestDomain};
use florn_host::domain::Domain;

const EVENTS: &str = "test:kit/events@0.1.0";

/// The test domain's world with more exports, which the module must then have.
fn world(extra: Vec<FuncSig>) -> WorldSpec {
    let mut world: WorldSpec = TestDomain::new().world().unwrap().spec();
    world.exports.extend(extra);
    world
}

/// The prober with more functions in it.
fn module(extra: &str) -> Vec<u8> {
    let wat = td::guest_wat();
    let body = wat.trim_end().strip_suffix(')').expect("a module");
    td::guest_bytes_of(&format!("{body}\n{extra})"))
}

fn admit(world: WorldSpec, bytes: &[u8]) -> Result<(), florn_admission::Errors> {
    let mut adm = Admission::default();
    adm.add_world(world);
    adm.admit("prober", bytes, &host::kit_limits()).map(|_| ())
}

fn export(name: &str, params: &[Ty], result: Option<&Ty>) -> FuncSig {
    FuncSig::export(format!("{EVENTS}#{name}"), params, result)
}

const EVALUATE: &str = r#"
  (func (export "test:kit/events@0.1.0#evaluate") (param i32 i32) (result i32) (i32.const 0))"#;
const EVALUATE_POST: &str = r#"
  (func (export "cabi_post_test:kit/events@0.1.0#evaluate") (param i32))"#;

fn evaluate() -> FuncSig {
    let list = Ty::list(Ty::F64);
    export("evaluate", std::slice::from_ref(&list), Some(&list))
}

#[test]
fn a_list_result_is_returned_as_one_pointer_and_needs_its_post_return() {
    let e = evaluate();
    // The export convention: the list comes in as (pointer, length) and goes out as one pointer.
    assert_eq!(e.params, vec![FlatType::I32, FlatType::I32]);
    assert_eq!(e.results, vec![FlatType::I32]);
    assert!(e.post_return);
    assert_eq!(
        Shape::of(&e.params, &e.results).map(Shape::number),
        Some(15)
    );
    let post = e
        .post_return_sig()
        .expect("a post-return export is required");
    assert_eq!(post.name, "cabi_post_test:kit/events@0.1.0#evaluate");
    assert_eq!((post.params, post.results), (vec![FlatType::I32], vec![]));
    // An import is the other convention, and a function with a scalar result needs none.
    assert!(!FuncSig::import("probe", &[Ty::U32], Some(&Ty::U32)).post_return);
    assert!(!export("count", &[Ty::U32], Some(&Ty::U32)).post_return);
}

#[test]
fn a_module_with_the_post_return_export_is_admitted() {
    admit(
        world(vec![evaluate()]),
        &module(&format!("{EVALUATE}{EVALUATE_POST}")),
    )
    .expect("admitted");
}

#[test]
fn a_module_without_the_post_return_export_is_module_export_missing() {
    let Err(e) = admit(world(vec![evaluate()]), &module(EVALUATE)) else {
        panic!("admitted a list-returning export without its post-return");
    };
    assert_eq!(e.codes(), vec!["module.export-missing"]);
    assert!(
        e.to_string()
            .contains("cabi_post_test:kit/events@0.1.0#evaluate"),
        "the error names the export: {e}"
    );
}

#[test]
fn a_post_return_export_of_another_type_is_module_export_missing() {
    for wrong in [
        r#"(func (export "cabi_post_test:kit/events@0.1.0#evaluate") (param i64))"#,
        r#"(func (export "cabi_post_test:kit/events@0.1.0#evaluate"))"#,
        r#"(func (export "cabi_post_test:kit/events@0.1.0#evaluate") (param i32) (result i32) (i32.const 0))"#,
    ] {
        let Err(e) = admit(
            world(vec![evaluate()]),
            &module(&format!("{EVALUATE}{wrong}")),
        ) else {
            panic!("admitted {wrong}");
        };
        assert_eq!(e.codes(), vec!["module.export-missing"], "{wrong}");
    }
}

#[test]
fn a_string_result_needs_its_post_return_and_a_scalar_result_does_not() {
    // `name(port) -> string`: the flat signature is (i32) -> i32, a row of the table, and the
    // result is a buffer in the guest's memory.
    let name = export("name", &[Ty::U32], Some(&Ty::String));
    assert!(name.post_return);
    let func =
        r#"(func (export "test:kit/events@0.1.0#name") (param i32) (result i32) (i32.const 0))"#;
    let post = r#"(func (export "cabi_post_test:kit/events@0.1.0#name") (param i32))"#;
    assert_eq!(
        admit(world(vec![name.clone()]), &module(func))
            .unwrap_err()
            .codes(),
        vec!["module.export-missing"]
    );
    admit(world(vec![name]), &module(&format!("{func}{post}"))).expect("admitted");
    // A scalar result: no post-return is required, and one that is there is harmless.
    let count = export("count", &[Ty::U32], Some(&Ty::U32));
    let func =
        r#"(func (export "test:kit/events@0.1.0#count") (param i32) (result i32) (i32.const 0))"#;
    admit(world(vec![count]), &module(func)).expect("admitted without a post-return");
}

#[test]
fn a_record_that_holds_a_list_needs_its_post_return_too() {
    // RFC 002 rule 2 says "an export that returns a list or a string"; a record with a list in it
    // returns one, and leaves the same buffer behind.
    let pair = Ty::record(&[("n", Ty::U32), ("xs", Ty::list(Ty::F64))]);
    let sig = export("pair", &[Ty::U32], Some(&pair));
    assert!(sig.post_return);
    assert_eq!(
        sig.results,
        vec![FlatType::I32],
        "returned through a pointer"
    );
}

#[test]
fn the_five_circuit_shapes_are_admitted_as_exports() {
    // RFC 012 finding F-1: on-sample (i64, i64), begin-step (i64, f64), reject-step (),
    // evaluate (i32, i32) -> i32, set-output (i32, f64).
    let sigs = vec![
        export("on-sample", &[Ty::U64, Ty::U64], None),
        export("begin-step", &[Ty::U64, Ty::F64], None),
        export("reject-step", &[], None),
        evaluate(),
        export("set-output", &[Ty::U32, Ty::F64], None),
    ];
    let shapes: Vec<u32> = sigs
        .iter()
        .map(|s| {
            Shape::of(&s.params, &s.results)
                .expect("in the table")
                .number()
        })
        .collect();
    assert_eq!(shapes, vec![12, 13, 14, 15, 16]);
    let funcs = r#"
  (func (export "test:kit/events@0.1.0#on-sample") (param i64 i64))
  (func (export "test:kit/events@0.1.0#begin-step") (param i64 f64))
  (func (export "test:kit/events@0.1.0#reject-step"))
  (func (export "test:kit/events@0.1.0#set-output") (param i32 f64))"#;
    admit(
        world(sigs),
        &module(&format!("{funcs}{EVALUATE}{EVALUATE_POST}")),
    )
    .expect("admitted");
}

#[test]
fn a_shape_outside_the_table_is_module_export_unsupported() {
    // Floats are flat types, but a function of two floats is not a row (RFC 008 rule 6); nor is a
    // single f32. Refused whatever the module has.
    for (params, wat) in [
        (
            vec![Ty::F64, Ty::F64],
            r#"(func (export "test:kit/events@0.1.0#odd") (param f64 f64))"#,
        ),
        (
            vec![Ty::F32],
            r#"(func (export "test:kit/events@0.1.0#odd") (param f32))"#,
        ),
    ] {
        let Err(e) = admit(world(vec![export("odd", &params, None)]), &module(wat)) else {
            panic!("admitted a shape outside the table: {params:?}");
        };
        assert_eq!(e.codes(), vec!["module.export-unsupported"], "{params:?}");
    }
    // What `flat_signature` said of an export before: the import convention, a return pointer as
    // the last parameter. For `evaluate` that is (i32, i32, i32) with no result, which is the
    // shape of `send` (row 5): admission would have asked the module for a function no compiler
    // emits for it, and the shape would have named the wrong trampoline.
    let list = Ty::list(Ty::F64);
    let (p, r) = Ty::import_signature(std::slice::from_ref(&list), Some(&list));
    assert_eq!(Shape::of(&p, &r).map(Shape::number), Some(5));
    assert_ne!(
        Shape::of(&p, &r),
        Shape::of(&evaluate().params, &evaluate().results)
    );
}
