//! Series (RFC 014 rules 3 to 5): what a report draws as a line over time, kept with retention.
//!
//! A series is one `f64` field of an actor's reports. It arrives as a display `sample` record
//! (the live path: the index of the field among the actor's declared `f64` fields, counted in
//! channel order and then field order, the actor's place in id order, the value as a float of 32
//! bits, the tick) and can be decoded again from a trace window's report payloads (the second
//! path: the same index, the exact value, taken to the same float). A series keeps **a ring of the
//! most recent [`RING`] samples** and, for the window it is drawn in, **the minimum and maximum of
//! each pixel column** ([`Series::columns`]), so that drawing costs the window's width and not the
//! number of samples (rule 4). What lies before the ring is filled from `trace-window` when the
//! window is scrolled back ([`Series::backfill`]) and dropped again when it returns
//! ([`Series::forget_older`]).
//!
//! The model decides nothing a language decides: a label is the actor's name and the field's
//! (`m1.speed`, `pid.output`; rule 5), a unit is text (`rad/s`) the shell formats.
use std::collections::VecDeque;

use florn_control::message::{ActorSummary, ModuleSummary, ScenarioSummary, TraceEntry};

use crate::workspace::Drawn;

/// The ring's size (RFC 014 rule 4 and its open question 1): 4,096 samples of a series, 16 bytes
/// each, 64 KiB; a sensor of one millisecond holds four seconds, and the window the person scrolls
/// back to is filled from the trace. It is a constant, not a profile field: a profile that held
/// more would make what the display shows depend on a setting that is not part of the run.
pub const RING: usize = 4_096;

/// One sample of a series: the tick of the report and the value as a float of 32 bits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tick: u64,
    pub value: f32,
}

/// The samples of one pixel column of a window: the extremes, the first and the last, in time
/// order. A column nobody sampled in is `None` (the held value passes through it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    pub min: f32,
    pub max: f32,
    pub first: f32,
    pub last: f32,
}

/// Where a series' values come from: the actor, the field's index among the actor's declared `f64`
/// fields, and so the channel and the field's byte offset in a report's payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub actor: u32,
    pub index: u8,
    pub channel: u32,
    /// The byte offset of the `f64` in the report's payload.
    pub offset: usize,
    pub field: String,
}

/// The pixel columns of a window, kept while the window and the width stay.
#[derive(Clone, Debug, PartialEq)]
struct Columns {
    from: u64,
    to: u64,
    cells: Vec<Option<Column>>,
}

impl Columns {
    /// The column of a tick of the window, by integer arithmetic.
    fn of(&self, tick: u64) -> Option<usize> {
        if tick < self.from || tick > self.to {
            return None;
        }
        let span = u128::from(self.to - self.from) + 1;
        let n = self.cells.len() as u128;
        Some(((u128::from(tick - self.from) * n) / span) as usize)
    }

    fn add(&mut self, s: Sample) {
        if let Some(i) = self.of(s.tick) {
            let c = &mut self.cells[i];
            *c = Some(match *c {
                None => Column {
                    min: s.value,
                    max: s.value,
                    first: s.value,
                    last: s.value,
                },
                Some(c) => Column {
                    min: c.min.min(s.value),
                    max: c.max.max(s.value),
                    first: c.first,
                    last: s.value,
                },
            });
        }
    }
}

/// One series.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub source: Source,
    /// The actor's name and the field's, `m1.speed` (rule 5).
    pub label: String,
    /// The unit as text (`rad/s`); `None` is the axis of dimensionless values (rule 5).
    pub unit: Option<String>,
    /// A controller's series: off by default in the plots panel (rule 7); a sensor's are on.
    pub controller: bool,
    ring: VecDeque<Sample>,
    /// What was loaded from the trace before the ring's oldest sample, in time order.
    older: Vec<Sample>,
    /// The samples this series produced in the run, the ring's and those it let go.
    count: u64,
    columns: Option<Columns>,
}

