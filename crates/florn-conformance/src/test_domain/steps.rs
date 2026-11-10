//! The test domain's compound activation (PR-38; RFC 002 rules 44 and 45; RFC 012 findings F-2
//! and F-3): the four calls of a device-like contract, the guest's side of them, and a small
//! script language with which an intervention tells the domain what to call and when to close, so
//! that every property of the call-in handle is a scenario a test writes down.
//!
//! `step` is an intervention kind: `{ kind = "step", target = <a wire>, actor = <a prober>,
//! script = "open:2 eval:3 commit" }`. The domain schedules a `step` event for the actor, and
//! handles it with the call-in handle, one word of the script at a time:
//!
//! | word | does |
//! |---|---|
//! | `open:H` | open: `begin(tick, H)` |
//! | `eval:X` | `evaluate([X, 0, 0])`, the result bounded at [`BOUND`] elements |
//! | `evals:K` | `K` times `evaluate([-5, 0, 0])`, each of which records one report |
//! | `commit` | the committing close: `accept([1.0])` |
//! | `discard` | the discarding close: `reject()` |
//! | `fault` | the domain judges the contract broken: a Fault of kind contract, parameter 77 |
//! | `badseq` | `commit("evaluate")`: a call that is not the committing close, out of sequence |
//! | `closed` | `evaluate` with nothing open |
//! | `reopen` | `begin` while one is open |
//!
//! The words after one that faulted, or after the run ended, are not played. The domain reports
//! what happened as one `domain.event` notification named `compound`: one result per word.

use florn_abi::Val;
use florn_abi::core_world::Signature;
use florn_abi::{Ty, core_world};
use florn_host::compound::{CallError, CallIn, Compound};
use florn_kernel::{FaultKind, Identity};

pub const STEPS_ID: &str = "test:kit/steps@0.1.0";
pub const BEGIN: &str = "test:kit/steps@0.1.0#begin";
pub const EVALUATE: &str = "test:kit/steps@0.1.0#evaluate";
pub const ACCEPT: &str = "test:kit/steps@0.1.0#accept";
pub const REJECT: &str = "test:kit/steps@0.1.0#reject";
/// The intervention kind that starts a script.
pub const STEP: &str = "step";
/// The event class of a script, handled with the call-in handle.
pub const CLASS_STEP: u8 = 6;
/// The most elements of `evaluate`'s result the domain takes (the circuit's is `n + n² + 1`).
pub const BOUND: u32 = 8;

/// The functions of the `steps` interface, with the signatures their shapes come from:
/// `begin(i64, f64)`, `evaluate(i32, i32) -> i32`, `accept(i32, i32)`, `reject()`.
pub fn signatures() -> Vec<Signature> {
    let floats = Ty::list(Ty::F64);
    vec![
        Signature {
            name: "begin",
            params: vec![("start", Ty::U64), ("h", Ty::F64)],
            result: None,
        },
        Signature {
            name: "evaluate",
            params: vec![("trial", floats.clone())],
            result: Some(floats.clone()),
        },
        Signature {
            name: "accept",
            params: vec![("solution", floats)],
            result: None,
        },
        Signature {
            name: "reject",
            params: vec![],
            result: None,
        },
    ]
}

/// The compound activation the `steps` interface declares.
pub fn compound() -> Compound {
    Compound::new(BEGIN, &[EVALUATE], ACCEPT, REJECT)
}

