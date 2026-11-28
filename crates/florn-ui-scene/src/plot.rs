//! The waveform scene (RFC 014 rules 4 and 7): the plots drawn with the scene's primitives, one
//! lane for each unit with the lane's own value axis, a shared time axis under them, the
//! instant's cursor, and each series as **one polyline** over the visible window. A window holding
//! more samples than it has pixel columns is drawn from the series' per-column extremes
//! ([`Series::columns`]), so that what is drawn grows with the width and never with the samples: at
//! most two points for each column.
//!
//! A controller's series is a step, the held value flat until the next sample (what the plant
//! sees); a sensor's are joined. The axes carry numbers and units, never text a language decides:
//! the shell writes them (RFC 007 rule 19: the unit symbol is untranslated).
use florn_ui_model::ViewModel;
use florn_ui_model::plot::{PlotViewport, nice_step};
use florn_ui_model::series::{Column, Sample, Series};
use florn_ui_model::topology::Point;

use crate::{Axis, Color, Description, Item, LABEL, Scene, Stroke};

/// The frame and the grid of a lane, and the cursor.
pub const FRAME: Color = Color(120, 126, 136, 255);
pub const CURSOR: Color = Color(30, 30, 30, 255);
/// The series' colours, each against the canvas's white at 3 to 1 or better (WCAG 2.2 non-text
/// contrast); the dash patterns beside them are the other cue (rule 8).
pub const PALETTE: [Color; 6] = [
    Color(0, 90, 170, 255),
    Color(190, 70, 0, 255),
    Color(0, 120, 60, 255),
    Color(130, 50, 160, 255),
    Color(120, 80, 20, 255),
    Color(0, 110, 120, 255),
];
/// The number of dash patterns the canvas back end knows (0 is solid).
pub const DASHES: u8 = 4;

/// The gap between two lanes, in pixels.
const GAP: f32 = 8.0;
/// Pixels a time axis' tick label wants at least.
const TICK_SPACING_PX: f32 = 90.0;
/// Pixels a value axis' tick label wants at least.
const VALUE_SPACING_PX: f32 = 36.0;

/// One lane: the series of one unit and the range their values are drawn in.
struct Lane {
    unit: Option<String>,
    members: Vec<usize>,
    lo: f64,
    hi: f64,
    top: f32,
    height: f32,
}

impl Lane {
    fn y_of(&self, v: f32) -> f32 {
        let k = (f64::from(v) - self.lo) / (self.hi - self.lo);
        let y = self.top + self.height * (1.0 - k as f32);
        y.clamp(self.top, self.top + self.height)
    }
}

/// The scene of the plots at the run's instant. The series' columns are kept by the series, which
/// is why the model is borrowed mutably.
pub fn waveforms(vm: &mut ViewModel) -> Scene {
    let now = vm.run.now.map_or(0, |a| a.tick);
    let plots = vm.plots.clone();
    let vp = plots.viewport;
    let [ax, ay, aw, ah] = vp.area();
    let (from, to) = (vp.from, vp.to());
    let width = vp.columns();

    // The lanes, by unit in the order the series are declared.
    let shown: Vec<usize> = vm
        .series
        .list()
        .iter()
        .enumerate()
        .filter(|(_, s)| plots.shown(s))
        .map(|(i, _)| i)
        .collect();
    let mut lanes: Vec<Lane> = Vec::new();
    for &i in &shown {
        let unit = vm.series.list()[i].unit.clone();
        match lanes.iter_mut().find(|l| l.unit == unit) {
            Some(l) => l.members.push(i),
            None => lanes.push(Lane {
                unit,
                members: vec![i],
                lo: 0.0,
                hi: 1.0,
                top: 0.0,
                height: 0.0,
            }),
        }
    }
    let count = lanes.len().max(1) as f32;
    let lane_h = ((ah - GAP * (count - 1.0)) / count).max(1.0);
    for (n, l) in lanes.iter_mut().enumerate() {
        l.top = ay + n as f32 * (lane_h + GAP);
        l.height = lane_h;
        let mut extent: Option<(f32, f32)> = None;
        for &i in &l.members {
            let s = &vm.series.list()[i];
            for v in in_window(s, from, to)
                .map(|s| s.value)
                .chain(s.held_at(from.saturating_sub(1)).map(|s| s.value))
            {
                extent = Some(match extent {
                    None => (v, v),
                    Some((lo, hi)) => (lo.min(v), hi.max(v)),
                });
            }
        }
        (l.lo, l.hi) = padded(extent);
    }

    let mut items = Vec::new();
    let mut labels = Vec::new();
    for l in &lanes {
        let rect = [ax, l.top, aw, l.height];
        items.push(Item::Rects {
            rects: vec![rect],
            fill: None,
            stroke: Some(Stroke {
                color: FRAME,
                width: 1.0,
                dashed: false,
            }),
        });
        let step = nice_step(
            l.hi - l.lo,
            (l.height / VALUE_SPACING_PX).floor().max(2.0) as usize,
        );
        let mut ticks = Vec::new();
        let mut v = (l.lo / step).ceil() * step;
        while v <= l.hi && ticks.len() < 64 {
            ticks.push((l.y_of(v as f32), v));
            v += step;
        }
        items.push(Item::Axis(Axis {
            origin: Point {
                x: ax,
                y: l.top + l.height,
            },
            length: l.height,
            horizontal: false,
            // Offsets are measured from the origin along the axis: up for a value.
            ticks: ticks
                .into_iter()
                .map(|(y, v)| (l.top + l.height - y, v))
                .collect(),
            unit: l.unit.clone().unwrap_or_default(),
            stroke: Stroke {
                color: FRAME,
                width: 1.0,
                dashed: false,
            },
            font_px: 11.0,
            fill: LABEL,
        }));
        let mut lines = Vec::new();
        for (k, &i) in l.members.iter().enumerate() {
            let colour = PALETTE[i % PALETTE.len()];
            let dash = (i % usize::from(DASHES)) as u8;
            let s = &mut vm.series.list_mut()[i];
            let points = polyline(s, &vp, l, now, width);
            if !points.is_empty() {
                lines.push(Item::Polyline {
                    points,
                    stroke: Stroke {
                        color: colour,
                        width: 1.5,
                        dashed: false,
                    },
                    dash,
                });
            }
            labels.push((
                Point {
                    x: ax + 6.0,
                    y: l.top + 12.0 + 13.0 * k as f32,
                },
                s.label.clone(),
            ));
        }
        items.push(Item::Clip { rect, items: lines });
    }

    // The shared time axis, in seconds: the shell writes it in the run's unit.
    let seconds = (vp.span as f64) * plots.seconds_per_tick;
    let step = nice_step(seconds, (aw / TICK_SPACING_PX).floor().max(2.0) as usize);
    let t0 = vp.from as f64 * plots.seconds_per_tick;
    let mut ticks = Vec::new();
    let mut t = (t0 / step).ceil() * step;
    while t <= t0 + seconds && ticks.len() < 64 {
        let tick = (t / plots.seconds_per_tick).round() as u64;
        ticks.push((vp.x_of(tick) - ax, t));
        t += step;
    }
    items.push(Item::Axis(Axis {
        origin: Point { x: ax, y: ay + ah },
        length: aw,
        horizontal: true,
        ticks,
        unit: "s".to_string(),
        stroke: Stroke {
            color: FRAME,
            width: 1.0,
            dashed: false,
        },
        font_px: 11.0,
        fill: LABEL,
    }));

    // The cursor: the instant the run is at, or where the person put it.
    let at = plots.cursor.unwrap_or(now);
    if (from..=to).contains(&at) && !lanes.is_empty() {
        let x = vp.x_of(at);
        items.push(Item::Lines {
            segments: vec![[x, ay, x, ay + ah]],
            stroke: Stroke {
                color: CURSOR,
                width: 1.0,
                dashed: true,
            },
        });
    }
    if !labels.is_empty() {
        items.push(Item::Labels {
            labels,
            font_px: 11.0,
            fill: LABEL,
        });
    }
    Scene {
        items,
        description: Description {
            series: shown.len(),
            lanes: lanes.len(),
            ..Description::default()
        },
        nodes_visible: 0,
    }
}

