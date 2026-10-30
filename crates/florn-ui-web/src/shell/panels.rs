//! The panels (external design §3.2 to §3.13): header and run controls, labs and files, the
//! actors list (the canvas's twin), the canvas, the inspector, the timeline, the
//! interventions, the authoring form, the notices. Every string is a catalogue key; every
//! list is one Tab stop; fields are set from the model only when the selection changes.
use std::cell::Cell;
use std::rc::Rc;

use florn_ui_model::fields::{Field, Problem};
use florn_ui_model::florn_control::message::ExportPreview;
use florn_ui_model::florn_control::{
    Ends, ExportKind, Notification, Residency, RunState, Scope, Severity, Unit, Value,
};
use florn_ui_model::timeline::Kind;
use florn_ui_model::{Inspector, Point, Selection, ViewModel};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::i18n_gen::i18n::*;
use super::{SPEEDS, Shared, StepUnit, attach_canvas, catalogue, lang_of};

/// Arrow keys move the focus among the buttons of a list's items (one Tab stop).
fn roving(ev: web_sys::KeyboardEvent) {
    let step = match ev.key().as_str() {
        "ArrowDown" | "ArrowRight" => 1,
        "ArrowUp" | "ArrowLeft" => -1,
        _ => return,
    };
    ev.prevent_default();
    let Some(target) = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    else {
        return;
    };
    let Some(li) = target.closest("li").ok().flatten() else {
        return;
    };
    let sibling = if step > 0 {
        li.next_element_sibling()
    } else {
        li.previous_element_sibling()
    };
    if let Some(b) = sibling
        .and_then(|s| s.query_selector("button, input, select").ok().flatten())
        .and_then(|b| b.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = b.focus();
    }
}

/// A backward move waiting for the person's word: the tick and the undone, each as kind,
/// target and content.
type GoBackPlan = (u64, Vec<(String, String, String)>);

fn tabindex(focused: RwSignal<usize>, i: usize) -> impl Fn() -> &'static str {
    move || if focused.get() == i { "0" } else { "-1" }
}

#[component]
pub fn App() -> impl IntoView {
    let i18n = use_i18n();
    let Some(shared) = super::shared() else {
        return view! { <p>"florn"</p> }.into_any();
    };
    Effect::new(move |_| {
        let lang = i18n.get_locale().as_str();
        if let Some(el) = document().document_element() {
            let _ = el.set_attribute("lang", lang);
        }
    });
    let s = shared.clone();
    view! {
        <Header shared=s.clone()/>
        <main class="layout">
            <div class="column left">
                <Labs shared=s.clone()/>
                <Actors shared=s.clone()/>
                <Authoring shared=s.clone()/>
            </div>
            <Canvas shared=s.clone()/>
            <div class="column right">
                <InspectorPanel shared=s.clone()/>
                <Interventions shared=s.clone()/>
            </div>
        </main>
        <Timeline shared=s.clone()/>
        <Notices shared=s/>
    }
    .into_any()
}

