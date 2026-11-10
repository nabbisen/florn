//! The adapter cases of floats, lists and the post-return call (PR-37; RFC 012 findings F-1 and
//! F-16; RFC 002 rule 2): the flat shapes the circuit domain needs, a list of floats lowered into
//! the guest and a list of floats lifted out of it, and the guest's buffers freed by
//! `cabi_post_<export>` after every call. They run over one prebuilt guest, [`DEVICE_WASM`], so
//! that the browser test page runs exactly what the native test runs; the native test checks that
//! the bytes are what `cases/device.wat` assembles to.
//!
//! [`run`] returns the cases and what became of each; an adapter's test asserts that none failed.

use florn_abi::core_world::post_return_name;
use florn_abi::{Flat, FlatType, Memory, Ty, Val, canonical_f64, lift_list, lower_flat_params};

use crate::{
    CallError, Callee, Engine, GlobalValue, ImportFunction, ImportHandler, ImportSpec, InstanceId,
    Refusal, Shape, call_lifting,
};

/// The guest of the cases, assembled from [`DEVICE_WAT`].
pub const DEVICE_WASM: &[u8] = include_bytes!("cases/device.wasm");
/// The guest's source.
pub const DEVICE_WAT: &str = include_str!("cases/device.wat");

const HOST: &str = "florn:circuit/host@0.1.0";
const BEGIN_STEP: &str = "florn:circuit/device@0.1.0#begin-step";
const ON_SAMPLE: &str = "florn:circuit/controller@0.1.0#on-sample";
const REJECT_STEP: &str = "florn:circuit/device@0.1.0#reject-step";
const EVALUATE: &str = "florn:circuit/device@0.1.0#evaluate";

/// What the cases need of the import spec: `set-output(port: u32, value: f64)`.
pub fn import_spec() -> ImportSpec {
    ImportSpec {
        functions: vec![ImportFunction {
            module: HOST.into(),
            name: "set-output".into(),
            params: vec![FlatType::I32, FlatType::F64],
            results: vec![],
        }],
        globals: vec![],
    }
}

/// One case and its outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub name: &'static str,
    /// `Ok` or what went wrong.
    pub outcome: Result<(), String>,
}

/// The import handler of the cases: `set-output` is recorded; anything else is refused.
#[derive(Default)]
struct Outputs {
    seen: Vec<(u32, f64)>,
}

impl ImportHandler for Outputs {
    fn call(
        &mut self,
        _: InstanceId,
        function: &str,
        args: &[Flat],
        _: &mut dyn Memory,
    ) -> Result<Vec<Flat>, Refusal> {
        match (function, args) {
            ("set-output", [Flat::I32(port), Flat::F64(v)]) => {
                self.seen.push((*port as u32, canonical_f64(*v)));
                Ok(Vec::new())
            }
            _ => Err(Refusal),
        }
    }
}

type Check<E> = fn(&mut E) -> Result<(), String>;

