//! The timeline and the interventions (§3.5, §3.7).
use super::{roving, tabindex};
use crate::shell::i18n_gen::i18n::*;
use crate::shell::{Shared, catalogue, lang_of};
use florn_ui_model::Selection;
use florn_ui_model::timeline::Kind;
use leptos::prelude::*;

#[component]
pub(super) fn Timeline(shared: Shared) -> impl IntoView {
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
pub(super) fn Interventions(shared: Shared) -> impl IntoView {
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
