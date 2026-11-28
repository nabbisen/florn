//! The inspector and the fields of a form (§3.6).
use super::{roving, tabindex};
use crate::shell::i18n_gen::i18n::*;
use crate::shell::{Shared, catalogue, lang_of};
use florn_ui_model::fields::{Field, Problem};
use florn_ui_model::florn_control::{ExportKind, Value};
use florn_ui_model::{Inspector, Selection, ViewModel};
use leptos::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;

/// A field's problem as the catalogue says it.
pub(super) fn problem_text(p: Problem) -> String {
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
pub(super) fn FieldInput(
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

pub(super) fn current_field<'a>(vm: &'a ViewModel, prefix: &str, name: &str) -> Option<&'a Field> {
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
pub(super) fn InspectorPanel(shared: Shared) -> impl IntoView {
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
