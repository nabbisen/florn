//! The plots' state (RFC 014 rule 7): the time window the waveforms are drawn in, which series are
//! on, and the cursor. A second viewport beside the topology's: time against value, where the
//! topology's is a plane. The window follows the run while it advances and is released when the
//! person pans or zooms; "fit" returns it.
//!
//! The window is in ticks, as the run is; a tick's length in seconds comes from the scenario
//! (`time_base`) so that the shell can write the axis in the run's time unit. Nothing here is
//! part of the run: it is what a person looks at (a per-browser convenience, rule 7).
use std::collections::BTreeSet;

use crate::series::Series;
use crate::topology::Point;

/// The room the axes take in the panel: the value axis's labels on the left, the time axis's under
/// the lanes, the unit of the value axis above them, a little air on the right.
pub const MARGIN_LEFT: f32 = 64.0;
pub const MARGIN_BOTTOM: f32 = 28.0;
pub const MARGIN_TOP: f32 = 20.0;
pub const MARGIN_RIGHT: f32 = 12.0;

/// The window the run starts with, in ticks, before any sample says more.
pub const DEFAULT_SPAN: u64 = 1_000_000;

/// The window of the plots and the panel's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotViewport {
    /// The first tick of the window.
    pub from: u64,
    /// The window's length in ticks, at least 1.
    pub span: u64,
    /// The window follows the run (rule 7): its end is the run's instant.
    pub follow: bool,
    /// The panel's size in pixels.
    pub size: Point,
}

impl Default for PlotViewport {
    fn default() -> Self {
        Self {
            from: 0,
            span: DEFAULT_SPAN,
            follow: true,
            size: Point { x: 640.0, y: 360.0 },
        }
    }
}

impl PlotViewport {
    /// The plot area inside the margins: `[x, y, width, height]`, never smaller than one pixel.
    pub fn area(&self) -> [f32; 4] {
        let w = (self.size.x - MARGIN_LEFT - MARGIN_RIGHT).max(1.0);
        let h = (self.size.y - MARGIN_TOP - MARGIN_BOTTOM).max(1.0);
        [MARGIN_LEFT, MARGIN_TOP, w, h]
    }

    /// The last tick of the window.
    pub fn to(&self) -> u64 {
        self.from.saturating_add(self.span - 1)
    }

    /// The window's width in whole pixel columns.
    pub fn columns(&self) -> usize {
        self.area()[2].floor().max(1.0) as usize
    }

    /// The x of a tick in screen pixels (left of the area for ticks before the window).
    pub fn x_of(&self, tick: u64) -> f32 {
        let [x, _, w, _] = self.area();
        let k = (tick as f64 - self.from as f64) / self.span as f64;
        x + (k * f64::from(w)) as f32
    }

    /// The tick under an x in screen pixels, clamped to the run's start.
    pub fn tick_at(&self, px: f32) -> u64 {
        let [x, _, w, _] = self.area();
        let k = f64::from(px - x) / f64::from(w);
        let t = self.from as f64 + k * self.span as f64;
        if t <= 0.0 { 0 } else { t as u64 }
    }

    /// The run is at `now`: a following window ends there, with its length kept; before the window
    /// has filled it starts at 0.
    pub fn follow_run(&mut self, now: u64) {
        if self.follow {
            self.from = (now + 1).saturating_sub(self.span);
        }
    }

    /// Pan by a screen-space delta (a drag): the window is released.
    pub fn pan_by(&mut self, dx: f32) {
        let [_, _, w, _] = self.area();
        let shift = f64::from(dx) / f64::from(w) * self.span as f64;
        let from = self.from as f64 - shift;
        self.from = if from <= 0.0 { 0 } else { from as u64 };
        self.follow = false;
    }

    /// Zoom by `factor` (above 1 shows less time) keeping the tick under `px` where it is: the
    /// window is released.
    pub fn zoom_at(&mut self, px: f32, factor: f32) {
        let [x, _, w, _] = self.area();
        let k = f64::from(px - x) / f64::from(w);
        let anchor = self.from as f64 + k * self.span as f64;
        let span = (self.span as f64 / f64::from(factor.max(1e-3))).clamp(2.0, 1.0e15);
        let from = anchor - k * span;
        self.span = span as u64;
        self.from = if from <= 0.0 { 0 } else { from as u64 };
        self.follow = false;
    }

    /// "Fit": the whole of what the run has produced, following it again.
    pub fn fit(&mut self, first: u64, now: u64) {
        self.from = first.min(now);
        self.span = (now - self.from + 1).max(2);
        self.follow = true;
    }
}

/// What the plots show: the window, the toggles and the cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct Plots {
    pub viewport: PlotViewport,
    /// The series whose state differs from its default (sensors on, controllers off; rule 7).
    toggled: BTreeSet<(u32, u8)>,
    /// The cursor's tick, set by the arrow keys or a pointer; `None` follows the run's instant.
    pub cursor: Option<u64>,
    /// A tick's length in seconds, from the scenario.
    pub seconds_per_tick: f64,
}

impl Default for Plots {
    fn default() -> Self {
        Self {
            viewport: PlotViewport::default(),
            toggled: BTreeSet::new(),
            cursor: None,
            seconds_per_tick: 1e-9,
        }
    }
}

impl Plots {
    /// Whether a series is drawn.
    pub fn shown(&self, s: &Series) -> bool {
        let default = !s.controller;
        default != self.toggled.contains(&(s.source.actor, s.source.index))
    }

    /// Turn a series on or off.
    pub fn toggle(&mut self, actor: u32, index: u8) {
        if !self.toggled.remove(&(actor, index)) {
            self.toggled.insert((actor, index));
        }
    }

    /// A new run: the window starts at 0 following the run, and the cursor follows it too.
    pub fn new_run(&mut self) {
        self.viewport.from = 0;
        self.viewport.follow = true;
        self.cursor = None;
    }

    /// A new scenario: the toggles of the old one go (an id may mean another actor).
    pub fn reset(&mut self, tick: &str) {
        *self = Self {
            seconds_per_tick: time_base(tick).unwrap_or(1e-9),
            ..Self::default()
        };
    }
}

/// A tick's length in seconds, from the scenario's text (`1ns`, `100us`, `1ms`, `1s`).
pub fn time_base(tick: &str) -> Option<f64> {
    let digits = tick
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(tick.len());
    let (n, unit) = tick.split_at(digits);
    let n: f64 = n.parse().ok()?;
    // Divided by a power of ten, which is exact, and not multiplied by its inverse, which is not.
    let per_second = match unit {
        "s" => 1.0,
        "ms" => 1e3,
        "us" | "µs" => 1e6,
        "ns" => 1e9,
        "ps" => 1e12,
        _ => return None,
    };
    Some(n / per_second)
}

/// The step of an axis' ticks: the smallest of 1, 2 and 5 times a power of ten that gives at most
/// `max` ticks over `range`.
pub fn nice_step(range: f64, max: usize) -> f64 {
    let raw = range / max.max(1) as f64;
    if raw.is_nan() || raw <= 0.0 || !raw.is_finite() {
        return 1.0;
    }
    let mut scale = 1.0;
    while scale > raw {
        scale /= 10.0;
    }
    while scale * 10.0 <= raw {
        scale *= 10.0;
    }
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * scale)
        .find(|s| *s >= raw)
        .unwrap_or(scale * 10.0)
}

#[cfg(test)]
mod tests;
