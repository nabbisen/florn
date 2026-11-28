//! The canvas back end: draws what the scene produced, batch by batch (RFC 005 rule 38).
use florn_ui_scene::{Axis, Color, Item, Scene, Stroke};
use web_sys::CanvasRenderingContext2d;

fn css(c: Color) -> String {
    format!("rgba({},{},{},{})", c.0, c.1, c.2, f32::from(c.3) / 255.0)
}

pub fn draw(ctx: &CanvasRenderingContext2d, scene: &Scene, dpr: f64, w: f64, h: f64) {
    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    ctx.clear_rect(0.0, 0.0, w, h);
    items(ctx, &scene.items);
}

/// The dash patterns of the plots' series (`Item::Polyline`'s `dash`): solid, then three that
/// differ at a glance, so that a series is told apart without its colour (RFC 014 rule 8).
const DASHES: [&[f64]; 4] = [&[], &[8.0, 4.0], &[2.0, 3.0], &[10.0, 3.0, 2.0, 3.0]];

fn stroke(ctx: &CanvasRenderingContext2d, s: &Stroke, dash: &[f64]) {
    ctx.set_stroke_style_str(&css(s.color));
    ctx.set_line_width(f64::from(s.width));
    let pattern = js_sys::Array::new();
    for d in dash {
        pattern.push(&(*d).into());
    }
    let _ = ctx.set_line_dash(&pattern);
}

