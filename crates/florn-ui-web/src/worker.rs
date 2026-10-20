//! The worker module's exports.

use std::cell::RefCell;
use std::fmt::Write as _;

use florn_control::{Answer, Command, Envelope, Message, Query, SeedChoice, Subscription, Unit};
use florn_engine::bench;
use florn_engine_web::WebEngine;
use florn_host::{Controller, Options};
use florn_platform_web::WebPlatform;

thread_local! {
    static CONTROLLER: RefCell<Option<Controller<WebEngine>>> = const { RefCell::new(None) };
    /// The text or bytes of the last reply, read through `florn_out_ptr` and `florn_out_len`.
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn set_out(bytes: Vec<u8>) {
    OUT.with(|o| *o.borrow_mut() = bytes);
}

/// A buffer of `len` zero bytes the worker fills and then passes back by pointer and length.
#[unsafe(no_mangle)]
pub extern "C" fn florn_alloc(len: u32) -> *mut u8 {
    let mut v = vec![0u8; len as usize];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Take back a buffer of `florn_alloc`.
///
/// # Safety
/// `ptr` came from `florn_alloc(len)` and is taken back once.
unsafe fn take(ptr: *mut u8, len: u32) -> Vec<u8> {
    unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) }
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_out_len() -> u32 {
    OUT.with(|o| o.borrow().len() as u32)
}

/// The Host over this worker, with the capacity the page gives (RFC 003 rule 20).
#[unsafe(no_mangle)]
pub extern "C" fn florn_start(instances: u32, memory_bytes: f64) -> i32 {
    let platform = WebPlatform::with_capacity(u64::from(instances), memory_bytes.max(0.0) as u64);
    let c = Controller::new(
        WebEngine::new(),
        florn_host::run::default_admission(),
        Box::new(platform),
        Options {
            keep_records: false,
            keep_payloads: false,
            ..Options::browser()
        },
    );
    CONTROLLER.with(|slot| *slot.borrow_mut() = Some(c));
    0
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opt_str(v: &Option<String>) -> String {
    v.as_deref().map_or("null".into(), json_str)
}

/// One reply as JSON: a notification as RFC 007's JSON line, an answer by kind.
fn message_json(m: &Message) -> String {
    match m {
        Message::Notification(n) => n.to_json(),
        Message::Answer(a) => match a {
            Answer::RunState(r) => format!(
                "{{\"kind\":\"run-state\",\"state\":\"{}\",\"handled\":{},\"records\":{},\"chain\":{},\"seed\":{},\"ended\":{},\"reproducible\":{},\"run_id\":{}}}",
                r.state.as_str(),
                r.handled,
                r.records,
                opt_str(&r.chain),
                r.seed.map_or("null".into(), |s| s.to_string()),
                opt_str(&r.ended),
                r.reproducible,
                opt_str(&r.run_id)
            ),
            Answer::Admitted {
                module,
                admitted_sha256,
            } => format!(
                "{{\"kind\":\"admitted\",\"module\":{},\"source\":{},\"admitted\":{},\"contract\":{},\"module_kind\":{}}}",
                json_str(&module.name),
                json_str(&module.source_sha256),
                json_str(admitted_sha256),
                json_str(&module.contract),
                json_str(&module.kind)
            ),
            Answer::Support(s) => format!(
                "{{\"kind\":\"support\",\"engine\":{},\"engine_version\":{},\"host_build\":{},\"arch\":{},\"florn\":{},\"control\":{}}}",
                json_str(&s.engine),
                json_str(&s.engine_version),
                json_str(&s.host_build),
                json_str(&s.arch),
                json_str(&s.florn_version),
                s.control_version
            ),
            Answer::Capacity(c) => format!(
                "{{\"kind\":\"capacity\",\"instances\":{},\"memory_bytes\":{}}}",
                c.instances, c.memory_bytes
            ),
            Answer::Exported { name, bytes, .. } => format!(
                "{{\"kind\":\"exported\",\"name\":{},\"len\":{}}}",
                json_str(name),
                bytes.len()
            ),
            Answer::Done => "{\"kind\":\"done\"}".into(),
            other => format!(
                "{{\"kind\":{}}}",
                json_str(&format!("{other:?}").chars().take(40).collect::<String>())
            ),
        },
        Message::Telemetry(t) => format!(
            "{{\"kind\":\"telemetry\",\"actor\":{},\"channel\":{},\"len\":{}}}",
            t.actor,
            t.channel,
            t.payload.len()
        ),
        Message::Progress(p) => format!(
            "{{\"kind\":\"progress\",\"handled\":{},\"records\":{}}}",
            p.handled, p.records
        ),
        other => format!(
            "{{\"kind\":{}}}",
            json_str(&format!("{other:?}").chars().take(40).collect::<String>())
        ),
    }
}

fn replies_json(replies: &[Message]) -> String {
    let items: Vec<String> = replies.iter().map(message_json).collect();
    format!("[{}]", items.join(","))
}

/// Send one message to the controller; the replies as JSON in `OUT`; their count, or -1
/// before `florn_start`.
fn send(m: Message) -> i32 {
    CONTROLLER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else {
            set_out(b"[]".to_vec());
            return -1;
        };
        let replies = c.handle(Envelope::new(m));
        let n = replies.len() as i32;
        set_out(replies_json(&replies).into_bytes());
        n
    })
}

