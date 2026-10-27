//! The timeline (external design §3.7; OBS-004, NET-009): the ordered record of the run,
//! from the trace and the Host's notifications, never a source of truth of its own. Entries
//! hold codes, names and numbers; the shell's catalogue renders them.
use florn_control::message::{ChangeSummary, SendSummary, TraceEntry};
use florn_control::{At, Notification, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An activation of an actor: a handled event of the core's classes or a domain's.
    Activation,
    /// A fault that ended an activation.
    Fault,
    /// An intervention applied at an instant boundary.
    Intervention,
    /// A scheduled change of the scenario (before and during the run).
    Change,
    /// A domain's event (`domain.event`): link state, a drop, a node change.
    Event,
    /// A report on a named channel the view reads (RFC 008 §7): neighbours, routes.
    Report,
    /// A notification of another kind, with its code.
    Notice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub at: At,
    pub kind: Kind,
    /// The event's class for an activation or a fault (0 control, 1 timer, 2 message, a
    /// domain's beyond); 0 otherwise.
    pub class: u8,
    pub origin: u32,
    pub target: u32,
    /// A fault's code, an event's name, a report's channel name, a notice's code.
    pub code: String,
    /// The metered work of an activation or fault.
    pub metered: u64,
    /// An event's or report's detail as `key=value` pairs; a notice's parameters likewise.
    pub detail: String,
    /// The committed sends of an activation, for the message inspector.
    pub sends: Vec<SendSummary>,
    /// The trace record's index, for entries of the trace.
    pub record: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub actor: Option<u32>,
    /// Empty: every kind.
    pub kinds: Vec<Kind>,
    pub from: Option<u64>,
    pub to: Option<u64>,
}

