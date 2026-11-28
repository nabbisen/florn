//! The header and the run controls (external design §3.3).
use super::GoBackPlan;
use crate::shell::catalogue::Lang;
use crate::shell::i18n_gen::i18n::*;
use crate::shell::{SPEEDS, Shared, StepUnit, catalogue, lang_of};
use florn_ui_model::ViewModel;
use florn_ui_model::florn_control::{Residency, RunState, Unit};
use leptos::prelude::*;

#[component]
pub(super) fn Header(shared: Shared) -> impl IntoView {
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

/// The sentence that says which actors a draft could not take, or nothing when it took them all
/// (the draft is then not applied and not saved: `Draft::can_apply`).
pub(super) fn dropped_sentence(lang: Lang, dropped: &[(u32, String)]) -> String {
    if dropped.is_empty() {
        return String::new();
    }
    let actors = dropped
        .iter()
        .map(|(id, name)| format!("{name} ({id})"))
        .collect::<Vec<_>>()
        .join(", ");
    catalogue::label(lang, "authoring.dropped", &[("actors", actors)])
}