/// An axis' number in the language-neutral form: no grouping, a point for a decimal, as short as
/// the value allows; the time axis is in the unit its range calls for (RFC 007 rule 19).
fn number(v: f64) -> String {
    let t = format!("{v:.6}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-0" {
        "0".to_string()
    } else {
        t.to_string()
    }
}

fn axis(ctx: &CanvasRenderingContext2d, a: &Axis) {
    stroke(ctx, &a.stroke, &[]);
    let (ox, oy) = (f64::from(a.origin.x), f64::from(a.origin.y));
    let len = f64::from(a.length);
    ctx.begin_path();
    ctx.move_to(ox, oy);
    if a.horizontal {
        ctx.line_to(ox + len, oy);
    } else {
        ctx.line_to(ox, oy - len);
    }
    // A time axis (seconds) is written in the unit its largest value calls for.
    let top = a.ticks.iter().map(|t| t.1.abs()).fold(0.0, f64::max);
    let (scale, unit) = if a.unit == "s" {
        if top >= 1.0 || top == 0.0 {
            (1.0, "s")
        } else if top >= 1e-3 {
            (1e3, "ms")
        } else if top >= 1e-6 {
            (1e6, "µs")
        } else {
            (1e9, "ns")
        }
    } else {
        (1.0, a.unit.as_str())
    };
    ctx.set_fill_style_str(&css(a.fill));
    ctx.set_font(&format!("{}px system-ui, sans-serif", a.font_px));
    for (offset, value) in &a.ticks {
        let o = f64::from(*offset);
        let text = number(value * scale);
        if a.horizontal {
            ctx.move_to(ox + o, oy);
            ctx.line_to(ox + o, oy + 4.0);
            ctx.set_text_align("center");
            let _ = ctx.fill_text(&text, ox + o, oy + 4.0 + f64::from(a.font_px));
        } else {
            ctx.move_to(ox, oy - o);
            ctx.line_to(ox - 4.0, oy - o);
            ctx.set_text_align("right");
            let _ = ctx.fill_text(&text, ox - 6.0, oy - o + f64::from(a.font_px) * 0.35);
        }
    }
    ctx.stroke();
    if !unit.is_empty() {
        if a.horizontal {
            ctx.set_text_align("right");
            let _ = ctx.fill_text(unit, ox + len, oy + 4.0 + 2.0 * f64::from(a.font_px));
        } else {
            ctx.set_text_align("right");
            let _ = ctx.fill_text(unit, ox - 6.0, oy - len - 2.0);
        }
    }
}

fn items(ctx: &CanvasRenderingContext2d, list: &[Item]) {
    for item in list {
        match item {
            Item::Polyline {
                points,
                stroke: s,
                dash,
            } => {
                let pattern = DASHES[usize::from(*dash) % DASHES.len()];
                stroke(ctx, s, pattern);
                ctx.begin_path();
                for (i, p) in points.iter().enumerate() {
                    if i == 0 {
                        ctx.move_to(f64::from(p.x), f64::from(p.y));
                    } else {
                        ctx.line_to(f64::from(p.x), f64::from(p.y));
                    }
                }
                ctx.stroke();
                let _ = ctx.set_line_dash(&js_sys::Array::new());
            }
            Item::Rects {
                rects,
                fill,
                stroke: outline,
            } => {
                if let Some(f) = fill {
                    ctx.set_fill_style_str(&css(*f));
                    for r in rects {
                        ctx.fill_rect(
                            f64::from(r[0]),
                            f64::from(r[1]),
                            f64::from(r[2]),
                            f64::from(r[3]),
                        );
                    }
                }
                if let Some(s) = outline {
                    stroke(ctx, s, &[]);
                    for r in rects {
                        ctx.stroke_rect(
                            f64::from(r[0]),
                            f64::from(r[1]),
                            f64::from(r[2]),
                            f64::from(r[3]),
                        );
                    }
                }
            }
            Item::Axis(a) => axis(ctx, a),
            Item::Clip { rect, items: inner } => {
                ctx.save();
                ctx.begin_path();
                ctx.rect(
                    f64::from(rect[0]),
                    f64::from(rect[1]),
                    f64::from(rect[2]),
                    f64::from(rect[3]),
                );
                ctx.clip();
                items(ctx, inner);
                ctx.restore();
            }
            Item::Lines { segments, stroke } => {
                ctx.set_stroke_style_str(&css(stroke.color));
                ctx.set_line_width(f64::from(stroke.width));
                let dash = js_sys::Array::new();
                if stroke.dashed {
                    dash.push(&6.0.into());
                    dash.push(&4.0.into());
                }
                let _ = ctx.set_line_dash(&dash);
                ctx.begin_path();
                for s in segments {
                    ctx.move_to(f64::from(s[0]), f64::from(s[1]));
                    ctx.line_to(f64::from(s[2]), f64::from(s[3]));
                }
                ctx.stroke();
                if stroke.dashed {
                    let _ = ctx.set_line_dash(&js_sys::Array::new());
                }
            }
            Item::Circles {
                centers,
                radius,
                fill,
            } => {
                ctx.set_fill_style_str(&css(*fill));
                ctx.begin_path();
                let r = f64::from(*radius);
                for c in centers {
                    ctx.move_to(f64::from(c.x) + r, f64::from(c.y));
                    let _ = ctx.arc(
                        f64::from(c.x),
                        f64::from(c.y),
                        r,
                        0.0,
                        std::f64::consts::TAU,
                    );
                }
                ctx.fill();
            }
            Item::Crosses {
                centers,
                size,
                stroke,
            } => {
                ctx.set_stroke_style_str(&css(stroke.color));
                ctx.set_line_width(f64::from(stroke.width));
                ctx.begin_path();
                let s = f64::from(*size);
                for c in centers {
                    let (x, y) = (f64::from(c.x), f64::from(c.y));
                    ctx.move_to(x - s, y - s);
                    ctx.line_to(x + s, y + s);
                    ctx.move_to(x - s, y + s);
                    ctx.line_to(x + s, y - s);
                }
                ctx.stroke();
            }
            Item::Labels {
                labels,
                font_px,
                fill,
            } => {
                ctx.set_fill_style_str(&css(*fill));
                ctx.set_font(&format!("{font_px}px system-ui, sans-serif"));
                ctx.set_text_align("center");
                for (p, text) in labels {
                    let _ = ctx.fill_text(text, f64::from(p.x), f64::from(p.y));
                }
            }
        }
    }
}
