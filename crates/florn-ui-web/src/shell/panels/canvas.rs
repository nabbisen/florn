//! The canvas and its description (§3.2, §3.11).
use crate::shell::i18n_gen::i18n::*;
use crate::shell::{Shared, attach_canvas, catalogue, lang_of};
use florn_ui_model::Point;
use leptos::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;

#[component]
pub(super) fn Canvas(shared: Shared) -> impl IntoView {
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
