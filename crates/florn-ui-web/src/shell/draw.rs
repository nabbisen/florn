//! The canvas back end: draws what the scene produced, batch by batch (RFC 005 rule 38).
use florn_ui_scene::{Color, Item, Scene};
use web_sys::CanvasRenderingContext2d;

fn css(c: Color) -> String {
    format!("rgba({},{},{},{})", c.0, c.1, c.2, f32::from(c.3) / 255.0)
}

pub fn draw(ctx: &CanvasRenderingContext2d, scene: &Scene, dpr: f64, w: f64, h: f64) {
    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    ctx.clear_rect(0.0, 0.0, w, h);
    for item in &scene.items {
        match item {
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