#[component]
fn Header(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let status_rev = shared.status_rev;
    let sv = StoredValue::new_local(shared.clone());
    let read =
        move |f: &dyn Fn(&ViewModel) -> String| sv.with_value(|s| s.read(f).unwrap_or_default());
    let scenario_name = move || {
        rev.track();
        read(&|vm| {
            vm.scenario
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default()
        })
    };
    let state = move || {
        status_rev.track();
        rev.track();
        let st = read(&|vm| vm.run.state.as_str().to_string());
        catalogue::enum_text(lang_of(i18n.get_locale()), "run-state", &st)
    };
    let instant = move || {
        status_rev.track();
        read(&|vm| {
            let tick = vm
                .scenario
                .as_ref()
                .map(|s| s.tick.clone())
                .unwrap_or_default();
            format!("{} × {}", catalogue::group(vm.topology.instant), tick)
        })
    };
    let interventions = move || {
        rev.track();
        read(&|vm| vm.interventions.len().to_string())
            .parse::<u64>()
            .unwrap_or(0)
    };
    let transport = move || {
        rev.track();
        read(&|vm| vm.status.transport.clone().unwrap_or_default())
    };
    let dropped = move || {
        rev.track();
        read(&|vm| vm.status.dropped.to_string())
            .parse::<u64>()
            .unwrap_or(0)
    };
    let limited = move || {
        rev.track();
        read(&|vm| vm.status.limited_by.clone().unwrap_or_default())
    };
    let swapping = move || -> Option<(u64, u64)> {
        rev.track();
        sv.with_value(|s| s.read(|vm| vm.status.swapping).flatten())
    };
    // A backward move waiting for the person's word (RFC 009 rule 19): the tick and what it
    // would undo, each named.
    let go_back = move || -> Option<GoBackPlan> {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.go_back.as_ref().map(|g| {
                    let name = |id: u32| {
                        vm.topology
                            .node(id)
                            .map(|n| n.name.clone())
                            .or_else(|| vm.topology.link(id).map(|l| l.name.clone()))
                            .unwrap_or_else(|| id.to_string())
                    };
                    (
                        g.tick,
                        g.undone
                            .iter()
                            .map(|u| {
                                (
                                    u.kind.clone(),
                                    name(u.target),
                                    florn_ui_model::timeline::pairs(&u.content),
                                )
                            })
                            .collect(),
                    )
                })
            })
            .flatten()
        })
    };
    let can_run = move || {
        rev.track();
        read(&|vm| vm.run.state.as_str().to_string())
    };
    let frame = shared.frame;
    let offline = shared.offline;
    let step_unit = shared.step_unit;
    let speed = shared.speed;
    let (about_origin, about_isolated, about_version) = shared.about();
    let s = shared.clone();
    let on_run = move |_| {
        s.update(|vm| match vm.run.state {
            RunState::Ready | RunState::Ended => vm.new_run(None),
            _ => vm.run(),
        });
        let s2 = s.clone();
        // A run that was ready starts and runs: the second command follows the first's answer.
        if s.read(|vm| vm.run.state == RunState::Ready)
            .unwrap_or(false)
        {
            s2.update(|vm| vm.run());
        }
    };
    let s = shared.clone();
    let on_pause = move |_| s.update(|vm| vm.pause());
    let s = shared.clone();
    let on_step = move |_| {
        let unit = match step_unit.get_untracked() {
            StepUnit::Event => Unit::OneEvent,
            StepUnit::Instant => Unit::OneInstant,
            StepUnit::Tick => Unit::OneTick,
        };
        s.update(|vm| {
            if matches!(vm.run.state, RunState::Ready | RunState::Ended) {
                vm.new_run(None);
            }
            vm.step(unit);
        });
    };
    let s = shared.clone();
    let on_reset = move |_| s.update(|vm| vm.reset());
    let goto_ref = NodeRef::<leptos::html::Input>::new();
    let s = shared.clone();
    let on_goto = move |_| {
        let Some(input) = goto_ref.get() else { return };
        if let Ok(t) = input.value().trim().parse::<u64>() {
            s.update(|vm| {
                if vm.run.state == RunState::Ready {
                    vm.new_run(None);
                }
                // Backward asks first when interventions would be undone (RFC 009 rule 19).
                vm.goto(t);
            });
        }
    };
    let s = shared.clone();
    let on_residency = move |ev: web_sys::Event| {
        let v = event_target_value(&ev);
        s.update(|vm| {
            vm.set_residency(if v == "swap" {
                Residency::Swap
            } else {
                Residency::Resident
            })
        });
    };
    let s = shared.clone();
    let on_speed = move |ev: web_sys::Event| {
        let v = event_target_value(&ev).parse::<u64>().unwrap_or(1_000);
        s.set_speed(v);
    };
    let on_unit = move |ev: web_sys::Event| {
        step_unit.set(match event_target_value(&ev).as_str() {
            "instant" => StepUnit::Instant,
            "tick" => StepUnit::Tick,
            _ => StepUnit::Event,
        })
    };
    view! {
        <header>
            <div class="row">
                <h1>{t!(i18n, title)}</h1>
                <span id="scenario-name" class="scenario-name">{scenario_name}</span>
                <div class="controls" role="group" aria-label=move || t_string!(i18n, run.controls)>
                    <button type="button" id="run" on:click=on_run disabled=move || matches!(can_run().as_str(), "no-scenario" | "invalid" | "running")>{t!(i18n, run.run)}</button>
                    <button type="button" id="pause" on:click=on_pause disabled=move || can_run() != "running">{t!(i18n, run.pause)}</button>
                    <button type="button" id="step" on:click=on_step disabled=move || matches!(can_run().as_str(), "no-scenario" | "invalid" | "running")>{t!(i18n, run.step)}</button>
                    <label>{t!(i18n, run.step_unit)}
                        <select id="step-unit" on:change=on_unit>
                            <option value="event">{t!(i18n, run.unit_event)}</option>
                            <option value="instant">{t!(i18n, run.unit_instant)}</option>
                            <option value="tick">{t!(i18n, run.unit_tick)}</option>
                        </select>
                    </label>
                    <label>{t!(i18n, run.speed)}
                        <select id="speed" on:change=on_speed prop:value=move || speed.get().to_string()>
                            {SPEEDS.iter().map(|&v| view! {
                                <option value=v.to_string()>{move || if v == 0 { t_string!(i18n, run.speed_fastest).to_string() } else { catalogue::label(lang_of(i18n.get_locale()), "run.speed_ticks", &[("ticks", catalogue::group(v))]) }}</option>
                            }).collect_view()}
                        </select>
                    </label>
                    <button type="button" id="reset" on:click=on_reset disabled=move || matches!(can_run().as_str(), "no-scenario" | "invalid")>{t!(i18n, run.reset)}</button>
                    <label>{t!(i18n, run.goto)}
                        <input type="number" id="goto-tick" node_ref=goto_ref min="0" step="1"/>
                    </label>
                    <button type="button" id="goto" on:click=on_goto disabled=move || matches!(can_run().as_str(), "no-scenario" | "invalid" | "running")>{t!(i18n, run.goto_button)}</button>
                    <label>{t!(i18n, run.residency)}
                        <select id="residency" on:change=on_residency>
                            <option value="resident">{t!(i18n, run.resident)}</option>
                            <option value="swap">{t!(i18n, run.swap)}</option>
                        </select>
                    </label>
                </div>
                <div class="lang" role="group" aria-label=move || t_string!(i18n, language)>
                    <button type="button" id="lang-en" aria-pressed=move || (i18n.get_locale() == Locale::en).to_string() on:click=move |_| i18n.set_locale(Locale::en)>{t!(i18n, lang_en)}</button>
                    <button type="button" id="lang-ja" aria-pressed=move || (i18n.get_locale() == Locale::ja).to_string() on:click=move |_| i18n.set_locale(Locale::ja)>{t!(i18n, lang_ja)}</button>
                </div>
            </div>
            <div class="status-line" role="status" aria-live="polite" id="status">
                <span id="status-state">{state}</span>
                <span id="status-instant">{t!(i18n, status.instant)}" "{instant}</span>
                <span id="status-interventions">{move || catalogue::label(lang_of(i18n.get_locale()), "status.interventions", &[("count", interventions().to_string())])}</span>
                <span id="status-transport">{move || if transport() == "messages" { t_string!(i18n, status.messages).to_string() } else { String::new() }}</span>
                <span id="status-dropped">{move || if dropped() > 0 { catalogue::label(lang_of(i18n.get_locale()), "status.dropped", &[("count", catalogue::group(dropped()))]) } else { String::new() }}</span>
                <span id="status-limited">{move || { let l = limited(); if l.is_empty() { String::new() } else { catalogue::label(lang_of(i18n.get_locale()), "status.limited", &[("what", catalogue::enum_text(lang_of(i18n.get_locale()), "limiting-factor", &l))]) } }}</span>
                <span id="status-swapping">{move || swapping().map(|(r, a)| catalogue::label(lang_of(i18n.get_locale()), "status.swapping", &[("resident", catalogue::group(r)), ("actors", catalogue::group(a))])).unwrap_or_default()}</span>
                <span id="status-frame">{move || { let (m, p) = frame.get(); catalogue::label(lang_of(i18n.get_locale()), "status.frame", &[("median", format!("{m:.1}")), ("p95", format!("{p:.1}"))]) }}</span>
                <span id="status-offline">{move || match offline.get() {
                    1 => t_string!(i18n, offline.ready).to_string(),
                    2 => t_string!(i18n, offline.unsupported).to_string(),
                    _ => t_string!(i18n, offline.not_ready).to_string(),
                }}</span>
                <span id="status-about" class="about">{move || catalogue::label(lang_of(i18n.get_locale()), "about", &[("version", about_version.clone()), ("origin", about_origin.clone()), ("isolated", if about_isolated { t_string!(i18n, yes).to_string() } else { t_string!(i18n, no).to_string() })])}</span>
            </div>
            {move || go_back().map(|(tick, undone)| view! {
                <div class="preview" id="go-back" role="dialog" aria-labelledby="go-back-title">
                    <h3 id="go-back-title">{t!(i18n, go_back.title)}</h3>
                    <p>{catalogue::label(lang_of(i18n.get_locale()), "go_back.text", &[("tick", catalogue::group(tick)), ("count", undone.len().to_string())])}</p>
                    <p class="help">{t!(i18n, go_back.help)}</p>
                    <ul>{undone.into_iter().map(|(k, t, c)| view! { <li>{k}" "{t}" "{c}</li> }).collect_view()}</ul>
                    <button type="button" id="go-back-confirm" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.confirm_go_back()))>{t!(i18n, go_back.confirm)}</button>
                    <button type="button" id="go-back-cancel" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.cancel_go_back()))>{t!(i18n, cancel)}</button>
                </div>
            })}
        </header>
    }
}