impl Series {
    pub(crate) fn new(
        source: Source,
        label: String,
        unit: Option<String>,
        controller: bool,
    ) -> Self {
        Self {
            source,
            label,
            unit,
            controller,
            ring: VecDeque::new(),
            older: Vec::new(),
            count: 0,
            columns: None,
        }
    }

    /// A sample of the run, in time order. The oldest of a full ring goes.
    pub fn push(&mut self, s: Sample) {
        if self.ring.len() == RING
            && let Some(gone) = self.ring.pop_front()
            && self.columns.as_ref().is_some_and(|c| gone.tick >= c.from)
        {
            // What the window counted of it is not in the ring any more: counted again at the
            // next draw.
            self.columns = None;
        }
        self.ring.push_back(s);
        self.count += 1;
        if let Some(c) = &mut self.columns {
            c.add(s);
        }
    }

    /// The samples this series produced in the run: the ring holds the newest [`RING`] of them.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// The newest sample.
    pub fn latest(&self) -> Option<Sample> {
        self.ring.back().copied()
    }

    /// The tick of the oldest sample the ring holds, or of the older ones loaded from the trace.
    pub fn oldest(&self) -> Option<u64> {
        self.older
            .first()
            .map(|s| s.tick)
            .or_else(|| self.ring.front().map(|s| s.tick))
    }

    /// The ring's samples and the older ones, in time order.
    pub fn samples(&self) -> impl Iterator<Item = Sample> + '_ {
        self.older.iter().copied().chain(self.ring.iter().copied())
    }

    /// How many samples are kept (the ring's and the older).
    pub fn kept(&self) -> usize {
        self.older.len() + self.ring.len()
    }

    /// The last sample at or before `tick`: the value the plant holds there.
    pub fn held_at(&self, tick: u64) -> Option<Sample> {
        let at = self.ring.partition_point(|s| s.tick <= tick);
        if at > 0 {
            return self.ring.get(at - 1).copied();
        }
        let at = self.older.partition_point(|s| s.tick <= tick);
        at.checked_sub(1).and_then(|i| self.older.get(i).copied())
    }

    /// Fill what lies before the ring from a window of the trace: the samples older than the
    /// oldest kept, in time order, are put in front (a sample the series has is not taken twice).
    pub fn backfill(&mut self, samples: &[Sample]) {
        let before = self.oldest().unwrap_or(u64::MAX);
        let mut older: Vec<Sample> = samples
            .iter()
            .copied()
            .filter(|s| s.tick < before)
            .collect();
        if older.is_empty() {
            return;
        }
        older.extend(std::mem::take(&mut self.older));
        self.older = older;
        self.columns = None;
    }

    /// Let go of what `backfill` loaded: the window is on the run again.
    pub fn forget_older(&mut self) {
        if !self.older.is_empty() {
            self.older.clear();
            self.columns = None;
        }
    }

    /// The extremes of the samples of the window `from..=to` in `width` pixel columns. Kept while
    /// the window and the width stay, and updated by each sample that arrives; built again, from
    /// the ring, when they change. The cells are at most `width`: what is drawn does not grow with
    /// the samples.
    pub fn columns(&mut self, from: u64, to: u64, width: usize) -> &[Option<Column>] {
        let width = width.max(1);
        let to = to.max(from);
        let fresh = self
            .columns
            .as_ref()
            .is_some_and(|c| c.from == from && c.to == to && c.cells.len() == width);
        if !fresh {
            let mut c = Columns {
                from,
                to,
                cells: vec![None; width],
            };
            for s in self.older.iter().chain(self.ring.iter()) {
                c.add(*s);
            }
            self.columns = Some(c);
        }
        match &self.columns {
            Some(c) => &c.cells,
            None => &[],
        }
    }

    /// The values and the ticks over all kept samples: for a fit.
    pub fn extent(&self) -> Option<(f32, f32)> {
        self.samples().map(|s| s.value).fold(None, |e, v| match e {
            None => Some((v, v)),
            Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
        })
    }

    /// A move backward (RFC 009 rule 7): the run is as it stood at the start of the instant `tick`,
    /// every event before it handled and none at it, so the samples from `tick` on are not of the
    /// run any more.
    pub fn truncate_from(&mut self, tick: u64) {
        let before = self.ring.len() + self.older.len();
        while self.ring.back().is_some_and(|s| s.tick >= tick) {
            self.ring.pop_back();
        }
        self.older.retain(|s| s.tick < tick);
        let gone = before - self.ring.len() - self.older.len();
        self.count = self.count.saturating_sub(gone as u64);
        if gone > 0 {
            self.columns = None;
        }
    }

    /// Forget the run's samples: the series stays, as the scenario declares it.
    fn clear(&mut self) {
        self.ring.clear();
        self.older.clear();
        self.count = 0;
        self.columns = None;
    }
}

