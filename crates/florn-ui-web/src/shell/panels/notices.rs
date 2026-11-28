//! The notices (§3.8).
use crate::shell::i18n_gen::i18n::*;
use crate::shell::{Shared, catalogue, lang_of};
use florn_ui_model::florn_control::Notification;
use leptos::prelude::*;

#[component]
pub(super) fn Notices(shared: Shared) -> impl IntoView {
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
