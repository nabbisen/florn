//! The adapter conformance cases of `florn-engine`, plus the watchdog, which only a native engine
//! can be tested for from another thread.
use florn_abi::core_world;
use florn_abi::{Ty, Val};
use florn_engine::conformance::{RecordingHandler, call_export, core_import_spec, guest_bytes};
use florn_engine::{Engine, TrapKind};
use florn_engine_wasmtime::{Limits, WasmtimeEngine};

#[test]
fn adapter_conformance() {
    florn_engine::conformance::run_all(&mut || WasmtimeEngine::new(Limits::default()).unwrap());
}

#[test]
fn the_watchdog_interrupts_a_guest_that_never_returns() {
    let mut e = WasmtimeEngine::new(Limits::default()).unwrap();
    let module = e.compile(&guest_bytes()).unwrap();
    let mut inst = e.instantiate(&module, &core_import_spec()).unwrap();
    let watchdog = e.watchdog();
    let t = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        watchdog.interrupt();
    });
    let msg_tys: Vec<Ty> = core_world::actor_functions()[1].param_types();
    let mut handler = RecordingHandler {
        ports: vec![0],
        ..Default::default()
    };
    let trap = call_export(
        &mut e,
        &mut inst,
        core_world::EXPORT_ON_MESSAGE,
        &msg_tys,
        &[Val::U64(0), Val::U32(1004), Val::bytes(&[])],
        &mut handler,
    )
    .expect_err("interrupted");
    assert_eq!(trap.kind, TrapKind::Watchdog, "{trap}");
    t.join().unwrap();
}

#[test]
fn store_limits_bound_instances_and_memory() {
    let mut e = WasmtimeEngine::new(Limits {
        instances: 2,
        memories: 2,
        tables: 2,
        memory_bytes: 65_536 * 3,
    })
    .unwrap();
    let module = e.compile(&guest_bytes()).unwrap();
    let spec = core_import_spec();
    let _a = e.instantiate(&module, &spec).unwrap();
    let mut b = e.instantiate(&module, &spec).unwrap();
    assert!(
        e.instantiate(&module, &spec).is_err(),
        "a third instance exceeds the store limit"
    );
    assert!(
        e.memory_grow_to(&mut b, 65_536 * 4).is_err(),
        "growth beyond the memory limit is refused"
    );
}