/// The series of a scenario, in the order of their declaration.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeriesSet {
    /// The scenario's actor ids in id order: a `sample` record's actor is a place in it.
    ids: Vec<u32>,
    series: Vec<Series>,
}

impl SeriesSet {
    /// The series a scenario declares: for each actor in id order, each drawn `f64` field of its
    /// channels (RFC 014 rule 3). A guest module's layouts are in its summary; a built-in's are
    /// the domain's ([`Drawn`]), as its sensors declare them (RFC 012 rule 24).
    pub fn of(s: &ScenarioSummary, drawn: &Drawn) -> Self {
        let mut ids: Vec<u32> = s.actors.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        let mut actors: Vec<&ActorSummary> = s.actors.iter().collect();
        actors.sort_by_key(|a| a.id);
        let mut series = Vec::new();
        for a in actors {
            match (&a.module, &a.builtin) {
                (Some(module), _) => {
                    if let Some(m) = s.modules.iter().find(|m| m.name == *module) {
                        guest_series(a, m, drawn, &mut series);
                    }
                }
                (None, Some(builtin)) => builtin_series(s, a, builtin, drawn, &mut series),
                (None, None) => {}
            }
        }
        Self { ids, series }
    }

    pub fn list(&self) -> &[Series] {
        &self.series
    }
    pub fn list_mut(&mut self) -> &mut [Series] {
        &mut self.series
    }
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    /// The series of an actor's field.
    pub fn get(&self, actor: u32, index: u8) -> Option<&Series> {
        self.series
            .iter()
            .find(|s| s.source.actor == actor && s.source.index == index)
    }

    /// A display `sample` record (the live path). A record of a series the scenario does not
    /// declare is not kept: the display is a view, and what it says that is not known is not drawn.
    pub fn display(&mut self, actor: u16, index: u8, value: f32, tick: u64) -> bool {
        let Some(&id) = self.ids.get(usize::from(actor)) else {
            return false;
        };
        let Some(s) = self
            .series
            .iter_mut()
            .find(|s| s.source.actor == id && s.source.index == index)
        else {
            return false;
        };
        s.push(Sample { tick, value });
        true
    }

    /// The samples a window of the trace holds, by series (the second path): the committed
    /// reports that carry their payloads, each `f64` of a drawn field taken to a float of 32 bits
    /// as the display does. Entries with no payload give nothing.
    pub fn decode(&self, entries: &[TraceEntry]) -> Vec<(usize, Sample)> {
        let mut out = Vec::new();
        for e in entries {
            let TraceEntry::Handled {
                at,
                target,
                reports,
                ..
            } = e
            else {
                continue;
            };
            for r in reports {
                let Some(payload) = &r.payload else {
                    continue;
                };
                for (i, s) in self.series.iter().enumerate() {
                    if s.source.actor != *target || s.source.channel != r.channel {
                        continue;
                    }
                    let Some(bytes) = payload.get(s.source.offset..s.source.offset + 8) else {
                        continue;
                    };
                    let Ok(bytes) = <[u8; 8]>::try_from(bytes) else {
                        continue;
                    };
                    out.push((
                        i,
                        Sample {
                            tick: at.tick,
                            value: f64::from_le_bytes(bytes) as f32,
                        },
                    ));
                }
            }
        }
        out
    }