/// The samples of a series inside the window.
fn in_window(s: &Series, from: u64, to: u64) -> impl Iterator<Item = Sample> + '_ {
    s.samples().filter(move |p| p.tick >= from && p.tick <= to)
}

/// A value range with air around it; a flat series gets a range around its value.
fn padded(extent: Option<(f32, f32)>) -> (f64, f64) {
    let Some((lo, hi)) = extent else {
        return (0.0, 1.0);
    };
    let (lo, hi) = (f64::from(lo), f64::from(hi));
    if (hi - lo).abs() < f64::EPSILON * lo.abs().max(1.0) {
        let pad = (lo.abs() * 0.1).max(1.0);
        return (lo - pad, hi + pad);
    }
    let pad = (hi - lo) * 0.05;
    (lo - pad, hi + pad)
}

/// A series as the points of one polyline, inside its lane.
fn polyline(s: &mut Series, vp: &PlotViewport, lane: &Lane, now: u64, width: usize) -> Vec<Point> {
    let (from, to) = (vp.from, vp.to());
    let step = s.controller;
    let held = s.held_at(from.saturating_sub(1)).filter(|_| from > 0);
    let inside = in_window(s, from, to).count();
    let [ax, ..] = vp.area();
    let mut pts: Vec<Point> = Vec::new();
    let mut last_y = held.map(|h| lane.y_of(h.value));
    if let Some(y) = last_y {
        pts.push(Point { x: ax, y });
    }
    if inside <= width {
        for p in in_window(s, from, to) {
            let (x, y) = (vp.x_of(p.tick), lane.y_of(p.value));
            if step && let Some(prev) = last_y {
                pts.push(Point { x, y: prev });
            }
            pts.push(Point { x, y });
            last_y = Some(y);
        }
    } else {
        let cells: Vec<Option<Column>> = s.columns(from, to, width).to_vec();
        for (i, c) in cells.iter().enumerate() {
            let Some(c) = c else { continue };
            let x = ax + i as f32 + 0.5;
            let (a, b) = if c.last >= c.first {
                (c.min, c.max)
            } else {
                (c.max, c.min)
            };
            pts.push(Point { x, y: lane.y_of(a) });
            pts.push(Point { x, y: lane.y_of(b) });
            last_y = Some(lane.y_of(c.last));
        }
    }
    // A step holds its last value up to the instant, inside the window.
    if step && let Some(y) = last_y {
        let edge = now.clamp(from, to);
        let x = vp.x_of(edge);
        if pts.last().is_some_and(|p| p.x < x) {
            pts.push(Point { x, y });
        }
    }
    pts
}

#[cfg(test)]
mod tests;
