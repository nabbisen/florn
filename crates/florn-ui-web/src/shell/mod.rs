//! The browser shell (RFC 005 rules 38 to 41; external design §3): Leptos on the main thread
//! over the view model and the scene. The page's loader (`www/app.js`) owns the worker, the
//! service worker and the display transport, and hands this module a `Bridge` of the few
//! things it needs from the page; this module hands the loader every control message to send
//! and draws the scene once per animation frame from what the loader read.
//!
//! The rules learned in Stage 0 (rule 39) hold here: the 60 Hz path never enters the reactive
//! graph (the view model lives in a `RefCell`, the panels re-render from a revision signal
//! bumped a few times per second); a field is set only when the selection changes and never
//! while a person types; no panic escapes a handler (`try_borrow`); every list is one Tab
//! stop; the canvas has its twin in the actors list and its description; every string is a
//! catalogue key; status is never colour alone.
mod catalogue;
mod draw;
mod i18n_gen;
mod panels;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use florn_ui_model::florn_control::display::decode_all;
use florn_ui_model::florn_control::{Envelope, FileKind, Pacing, decode, encode};
use florn_ui_model::{Selection, ViewModel};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use i18n_gen::i18n::*;

pub use catalogue::Lang;

/// What the page gives the shell (duck-typed, defined in `www/app.js`).
#[wasm_bindgen]
extern "C" {
    pub type Bridge;
    /// Post one encoded control message to the Host's worker.
    #[wasm_bindgen(method)]
    fn send(this: &Bridge, bytes: &[u8]);
    /// The ticks the worker advances per slice while running.
    #[wasm_bindgen(method)]
    fn pace(this: &Bridge, ticks_per_slice: f64);
    /// Every display record since the last read, as bytes.
    #[wasm_bindgen(method, js_name = readDisplay)]
    fn read_display(this: &Bridge) -> js_sys::Uint8Array;
    /// Hand a file to the person.
    #[wasm_bindgen(method)]
    fn download(this: &Bridge, name: &str, bytes: &[u8], mime: &str);
    /// Fetch a bundled scenario and its modules; the loader calls `shell_module` and
    /// `shell_scenario` with them.
    #[wasm_bindgen(method, js_name = loadLab)]
    fn load_lab(this: &Bridge, name: &str);
    /// Ask for a file of a kind (`scenario`, `record`, `module`); the loader calls `shell_file`.
    #[wasm_bindgen(method, js_name = pickFile)]
    fn pick_file(this: &Bridge, kind: &str);
    /// The operating system's reduced-motion preference.
    #[wasm_bindgen(method, js_name = reducedMotion)]
    fn reduced_motion(this: &Bridge) -> bool;
    /// Where the page came from and whether it is isolated, for the about line.
    #[wasm_bindgen(method)]
    fn origin(this: &Bridge) -> String;
    #[wasm_bindgen(method)]
    fn isolated(this: &Bridge) -> bool;
}

/// The step units a person can choose (§3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepUnit {
    Event,
    Instant,
    Tick,
}

/// The speeds a person can choose: logical ticks per wall-clock second (§3.3).
pub const SPEEDS: [u64; 5] = [100, 1_000, 10_000, 100_000, 0];
/// Slices per second the worker paces at.
const SLICES_PER_SECOND: f64 = 60.0;

/// What a lab is: its scenario's name, for the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lab {
    pub name: String,
}

/// State the panels share: the view model, non-reactive; a few signals for what a person
/// changes and what the status line shows a few times a second.
#[derive(Clone)]
pub struct Shared {
    pub vm: Rc<RefCell<ViewModel>>,
    bridge: Rc<Bridge>,
    /// Bumped when the view model changed in a way the panels show.
    pub rev: RwSignal<u64>,
    /// The status line's revision, at most four times a second.
    pub status_rev: RwSignal<u64>,
    /// Draw time in milliseconds: median and 95th percentile of the last second.
    pub frame: RwSignal<(f64, f64)>,
    /// 0 unknown, 1 available offline, 2 not available.
    pub offline: RwSignal<u8>,
    pub labs: RwSignal<Vec<Lab>>,
    pub step_unit: RwSignal<StepUnit>,
    pub speed: RwSignal<u64>,
    dirty: Rc<Cell<bool>>,
    last_status: Rc<Cell<f64>>,
    canvas: Rc<RefCell<Option<web_sys::HtmlCanvasElement>>>,
    draws: Rc<RefCell<Vec<f64>>>,
    frames: Rc<Cell<u64>>,
    last_report: Rc<Cell<f64>>,
}