    /// Fill what the ring lacks from a window of the trace (the person scrolled back).
    pub fn backfill(&mut self, entries: &[TraceEntry]) {
        let mut by: Vec<Vec<Sample>> = vec![Vec::new(); self.series.len()];
        for (i, s) in self.decode(entries) {
            by[i].push(s);
        }
        for (s, samples) in self.series.iter_mut().zip(&by) {
            s.backfill(samples);
        }
    }

    /// Let go of every series' older samples.
    pub fn forget_older(&mut self) {
        for s in &mut self.series {
            s.forget_older();
        }
    }

    /// A move backward: the samples from the instant it reached on go.
    pub fn truncate_from(&mut self, tick: u64) {
        for s in &mut self.series {
            s.truncate_from(tick);
        }
    }

    /// A new run: the samples go, the series stay.
    pub fn clear_run(&mut self) {
        for s in &mut self.series {
            s.clear();
        }
    }
}

/// The `f64` fields of a guest's channels that are drawn, with their indexes (RFC 014 rule 3) and
/// byte offsets (RFC 002 rule 16: a `bool` is one byte, `u64`, `s64` and `f64` eight, a field of
/// variable size ends the fixed prefix).
fn guest_series(a: &ActorSummary, m: &ModuleSummary, drawn: &Drawn, out: &mut Vec<Series>) {
    let mut index = 0usize;
    for c in &m.channels {
        let mut at = Some(0usize);
        for (name, ty) in &c.fields {
            let offset = at;
            at = match (ty.as_str(), at) {
                ("bool", Some(o)) => Some(o + 1),
                ("u64" | "s64" | "f64", Some(o)) => Some(o + 8),
                _ => None,
            };
            if ty != "f64" {
                continue;
            }
            let i = index;
            index += 1;
            let (Some(offset), Ok(i)) = (offset, u8::try_from(i)) else {
                continue;
            };
            if !drawn.channels.iter().any(|d| *d == c.name) {
                continue;
            }
            out.push(Series::new(
                Source {
                    actor: a.id,
                    index: i,
                    channel: c.channel,
                    offset,
                    field: name.clone(),
                },
                format!("{}.{name}", a.name),
                control_unit(m, name),
                true,
            ));
        }
    }
}

/// A built-in sensor's series: the domain declares the layouts of what they report ([`Drawn`]);
/// the label is the actor's name and its `signal-out` port's, and the unit is the signal's own
/// when the scenario writes one, else the sensor's.
fn builtin_series(
    s: &ScenarioSummary,
    a: &ActorSummary,
    builtin: &str,
    drawn: &Drawn,
    out: &mut Vec<Series>,
) {
    let Some(kind) = drawn.builtins.iter().find(|b| b.builtin == builtin) else {
        return;
    };
    let port = a.ports.iter().find(|p| p.ty.ends_with("signal-out"));
    let field = port.map_or(kind.field, |p| p.name.as_str());
    let signal_unit = port.and_then(|p| {
        s.connections
            .iter()
            .find(|c| c.ports.contains(&(a.id, p.id)))
            .and_then(|c| c.attrs.iter().find(|(k, _)| k == "unit"))
            .map(|(_, v)| v.clone())
    });
    out.push(Series::new(
        Source {
            actor: a.id,
            index: 0,
            channel: kind.channel,
            offset: 0,
            field: field.to_string(),
        },
        format!("{}.{field}", a.name),
        signal_unit.or_else(|| kind.unit.map(str::to_string)),
        false,
    ));
}

/// A controller's unit for a field of its `control` report (RFC 012 rule 18's `control-units`), as
/// the module summary carries it: the domain table's `control-units` entry, a map written as
/// text. A field the table leaves out is dimensionless.
fn control_unit(m: &ModuleSummary, field: &str) -> Option<String> {
    let (_, table) = m.tables.iter().find(|(name, _)| name == "circuit")?;
    let (_, units) = table.iter().find(|(k, _)| k == "control-units")?;
    let key = format!("\"{field}\": Text(\"");
    let start = units.find(&key)? + key.len();
    let end = units[start..].find('"')?;
    let unit = &units[start..start + end];
    (!unit.is_empty()).then(|| unit.to_string())
}

#[cfg(test)]
mod tests;