impl Filter {
    pub fn admits(&self, e: &Entry) -> bool {
        if let Some(a) = self.actor
            && e.origin != a
            && e.target != a
        {
            return false;
        }
        if !self.kinds.is_empty() && !self.kinds.contains(&e.kind) {
            return false;
        }
        if self.from.is_some_and(|t| e.at.tick < t) {
            return false;
        }
        if self.to.is_some_and(|t| e.at.tick > t) {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timeline {
    pub entries: Vec<Entry>,
    pub filter: Filter,
    pub selected: Option<usize>,
    /// The classes' names, from the scenario.
    pub class_names: Vec<(u8, String)>,
}

impl Timeline {
    pub fn class_name(&self, class: u8) -> Option<&str> {
        self.class_names
            .iter()
            .find(|(c, _)| *c == class)
            .map(|(_, n)| n.as_str())
    }

    /// The entries the filter admits, by instant then by arrival.
    pub fn visible(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.entries.len())
            .filter(|&i| self.filter.admits(&self.entries[i]))
            .collect();
        v.sort_by_key(|&i| (self.entries[i].at.tick, self.entries[i].at.microstep, i));
        v
    }

    pub fn push_trace(&mut self, from: u64, window: &[TraceEntry]) {
        for (i, t) in window.iter().enumerate() {
            let record = Some(from + i as u64);
            let e = match t {
                TraceEntry::Handled {
                    at,
                    class,
                    origin,
                    seq: _,
                    target,
                    metered,
                    draws: _,
                    effects: _,
                    sends,
                } => Entry {
                    at: *at,
                    kind: Kind::Activation,
                    class: *class,
                    origin: *origin,
                    target: *target,
                    code: String::new(),
                    metered: *metered,
                    detail: String::new(),
                    sends: sends.clone(),
                    record,
                },
                TraceEntry::Fault {
                    at,
                    class,
                    origin,
                    seq: _,
                    actor,
                    code,
                    parameter,
                    metered,
                } => Entry {
                    at: *at,
                    kind: Kind::Fault,
                    class: *class,
                    origin: *origin,
                    target: *actor,
                    code: code.clone(),
                    metered: *metered,
                    detail: format!("parameter={parameter}"),
                    sends: Vec::new(),
                    record,
                },
                TraceEntry::Intervention { at, seq, len } => Entry {
                    at: *at,
                    kind: Kind::Intervention,
                    class: 0,
                    origin: 0,
                    target: 0,
                    code: String::new(),
                    metered: 0,
                    detail: format!("seq={seq} len={len}"),
                    sends: Vec::new(),
                    record,
                },
            };
            self.entries.push(e);
        }
    }

    /// The scenario's scheduled changes, listed before the run starts.
    pub fn push_changes(&mut self, changes: &[ChangeSummary]) {
        for c in changes {
            self.entries.push(Entry {
                at: At {
                    tick: c.at,
                    microstep: 0,
                },
                kind: Kind::Change,
                class: 0,
                origin: 0,
                target: c.target,
                code: String::new(),
                metered: 0,
                detail: pairs(&c.payload),
                sends: Vec::new(),
                record: None,
            });
        }
    }

    /// A notification that belongs in the timeline: a domain's event; `intervention.applied`
    /// and `actor.faulted` are in the trace already and are not repeated.
    pub fn push_notification(&mut self, n: &Notification) {
        let at = n.at.unwrap_or_default();
        let get = |name: &str| n.params.iter().find(|(k, _)| k == name).map(|(_, v)| v);
        match n.code.as_str() {
            "domain.event" => {
                let target = match get("target") {
                    Some(Value::Identity(i)) => *i,
                    _ => 0,
                };
                let event = match get("event") {
                    Some(Value::Name(e)) => e.clone(),
                    _ => String::new(),
                };
                let detail = match get("detail") {
                    Some(Value::Text(t)) => t.clone(),
                    _ => String::new(),
                };
                self.entries.push(Entry {
                    at,
                    kind: Kind::Event,
                    class: 0,
                    origin: target,
                    target,
                    code: event,
                    metered: 0,
                    detail,
                    sends: Vec::new(),
                    record: None,
                });
            }
            "intervention.applied" | "actor.faulted" | "actor.text" => {}
            _ => {
                if n.scope.run || n.scope.actor.is_some() {
                    let detail = n
                        .params
                        .iter()
                        .map(|(k, v)| format!("{k}={}", value_text(v)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.entries.push(Entry {
                        at,
                        kind: Kind::Notice,
                        class: 0,
                        origin: n.scope.actor.unwrap_or(0),
                        target: n.scope.actor.unwrap_or(0),
                        code: n.code.clone(),
                        metered: 0,
                        detail,
                        sends: Vec::new(),
                        record: None,
                    });
                }
            }
        }
    }

    /// A report on a channel the view reads, decoded (NET-009: neighbours and routes).
    pub fn push_report(&mut self, at: At, actor: u32, channel: &str, fields: &[(String, String)]) {
        self.entries.push(Entry {
            at,
            kind: Kind::Report,
            class: 0,
            origin: actor,
            target: actor,
            code: channel.into(),
            metered: 0,
            detail: pairs(fields),
            sends: Vec::new(),
            record: None,
        });
    }

    pub fn clear_run(&mut self) {
        self.entries.retain(|e| e.kind == Kind::Change);
        self.selected = None;
    }
}

pub fn pairs(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A parameter's value as text, for a detail: the number, the name, the text as is.
pub fn value_text(v: &Value) -> String {
    match v {
        Value::Identity(i) => i.to_string(),
        Value::Name(n) | Value::Code(n) | Value::Text(n) | Value::Enum(n) => n.clone(),
        Value::Count(c) | Value::Bytes(c) | Value::Tick(c) => c.to_string(),
        Value::Quantity(n, u) => format!("{n}{u}"),
        Value::Instant(at) => format!("{}:{}", at.tick, at.microstep),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_ordered_by_instant_and_filtered() {
        let mut t = Timeline::default();
        t.push_trace(
            0,
            &[
                TraceEntry::Handled {
                    at: At {
                        tick: 5,
                        microstep: 0,
                    },
                    class: 2,
                    origin: 1,
                    seq: 1,
                    target: 2,
                    metered: 3,
                    draws: 0,
                    effects: 0,
                    sends: Vec::new(),
                },
                TraceEntry::Fault {
                    at: At {
                        tick: 7,
                        microstep: 0,
                    },
                    class: 1,
                    origin: 3,
                    seq: 1,
                    actor: 3,
                    code: "fault.budget".into(),
                    parameter: 0,
                    metered: 9,
                },
            ],
        );
        t.push_changes(&[ChangeSummary {
            at: 2,
            target: 100,
            payload: vec![("enabled".into(), "false".into())],
        }]);
        assert_eq!(t.visible(), vec![2, 0, 1]);
        assert_eq!(t.entries[2].detail, "enabled=false");
        t.filter.actor = Some(2);
        assert_eq!(t.visible(), vec![0]);
        t.filter = Filter {
            kinds: vec![Kind::Fault],
            ..Filter::default()
        };
        assert_eq!(t.visible(), vec![1]);
        assert_eq!(t.entries[1].record, Some(1));
        t.clear_run();
        assert_eq!(t.entries.len(), 1);
    }
}