thread_local! {
    static SHARED: RefCell<Option<Shared>> = const { RefCell::new(None) };
    /// The codes of the last notifications received, for the journeys and for support.
    static CODES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The shared state, for the panels (mounted after `shell_start` set it).
pub fn shared() -> Option<Shared> {
    SHARED.with(|s| s.borrow().clone())
}

fn with_shared(f: impl FnOnce(&Shared)) {
    SHARED.with(|s| {
        if let Some(s) = s.borrow().as_ref() {
            f(s);
        }
    });
}

impl Shared {
    /// Send what the view model wants sent.
    pub fn flush(&self) {
        let out = match self.vm.try_borrow_mut() {
            Ok(mut vm) => vm.take_outbox(),
            Err(_) => return,
        };
        for m in out {
            let bytes = encode(&Envelope::new(m));
            self.bridge.send(&bytes);
        }
        self.dirty.set(true);
    }
    /// Change the view model as a person asked: the panels re-render at once.
    pub fn update(&self, f: impl FnOnce(&mut ViewModel)) {
        if let Ok(mut vm) = self.vm.try_borrow_mut() {
            f(&mut vm);
        }
        self.flush();
        self.rev.update(|r| *r += 1);
    }
    pub fn read<T>(&self, f: impl FnOnce(&ViewModel) -> T) -> Option<T> {
        self.vm.try_borrow().ok().map(|vm| f(&vm))
    }
    pub fn select(&self, s: Selection) {
        self.update(|vm| vm.select(s));
    }
    pub fn set_speed(&self, ticks_per_second: u64) {
        self.speed.set(ticks_per_second);
        let pacing = if ticks_per_second == 0 {
            // As fast as possible: a slice that reaches the end.
            self.bridge.pace(f64::from(u32::MAX));
            Pacing::TicksPerSecond(u64::from(u32::MAX) * 60)
        } else {
            self.bridge
                .pace((ticks_per_second as f64 / SLICES_PER_SECOND).max(1.0));
            Pacing::TicksPerSecond(ticks_per_second)
        };
        self.update(|vm| vm.set_speed(pacing));
    }
    pub fn load_lab(&self, name: &str) {
        self.bridge.load_lab(name);
    }
    pub fn pick_file(&self, kind: &str) {
        self.bridge.pick_file(kind);
    }
    pub fn save_scenario(&self) {
        let Some((name, text)) =
            self.read(|vm| (vm.draft.name.committed.clone(), vm.draft.to_toml()))
        else {
            return;
        };
        let file = format!("{}.toml", if name.is_empty() { "scenario" } else { &name });
        self.bridge
            .download(&file, text.as_bytes(), "application/toml");
    }
    pub fn about(&self) -> (String, bool, String) {
        (
            self.bridge.origin(),
            self.bridge.isolated(),
            env!("CARGO_PKG_VERSION").to_string(),
        )
    }
}

/// One encoded reply from the Host.
#[wasm_bindgen]
pub fn shell_reply(bytes: &[u8]) {
    with_shared(|s| {
        let Ok(env) = decode(bytes) else { return };
        if let florn_ui_model::florn_control::Message::Notification(n) = &env.message {
            CODES.with(|c| {
                let mut c = c.borrow_mut();
                c.push(n.code.clone());
                if c.len() > 50 {
                    c.remove(0);
                }
            });
        }
        if let Ok(mut vm) = s.vm.try_borrow_mut() {
            vm.apply(&env.message);
        }
        s.flush();
        // A file the Host built goes to the person now.
        let exported =
            s.vm.try_borrow_mut()
                .ok()
                .and_then(|mut vm| vm.take_exported());
        if let Some((kind, name, bytes)) = exported {
            let mime = match kind {
                FileKind::Manifest => "application/toml",
                FileKind::DiagnosticLog => "application/x-ndjson",
                _ => "application/octet-stream",
            };
            s.bridge.download(&name, &bytes, mime);
        }
    });
}

/// A scenario's text to open (a lab or a file).
#[wasm_bindgen]
pub fn shell_scenario(text: &[u8]) {
    with_shared(|s| s.update(|vm| vm.open_scenario(text.to_vec())));
}

/// A module the scenario references, by its bytes.
#[wasm_bindgen]
pub fn shell_module(bytes: &[u8]) {
    with_shared(|s| s.update(|vm| vm.load_module(bytes.to_vec())));
}

/// A module for the palette: admitted and shown before it can be placed (§3.9), and held
/// by the Host for the runs of the draft.
#[wasm_bindgen]
pub fn shell_admit(bytes: &[u8]) {
    with_shared(|s| {
        s.update(|vm| {
            vm.admit(bytes.to_vec());
            vm.load_module(bytes.to_vec());
        })
    });
}

/// A file the person chose: `scenario`, `record` (to replay) or `module`.
#[wasm_bindgen]
pub fn shell_file(kind: &str, bytes: &[u8]) {
    with_shared(|s| match kind {
        "scenario" => s.update(|vm| vm.open_scenario(bytes.to_vec())),
        "record" => s.update(|vm| vm.replay(bytes.to_vec())),
        "module" => s.update(|vm| {
            vm.admit(bytes.to_vec());
            vm.load_module(bytes.to_vec());
        }),
        _ => {}
    });
}

/// The bundled labs, by name, one per line.
#[wasm_bindgen]
pub fn shell_labs(names: &str) {
    with_shared(|s| {
        s.labs.set(
            names
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| Lab {
                    name: l.trim().to_string(),
                })
                .collect(),
        )
    });
}