#[component]
fn Labs(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let labs = shared.labs;
    let lab = shared.lab;
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    let focused = RwSignal::new(0usize);
    let story = move |part: &'static str| {
        let lang = lang_of(i18n.get_locale());
        lab.get()
            .and_then(|n| catalogue::lab_text(lang, &n, part))
            .unwrap_or_default()
    };
    let preview = move || -> Option<ExportPreview> {
        rev.track();
        sv.with_value(|s| s.read(|vm| vm.preview.clone()).flatten())
    };
    // Origin storage's files (RFC 009 rule 16): kind, name, key, size.
    let files = move || -> Vec<(String, String, String, u64)> {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.files
                    .iter()
                    .map(|f| (f.kind.clone(), f.name.clone(), f.key.clone(), f.bytes))
                    .collect()
            })
            .unwrap_or_default()
        })
    };
    let file_focus = RwSignal::new(0usize);
    let storage_note = move || -> String {
        rev.track();
        let items = sv
            .with_value(|s| s.read(|vm| vm.files.len() as u64))
            .unwrap_or(0);
        let n = Notification::new(
            "storage.cache",
            Severity::Info,
            Scope::default(),
            Ends::Nothing,
        )
        .with("items", Value::Count(items));
        catalogue::render(lang_of(i18n.get_locale()), &n)
    };
    view! {
        <section class="labs" aria-labelledby="labs-title">
            <h2 id="labs-title">{t!(i18n, labs.title)}</h2>
            <p class="help">{t!(i18n, labs.help)}</p>
            <ol class="list" on:keydown=roving>
                <For each=move || labs.get().into_iter().enumerate() key=|(_, l)| l.name.clone() children=move |(i, lab)| {
                    let name = lab.name.clone();
                    let (attr, label, target) = (name.clone(), name.clone(), name);
                    let title = move || catalogue::lab_text(lang_of(i18n.get_locale()), &label, "title").unwrap_or_else(|| label.clone());
                    view! {
                        <li><button type="button" class="lab" data-lab=attr tabindex=tabindex(focused, i) on:focus=move |_| focused.set(i) on:click=move |_| sv.with_value(|s| s.load_lab(&target))>{title}</button></li>
                    }
                }/>
            </ol>
            <div class="story" id="lab-story">
                <h3 id="lab-title">{move || story("title")}</h3>
                <p>{move || story("story")}</p>
                <p class="help">{move || story("try")}</p>
            </div>
            <h2>{t!(i18n, files.title)}</h2>
            <p class="help">{t!(i18n, files.cache_note)}</p>
            <div class="buttons">
                <button type="button" id="open-scenario" on:click=move |_| sv.with_value(|s| s.pick_file("scenario"))>{t!(i18n, files.open_scenario)}</button>
                <button type="button" id="save-scenario" on:click=move |_| sv.with_value(|s| s.save_scenario())>{t!(i18n, files.save_scenario)}</button>
                <button type="button" id="import-module" on:click=move |_| sv.with_value(|s| s.pick_file("module"))>{t!(i18n, files.import_module)}</button>
                <button type="button" id="export-record" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.preview_export(ExportKind::RunRecord { trace: true, payloads: false })))>{t!(i18n, files.export_record)}</button>
                <button type="button" id="replay-record" on:click=move |_| sv.with_value(|s| s.pick_file("record"))>{t!(i18n, files.replay_record)}</button>
                <button type="button" id="load-checkpoint" on:click=move |_| sv.with_value(|s| s.pick_file("checkpoint"))>{t!(i18n, files.load_checkpoint)}</button>
            </div>
            <p class="help">{t!(i18n, files.replay_note)}</p>
            <h3 id="files-title">{t!(i18n, files.stored)}</h3>
            <p class="help" id="storage-note">{storage_note}</p>
            <ul class="files list" id="files" on:keydown=roving>{move || { let list = files(); if list.is_empty() { vec![view! { <li class="help">{t!(i18n, files.none_stored)}</li> }.into_any()] } else { list.into_iter().enumerate().map(|(i, (kind, name, key, bytes))| {
                let (k1, k2, attr) = (key.clone(), key.clone(), key);
                let text = catalogue::label(lang_of(i18n.get_locale()), "files.item", &[("kind", catalogue::enum_text(lang_of(i18n.get_locale()), "file-kind", &kind)), ("name", name), ("bytes", catalogue::value_text(lang_of(i18n.get_locale()), None, &Value::Bytes(bytes)))]);
                // One Tab stop for the list: the focused row's buttons, arrow keys between rows.
                view! { <li class="file" data-key=attr data-kind=kind><span>{text}</span><button type="button" class="open-stored" tabindex=tabindex(file_focus, i) on:focus=move |_| file_focus.set(i) on:click=move |_| sv.with_value(|s| s.open_stored(&k1))>{t!(i18n, files.open_stored)}</button><button type="button" class="export-stored" tabindex=tabindex(file_focus, i) on:click=move |_| sv.with_value(|s| s.export_stored(&k2))>{t!(i18n, files.export_stored)}</button></li> }.into_any()
            }).collect::<Vec<_>>() } }}</ul>
            {move || preview().map(|p| {
                let kind = p.kind.as_str().to_string();
                let payloads = if p.payloads { t_string!(i18n, yes).to_string() } else { t_string!(i18n, no).to_string() };
                let modules: Vec<String> = p.modules.iter().map(|(n, h)| format!("{n} {h}")).collect();
                view! {
                    <div class="preview" id="export-preview" role="dialog" aria-labelledby="preview-title">
                        <h3 id="preview-title">{t!(i18n, files.preview_title)}</h3>
                        <p>{catalogue::label(lang_of(i18n.get_locale()), "files.preview", &[("kind", catalogue::enum_text(lang_of(i18n.get_locale()), "file-kind", &kind)), ("size", catalogue::group(p.bytes)), ("payloads", payloads)])}</p>
                        <ul>{modules.into_iter().map(|m| view! { <li>{m}</li> }).collect_view()}</ul>
                        <button type="button" id="export-confirm" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.confirm_pending_export()))>{t!(i18n, files.confirm)}</button>
                        <button type="button" id="export-cancel" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.dismiss_preview()))>{t!(i18n, cancel)}</button>
                    </div>
                }
            })}
        </section>
    }
}

#[component]
fn Actors(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    let focused = RwSignal::new(0usize);
    let nodes = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.topology
                    .nodes
                    .iter()
                    .map(|n| {
                        (
                            n.id,
                            n.name.clone(),
                            n.faulted,
                            n.down,
                            vm.selection == Selection::Actor(n.id),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        })
    };
    view! {
        <nav class="actors" aria-labelledby="actors-title">
            <h2 id="actors-title">{t!(i18n, actors.title)}</h2>
            <p class="help">{t!(i18n, actors.help)}</p>
            <ol class="list" id="actors" on:keydown=roving>
                <For each=move || nodes().into_iter().enumerate() key=|(_, n)| (n.0, n.2, n.3, n.4) children=move |(i, (id, name, faulted, down, selected))| {
                    let status = if faulted { t_string!(i18n, actors.faulted).to_string() } else if down { t_string!(i18n, actors.down).to_string() } else { String::new() };
                    view! {
                        <li><button type="button" class="actor" data-actor=id.to_string() tabindex=tabindex(focused, i) aria-pressed=selected.to_string() on:focus=move |_| focused.set(i) on:click=move |_| sv.with_value(|s| s.select(Selection::Actor(id)))>
                            <span class="name">{name}</span>
                            <span class="status-word">{status}</span>
                        </button></li>
                    }
                }/>
            </ol>
        </nav>
    }
}

#[component]
fn Canvas(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();
    let drag: Rc<Cell<Option<(f32, f32, bool)>>> = Rc::new(Cell::new(None));
    Effect::new(move |_| {
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        let canvas: web_sys::HtmlCanvasElement = canvas;
        let dpr = window().device_pixel_ratio();
        let resize = {
            let canvas = canvas.clone();
            move || {
                let w = f64::from(canvas.client_width());
                let h = f64::from(canvas.client_height());
                canvas.set_width((w * dpr) as u32);
                canvas.set_height((h * dpr) as u32);
                sv.with_value(|s| {
                    if let Ok(mut vm) = s.vm.try_borrow_mut() {
                        vm.topology.viewport.size = Point {
                            x: w as f32,
                            y: h as f32,
                        };
                        florn_ui_scene::fit_all(&mut vm);
                    }
                });
            }
        };
        resize();
        let on_resize = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(resize);
        let _ =
            window().add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref());
        on_resize.forget();
        attach_canvas(canvas);
    });
    // Fit the view when a scenario arrives.
    let last_fit = Rc::new(std::cell::RefCell::new(String::new()));
    Effect::new(move |_| {
        rev.track();
        let name = sv.with_value(|s| {
            s.read(|vm| {
                vm.scenario
                    .as_ref()
                    .map(|x| x.sha256.clone())
                    .unwrap_or_default()
            })
            .unwrap_or_default()
        });
        if name != *last_fit.borrow() {
            *last_fit.borrow_mut() = name;
            sv.with_value(|s| {
                if let Ok(mut vm) = s.vm.try_borrow_mut() {
                    florn_ui_scene::fit_all(&mut vm);
                }
            });
        }
    });
    let description = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                let d = florn_ui_scene::build(vm, vm.topology.instant).description;
                catalogue::label(
                    lang_of(i18n.get_locale()),
                    "canvas.label",
                    &[
                        ("nodes", d.nodes.to_string()),
                        ("links", d.links.to_string()),
                        ("down", d.links_down.to_string()),
                        ("faulted", d.faulted.to_string()),
                        ("flights", d.flights.to_string()),
                    ],
                )
            })
            .unwrap_or_default()
        })
    };
    let d1 = drag.clone();
    let on_down = move |ev: web_sys::PointerEvent| {
        d1.set(Some((ev.offset_x() as f32, ev.offset_y() as f32, false)));
        if let Some(t) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let _ = t.set_pointer_capture(ev.pointer_id());
        }
    };
    let d2 = drag.clone();
    let on_move = move |ev: web_sys::PointerEvent| {
        if let Some((x, y, moved)) = d2.get() {
            let (nx, ny) = (ev.offset_x() as f32, ev.offset_y() as f32);
            // A drag starts after a few pixels; a click that wobbles is still a click.
            let moved = moved || (nx - x).abs() + (ny - y).abs() > 3.0;
            if moved {
                sv.with_value(|s| {
                    if let Ok(mut vm) = s.vm.try_borrow_mut() {
                        vm.topology.viewport.pan_by(nx - x, ny - y);
                    }
                });
            }
            d2.set(Some((nx, ny, moved)));
        }
    };
    let d3 = drag.clone();
    let on_up = move |ev: web_sys::PointerEvent| {
        if let Some((_, _, moved)) = d3.take()
            && !moved
        {
            let p = Point {
                x: ev.offset_x() as f32,
                y: ev.offset_y() as f32,
            };
            let hit = sv.with_value(|s| {
                s.read(|vm| florn_ui_scene::hit_test(vm, p, 10.0))
                    .unwrap_or_default()
            });
            sv.with_value(|s| s.select(hit));
        }
    };
    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let factor = if ev.delta_y() < 0.0 { 1.15 } else { 1.0 / 1.15 };
        let p = Point {
            x: ev.offset_x() as f32,
            y: ev.offset_y() as f32,
        };
        sv.with_value(|s| {
            if let Ok(mut vm) = s.vm.try_borrow_mut() {
                vm.topology.viewport.zoom_at(p, factor);
            }
        });
    };
    let on_key = move |ev: web_sys::KeyboardEvent| {
        let handled = sv.with_value(|s| {
            let Ok(mut vm) = s.vm.try_borrow_mut() else {
                return false;
            };
            let vp = &mut vm.topology.viewport;
            let centre = Point {
                x: vp.size.x / 2.0,
                y: vp.size.y / 2.0,
            };
            match ev.key().as_str() {
                "ArrowLeft" => vp.pan_by(40.0, 0.0),
                "ArrowRight" => vp.pan_by(-40.0, 0.0),
                "ArrowUp" => vp.pan_by(0.0, 40.0),
                "ArrowDown" => vp.pan_by(0.0, -40.0),
                "+" | "=" => vp.zoom_at(centre, 1.2),
                "-" => vp.zoom_at(centre, 1.0 / 1.2),
                _ => return false,
            }
            true
        });
        if handled {
            ev.prevent_default();
        }
    };
    view! {
        <div class="canvas-area">
            <canvas id="topology" node_ref=canvas_ref tabindex="0" role="img" aria-label=description
                on:pointerdown=on_down on:pointermove=on_move on:pointerup=on_up on:wheel=on_wheel on:keydown=on_key></canvas>
            <p class="canvas-description" id="topology-description">{description}</p>
        </div>
    }
}