/// Run every case against adapters made by `make`; each case gets an engine of its own.
pub fn run<E: Engine>(make: &mut dyn FnMut() -> E) -> Vec<Case> {
    let cases: &[(&'static str, Check<E>)] = &[
        (
            "a float and an import with a float cross the call",
            floats_cross,
        ),
        (
            "two 64-bit integers and no parameters cross the call",
            integers_and_nothing,
        ),
        (
            "a list of floats goes in and a list of floats comes out",
            list_round_trip,
        ),
        (
            "the post-return call frees the result: a thousand calls leave the memory as it was",
            thousand_calls,
        ),
        (
            "without the post-return call the guest's memory grows",
            no_post_return_grows,
        ),
        (
            "a result that needs a post-return and has none resolved is refused",
            no_post_return_resolved,
        ),
        (
            "a NaN the guest returns is the canonical one",
            nan_is_canonical,
        ),
        (
            "a list longer than the memory is refused before it is lifted",
            list_longer_than_memory,
        ),
        (
            "a list result over the caller's bound is refused before it is lifted, and its post-return is not called",
            list_bound,
        ),
    ];
    cases
        .iter()
        .map(|(name, check)| {
            let mut engine = make();
            Case {
                name,
                outcome: check(&mut engine),
            }
        })
        .collect()
}

struct Guest<E: Engine> {
    inst: E::Instance,
    handler: Outputs,
}

fn guest<E: Engine>(e: &mut E) -> Result<Guest<E>, String> {
    let module = e
        .compile(DEVICE_WASM)
        .map_err(|x| format!("compile: {x}"))?;
    let inst = e
        .instantiate(&module, &import_spec())
        .map_err(|x| format!("instantiate: {x}"))?;
    Ok(Guest {
        inst,
        handler: Outputs::default(),
    })
}

fn shape_of(params: &[Ty], result: Option<&Ty>) -> Shape {
    let (p, r) = Ty::export_signature(params, result);
    Shape::of(&p, &r).expect("the signature is in the table")
}

fn func<E: Engine>(
    e: &mut E,
    g: &mut Guest<E>,
    name: &str,
    params: &[Ty],
    result: Option<&Ty>,
) -> Result<E::Func, String> {
    e.func(&mut g.inst, name, shape_of(params, result))
        .ok_or_else(|| format!("no export {name}"))
}

fn post<E: Engine>(e: &mut E, g: &mut Guest<E>, name: &str) -> Result<E::Func, String> {
    e.func(&mut g.inst, &post_return_name(name), Shape::POST_RETURN)
        .ok_or_else(|| format!("no export {}", post_return_name(name)))
}

fn lower<E: Engine>(
    e: &mut E,
    g: &mut Guest<E>,
    tys: &[Ty],
    vals: &[Val],
) -> Result<Vec<Flat>, String> {
    let mut out = Err("no memory".to_string());
    e.with_memory(&mut g.inst, &mut |m| {
        out = lower_flat_params(m, tys, vals).map_err(|x| format!("lowering: {x}"));
    })
    .map_err(|x| x.to_string())?;
    out
}

fn read<E: Engine>(e: &mut E, g: &mut Guest<E>, addr: u32, len: u32) -> Result<Vec<u8>, String> {
    let mut out = Err("no memory".to_string());
    e.with_memory(&mut g.inst, &mut |m| {
        out = m.read(addr, len).map_err(|x| x.to_string());
    })
    .map_err(|x| x.to_string())?;
    out
}

fn top<E: Engine>(e: &mut E, g: &mut Guest<E>) -> Result<i32, String> {
    match e.global_get(&mut g.inst, None, "top") {
        Some(GlobalValue::I32(v)) => Ok(v),
        other => Err(format!("global top: {other:?}")),
    }
}

fn expect<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{what}: got {got:?}, want {want:?}"))
    }
}

fn floats_cross<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    let tys = [Ty::U64, Ty::F64];
    let f = func(e, &mut g, BEGIN_STEP, &tys, None)?;
    let start = 0x0123_4567_89ab_cdef_u64;
    let flats = lower(e, &mut g, &tys, &[Val::U64(start), Val::F64(0.25)])?;
    e.call(&mut g.inst, &f, &flats, &mut g.handler)
        .map_err(|t| t.to_string())?;
    let cells = read(e, &mut g, 0, 16)?;
    expect(
        "start",
        u64::from_le_bytes(cells[0..8].try_into().unwrap()),
        start,
    )?;
    expect(
        "h",
        f64::from_le_bytes(cells[8..16].try_into().unwrap()),
        0.25,
    )?;
    // The guest called the import with a float: port 7 and h * 2.
    expect("set-output", g.handler.seen.clone(), vec![(7, 0.5)])
}

