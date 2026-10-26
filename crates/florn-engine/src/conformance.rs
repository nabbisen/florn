//! The adapter conformance cases: one hand-written guest of the core world, one recording import
//! handler, and the cases every adapter runs through them. An adapter's test calls [`run_all`].

use std::collections::BTreeMap;

use florn_abi::core_world;
use florn_abi::{Flat, Memory, Shape, Ty, Val, lift_flat_params, lower_flat_params};

use crate::{
    Engine, GlobalValue, HostGlobal, ImportFunction, ImportHandler, ImportSpec, InstanceId,
    Refusal, TrapKind,
};

/// The Host-owned budget global as admission names it (RFC 003 §3; `fvm-wasm-instrument`'s import).
pub const BUDGET_MODULE: &str = "env";
pub const BUDGET_NAME: &str = "gas_counter";

/// A guest of the core world, written in text so that it can misbehave on request. On a message:
/// port < 1000 echoes the payload through the same port and counts in an exported global;
/// 1000 traps; 1001 sends from an address outside memory; 1002 sends through an undeclared port;
/// 1003 reports on an undeclared channel; 1004 loops for ever. On a timer: draws `random`, reports
/// it on channel 1, calls through the table with the timer's parity, and re-arms the timer.
pub const GUEST_WAT: &str = r#"
(module
  (import "florn:core/host@0.1.0" "send" (func $send (param i32 i32 i32)))
  (import "florn:core/host@0.1.0" "set-timer" (func $set_timer (param i32 i64)))
  (import "florn:core/host@0.1.0" "cancel-timer" (func $cancel (param i32)))
  (import "florn:core/host@0.1.0" "random" (func $random (result i64)))
  (import "florn:core/host@0.1.0" "report" (func $report (param i32 i32 i32)))
  (import "env" "gas_counter" (global $gas (mut i64)))
  (memory (export "memory") 1)
  (type $ret (func (result i32)))
  (global $counter (export "florn.global.0") (mut i32) (i32.const 0))
  (global $bump (mut i32) (i32.const 1024))
  (table (export "florn.table.0") 2 2 funcref)
  (elem (i32.const 0) $f0 $f1)
  (func $f0 (result i32) (i32.const 10))
  (func $f1 (result i32) (i32.const 20))

  ;; cabi_realloc(old_ptr, old_size, align, new_size): a bump allocator that grows the memory.
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
    (local $p i32)
    (local.set $p (i32.and (i32.add (global.get $bump) (i32.sub (local.get 2) (i32.const 1)))
                           (i32.sub (i32.const 0) (local.get 2))))
    (global.set $bump (i32.add (local.get $p) (local.get 3)))
    (block $ok (loop $grow
      (br_if $ok (i32.le_u (global.get $bump) (i32.mul (memory.size) (i32.const 65536))))
      (drop (memory.grow (i32.const 1)))
      (br $grow)))
    (local.get $p))

  ;; init(now, actor, ports.ptr, ports.len, config.ptr, config.len): record what arrived.
  (func (export "florn:core/actor@0.1.0#init") (param i64 i32 i32 i32 i32 i32)
    (i32.store (i32.const 8) (local.get 3))
    (i32.store (i32.const 12) (local.get 1))
    (i32.store (i32.const 16) (local.get 5))
    ;; the first port's name length, read from the lowered record (id u32, kind u32, name ptr, name len)
    (if (i32.gt_u (local.get 3) (i32.const 0))
      (then (i32.store (i32.const 20) (i32.load (i32.add (local.get 2) (i32.const 12)))))))

  ;; on-message(now, port, payload.ptr, payload.len)
  (func (export "florn:core/actor@0.1.0#on-message") (param i64 i32 i32 i32)
    (block $echo
      (br_if $echo (i32.lt_u (local.get 1) (i32.const 1000)))
      (if (i32.eq (local.get 1) (i32.const 1000)) (then (unreachable)))
      (if (i32.eq (local.get 1) (i32.const 1001))
        (then (call $send (i32.const 0) (i32.const 0x7fff0000) (i32.const 16)) (return)))
      (if (i32.eq (local.get 1) (i32.const 1002))
        (then (call $send (i32.const 77) (local.get 2) (local.get 3)) (return)))
      (if (i32.eq (local.get 1) (i32.const 1003))
        (then (call $report (i32.const 9) (local.get 2) (local.get 3)) (return)))
      (if (i32.eq (local.get 1) (i32.const 1004))
        (then (loop $l (global.set $counter (i32.add (global.get $counter) (i32.const 1))) (br $l))))
      (return))
    (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
    (call $send (local.get 1) (local.get 2) (local.get 3)))

  ;; on-timer(now, timer)
  (func (export "florn:core/actor@0.1.0#on-timer") (param i64 i32)
    (i64.store (i32.const 24) (call $random))
    (call $report (i32.const 1) (i32.const 24) (i32.const 8))
    (i32.store (i32.const 32) (call_indirect (type $ret) (i32.and (local.get 1) (i32.const 1))))
    (call $set_timer (local.get 1) (i64.const 100))))