/// A field's problem as the catalogue says it.
fn problem_text(p: Problem) -> String {
    let i18n = use_i18n();
    match p {
        Problem::Empty => t_string!(i18n, field.empty).to_string(),
        Problem::NotANumber => t_string!(i18n, field.not_a_number).to_string(),
        Problem::NeedsUnit => t_string!(i18n, field.needs_unit).to_string(),
        Problem::NotABoolean => t_string!(i18n, field.not_a_boolean).to_string(),
    }
}

/// One field of a form: the input is created when the selection changes and typed into
/// without the model writing back; errors and problems render beside it.
#[component]
fn FieldInput(
    shared: Shared,
    prefix: String,
    index: usize,
    field: Field,
    #[prop(into)] on_type: UnsyncCallback<(usize, String)>,
    #[prop(into)] on_commit: UnsyncCallback<usize>,
) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    let name = field.name.clone();
    let name2 = name.clone();
    let prefix2 = prefix.clone();
    let id = format!("{prefix}-{name}");
    let composing = Rc::new(Cell::new(false));
    let (c1, c2, c3) = (composing.clone(), composing.clone(), composing.clone());
    let unit = field
        .unit
        .clone()
        .map(|u| format!(" ({u})"))
        .unwrap_or_default();
    let messages = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                let f = current_field(vm, &prefix2, &name2);
                let mut out = Vec::new();
                if let Some(f) = f {
                    if let Some(p) = f.problem {
                        out.push(problem_text(p));
                    }
                    for e in &f.errors {
                        out.push(catalogue::render(lang_of(i18n.get_locale()), e));
                    }
                }
                out
            })
            .unwrap_or_default()
        })
    };
    view! {
        <div class="field">
            <label for=id.clone()>{name.clone()}{unit}</label>
            <input type="text" id=id value=field.text.clone()
                on:compositionstart=move |_| c1.set(true)
                on:compositionend=move |ev: web_sys::CompositionEvent| { c2.set(false); if let Some(t) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) { on_type.run((index, t.value())); } }
                on:input=move |ev| { if !c3.get() { on_type.run((index, event_target_value(&ev))); } }
                on:change=move |_| on_commit.run(index)/>
            <ul class="messages" aria-live="polite">{move || messages().into_iter().map(|m| view! { <li class="error">{m}</li> }).collect_view()}</ul>
        </div>
    }
}

fn current_field<'a>(vm: &'a ViewModel, prefix: &str, name: &str) -> Option<&'a Field> {
    match prefix {
        "actor" => match &vm.inspector {
            Inspector::Actor(a) => a.config.iter().find(|f| f.name == name),
            _ => None,
        },
        "connection" => match &vm.inspector {
            Inspector::Connection(c) => c.attrs.iter().find(|f| f.name == name),
            _ => None,
        },
        p if p.starts_with("draft-actor-") => {
            let id: u32 = p["draft-actor-".len()..].parse().ok()?;
            vm.draft
                .actors
                .iter()
                .find(|a| a.id == id)?
                .config
                .iter()
                .find(|f| f.name == name)
        }
        p if p.starts_with("draft-connection-") => {
            let id: u32 = p["draft-connection-".len()..].parse().ok()?;
            vm.draft
                .connections
                .iter()
                .find(|c| c.id == id)?
                .attrs
                .iter()
                .find(|f| f.name == name)
        }
        "draft" => match name {
            "name" => Some(&vm.draft.name),
            "end" => Some(&vm.draft.end),
            "tick" => Some(&vm.draft.tick),
            "root" => Some(&vm.draft.seed),
            _ => None,
        },
        _ => None,
    }
}