/// The prober's functions of the `steps` interface (spliced into the module by `guest_wat`).
///
/// `begin` draws a random value, keeps it and the number of loops its post-return will make
/// (`h * 100`), and reports the draw on channel 3: an effect, to be committed or dropped.
/// `evaluate` looks at `trial[0] = x`: `x >= 0` returns a list that claims `x` elements and holds
/// `min(x, 64)`; -1 traps; -2 loops for ever; -3 sends; -4 sets a timer; -5 reports 8 bytes on channel 3. `accept` reports the
/// draw on channel 2 and the text "accepted" on channel 0; `reject` reports on channel 2 too. The
/// post-return of `evaluate` loops `h * 100` times and frees from the argument's buffer on.
pub const GUEST_FUNCTIONS: &str = r#"
  (global $step_draw (mut i64) (i64.const 0))
  (global $post_loops (mut i32) (i32.const 0))
  (global $mark (mut i32) (i32.const 0))
  (data (i32.const 112) "accepted")
  (func (export "test:kit/steps@0.1.0#begin") (param $start i64) (param $h f64)
    (global.set $step_draw (call $random))
    (global.set $post_loops (i32.mul (i32.trunc_sat_f64_u (local.get $h)) (i32.const 100)))
    (i64.store (i32.const 128) (global.get $step_draw))
    (call $report (i32.const 3) (i32.const 128) (i32.const 8)))
  (func (export "test:kit/steps@0.1.0#evaluate") (param $ptr i32) (param $len i32) (result i32)
    (local $x f64)
    (local $n i32)
    (local $held i32)
    (local $i i32)
    (local $area i32)
    (local $out i32)
    (global.set $mark (local.get $ptr))
    (local.set $x (f64.load (local.get $ptr)))
    (if (f64.eq (local.get $x) (f64.const -1)) (then (unreachable)))
    (if (f64.eq (local.get $x) (f64.const -2)) (then (loop $forever (br $forever))))
    (if (f64.eq (local.get $x) (f64.const -3))
      (then (call $send (i32.const 0) (i32.const 64) (i32.const 4))))
    (if (f64.eq (local.get $x) (f64.const -4))
      (then (call $set_timer (i32.const 9) (i64.const 5))))
    (if (f64.eq (local.get $x) (f64.const -5))
      (then (i64.store (i32.const 152) (i64.const 5))
            (call $report (i32.const 3) (i32.const 152) (i32.const 8))))
    (local.set $n (i32.trunc_sat_f64_u (local.get $x)))
    (local.set $held (select (local.get $n) (i32.const 64) (i32.lt_u (local.get $n) (i32.const 64))))
    (local.set $area (call $realloc (i32.const 0) (i32.const 0) (i32.const 4) (i32.const 8)))
    (local.set $out (call $realloc (i32.const 0) (i32.const 0) (i32.const 8)
                          (i32.mul (local.get $held) (i32.const 8))))
    (block $done
      (loop $each
        (br_if $done (i32.ge_u (local.get $i) (local.get $held)))
        (f64.store (i32.add (local.get $out) (i32.mul (local.get $i) (i32.const 8))) (f64.const 1.5))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (i32.store (local.get $area) (local.get $out))
    (i32.store offset=4 (local.get $area) (local.get $n))
    (local.get $area))
  (func (export "cabi_post_test:kit/steps@0.1.0#evaluate") (param $area i32)
    (local $i i32)
    (block $done
      (loop $spin
        (br_if $done (i32.ge_u (local.get $i) (global.get $post_loops)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $spin)))
    (global.set $bump (global.get $mark)))
  (func (export "test:kit/steps@0.1.0#accept") (param $ptr i32) (param $len i32)
    (i64.store (i32.const 136) (global.get $step_draw))
    (call $report (i32.const 2) (i32.const 136) (i32.const 8))
    (call $report (i32.const 0) (i32.const 112) (i32.const 8)))
  (func (export "test:kit/steps@0.1.0#reject")
    (i64.store (i32.const 144) (i64.const 1))
    (call $report (i32.const 2) (i32.const 144) (i32.const 8)))"#;

fn floats(values: &[f64]) -> Val {
    Val::List(values.iter().map(|v| Val::F64(*v)).collect())
}

/// What a call through the handle came to, as one word of the report.
fn said(r: Result<String, CallError>) -> (String, bool) {
    match r {
        Ok(text) => (text, false),
        Err(CallError::Faulted { kind, parameter }) => (
            format!("faulted:{}:{parameter}", core_world_code(kind)),
            true,
        ),
        Err(CallError::ResultTooLong { len, bound }) => (format!("too-long:{len}>{bound}"), false),
        Err(CallError::Refused(why)) => (format!("refused:{why}"), false),
        Err(CallError::RunEnded) => ("run-ended".into(), true),
    }
}

fn core_world_code(kind: FaultKind) -> &'static str {
    match kind {
        FaultKind::Trap => "trap",
        FaultKind::Budget => "budget",
        FaultKind::Stack => "stack",
        FaultKind::Memory => "memory",
        FaultKind::Quota => "quota",
        FaultKind::Contract => "contract",
    }
}

/// Play `script` against `actor` through `calls`; the report, a word per word of the script.
pub fn play(script: &str, actor: Identity, tick: u64, calls: &mut dyn CallIn) -> String {
    let _ = core_world::PACKAGE;
    let mut report = Vec::new();
    for word in script.split_whitespace() {
        let (name, arg) = word.split_once(':').unwrap_or((word, ""));
        let number = arg.parse::<f64>().unwrap_or(0.0);
        let result = match name {
            "open" => calls
                .open(actor, BEGIN, &[Val::U64(tick), Val::F64(number)])
                .map(|()| "opened".to_string()),
            "reopen" => calls
                .open(actor, BEGIN, &[Val::U64(tick), Val::F64(0.0)])
                .map(|()| "opened".to_string()),
            "eval" => calls
                .call(EVALUATE, &[floats(&[number, 0.0, 0.0])], Some(BOUND))
                .map(|v| match v {
                    Some(Val::List(items)) => format!("evaluated:{}", items.len()),
                    other => format!("unexpected:{other:?}"),
                }),
            "evals" => {
                let mut last = Ok(None);
                for _ in 0..arg.parse::<u32>().unwrap_or(0) {
                    last = calls.call(EVALUATE, &[floats(&[-5.0, 0.0, 0.0])], Some(BOUND));
                    if last.is_err() {
                        break;
                    }
                }
                last.map(|_| "evaluated".to_string())
            }
            "closed" => calls
                .call(EVALUATE, &[floats(&[1.0])], Some(BOUND))
                .map(|_| "evaluated".to_string()),
            "commit" => calls
                .commit(ACCEPT, &[floats(&[1.0])])
                .map(|()| "committed".to_string()),
            "discard" => calls.discard(REJECT, &[]).map(|()| "discarded".to_string()),
            "badseq" => calls
                .commit(EVALUATE, &[floats(&[1.0])])
                .map(|()| "committed".to_string()),
            "fault" => Err(calls.fault(FaultKind::Contract, 77)),
            other => Ok(format!("unknown:{other}")),
        };
        let (text, stop) = said(result);
        report.push(format!("{name}={text}"));
        if stop || calls.run_ended() {
            break;
        }
    }
    report.join(" ")
}