"#;

/// The guest as bytes.
pub fn guest_bytes() -> Vec<u8> {
    wat::parse_str(GUEST_WAT).expect("the conformance guest parses")
}

/// The core world's imports, as flat signatures, plus the budget global.
pub fn core_import_spec() -> ImportSpec {
    let functions = core_world::host_functions()
        .into_iter()
        .map(|s| ImportFunction {
            module: core_world::HOST_MODULE.to_string(),
            name: s.name.to_string(),
            params: s.param_types().iter().flat_map(Ty::flat_types).collect(),
            results: s.result.as_ref().map_or(Vec::new(), Ty::flat_types),
        })
        .collect();
    ImportSpec {
        functions,
        globals: vec![HostGlobal {
            module: BUDGET_MODULE.into(),
            name: BUDGET_NAME.into(),
            initial: 0,
        }],
    }
}

/// What the handler saw: one entry per import call.
#[derive(Clone, Debug, PartialEq)]
pub enum Seen {
    Send {
        instance: InstanceId,
        port: u32,
        payload: Vec<u8>,
    },
    SetTimer {
        instance: InstanceId,
        timer: u32,
        delay: u64,
    },
    CancelTimer {
        instance: InstanceId,
        timer: u32,
    },
    Random {
        instance: InstanceId,
    },
    Report {
        instance: InstanceId,
        channel: u32,
        payload: Vec<u8>,
    },
    /// A refusal, with the reason as text.
    Refused {
        instance: InstanceId,
        function: String,
        why: String,
    },
}

/// A handler that lifts every call through `florn-abi`, records it, refuses undeclared ports and
/// channels, and answers `random` with a fixed value.
#[derive(Debug, Default)]
pub struct RecordingHandler {
    pub seen: Vec<Seen>,
    pub ports: Vec<u32>,
    pub channels: Vec<u32>,
    pub random_answer: u64,
}

impl ImportHandler for RecordingHandler {
    fn call(
        &mut self,
        instance: InstanceId,
        function: &str,
        args: &[Flat],
        memory: &mut dyn Memory,
    ) -> Result<Vec<Flat>, Refusal> {
        let sig = core_world::host_functions()
            .into_iter()
            .find(|s| s.name == function)
            .ok_or(Refusal)?;
        let vals = match lift_flat_params(memory, &sig.param_types(), args) {
            Ok(v) => v,
            Err(e) => {
                self.seen.push(Seen::Refused {
                    instance,
                    function: function.into(),
                    why: e.to_string(),
                });
                return Err(Refusal);
            }
        };
        match (function, vals.as_slice()) {
            ("send", [Val::U32(port), payload]) => {
                if !self.ports.contains(port) {
                    self.seen.push(Seen::Refused {
                        instance,
                        function: function.into(),
                        why: format!("port {port} undeclared"),
                    });
                    return Err(Refusal);
                }
                self.seen.push(Seen::Send {
                    instance,
                    port: *port,
                    payload: payload.as_bytes().unwrap(),
                });
                Ok(vec![])
            }
            ("set-timer", [Val::U32(timer), Val::U64(delay)]) => {
                self.seen.push(Seen::SetTimer {
                    instance,
                    timer: *timer,
                    delay: *delay,
                });
                Ok(vec![])
            }
            ("cancel-timer", [Val::U32(timer)]) => {
                self.seen.push(Seen::CancelTimer {
                    instance,
                    timer: *timer,
                });
                Ok(vec![])
            }
            ("random", []) => {
                self.seen.push(Seen::Random { instance });
                Ok(vec![Flat::I64(self.random_answer as i64)])
            }
            ("report", [Val::U32(channel), payload]) => {
                if *channel != 0 && !self.channels.contains(channel) {
                    self.seen.push(Seen::Refused {
                        instance,
                        function: function.into(),
                        why: format!("channel {channel} undeclared"),
                    });
                    return Err(Refusal);
                }
                self.seen.push(Seen::Report {
                    instance,
                    channel: *channel,
                    payload: payload.as_bytes().unwrap(),
                });
                Ok(vec![])
            }
            _ => Err(Refusal),
        }
    }
}