fn integers_and_nothing<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    let tys = [Ty::U64, Ty::U64];
    let f = func(e, &mut g, ON_SAMPLE, &tys, None)?;
    let flats = lower(
        e,
        &mut g,
        &tys,
        &[Val::U64(1_000_000_007), Val::U64(u64::MAX)],
    )?;
    e.call(&mut g.inst, &f, &flats, &mut g.handler)
        .map_err(|t| t.to_string())?;
    let cells = read(e, &mut g, 16, 16)?;
    expect(
        "now",
        u64::from_le_bytes(cells[0..8].try_into().unwrap()),
        1_000_000_007,
    )?;
    expect(
        "index",
        u64::from_le_bytes(cells[8..16].try_into().unwrap()),
        u64::MAX,
    )?;
    let reject = func(e, &mut g, REJECT_STEP, &[], None)?;
    for _ in 0..3 {
        e.call(&mut g.inst, &reject, &[], &mut g.handler)
            .map_err(|t| t.to_string())?;
    }
    let count = read(e, &mut g, 32, 4)?;
    expect("rejects", u32::from_le_bytes(count.try_into().unwrap()), 3)
}

fn floats(n: usize) -> Vec<Val> {
    (0..n).map(|i| Val::F64(i as f64 - 2.5)).collect()
}

/// One call of `evaluate` over `inputs`, with the post-return call.
fn evaluate<E: Engine>(
    e: &mut E,
    g: &mut Guest<E>,
    inputs: Vec<Val>,
    with_post: bool,
) -> Result<Vec<f64>, String> {
    let params = [Ty::list(Ty::F64)];
    let result = Ty::list(Ty::F64);
    let f = func(e, g, EVALUATE, &params, Some(&result))?;
    let p = if with_post {
        Some(post(e, g, EVALUATE)?)
    } else {
        None
    };
    let flats = lower(e, g, &params, &[Val::List(inputs)])?;
    // Without the post-return resolved the result still needs one: the call is for the control that
    // shows what the guest's memory does without it, so the call is made and lifted by hand.
    let value = if with_post {
        call_lifting(
            e,
            &mut g.inst,
            Callee {
                func: &f,
                post: p.as_ref(),
                result: Some(&result),
                list_bound: None,
            },
            &flats,
            &mut g.handler,
        )
        .map_err(|x| format!("{x:?}"))?
    } else {
        let out = e
            .call(&mut g.inst, &f, &flats, &mut g.handler)
            .map_err(|t| t.to_string())?;
        let mut lifted = Ok(None);
        e.with_memory(&mut g.inst, &mut |m| {
            lifted = florn_abi::lift_results(m, Some(&result), &out);
        })
        .map_err(|x| x.to_string())?;
        lifted.map_err(|x| x.to_string())?
    };
    match value {
        Some(Val::List(items)) => items
            .into_iter()
            .map(|v| match v {
                Val::F64(x) => Ok(x),
                other => Err(format!("not a float: {other:?}")),
            })
            .collect(),
        other => Err(format!("not a list: {other:?}")),
    }
}

fn list_round_trip<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    let before = top(e, &mut g)?;
    let out = evaluate(
        e,
        &mut g,
        vec![
            Val::F64(1.0),
            Val::F64(-2.5),
            Val::F64(0.0),
            Val::F64(1e300),
        ],
        true,
    )?;
    expect("outputs", out, vec![2.5, -4.5, 0.5, 2e300 + 0.5])?;
    // The post-return was called once, with the pointer `evaluate` returned.
    let cells = read(e, &mut g, 40, 12)?;
    let word = |i: usize| u32::from_le_bytes(cells[i..i + 4].try_into().unwrap());
    expect("post-return calls", word(4), 1)?;
    expect("post-return argument", word(0), word(8))?;
    expect("top after the post-return", top(e, &mut g)?, before)
}

fn thousand_calls<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    // One call first: the memory grows to hold the first call's buffers, once.
    evaluate(e, &mut g, floats(512), true)?;
    let (size, mark) = (e.memory_size(&mut g.inst), top(e, &mut g)?);
    for i in 0..1000 {
        let out = evaluate(e, &mut g, floats(512), true)?;
        if out.len() != 512 {
            return Err(format!("call {i}: {} outputs", out.len()));
        }
    }
    let count = read(e, &mut g, 44, 4)?;
    expect(
        "post-return calls",
        u32::from_le_bytes(count.try_into().unwrap()),
        1001,
    )?;
    expect("memory size", e.memory_size(&mut g.inst), size)?;
    expect("top", top(e, &mut g)?, mark)
}