/// Whether the page is cached for offline use (rule 35).
#[wasm_bindgen]
pub fn shell_offline(ready: bool) {
    with_shared(|s| s.offline.set(if ready { 1 } else { 2 }));
}

/// The 60 Hz path (rule 39): the display records read by the loader go to the view model and
/// the scene is drawn; the panels are told a few times a second.
#[wasm_bindgen]
pub fn shell_frame(ts: f64) {
    with_shared(|s| {
        let records = s.bridge.read_display().to_vec();
        if let Ok(mut vm) = s.vm.try_borrow_mut()
            && !records.is_empty()
            && let Some(decoded) = decode_all(&records)
        {
            vm.display(&decoded);
            s.dirty.set(true);
        }
        s.frames.set(s.frames.get() + 1);
        let reduced = s.bridge.reduced_motion();
        if !reduced || s.frames.get().is_multiple_of(4) {
            draw_now(s);
        }
        if ts - s.last_status.get() >= 250.0 {
            s.last_status.set(ts);
            if s.dirty.replace(false) {
                s.rev.update(|r| *r += 1);
            }
            s.status_rev.update(|r| *r += 1);
        }
        if ts - s.last_report.get() >= 1000.0 {
            s.last_report.set(ts);
            let mut d = s.draws.borrow_mut();
            d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let q = |p: f64| {
                if d.is_empty() {
                    0.0
                } else {
                    d[((d.len() - 1) as f64 * p).round() as usize]
                }
            };
            s.frame.set((q(0.5), q(0.95)));
            d.clear();
        }
    });
}

fn draw_now(s: &Shared) {
    let Some(canvas) = s.canvas.borrow().clone() else {
        return;
    };
    // The canvas's size follows the page (a panel grew, the window changed): its drawing size
    // and the viewport are kept at its size, so that what is drawn is where it is clicked.
    let dpr = window().device_pixel_ratio();
    let (w, h) = (
        f64::from(canvas.client_width()),
        f64::from(canvas.client_height()),
    );
    if w > 0.0 && h > 0.0 {
        let (pw, ph) = ((w * dpr) as u32, (h * dpr) as u32);
        let size_differs =
            s.vm.try_borrow()
                .map(|vm| {
                    (f64::from(vm.topology.viewport.size.x) - w).abs() > 0.5
                        || (f64::from(vm.topology.viewport.size.y) - h).abs() > 0.5
                })
                .unwrap_or(false);
        if canvas.width() != pw || canvas.height() != ph || size_differs {
            canvas.set_width(pw);
            canvas.set_height(ph);
            if let Ok(mut vm) = s.vm.try_borrow_mut() {
                vm.topology.viewport.size = florn_ui_model::Point {
                    x: w as f32,
                    y: h as f32,
                };
                // The whole topology stays in view at the new size.
                florn_ui_scene::fit_all(&mut vm);
            }
        }
    }
    let Ok(vm) = s.vm.try_borrow() else {
        return;
    };
    let perf = window().performance();
    let t0 = perf.as_ref().map_or(0.0, |p| p.now());
    let scene = florn_ui_scene::build(&vm, vm.topology.instant);
    let ctx: Option<web_sys::CanvasRenderingContext2d> = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into().ok());
    if let Some(ctx) = ctx {
        draw::draw(
            &ctx,
            &scene,
            dpr,
            f64::from(vm.topology.viewport.size.x),
            f64::from(vm.topology.viewport.size.y),
        );
    }
    s.draws
        .borrow_mut()
        .push(perf.as_ref().map_or(0.0, |p| p.now()) - t0);
}