/// # Safety
/// `ptr` and `len` are a buffer of `florn_alloc`, taken back here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn florn_load_scenario(ptr: *mut u8, len: u32) -> i32 {
    let bytes = unsafe { take(ptr, len) };
    send(Message::Command(Command::LoadScenario { bytes }))
}

/// # Safety
/// As `florn_load_scenario`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn florn_load_module(ptr: *mut u8, len: u32) -> i32 {
    let bytes = unsafe { take(ptr, len) };
    send(Message::Command(Command::LoadModule { bytes }))
}

/// # Safety
/// As `florn_load_scenario`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn florn_admit(ptr: *mut u8, len: u32) -> i32 {
    let bytes = unsafe { take(ptr, len) };
    send(Message::Command(Command::Admit { bytes }))
}

/// The admission step on its own, under the conformance kit's limits (the defaults with 2 MiB
/// of memory): the source and admitted hashes, as the kit records them (RFC 003 §10,
/// transformed bytes).
///
/// # Safety
/// As `florn_load_scenario`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn florn_admit_kit(ptr: *mut u8, len: u32) -> i32 {
    let bytes = unsafe { take(ptr, len) };
    let mut limits = Options::default().class_defaults;
    limits.memory_bytes = 2 * 1024 * 1024;
    match florn_host::run::default_admission().admit("module", &bytes, &limits) {
        Ok(a) => {
            set_out(
                format!(
                    "[{{\"kind\":\"admitted\",\"source\":{},\"admitted\":{}}}]",
                    json_str(&a.source_hex()),
                    json_str(&a.admitted_hex())
                )
                .into_bytes(),
            );
            0
        }
        Err(e) => {
            set_out(
                format!(
                    "[{{\"kind\":\"refused\",\"codes\":{}}}]",
                    json_str(&format!("{:?}", e.codes()))
                )
                .into_bytes(),
            );
            -1
        }
    }
}

/// A new run: the seed given as two words when `given` is not zero, else chosen by the platform.
#[unsafe(no_mangle)]
pub extern "C" fn florn_new_run(seed_lo: u32, seed_hi: u32, given: i32) -> i32 {
    let seed = if given != 0 {
        SeedChoice::Given((u64::from(seed_hi) << 32) | u64::from(seed_lo))
    } else {
        SeedChoice::Choose
    };
    send(Message::Command(Command::NewRun { seed }))
}

/// Advance to the tick given as two words.
#[unsafe(no_mangle)]
pub extern "C" fn florn_advance(tick_lo: u32, tick_hi: u32) -> i32 {
    send(Message::Command(Command::Advance {
        unit: Unit::ToTick((u64::from(tick_hi) << 32) | u64::from(tick_lo)),
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_subscribe_events() -> i32 {
    send(Message::Subscribe(Subscription::Events))
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_run_state() -> i32 {
    send(Message::Query(Query::RunState))
}

#[unsafe(no_mangle)]
pub extern "C" fn florn_support() -> i32 {
    send(Message::Query(Query::Support))
}

/// A slice of a paced advance (RFC 005 rule 26): the controller's `pump`.
#[unsafe(no_mangle)]
pub extern "C" fn florn_pump(max_ticks: u32) -> i32 {
    CONTROLLER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else {
            return -1;
        };
        let replies = c.pump(u64::from(max_ticks));
        let n = replies.len() as i32;
        set_out(replies_json(&replies).into_bytes());
        n
    })
}

/// The transport's form: one encoded control message in, the replies as JSON out.
///
/// # Safety
/// As `florn_load_scenario`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn florn_handle(ptr: *mut u8, len: u32) -> i32 {
    let bytes = unsafe { take(ptr, len) };
    CONTROLLER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else {
            return -1;
        };
        let replies: Vec<Message> = match florn_control::decode(&bytes) {
            Ok(env) => c.handle(env),
            Err(e) => vec![Message::Notification(e.notification())],
        };
        let n = replies.len() as i32;
        set_out(replies_json(&replies).into_bytes());
        n
    })
}

