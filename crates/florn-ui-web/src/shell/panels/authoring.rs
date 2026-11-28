//! The authoring form (§3.9).
use super::header::dropped_sentence;
use super::inspector::FieldInput;
use crate::shell::i18n_gen::i18n::*;
use crate::shell::{Shared, catalogue, lang_of};
use florn_ui_model::fields::Field;
use leptos::prelude::*;

#[component]
pub(super) fn Authoring(shared: Shared) -> impl IntoView {
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
                            a.kind_name().to_string(),
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
    let dropped = move || -> Vec<(u32, String)> {
        rev.track();
        draft_rev.track();
        sv.with_value(|s| s.read(|vm| vm.draft.dropped.clone()).unwrap_or_default())
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
                <button type="button" id="apply-draft" prop:disabled=move || !dropped().is_empty() on:click=apply>{t!(i18n, authoring.apply)}</button>
            </div>
            <p id="draft-dropped" class="problems">{move || dropped_sentence(lang_of(i18n.get_locale()), &dropped())}</p>
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