#[component]
fn InspectorPanel(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    // The selection's revision: the fields are created from the model then and not again.
    let selection = Memo::new(move |_| {
        rev.track();
        sv.with_value(|s| s.read(|vm| vm.selection).unwrap_or_default())
    });
    let lang = move || lang_of(i18n.get_locale());
    let facts = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| match &vm.inspector {
                Inspector::Actor(a) => {
                    let hash = a
                        .module
                        .as_ref()
                        .map(|m| m.source_sha256.clone())
                        .unwrap_or_default();
                    let status = if !a.alive {
                        t_string!(i18n, actors.faulted).to_string()
                    } else if a.down {
                        t_string!(i18n, actors.down).to_string()
                    } else {
                        t_string!(i18n, inspector.running).to_string()
                    };
                    let faults: Vec<String> = a
                        .faults
                        .iter()
                        .map(|f| {
                            catalogue::label(
                                lang(),
                                "inspector.fault",
                                &[
                                    ("code", f.code.clone()),
                                    ("at", catalogue::group(f.at.tick)),
                                    ("metered", catalogue::group(f.metered)),
                                ],
                            )
                        })
                        .collect();
                    let reports: Vec<(String, String)> = a
                        .reports
                        .iter()
                        .map(|r| (r.name.clone(), florn_ui_model::timeline::pairs(&r.fields)))
                        .collect();
                    let ports: Vec<String> = a
                        .summary
                        .ports
                        .iter()
                        .map(|p| format!("{} ({})", p.name, p.ty))
                        .collect();
                    vec![
                        ("name", a.summary.name.clone()),
                        ("identity", a.summary.id.to_string()),
                        (
                            "module",
                            format!("{} {}", a.summary.module.clone().unwrap_or_default(), hash),
                        ),
                        ("class", a.summary.class.clone()),
                        ("status", status),
                        ("ports", ports.join(", ")),
                        ("faults", faults.join("\n")),
                        (
                            "reports",
                            reports
                                .into_iter()
                                .map(|(n, f)| format!("{n}: {f}"))
                                .collect::<Vec<_>>()
                                .join("\n"),
                        ),
                    ]
                }
                Inspector::Connection(c) => {
                    let ports: Vec<String> = c
                        .summary
                        .ports
                        .iter()
                        .map(|(a, p)| {
                            let an = vm
                                .topology
                                .node(*a)
                                .map(|n| n.name.clone())
                                .unwrap_or_else(|| a.to_string());
                            format!("{an}.{p}")
                        })
                        .collect();
                    vec![
                        ("name", c.summary.name.clone()),
                        ("identity", c.summary.id.to_string()),
                        ("type", c.summary.ty.clone().unwrap_or_default()),
                        ("ports", ports.join(", ")),
                        (
                            "status",
                            catalogue::enum_text(
                                lang(),
                                "link-state",
                                if c.up { "up" } else { "down" },
                            ),
                        ),
                        ("in_flight", c.in_flight.to_string()),
                    ]
                }
                Inspector::Message(m) => {
                    let origin = vm
                        .topology
                        .node(m.origin)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| m.origin.to_string());
                    let bytes = m
                        .payload
                        .as_ref()
                        .map(|b| {
                            b.iter()
                                .take(256)
                                .map(|x| format!("{x:02x}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_else(|| t_string!(i18n, inspector.no_payload).to_string());
                    let decoded = m
                        .decoded
                        .as_ref()
                        .map(|d| florn_ui_model::timeline::pairs(d))
                        .unwrap_or_default();
                    vec![
                        ("origin", origin),
                        ("sequence", m.send.seq.to_string()),
                        ("at", catalogue::group(m.at.tick)),
                        ("port", m.send.port.to_string()),
                        ("length", m.send.len.to_string()),
                        ("digest", m.send.digest.clone()),
                        ("protocol", m.protocol.clone().unwrap_or_default()),
                        ("bytes", bytes),
                        ("decoded", decoded),
                    ]
                }
                Inspector::World(w) => vec![
                    (
                        "parent",
                        vm.run
                            .parent
                            .as_ref()
                            .map(|(t, p)| {
                                catalogue::label(
                                    lang(),
                                    "inspector.parent",
                                    &[("parent", p.clone()), ("instant", catalogue::group(*t))],
                                )
                            })
                            .unwrap_or_default(),
                    ),
                    ("domains", w.domains.join(", ")),
                    (
                        "classes",
                        w.classes
                            .iter()
                            .map(|(c, n)| format!("{c} {n}"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    (
                        "events",
                        w.events
                            .iter()
                            .rev()
                            .take(50)
                            .map(|(at, t, e, d)| {
                                format!("{} {t} {e} {d}", catalogue::group(at.tick))
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                ],
                Inspector::None => Vec::new(),
            })
            .unwrap_or_default()
        })
    };
    let label = move |key: &str| -> String {
        match key {
            "name" => t_string!(i18n, inspector.name).to_string(),
            "identity" => t_string!(i18n, inspector.identity).to_string(),
            "module" => t_string!(i18n, inspector.module).to_string(),
            "class" => t_string!(i18n, inspector.class).to_string(),
            "status" => t_string!(i18n, inspector.status).to_string(),
            "ports" => t_string!(i18n, inspector.ports).to_string(),
            "faults" => t_string!(i18n, inspector.faults).to_string(),
            "reports" => t_string!(i18n, inspector.reports).to_string(),
            "type" => t_string!(i18n, inspector.kind).to_string(),
            "in_flight" => t_string!(i18n, inspector.in_flight).to_string(),
            "origin" => t_string!(i18n, inspector.origin).to_string(),
            "sequence" => t_string!(i18n, inspector.sequence).to_string(),
            "at" => t_string!(i18n, inspector.at).to_string(),
            "port" => t_string!(i18n, inspector.port).to_string(),
            "length" => t_string!(i18n, inspector.length).to_string(),
            "digest" => t_string!(i18n, inspector.digest).to_string(),
            "protocol" => t_string!(i18n, inspector.protocol).to_string(),
            "bytes" => t_string!(i18n, inspector.bytes).to_string(),
            "decoded" => t_string!(i18n, inspector.decoded).to_string(),
            "parent" => t_string!(i18n, inspector.parent).to_string(),
            "domains" => t_string!(i18n, inspector.domains).to_string(),
            "classes" => t_string!(i18n, inspector.classes).to_string(),
            "events" => t_string!(i18n, inspector.events).to_string(),
            _ => key.to_string(),
        }
    };
    let checkpoint_focus = RwSignal::new(0usize);
    let checkpoints = move || -> Vec<(u32, u64, u64, u64)> {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.checkpoints
                    .iter()
                    .map(|c| (c.index, c.instant.tick, c.records, c.bytes))
                    .collect()
            })
            .unwrap_or_default()
        })
    };
    let title = move || match selection.get() {
        Selection::None => t_string!(i18n, inspector.none).to_string(),
        Selection::Actor(_) => t_string!(i18n, inspector.actor).to_string(),
        Selection::Connection(_) => t_string!(i18n, inspector.connection).to_string(),
        Selection::Message { .. } => t_string!(i18n, inspector.message).to_string(),
        Selection::World => t_string!(i18n, inspector.world).to_string(),
    };
    let on_type = UnsyncCallback::new(move |(i, text): (usize, String)| {
        sv.with_value(|s| {
            if let Ok(mut vm) = s.vm.try_borrow_mut() {
                match &mut vm.inspector {
                    Inspector::Actor(a) => {
                        if let Some(f) = a.config.get_mut(i) {
                            f.type_text(&text);
                        }
                    }
                    Inspector::Connection(c) => {
                        if let Some(f) = c.attrs.get_mut(i) {
                            f.type_text(&text);
                        }
                    }
                    _ => {}
                }
            }
            s.dirty.set(true);
        })
    });
    let on_commit = UnsyncCallback::new(move |i: usize| {
        sv.with_value(|s| {
            s.update(|vm| {
                vm.commit_field(i);
            })
        })
    });
    let fields = move || {
        let sel = selection.get();
        let prefix = match sel {
            Selection::Actor(_) => "actor",
            Selection::Connection(_) => "connection",
            _ => return Vec::new(),
        };
        sv.with_value(|s| {
            s.read(|vm| match &vm.inspector {
                Inspector::Actor(a) => a.config.clone(),
                Inspector::Connection(c) => c.attrs.clone(),
                _ => Vec::new(),
            })
            .unwrap_or_default()
        })
        .into_iter()
        .enumerate()
        .map(|(i, f)| (prefix.to_string(), i, f))
        .collect::<Vec<_>>()
    };
    let actions = move || {
        let sel = selection.get();
        match sel {
            Selection::Actor(id) => view! {
                <div class="buttons">
                    <button type="button" id="open-in-timeline" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.timeline.filter.actor = Some(id)))>{t!(i18n, inspector.open_in_timeline)}</button>
                    <button type="button" id="node-down" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.set_node(id, false)))>{t!(i18n, inspector.take_node_down)}</button>
                    <button type="button" id="node-up" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.set_node(id, true)))>{t!(i18n, inspector.bring_node_up)}</button>
                </div>
            }.into_any(),
            Selection::Connection(id) => view! {
                <div class="buttons">
                    <button type="button" id="link-down" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.set_link(id, false)))>{t!(i18n, inspector.take_link_down)}</button>
                    <button type="button" id="link-up" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.set_link(id, true)))>{t!(i18n, inspector.bring_link_up)}</button>
                </div>
            }.into_any(),
            // The run's checkpoints and the controls of RFC 009 rule 19.
            Selection::World => view! {
                <div class="buttons">
                    <button type="button" id="take-checkpoint" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.take_checkpoint()))>{t!(i18n, inspector.take_checkpoint)}</button>
                    <button type="button" id="branch" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.branch()))>{t!(i18n, inspector.branch)}</button>
                </div>
                <h3 id="checkpoints-title">{t!(i18n, inspector.checkpoints)}</h3>
                <ul class="checkpoints list" id="checkpoints" on:keydown=roving>{move || checkpoints().into_iter().enumerate().map(|(i, (index, tick, records, bytes))| view! {
                    <li class="checkpoint" data-index=index.to_string()><span>{catalogue::label(lang(), "inspector.checkpoint", &[("instant", catalogue::group(tick)), ("records", catalogue::group(records)), ("bytes", catalogue::value_text(lang(), None, &Value::Bytes(bytes)))])}</span><button type="button" class="export-checkpoint" tabindex=tabindex(checkpoint_focus, i) on:focus=move |_| checkpoint_focus.set(i) on:click=move |_| sv.with_value(|s| s.update(|vm| vm.preview_export(ExportKind::Checkpoint { index })))>{t!(i18n, inspector.export_checkpoint)}</button></li>
                }).collect_view()}</ul>
            }.into_any(),
            _ => view! { <div class="buttons"></div> }.into_any(),
        }
    };
    view! {
        <aside class="inspector" aria-labelledby="inspector-title">
            <h2 id="inspector-title">{t!(i18n, inspector.title)}</h2>
            <p class="help">{t!(i18n, inspector.help)}</p>
            <p id="inspector-kind" class="kind">{title}</p>
            <dl id="inspector-facts">{move || facts().into_iter().map(|(k, v)| view! { <dt>{label(k)}</dt><dd data-fact=k>{v}</dd> }).collect_view()}</dl>
            <form id="inspector-fields" on:submit=|ev| ev.prevent_default()>
                {move || fields().into_iter().map(|(prefix, i, f)| view! { <FieldInput shared=sv.with_value(|s| s.clone()) prefix=prefix index=i field=f on_type=on_type on_commit=on_commit/> }).collect_view()}
            </form>
            {actions}
            <button type="button" id="select-world" on:click=move |_| sv.with_value(|s| s.select(Selection::World))>{t!(i18n, inspector.world)}</button>
        </aside>
    }
}

