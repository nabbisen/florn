//! The labs and files, and the actors list, the canvas's twin (§3.2, §3.9, §3.11).
use super::header::dropped_sentence;
use super::{roving, tabindex};
use crate::shell::i18n_gen::i18n::*;
use crate::shell::{Shared, catalogue, lang_of};
use florn_ui_model::Selection;
use florn_ui_model::florn_control::message::ExportPreview;
use florn_ui_model::florn_control::{Ends, ExportKind, Notification, Scope, Severity, Value};
use leptos::prelude::*;

#[component]
pub(super) fn Labs(shared: Shared) -> impl IntoView {
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
    // A draft that could not take an actor is not saved (`Draft::can_apply`): said beside the button.
    let dropped = move || -> Vec<(u32, String)> {
        rev.track();
        sv.with_value(|s| s.read(|vm| vm.draft.dropped.clone()).unwrap_or_default())
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
                <button type="button" id="save-scenario" prop:disabled=move || !dropped().is_empty() on:click=move |_| sv.with_value(|s| s.save_scenario())>{t!(i18n, files.save_scenario)}</button>
                <button type="button" id="import-module" on:click=move |_| sv.with_value(|s| s.pick_file("module"))>{t!(i18n, files.import_module)}</button>
                <button type="button" id="export-record" on:click=move |_| sv.with_value(|s| s.update(|vm| vm.preview_export(ExportKind::RunRecord { trace: true, payloads: false })))>{t!(i18n, files.export_record)}</button>
                <button type="button" id="replay-record" on:click=move |_| sv.with_value(|s| s.pick_file("record"))>{t!(i18n, files.replay_record)}</button>
                <button type="button" id="load-checkpoint" on:click=move |_| sv.with_value(|s| s.pick_file("checkpoint"))>{t!(i18n, files.load_checkpoint)}</button>
            </div>
            <p id="save-dropped" class="problems">{move || dropped_sentence(lang_of(i18n.get_locale()), &dropped())}</p>
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
pub(super) fn Actors(shared: Shared) -> impl IntoView {
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