/// What the journeys read (checklist C20): the facts a person sees, as JSON.
#[wasm_bindgen]
pub fn shell_state() -> String {
    let mut out = String::from("{}");
    with_shared(|s| {
        if let Some(text) = s.read(|vm| {
            let scene = florn_ui_scene::build(vm, vm.topology.instant);
            let d = &scene.description;
            let routes = vm
                .timeline
                .entries
                .iter()
                .filter(|e| e.kind == florn_ui_model::timeline::Kind::Report && e.code == "route")
                .count();
            let neighbours = vm
                .timeline
                .entries
                .iter()
                .filter(|e| e.kind == florn_ui_model::timeline::Kind::Report && e.code == "neighbour")
                .count();
            let (frame_median, frame_p95) = s.frame.get_untracked();
            let codes = CODES.with(|c| c.borrow().iter().map(|x| js_str(x)).collect::<Vec<_>>().join(","));
            // The nodes' places on the canvas, for a script that clicks what a person sees.
            let layout = florn_ui_scene::Layout::of(&vm.topology);
            let positions: Vec<String> = vm
                .topology
                .nodes
                .iter()
                .zip(&layout.nodes)
                .map(|(n, p)| {
                    let q = vm.topology.viewport.world_to_screen(*p);
                    format!("\"{}\":[{},{}]", n.id, q.x, q.y)
                })
                .collect();
            format!(
                "{{\"state\":{},\"now\":{},\"instant\":{},\"handled\":{},\"records\":{},\"chain\":{},\"seed\":{},\"ended\":{},\"replay\":{},\"transport\":{},\"dropped\":{},\"nodes\":{},\"links\":{},\"links_down\":{},\"faulted\":{},\"flights\":{},\"selected\":{},\"timeline\":{},\"routes\":{},\"neighbours\":{},\"interventions\":{},\"applied\":{},\"notices\":{},\"draft_problems\":{},\"draft_errors\":{},\"admitted\":{},\"frame_median_ms\":{},\"frame_p95_ms\":{},\"frames\":{},\"offline\":{},\"positions\":{{{}}},\"codes\":[{}]}}",
                js_str(vm.run.state.as_str()),
                vm.run.now.map_or("null".into(), |a| a.tick.to_string()),
                vm.topology.instant,
                vm.run.handled,
                vm.run.records,
                vm.run.chain.as_deref().map_or("null".into(), js_str),
                vm.run.seed.map_or("null".into(), |x| x.to_string()),
                vm.run.ended.as_deref().map_or("null".into(), js_str),
                vm.status.replay.as_deref().map_or("null".into(), js_str),
                vm.status.transport.as_deref().map_or("null".into(), js_str),
                vm.status.dropped,
                d.nodes,
                d.links,
                d.links_down,
                d.faulted,
                d.flights,
                d.selected.as_deref().map_or("null".into(), js_str),
                vm.timeline.entries.len(),
                routes,
                neighbours,
                vm.interventions.len(),
                vm.interventions.iter().filter(|i| i.applied_at.is_some()).count(),
                vm.notices.len(),
                vm.draft.problems(),
                vm.draft.errors.len(),
                vm.admitted.len(),
                frame_median,
                frame_p95,
                s.frames.get(),
                s.offline.get_untracked(),
                positions.join(","),
                codes,
            )
        }) {
            out = text;
        }
    });
    out
}

fn js_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Mount the shell; the loader calls this once the worker is ready (rule 32).
#[wasm_bindgen]
pub fn shell_start(bridge: Bridge) {
    console_error_panic_hook::set_once();
    let mut vm = ViewModel::new();
    vm.connect();
    let shared = Shared {
        vm: Rc::new(RefCell::new(vm)),
        bridge: Rc::new(bridge),
        rev: RwSignal::new(0),
        status_rev: RwSignal::new(0),
        frame: RwSignal::new((0.0, 0.0)),
        offline: RwSignal::new(0),
        labs: RwSignal::new(Vec::new()),
        step_unit: RwSignal::new(StepUnit::Event),
        speed: RwSignal::new(1_000),
        dirty: Rc::new(Cell::new(false)),
        last_status: Rc::new(Cell::new(0.0)),
        canvas: Rc::new(RefCell::new(None)),
        draws: Rc::new(RefCell::new(Vec::new())),
        frames: Rc::new(Cell::new(0)),
        last_report: Rc::new(Cell::new(0.0)),
    };
    shared.flush();
    shared.set_speed(1_000);
    SHARED.with(|s| *s.borrow_mut() = Some(shared.clone()));
    leptos::mount::mount_to_body(
        || view! { <I18nContextProvider> <panels::App/> </I18nContextProvider> },
    );
}

/// The canvas the frame loop draws on, once it exists.
pub fn attach_canvas(canvas: web_sys::HtmlCanvasElement) {
    with_shared(|s| {
        *s.canvas.borrow_mut() = Some(canvas);
    });
}

/// The language of the catalogue of codes, from the interface's locale.
pub fn lang_of(locale: Locale) -> Lang {
    match locale {
        Locale::en => Lang::En,
        Locale::ja => Lang::Ja,
    }
}
