//! What the domain says of one of its elements (RFC 012 rule 31): a built-in with its kind and,
//! from the last accepted state, its pin voltages and current; a net with its voltage; a signal
//! with its held value, its unit and its producer; the world with the solver's status. A read of
//! what was solved: it never advances the plant (rule 8).

use florn_control::message::{InspectField, InspectValue};
use florn_kernel::Identity;

use crate::dc::Solution;
use crate::live::{Live, Status};
use crate::plant::{Element, Node, Plant};
use crate::profile::{Control, Method};

fn text(name: &str, value: impl Into<String>) -> InspectField {
    InspectField {
        name: name.to_string(),
        value: InspectValue::Text(value.into()),
    }
}

fn real(name: &str, value: f64, unit: &str) -> InspectField {
    InspectField {
        name: name.to_string(),
        value: InspectValue::Real {
            value,
            unit: unit.to_string(),
        },
    }
}

fn volt(x: &[f64], n: Node) -> f64 {
    n.map_or(0.0, |k| x[k])
}

pub fn inspect(
    plant: Option<&Plant>,
    solution: Option<&Solution>,
    live: Option<&Live>,
    target: Identity,
) -> Option<Vec<InspectField>> {
    let plant = plant?;
    // The last accepted state: the solver's once the run has started, else the operating point.
    let x: &[f64] = match live.and_then(|l| l.transient.as_ref()) {
        Some(t) => t.x(),
        None => solution.map_or(&[], |s| s.x.as_slice()),
    };
    let solved =
        x.len() >= plant.transient_unknowns && !x.is_empty() || (x.is_empty() && plant.size() == 0);
    if let Some(k) = plant.nets.iter().position(|n| n.id == target) {
        let v = if solved { x[k] } else { 0.0 };
        return Some(vec![text("kind", "net"), real("voltage", v, "V")]);
    }
    if let Some(s) = plant.signals.iter().find(|s| s.id == target) {
        let index = plant.signals.iter().position(|q| q.id == target)?;
        let (held, frozen) = live.map_or((s.held, false), |l| {
            (l.held[index].value, l.held[index].frozen)
        });
        let mut out = vec![
            text("kind", "signal"),
            real("held", held, s.unit.as_deref().unwrap_or("")),
            text("frozen", if frozen { "yes" } else { "no" }),
        ];
        if let Some(u) = &s.unit {
            out.push(text("unit", u.clone()));
        }
        if let Some((actor, port)) = &s.producer {
            out.push(text("producer", format!("{actor}.{port}")));
        }
        return Some(out);
    }
    if let Some(d) = plant.devices.iter().find(|d| d.actor == target) {
        let mut out = vec![text("kind", d.kind)];
        if solved {
            match &d.element {
                Element::Resistor { a, b, g } => {
                    out.push(real("va", volt(x, *a), "V"));
                    out.push(real("vb", volt(x, *b), "V"));
                    out.push(real("current", g * (volt(x, *a) - volt(x, *b)), "A"));
                }
                Element::Capacitor { a, b, .. } => {
                    out.push(real("va", volt(x, *a), "V"));
                    out.push(real("vb", volt(x, *b), "V"));
                }
                Element::Inductor { a, b, branch, .. } | Element::Ammeter { a, b, branch, .. } => {
                    out.push(real("va", volt(x, *a), "V"));
                    out.push(real("vb", volt(x, *b), "V"));
                    out.push(real("current", x[*branch], "A"));
                }
                Element::VoltageSource { p, n, branch, .. } => {
                    out.push(real("vp", volt(x, *p), "V"));
                    out.push(real("vn", volt(x, *n), "V"));
                    out.push(real("current", x[*branch], "A"));
                }
                Element::Motor {
                    p,
                    n,
                    branch,
                    state,
                    ..
                } => {
                    out.push(real("va+", volt(x, *p), "V"));
                    out.push(real("va-", volt(x, *n), "V"));
                    out.push(real("current", x[*branch], "A"));
                    out.push(real("speed", x[*state], "rad/s"));
                }
                Element::Diode { a, k, .. } => {
                    out.push(real("va", volt(x, *a), "V"));
                    out.push(real("vk", volt(x, *k), "V"));
                }
                Element::Switch { a, b, .. } => {
                    out.push(real("va", volt(x, *a), "V"));
                    out.push(real("vb", volt(x, *b), "V"));
                }
                Element::Voltmeter { p, n, .. } | Element::Comparator { p, n, .. } => {
                    out.push(real("vp", volt(x, *p), "V"));
                    out.push(real("vn", volt(x, *n), "V"));
                }
                Element::CurrentSource { p, n, .. } => {
                    out.push(real("vp", volt(x, *p), "V"));
                    out.push(real("vn", volt(x, *n), "V"));
                }
                Element::Constant { .. } => {}
            }
        }
        return Some(out);
    }
    // The world has an identity beyond the reserved one (RFC 004 rule 8).
    (target >= florn_model::manifest::IDENTITY_RESERVED_FROM).then(|| {
        let mut out = vec![text("kind", "world")];
        if let Some(s) = solution {
            out.push(text("operating-point", s.stage.as_str()));
        }
        out.push(real("unknowns", plant.size() as f64, ""));
        if let Some(l) = live {
            let p = &plant.profile;
            out.push(text(
                "status",
                match l.status {
                    Status::Running => "running",
                    Status::Held => "held",
                },
            ));
            out.push(text(
                "method",
                match p.method {
                    Method::Trapezoidal => "trapezoidal",
                    Method::Gear2 => "gear2",
                },
            ));
            out.push(text(
                "control",
                match p.control {
                    Control::Fixed => "fixed",
                    Control::Adaptive => "adaptive",
                },
            ));
            if let Some(t) = &l.transient {
                out.push(real("solved", t.now() as f64, "ticks"));
                out.push(real("proposal", t.proposal() as f64, "ticks"));
            }
            out.push(real("steps", f64::from(l.summary.steps), ""));
            out.push(real("iterations", f64::from(l.summary.iterations), ""));
            out.push(real("rejected", f64::from(l.summary.rejected), ""));
            out.push(real("rejections", l.summary.rejections as f64, ""));
            out.push(real("pending-breakpoints", l.pending.len() as f64, ""));
            if let Some(at) = l.advance_at {
                out.push(real("next-advance", at as f64, "ticks"));
            }
        }
        out
    })
}
