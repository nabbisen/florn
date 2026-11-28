//! The panels (external design §3.2 to §3.13): header and run controls, labs and files, the
//! actors list (the canvas's twin), the canvas, the inspector, the timeline, the
//! interventions, the authoring form, the notices. Every string is a catalogue key; every
//! list is one Tab stop; fields are set from the model only when the selection changes.
mod authoring;
mod canvas;
mod header;
mod inspector;
mod labs;
mod notices;
mod timeline;

use super::i18n_gen::i18n::*;
use authoring::Authoring;
use canvas::Canvas;
use header::Header;
use inspector::InspectorPanel;
use labs::{Actors, Labs};
use leptos::prelude::*;
use notices::Notices;
use timeline::{Interventions, Timeline};
use wasm_bindgen::JsCast;

/// Arrow keys move the focus among the buttons of a list's items (one Tab stop).
pub(super) fn roving(ev: web_sys::KeyboardEvent) {
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
pub(super) type GoBackPlan = (u64, Vec<(String, String, String)>);

pub(super) fn tabindex(focused: RwSignal<usize>, i: usize) -> impl Fn() -> &'static str {
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