fn no_post_return_grows<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    let (size, mark) = (e.memory_size(&mut g.inst), top(e, &mut g)?);
    for _ in 0..100 {
        evaluate(e, &mut g, floats(512), false)?;
    }
    if e.memory_size(&mut g.inst) <= size || top(e, &mut g)? <= mark {
        return Err("the measurement does not see the leak it is there to see".into());
    }
    Ok(())
}

fn no_post_return_resolved<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    let params = [Ty::list(Ty::F64)];
    let result = Ty::list(Ty::F64);
    let f = func(e, &mut g, EVALUATE, &params, Some(&result))?;
    let flats = lower(e, &mut g, &params, &[Val::List(floats(3))])?;
    match call_lifting(
        e,
        &mut g.inst,
        Callee {
            func: &f,
            post: None,
            result: Some(&result),
            list_bound: None,
        },
        &flats,
        &mut g.handler,
    ) {
        Err(CallError::NoPostReturn) => Ok(()),
        other => Err(format!("{other:?}")),
    }
}

fn nan_is_canonical<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    // The guest returns 0x7ff8dead00000001 for an empty list; the lift makes it canonical.
    let out = evaluate(e, &mut g, vec![], true)?;
    match out.as_slice() {
        [x] if x.to_bits() == canonical_f64(f64::NAN).to_bits() => Ok(()),
        other => Err(format!(
            "{:?}",
            other
                .iter()
                .map(|x| format!("{:#x}", x.to_bits()))
                .collect::<Vec<_>>()
        )),
    }
}

fn list_longer_than_memory<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    let mut outcome = Err("no memory".to_string());
    e.with_memory(&mut g.inst, &mut |m| {
        // 2^29 floats are 4 GiB: more than the memory holds and more than a u32 of bytes; and
        // one that fits a u32 of bytes but not the memory.
        let too_many = lift_list(m, &Ty::F64, 0, 1 << 29);
        let too_long = lift_list(m, &Ty::F64, 0, (m.size() / 8) + 1);
        outcome = match (too_many, too_long) {
            (Err(_), Err(_)) => Ok(()),
            (a, b) => Err(format!("lifted: {:?} {:?}", a.map(|_| ()), b.map(|_| ()))),
        };
    })
    .map_err(|x| x.to_string())?;
    outcome
}

fn list_bound<E: Engine>(e: &mut E) -> Result<(), String> {
    let mut g = guest(e)?;
    let params = [Ty::list(Ty::F64)];
    let result = Ty::list(Ty::F64);
    let f = func(e, &mut g, EVALUATE, &params, Some(&result))?;
    let p = post(e, &mut g, EVALUATE)?;
    let call = |e: &mut E, g: &mut Guest<E>, n: usize, bound: u32| {
        let flats = lower(e, g, &params, &[Val::List(floats(n))])?;
        Ok::<_, String>(call_lifting(
            e,
            &mut g.inst,
            Callee {
                func: &f,
                post: Some(&p),
                result: Some(&result),
                list_bound: Some(bound),
            },
            &flats,
            &mut g.handler,
        ))
    };
    // Within the bound, the list comes back and the post-return runs.
    match call(e, &mut g, 5, 5)? {
        Ok(Some(Val::List(v))) if v.len() == 5 => {}
        other => return Err(format!("at the bound: {other:?}")),
    }
    let calls = |e: &mut E, g: &mut Guest<E>| {
        read(e, g, 44, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    expect("post-return calls at the bound", calls(e, &mut g)?, 1)?;
    // One beyond it: refused, and the guest's post-return was not called (the result was not
    // taken). The list was 6 floats that lie inside the memory and would have lifted.
    match call(e, &mut g, 6, 5)? {
        Err(CallError::ResultTooLong { len: 6, bound: 5 }) => {}
        other => return Err(format!("beyond the bound: {other:?}")),
    }
    expect("post-return calls beyond the bound", calls(e, &mut g)?, 1)
}