#[component]
fn Timeline(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    let focused = RwSignal::new(0usize);
    let kinds = [
        (Kind::Activation, "activation"),
        (Kind::Fault, "fault"),
        (Kind::Intervention, "intervention"),
        (Kind::Change, "change"),
        (Kind::Event, "event"),
        (Kind::Report, "report"),
        (Kind::Notice, "notice"),
    ];
    let kind_label = move |k: Kind| -> String {
        match k {
            Kind::Activation => t_string!(i18n, timeline.activation).to_string(),
            Kind::Fault => t_string!(i18n, timeline.fault).to_string(),
            Kind::Intervention => t_string!(i18n, timeline.intervention).to_string(),
            Kind::Change => t_string!(i18n, timeline.change).to_string(),
            Kind::Event => t_string!(i18n, timeline.event).to_string(),
            Kind::Report => t_string!(i18n, timeline.report).to_string(),
            Kind::Notice => t_string!(i18n, timeline.notice).to_string(),
        }
    };
    let rows = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                let visible = vm.timeline.visible();
                let start = visible.len().saturating_sub(300);
                visible[start..]
                    .iter()
                    .map(|&i| {
                        let e = &vm.timeline.entries[i];
                        let name = |id: u32| {
                            vm.topology
                                .node(id)
                                .map(|n| n.name.clone())
                                .or_else(|| vm.topology.link(id).map(|l| l.name.clone()))
                                .unwrap_or_else(|| id.to_string())
                        };
                        let class = vm.timeline.class_name(e.class).unwrap_or("").to_string();
                        (
                            i,
                            e.at.tick,
                            e.kind,
                            class,
                            name(e.origin),
                            name(e.target),
                            e.code.clone(),
                            e.detail.clone(),
                            !e.sends.is_empty(),
                            vm.timeline.selected == Some(i),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        })
    };
    let actors = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.topology
                    .nodes
                    .iter()
                    .map(|n| (n.id, n.name.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        })
    };
    let on_actor = move |ev: web_sys::Event| {
        let v = event_target_value(&ev);
        sv.with_value(|s| s.update(|vm| vm.timeline.filter.actor = v.parse::<u32>().ok()));
    };
    let toggle = move |k: Kind| {
        sv.with_value(|s| {
            s.update(|vm| {
                if let Some(i) = vm.timeline.filter.kinds.iter().position(|x| *x == k) {
                    vm.timeline.filter.kinds.remove(i);
                } else {
                    vm.timeline.filter.kinds.push(k);
                }
            })
        })
    };
    let filtered = move |k: Kind| {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.timeline.filter.kinds.is_empty() || vm.timeline.filter.kinds.contains(&k)
            })
            .unwrap_or(true)
        })
    };
    view! {
        <section class="timeline" aria-labelledby="timeline-title">
            <h2 id="timeline-title">{t!(i18n, timeline.title)}</h2>
            <div class="filters" role="group" aria-label=move || t_string!(i18n, timeline.filters)>
                <label>{t!(i18n, timeline.by_actor)}
                    <select id="timeline-actor" on:change=on_actor>
                        <option value="">{t!(i18n, timeline.every_actor)}</option>
                        {move || actors().into_iter().map(|(id, n)| view! { <option value=id.to_string()>{n}</option> }).collect_view()}
                    </select>
                </label>
                {kinds.iter().map(|&(k, key)| view! {
                    <label><input type="checkbox" class="kind-filter" data-kind=key prop:checked=move || filtered(k) on:change=move |_| toggle(k)/>{move || kind_label(k)}</label>
                }).collect_view()}
            </div>
            <ol class="list rows" id="timeline" on:keydown=roving>
                <For each=move || rows().into_iter().enumerate() key=|(_, r)| (r.0, r.9) children=move |(n, (i, tick, kind, class, origin, target, code, detail, has_send, selected))| {
                    let kind_text = kind_label(kind);
                    let kind_attr = kind_text.clone();
                    let select = move |_| sv.with_value(|s| if has_send { s.select(Selection::Message { entry: i, send: 0 }) } else { s.update(|vm| vm.timeline.selected = Some(i)) });
                    view! {
                        <li class="row" data-kind=kind_attr aria-selected=selected.to_string()>
                            <button type="button" tabindex=tabindex(focused, n) on:focus=move |_| focused.set(n) on:click=select>
                                <span class="tick">{catalogue::group(tick)}</span>
                                <span class="kind">{kind_text}" "{class}</span>
                                <span class="who">{origin}" → "{target}</span>
                                <span class="code">{code}</span>
                                <span class="detail">{detail}</span>
                            </button>
                            <button type="button" class="step-to" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.step_to(i)))>{t!(i18n, timeline.step_to)}</button>
                        </li>
                    }
                }/>
            </ol>
        </section>
    }
}