/// Lower the parameters of an export and call it.
pub fn call_export<E: Engine>(
    e: &mut E,
    inst: &mut E::Instance,
    name: &str,
    tys: &[Ty],
    vals: &[Val],
    handler: &mut dyn ImportHandler,
) -> Result<Vec<Flat>, crate::Trap> {
    let params: Vec<florn_abi::FlatType> = tys.iter().flat_map(Ty::flat_types).collect();
    let shape = Shape::of(&params, &[]).unwrap_or_else(|| panic!("a shape for {name}"));
    let func = e
        .func(inst, name, shape)
        .unwrap_or_else(|| panic!("export {name}"));
    let mut flats = Vec::new();
    let mut err = None;
    e.with_memory(inst, &mut |mem| match lower_flat_params(mem, tys, vals) {
        Ok(f) => flats = f,
        Err(x) => err = Some(x),
    })
    .expect("memory");
    if let Some(x) = err {
        panic!("lowering: {x}");
    }
    e.call(inst, &func, &flats, handler)
}

fn read_u32<E: Engine>(e: &mut E, inst: &mut E::Instance, addr: u32) -> u32 {
    let mut out = 0;
    e.with_memory(inst, &mut |mem| {
        out = u32::from_le_bytes(mem.read(addr, 4).unwrap().try_into().unwrap())
    })
    .unwrap();
    out
}