/// The crossing benchmark (RFC 005 rule 11) over the kit's crossing guest, given as bytes:
/// the figures as JSON.
///
/// # Safety
/// As `florn_load_scenario`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn florn_bench_crossing(ptr: *mut u8, len: u32, scale: u32) -> i32 {
    let raw = unsafe { take(ptr, len) };
    match bench_crossing(&raw, u64::from(scale)) {
        Ok(json) => {
            set_out(json.into_bytes());
            0
        }
        Err(e) => {
            set_out(json_str(&e).into_bytes());
            -1
        }
    }
}

fn admit(name: &str, raw: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut limits = Options::default().class_defaults;
    limits.memory_bytes = 2 * 1024 * 1024;
    let admitted = florn_host::run::default_admission()
        .admit(name, raw, &limits)
        .map_err(|e| format!("{:?}", e.codes()))?;
    let config =
        florn_host::run::default_config(&admitted).map_err(|e| format!("{:?}", e.codes()))?;
    Ok((admitted.bytes.clone(), config))
}

fn bench_crossing(raw: &[u8], scale: u64) -> Result<String, String> {
    let (metered, config) = admit("crossing", raw)?;
    let spec = bench::core_import_spec();
    let clock = florn_platform_web::now_seconds;
    let figures = bench::crossing(
        &mut WebEngine::new,
        bench::Subject {
            name: "crossing",
            raw,
            metered: &metered,
            config: &config,
        },
        &spec,
        scale,
        &clock,
    )?;
    let items: Vec<String> = figures
        .iter()
        .map(|c| {
            format!(
                "{{\"bytes\":{},\"metered\":{},\"micros\":{}}}",
                c.bytes, c.metered, c.micros
            )
        })
        .collect();
    Ok(format!("[{}]", items.join(",")))
}

/// The cost benchmark (RFC 003 section 7, step 2) for one guest given as bytes, named so that
/// the message of each activation is the guest's: the figure as JSON.
///
/// # Safety
/// As `florn_load_scenario`, for both buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn florn_bench_cost(
    ptr: *mut u8,
    len: u32,
    name_ptr: *mut u8,
    name_len: u32,
    activations: u32,
) -> i32 {
    let raw = unsafe { take(ptr, len) };
    let name = String::from_utf8_lossy(&unsafe { take(name_ptr, name_len) }).into_owned();
    match bench_cost(&name, &raw, u64::from(activations)) {
        Ok(json) => {
            set_out(json.into_bytes());
            0
        }
        Err(e) => {
            set_out(json_str(&e).into_bytes());
            -1
        }
    }
}

fn bench_cost(name: &str, raw: &[u8], activations: u64) -> Result<String, String> {
    let (metered, config) = admit(name, raw)?;
    let spec = bench::core_import_spec();
    let clock = florn_platform_web::now_seconds;
    let protocol = name == "bench_protocol";
    let payload = move |ordinal: u64| {
        if protocol {
            bench::protocol_buffer(ordinal)
        } else {
            ordinal.to_le_bytes().to_vec()
        }
    };
    let c = bench::cost(
        &mut WebEngine::new,
        bench::Subject {
            name,
            raw,
            metered: &metered,
            config: &config,
        },
        &spec,
        activations,
        &payload,
        &clock,
    )?;
    Ok(format!(
        "{{\"guest\":{},\"activations\":{},\"unmetered_micros\":{},\"metered_micros\":{}}}",
        json_str(&c.guest),
        c.activations,
        c.unmetered_micros,
        c.metered_micros
    ))
}