#[component]
fn Interventions(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    let undone_focus = RwSignal::new(0usize);
    let items = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                let name = |id: u32| {
                    vm.topology
                        .node(id)
                        .map(|n| n.name.clone())
                        .or_else(|| vm.topology.link(id).map(|l| l.name.clone()))
                        .unwrap_or_else(|| id.to_string())
                };
                let list = |v: &[florn_ui_model::InterventionView]| {
                    v.iter()
                        .map(|i| {
                            let applied = i
                                .applied_at
                                .map(|a| {
                                    catalogue::label(
                                        lang_of(i18n.get_locale()),
                                        "interventions.applied_at",
                                        &[("at", catalogue::group(a.tick))],
                                    )
                                })
                                .unwrap_or_else(|| {
                                    t_string!(i18n, interventions.pending).to_string()
                                });
                            (
                                i.kind.clone(),
                                name(i.target),
                                florn_ui_model::timeline::pairs(&i.content),
                                applied,
                            )
                        })
                        .collect::<Vec<_>>()
                };
                (list(&vm.interventions), list(&vm.kept), list(&vm.undone))
            })
            .unwrap_or_default()
        })
    };
    let keep = move |_| {
        sv.with_value(|s| {
            s.update(|vm| {
                let unit = vm
                    .scenario
                    .as_ref()
                    .map(|x| florn_ui_model::draft::unit_of(&x.tick).to_string())
                    .unwrap_or_default();
                let kept = vm.kept.clone();
                for k in kept {
                    if let Some(at) = k.applied_at {
                        let at = format!("{}{unit}", at.tick);
                        let payload: Vec<(&str, &str)> = k
                            .content
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        vm.draft.schedule(&at, k.target, &payload);
                    }
                }
                vm.kept.clear();
            })
        })
    };
    view! {
        <section class="interventions" aria-labelledby="interventions-title">
            <h2 id="interventions-title">{t!(i18n, interventions.title)}</h2>
            <p class="help">{t!(i18n, interventions.help)}</p>
            <ul id="interventions">{move || items().0.into_iter().map(|(kind, target, content, applied)| view! { <li class="intervention">{kind}" "{target}" "{content}" — "{applied}</li> }).collect_view()}</ul>
            {move || { let undone = items().2; if undone.is_empty() { view! { <div></div> }.into_any() } else { view! {
                <div>
                    <h3 id="undone-title">{t!(i18n, interventions.undone)}</h3>
                    <ul class="undone list" id="undone" on:keydown=roving>{undone.into_iter().enumerate().map(|(i, (kind, target, content, applied))| view! { <li>{kind}" "{target}" "{content}" — "{applied}<button type="button" class="reapply" tabindex=tabindex(undone_focus, i) on:focus=move |_| undone_focus.set(i) on:click=move |_| sv.with_value(|s| s.update(|vm| vm.reapply(i)))>{t!(i18n, interventions.reapply)}</button><button type="button" class="discard-undone" tabindex=tabindex(undone_focus, i) on:click=move |_| sv.with_value(|s| s.update(|vm| vm.discard_undone(i)))>{t!(i18n, interventions.discard)}</button></li> }).collect_view()}</ul>
                </div>
            }.into_any() } }}
            {move || { let kept = items().1; if kept.is_empty() { view! { <div></div> }.into_any() } else { view! {
                <div>
                    <h3>{t!(i18n, interventions.kept)}</h3>
                    <ul id="kept">{kept.into_iter().map(|(kind, target, content, applied)| view! { <li>{kind}" "{target}" "{content}" — "{applied}</li> }).collect_view()}</ul>
                    <button type="button" id="keep-as-changes" on:click=keep>{t!(i18n, interventions.keep_as_changes)}</button>
                </div>
            }.into_any() } }}
        </section>
    }
}