/// Run every case against an adapter made by `make`. Panics on the first failure with the case's name.
pub fn run_all<E: Engine>(make: &mut dyn FnMut() -> E) {
    let bytes = guest_bytes();
    let spec = core_import_spec();
    let init_tys: Vec<Ty> = core_world::actor_functions()[0].param_types();
    let msg_tys: Vec<Ty> = core_world::actor_functions()[1].param_types();
    let timer_tys: Vec<Ty> = core_world::actor_functions()[2].param_types();
    let ports = Val::List(vec![
        Val::Record(vec![Val::U32(0), Val::U32(0), Val::str("eth0")]),
        Val::Record(vec![Val::U32(1), Val::U32(0), Val::str("eth1")]),
    ]);
    let mut handler = RecordingHandler {
        ports: vec![0, 1],
        channels: vec![1],
        random_answer: 0xDEAD_BEEF_CAFE_F00D,
        ..Default::default()
    };

    // 1. Compile once, instantiate twice: independent instances with independent memories; the cache holds.
    let mut e = make();
    let module = e.compile(&bytes).expect("compile");
    let _again = e.compile(&bytes).expect("compile again");
    assert_eq!(e.compiled_count(), 1, "case 1: compiled once");
    let mut a = e.instantiate(&module, &spec).expect("instantiate a");
    let mut b = e.instantiate(&module, &spec).expect("instantiate b");
    assert_ne!(
        e.instance_id(&a),
        e.instance_id(&b),
        "case 1: distinct instance ids"
    );
    let a_id = e.instance_id(&a);

    // 2. init: the lowered list of records arrives; the guest records counts and the first name's length.
    call_export(
        &mut e,
        &mut a,
        core_world::EXPORT_INIT,
        &init_tys,
        &[
            Val::U64(0),
            Val::U32(7),
            ports.clone(),
            Val::bytes(&[1, 2, 3]),
        ],
        &mut handler,
    )
    .expect("case 2: init");
    assert_eq!(read_u32(&mut e, &mut a, 8), 2, "case 2: two ports");
    assert_eq!(read_u32(&mut e, &mut a, 12), 7, "case 2: actor id");
    assert_eq!(read_u32(&mut e, &mut a, 16), 3, "case 2: config length");
    assert_eq!(
        read_u32(&mut e, &mut a, 20),
        4,
        "case 2: first port name length"
    );
    assert_eq!(
        read_u32(&mut e, &mut b, 8),
        0,
        "case 2: instance b untouched"
    );

    // 3. Echo: the import lifts the payload the Host lowered, through views, with the instance id.
    let payload: Vec<u8> = (0..1500u32).map(|i| (i * 7) as u8).collect();
    call_export(
        &mut e,
        &mut a,
        core_world::EXPORT_ON_MESSAGE,
        &msg_tys,
        &[Val::U64(5), Val::U32(1), Val::bytes(&payload)],
        &mut handler,
    )
    .expect("case 3: on-message");
    assert_eq!(
        handler.seen.last(),
        Some(&Seen::Send {
            instance: a_id,
            port: 1,
            payload: payload.clone()
        }),
        "case 3: echoed"
    );
    assert_eq!(
        e.global_get(&mut a, None, "florn.global.0"),
        Some(GlobalValue::I32(1)),
        "case 3: exported global counted"
    );

    // 4. A timer: the guest draws random, reports it on channel 1, calls through the table, re-arms.
    handler.seen.clear();
    call_export(
        &mut e,
        &mut a,
        core_world::EXPORT_ON_TIMER,
        &timer_tys,
        &[Val::U64(9), Val::U32(3)],
        &mut handler,
    )
    .expect("case 4: on-timer");
    assert_eq!(
        handler.seen,
        vec![
            Seen::Random { instance: a_id },
            Seen::Report {
                instance: a_id,
                channel: 1,
                payload: 0xDEAD_BEEF_CAFE_F00Du64.to_le_bytes().to_vec()
            },
            Seen::SetTimer {
                instance: a_id,
                timer: 3,
                delay: 100
            }
        ],
        "case 4: random answered, reported, re-armed"
    );
    assert_eq!(
        read_u32(&mut e, &mut a, 32),
        20,
        "case 4: call_indirect through the table"
    );

    // 5. Traps: a guest trap, a refused import (bad pointer, undeclared port, undeclared channel).
    let t = call_export(
        &mut e,
        &mut a,
        core_world::EXPORT_ON_MESSAGE,
        &msg_tys,
        &[Val::U64(1), Val::U32(1000), Val::bytes(&[])],
        &mut handler,
    )
    .expect_err("case 5: trap");
    assert_eq!(
        t.kind,
        TrapKind::Guest,
        "case 5: unreachable is a guest trap: {t}"
    );
    for (port, why) in [
        (1001u32, "outside memory"),
        (1002, "port 77 undeclared"),
        (1003, "channel 9 undeclared"),
    ] {
        handler.seen.clear();
        let t = call_export(
            &mut e,
            &mut a,
            core_world::EXPORT_ON_MESSAGE,
            &msg_tys,
            &[Val::U64(1), Val::U32(port), Val::bytes(&[1])],
            &mut handler,
        )
        .expect_err("case 5: refused");
        assert_eq!(t.kind, TrapKind::HostRefused, "case 5: port {port}: {t}");
        match handler.seen.last() {
            Some(Seen::Refused { why: w, .. }) => {
                assert!(w.contains(why), "case 5: port {port}: {w}")
            }
            other => panic!("case 5: port {port}: {other:?}"),
        }
    }

    // 6. Host-owned and exported globals: set, call, read back; the guest never touches the budget.
    e.global_set(
        &mut a,
        Some(BUDGET_MODULE),
        BUDGET_NAME,
        GlobalValue::I64(123_456),
    )
    .expect("case 6: set budget");
    call_export(
        &mut e,
        &mut a,
        core_world::EXPORT_ON_MESSAGE,
        &msg_tys,
        &[Val::U64(1), Val::U32(0), Val::bytes(&[9])],
        &mut handler,
    )
    .expect("case 6: call");
    assert_eq!(
        e.global_get(&mut a, Some(BUDGET_MODULE), BUDGET_NAME),
        Some(GlobalValue::I64(123_456)),
        "case 6: budget global unchanged by the guest"
    );
    e.global_set(&mut a, None, "florn.global.0", GlobalValue::I32(41))
        .expect("case 6: set exported global");
    call_export(
        &mut e,
        &mut a,
        core_world::EXPORT_ON_MESSAGE,
        &msg_tys,
        &[Val::U64(1), Val::U32(0), Val::bytes(&[9])],
        &mut handler,
    )
    .expect("case 6: call");
    assert_eq!(
        e.global_get(&mut a, None, "florn.global.0"),
        Some(GlobalValue::I32(42)),
        "case 6: exported global set then counted"
    );
    assert!(
        e.global_get(&mut a, None, "no.such").is_none(),
        "case 6: unknown global is None"
    );

    // 7. Memory: a large payload makes the guest's allocator grow the memory; the view follows (RFC 005 rule 6).
    let big: Vec<u8> = vec![0xAB; 200_000];
    call_export(
        &mut e,
        &mut a,
        core_world::EXPORT_ON_MESSAGE,
        &msg_tys,
        &[Val::U64(1), Val::U32(0), Val::bytes(&big)],
        &mut handler,
    )
    .expect("case 7: big message");
    assert!(
        e.memory_size(&mut a) >= 65_536 * 4,
        "case 7: memory grew to {}",
        e.memory_size(&mut a)
    );
    assert_eq!(
        handler.seen.last(),
        Some(&Seen::Send {
            instance: a_id,
            port: 0,
            payload: big.clone()
        }),
        "case 7: lifted after growth"
    );
    e.memory_grow_to(&mut a, 65_536 * 8)
        .expect("case 7: grow to");
    assert!(e.memory_size(&mut a) >= 65_536 * 8, "case 7: grown");
    assert_eq!(
        e.table_size(&mut a, "florn.table.0"),
        Some(2),
        "case 7: table size"
    );
    assert_eq!(e.table_size(&mut a, "none"), None);

    // 8. Dropping an instance leaves the other usable.
    e.drop_instance(b);
    call_export(
        &mut e,
        &mut a,
        core_world::EXPORT_ON_MESSAGE,
        &msg_tys,
        &[Val::U64(1), Val::U32(0), Val::bytes(&[1])],
        &mut handler,
    )
    .expect("case 8: a still runs");

    // 9. An out-of-range read requested by the Host is an error, never a panic.
    let mut got = None;
    e.with_memory(&mut a, &mut |mem| {
        got = Some(mem.read(u32::MAX - 4, 8).is_err())
    })
    .unwrap();
    assert_eq!(got, Some(true), "case 9: out-of-range read is an error");

    // 10. The same bytes compile to one cached module across a fresh instantiation (a restore never compiles).
    let m2 = e.compile(&bytes).expect("compile");
    let mut c = e.instantiate(&m2, &spec).expect("instantiate c");
    assert_eq!(e.compiled_count(), 1, "case 10: still one compilation");
    call_export(
        &mut e,
        &mut c,
        core_world::EXPORT_ON_MESSAGE,
        &msg_tys,
        &[Val::U64(1), Val::U32(0), Val::bytes(&[2])],
        &mut handler,
    )
    .expect("case 10: c runs");
    let seen_by_instance: BTreeMap<InstanceId, usize> = handler
        .seen
        .iter()
        .filter_map(|s| {
            if let Seen::Send { instance, .. } = s {
                Some(*instance)
            } else {
                None
            }
        })
        .fold(BTreeMap::new(), |mut m, i| {
            *m.entry(i).or_default() += 1;
            m
        });
    assert_eq!(
        seen_by_instance.len(),
        2,
        "case 10: two instances seen by the handler"
    );
}