#[component]
fn Authoring(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    let draft_rev = RwSignal::new(0u64);
    let bump = move || draft_rev.update(|r| *r += 1);
    let modules = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.draft
                    .modules
                    .iter()
                    .map(|m| {
                        let ports = m
                            .summary
                            .as_ref()
                            .and_then(|s| {
                                s.port_kinds
                                    .first()
                                    .map(|(_, ty, min, _)| (ty.clone(), (*min).max(1) as u32))
                            })
                            .unwrap_or_default();
                        (
                            m.name.clone(),
                            ports,
                            m.sha256.clone(),
                            vm.admitted_hash_of(&m.sha256).map(str::to_string),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        })
    };
    let draft_actors = move || {
        draft_rev.track();
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.draft
                    .actors
                    .iter()
                    .map(|a| {
                        (
                            a.id,
                            a.name.committed.clone(),
                            a.module.clone(),
                            a.ports
                                .iter()
                                .map(|p| (p.0, p.1.clone()))
                                .collect::<Vec<_>>(),
                            a.config.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        })
    };
    let draft_connections = move || {
        draft_rev.track();
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.draft
                    .connections
                    .iter()
                    .map(|c| {
                        (
                            c.id,
                            c.name.committed.clone(),
                            c.ty.clone(),
                            c.ports.clone(),
                            c.attrs.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        })
    };
    let top_fields = move || {
        draft_rev.track();
        sv.with_value(|s| {
            s.read(|vm| vec![vm.draft.name.clone(), vm.draft.end.clone()])
                .unwrap_or_default()
        })
    };
    let problems = move || {
        rev.track();
        draft_rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                (
                    vm.draft.problems(),
                    vm.draft
                        .errors
                        .iter()
                        .map(|e| catalogue::render(lang_of(i18n.get_locale()), e))
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default()
        })
    };
    let ports_ref = NodeRef::<leptos::html::Input>::new();
    let place = move |name: String, ty: String, ports: u32| {
        let ports = ports_ref
            .get()
            .and_then(|i| i.value().parse::<u32>().ok())
            .unwrap_or(ports)
            .max(1);
        sv.with_value(|s| {
            s.update(|vm| {
                vm.draft.place(&name, &ty, ports);
            })
        });
        bump();
    };
    let from_ref = NodeRef::<leptos::html::Select>::new();
    let to_ref = NodeRef::<leptos::html::Select>::new();
    let connect = move |_| {
        let (Some(a), Some(b)) = (from_ref.get(), to_ref.get()) else {
            return;
        };
        let parse = |v: String| -> Option<(u32, u32)> {
            let (x, y) = v.split_once('.')?;
            Some((x.parse().ok()?, y.parse().ok()?))
        };
        let (Some(a), Some(b)) = (parse(a.value()), parse(b.value())) else {
            return;
        };
        sv.with_value(|s| {
            s.update(|vm| {
                let ty = vm
                    .draft
                    .actors
                    .iter()
                    .find(|x| x.id == a.0)
                    .and_then(|x| x.ports.iter().find(|p| p.0 == a.1))
                    .map(|p| p.2.clone())
                    .unwrap_or_default();
                let ty = if let Some((d, _)) = ty.split_once('/') {
                    format!("{d}/link")
                } else {
                    String::new()
                };
                vm.draft.connect(&ty, &[a, b]);
            })
        });
        bump();
    };
    let attr_key = NodeRef::<leptos::html::Input>::new();
    let attr_value = NodeRef::<leptos::html::Input>::new();
    let attr_target = NodeRef::<leptos::html::Select>::new();
    let set_attr = move |_| {
        let (Some(k), Some(v), Some(t)) = (attr_key.get(), attr_value.get(), attr_target.get())
        else {
            return;
        };
        let Ok(id) = t.value().parse::<u32>() else {
            return;
        };
        let (key, value) = (k.value(), v.value());
        if key.trim().is_empty() {
            return;
        }
        sv.with_value(|s| {
            s.update(|vm| {
                if vm.draft.connections.iter().any(|c| c.id == id) {
                    vm.draft.set_attr(id, key.trim(), value.trim());
                } else {
                    vm.draft.set_config(id, key.trim(), value.trim());
                }
            })
        });
        bump();
    };
    let change_at = NodeRef::<leptos::html::Input>::new();
    let change_key = NodeRef::<leptos::html::Input>::new();
    let change_value = NodeRef::<leptos::html::Input>::new();
    let change_target = NodeRef::<leptos::html::Select>::new();
    let schedule = move |_| {
        let (Some(at), Some(k), Some(v), Some(t)) = (
            change_at.get(),
            change_key.get(),
            change_value.get(),
            change_target.get(),
        ) else {
            return;
        };
        let Ok(id) = t.value().parse::<u32>() else {
            return;
        };
        let (at, key, value) = (at.value(), k.value(), v.value());
        if key.trim().is_empty() || at.trim().is_empty() {
            return;
        }
        sv.with_value(|s| {
            s.update(|vm| {
                vm.draft
                    .schedule(at.trim(), id, &[(key.trim(), value.trim())])
            })
        });
        bump();
    };
    let type_into = move |prefix: String, name: String, text: String| {
        sv.with_value(|s| {
            if let Ok(mut vm) = s.vm.try_borrow_mut() {
                let f: Option<&mut Field> = match prefix.as_str() {
                    "draft" => match name.as_str() {
                        "name" => Some(&mut vm.draft.name),
                        "end" => Some(&mut vm.draft.end),
                        _ => None,
                    },
                    p if p.starts_with("draft-actor-") => p["draft-actor-".len()..]
                        .parse::<u32>()
                        .ok()
                        .and_then(|id| vm.draft.actors.iter_mut().find(|a| a.id == id))
                        .and_then(|a| a.config.iter_mut().find(|f| f.name == name)),
                    p if p.starts_with("draft-connection-") => p["draft-connection-".len()..]
                        .parse::<u32>()
                        .ok()
                        .and_then(|id| vm.draft.connections.iter_mut().find(|c| c.id == id))
                        .and_then(|c| c.attrs.iter_mut().find(|f| f.name == name)),
                    _ => None,
                };
                if let Some(f) = f {
                    f.type_text(&text);
                    f.commit();
                }
            }
            s.dirty.set(true);
        });
    };
    let apply = move |_| {
        sv.with_value(|s| s.update(|vm| vm.load_draft()));
        bump();
    };
    let new_draft = move |_| {
        sv.with_value(|s| {
            s.update(|vm| {
                let modules = vm.draft.modules.clone();
                vm.draft = florn_ui_model::Draft::new("scenario");
                for m in modules {
                    vm.draft.add_module(m);
                }
            })
        });
        bump();
    };
    let field_view = move |prefix: String, index: usize, field: Field| {
        let (p2, n2) = (prefix.clone(), field.name.clone());
        let on_t = UnsyncCallback::new(move |(_, text): (usize, String)| {
            type_into(p2.clone(), n2.clone(), text)
        });
        view! { <FieldInput shared=sv.with_value(|s| s.clone()) prefix=prefix index=index field=field on_type=on_t on_commit=UnsyncCallback::new(|_| {})/> }
    };
    view! {
        <details class="authoring" id="authoring">
            <summary id="authoring-title">{t!(i18n, authoring.title)}</summary>
            <p class="help">{t!(i18n, authoring.help)}</p>
            <div class="buttons">
                <button type="button" id="new-draft" on:click=new_draft>{t!(i18n, authoring.new)}</button>
                <button type="button" id="apply-draft" on:click=apply>{t!(i18n, authoring.apply)}</button>
            </div>
            <form id="draft-fields" on:submit=|ev| ev.prevent_default()>
                {move || top_fields().into_iter().enumerate().map(|(i, f)| field_view("draft".into(), i, f)).collect_view()}
            </form>
            <h3>{t!(i18n, authoring.palette)}</h3>
            <label>{t!(i18n, authoring.ports)}<input type="number" id="place-ports" node_ref=ports_ref min="1" max="255" value="2"/></label>
            <ul id="palette">{move || modules().into_iter().map(|(name, (ty, ports), source, admitted)| { let n2 = name.clone(); let hashes = admitted.map(|a| catalogue::label(lang_of(i18n.get_locale()), "authoring.hashes", &[("source", source.clone()), ("admitted", a)])).unwrap_or_default(); view! { <li data-module=name.clone()><button type="button" class="place" data-module=name.clone() on:click=move |_| place(n2.clone(), ty.clone(), ports)>{catalogue::label(lang_of(i18n.get_locale()), "authoring.place", &[("module", name.clone())])}</button><span class="hash">{hashes}</span></li> } }).collect_view()}</ul>
            <h3>{t!(i18n, authoring.actors)}</h3>
            <ul id="draft-actors">{move || draft_actors().into_iter().map(|(id, name, module, _ports, config)| view! {
                <li class="draft-actor" data-actor=id.to_string()><span>{name}" ("{module}")"</span>
                    <div class="fields">{config.into_iter().enumerate().map(|(i, f)| field_view(format!("draft-actor-{id}"), i, f)).collect_view()}</div>
                    <button type="button" class="remove" on:click=move |_| { sv.with_value(|s| s.update(|vm| vm.draft.remove_actor(id))); bump(); }>{t!(i18n, authoring.remove)}</button>
                </li>
            }).collect_view()}</ul>
            <h3>{t!(i18n, authoring.connect)}</h3>
            <div class="connect">
                <select id="connect-from" node_ref=from_ref>{move || draft_actors().into_iter().flat_map(|(id, name, _, ports, _)| ports.into_iter().map(move |(p, pn)| view! { <option value=format!("{id}.{p}")>{format!("{name}.{pn}")}</option> })).collect_view()}</select>
                <select id="connect-to" node_ref=to_ref>{move || draft_actors().into_iter().flat_map(|(id, name, _, ports, _)| ports.into_iter().map(move |(p, pn)| view! { <option value=format!("{id}.{p}")>{format!("{name}.{pn}")}</option> })).collect_view()}</select>
                <button type="button" id="connect" on:click=connect>{t!(i18n, authoring.connect_button)}</button>
            </div>
            <ul id="draft-connections">{move || draft_connections().into_iter().map(|(id, name, ty, ports, attrs)| view! {
                <li class="draft-connection" data-connection=id.to_string()><span>{name}" "{ty}" "{ports.iter().map(|(a, p)| format!("{a}.{p}")).collect::<Vec<_>>().join(", ")}</span>
                    <div class="fields">{attrs.into_iter().enumerate().map(|(i, f)| field_view(format!("draft-connection-{id}"), i, f)).collect_view()}</div>
                    <button type="button" class="remove" on:click=move |_| { sv.with_value(|s| s.update(|vm| vm.draft.remove_connection(id))); bump(); }>{t!(i18n, authoring.remove)}</button>
                </li>
            }).collect_view()}</ul>
            <h3>{t!(i18n, authoring.attribute)}</h3>
            <div class="attr">
                <select id="attr-target" node_ref=attr_target>
                    {move || draft_connections().into_iter().map(|(id, name, _, _, _)| view! { <option value=id.to_string()>{name}</option> }).collect_view()}
                    {move || draft_actors().into_iter().map(|(id, name, _, _, _)| view! { <option value=id.to_string()>{name}</option> }).collect_view()}
                </select>
                <input type="text" id="attr-key" node_ref=attr_key placeholder="delay"/>
                <input type="text" id="attr-value" node_ref=attr_value placeholder="5ms"/>
                <button type="button" id="attr-set" on:click=set_attr>{t!(i18n, authoring.set)}</button>
            </div>
            <h3>{t!(i18n, authoring.schedule)}</h3>
            <div class="schedule">
                <input type="text" id="change-at" node_ref=change_at placeholder="1000ns"/>
                <select id="change-target" node_ref=change_target>{move || draft_connections().into_iter().map(|(id, name, _, _, _)| view! { <option value=id.to_string()>{name}</option> }).collect_view()}</select>
                <input type="text" id="change-key" node_ref=change_key placeholder="enabled"/>
                <input type="text" id="change-value" node_ref=change_value placeholder="false"/>
                <button type="button" id="schedule" on:click=schedule>{t!(i18n, authoring.schedule_button)}</button>
            </div>
            <p id="draft-problems" class="problems">{move || { let (n, errors) = problems(); let mut s = catalogue::label(lang_of(i18n.get_locale()), "authoring.problems", &[("count", n.to_string())]); for e in errors { s.push('\n'); s.push_str(&e); } s }}</p>
        </details>
    }
}

#[component]
fn Notices(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let rev = shared.rev;
    let sv = StoredValue::new_local(shared.clone());
    let items = move || {
        rev.track();
        sv.with_value(|s| {
            s.read(|vm| {
                vm.notices
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (i, n.notification.clone()))
                    .collect::<Vec<(usize, Notification)>>()
            })
            .unwrap_or_default()
        })
    };
    view! {
        <section class="notices" aria-live="polite" aria-labelledby="notices-title">
            <h2 id="notices-title">{t!(i18n, notices.title)}</h2>
            <ul id="notices">{move || items().into_iter().map(|(i, n)| {
                let text = catalogue::render(lang_of(i18n.get_locale()), &n);
                let code = n.code.clone();
                let code_attr = code.clone();
                view! { <li class="notice" data-code=code_attr><span class="code">{code}</span>" "<span class="text">{text}</span><button type="button" class="dismiss" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.dismiss(i)))>{t!(i18n, dismiss)}</button></li> }
            }).collect_view()}</ul>
            <p class="help">{t!(i18n, emulation_note)}</p>
        </section>
    }
}
