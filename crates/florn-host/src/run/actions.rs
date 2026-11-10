//! What a boundary does with the events it takes: the core's interventions (RFC 001 rules 33 to 38),
//! a domain's actions and its interventions (RFC 008 rules 4, 5 and 19), the recorded target of a
//! domain intervention (RFC 001 rules 35 and 36).

use florn_abi::Val;
use florn_control::{Ends, Notification, Scope, Severity, Value};
use florn_engine::Engine;
use florn_kernel::{Event, EventKind, Identity, Instant};
use florn_model::manifest::DomainMap;

use crate::domain::{Action, BuiltinEvent, DomainEvent};
use crate::world::Intervention;

use super::compound::Calls;
use super::{ON_MESSAGE, ON_TIMER, Run, at};

/// The kind and the fields of an intervention's canonical map (RFC 005 rule 26), for the kinds
/// a domain decodes (RFC 008 rule 4). A recorded intervention of a domain kind carries its
/// target under `target`, which the decoding leaves to the Host.
pub fn decode_intervention(content: &[u8]) -> Option<(String, Option<Identity>, DomainMap)> {
    let mut map = florn_model::canonical::Decoder::new(content).domain_map()?;
    let kind = match map.remove("kind")? {
        florn_model::manifest::DomainValue::Text(k) => k,
        _ => return None,
    };
    let target = match map.remove("target") {
        Some(florn_model::manifest::DomainValue::Integer(t)) => Some(u32::try_from(t).ok()?),
        Some(_) => return None,
        None => None,
    };
    Some((kind, target, map))
}

/// The recorded content of a domain intervention: the request's map with its `target`, so
/// that a replay addresses it the same (RFC 001 rules 35 and 36).
fn encode_targeted(target: Identity, content: &[u8]) -> Vec<u8> {
    let Some(mut map) = florn_model::canonical::Decoder::new(content).domain_map() else {
        return content.to_vec();
    };
    map.insert(
        "target".into(),
        florn_model::manifest::DomainValue::Integer(i64::from(target)),
    );
    let mut e = florn_model::canonical::Encoder::default();
    florn_model::canonical::domain_map(&mut e, &map);
    e.finish()
}

impl<E: Engine> Run<E> {
    /// Request a core intervention (RFC 001 rules 33 to 38): applied at the next boundary,
    /// recorded as an input.
    pub fn intervene(&mut self, at_tick: Option<u64>, intervention: &Intervention) {
        self.kernel.intervene(at_tick, intervention.encode());
    }

    /// Request an intervention by its canonical map (RFC 005 rule 26): the core's `message` or
    /// a kind of a composed domain (RFC 008 rule 4). A refusal names the field; nothing is
    /// queued then.
    pub fn intervene_map(
        &mut self,
        at_tick: Option<u64>,
        target: Identity,
        content: &[u8],
    ) -> Result<(), Box<Notification>> {
        let invalid = |field: &str| {
            Box::new(
                Notification::new(
                    "control.invalid",
                    Severity::Error,
                    Scope::run(),
                    Ends::Command,
                )
                .with("command", Value::Name("intervene".into()))
                .with("reason", Value::Enum("wrong-type".into()))
                .with("field", Value::Text(field.into())),
            )
        };
        if let Some(i) = Intervention::decode(content) {
            let Intervention::Message { actor, .. } = &i;
            if *actor != target {
                return Err(invalid("target"));
            }
            if !self.alive(*actor) {
                return Err(invalid("target"));
            }
            self.kernel.intervene(at_tick, content.to_vec());
            return Ok(());
        }
        let (kind, _, map) = decode_intervention(content).ok_or_else(|| invalid("content"))?;
        let Some(d) = self.world.of_kind(&kind) else {
            return Err(invalid("kind"));
        };
        d.check_intervention(target, &kind, &map)?;
        self.kernel
            .intervene(at_tick, encode_targeted(target, content));
        Ok(())
    }

    pub(super) fn handle(&mut self, ev: Event) {
        let now = ev.key.tick;
        let instant = ev.key.instant();
        match &ev.kind {
            EventKind::Timer { timer } => {
                if self.builtins.contains_key(&ev.target) {
                    self.activate_builtin(
                        ev.target,
                        ev.clone(),
                        &BuiltinEvent::Timer { timer: *timer },
                    );
                } else {
                    let vals = vec![Val::U64(now), Val::U32(*timer)];
                    self.activate(ev.target, ev.clone(), ON_TIMER, &vals);
                }
            }
            EventKind::Message { port, payload } => {
                // A message of a domain's class to an actor that faulted: dropped at delivery
                // (RFC 008 rule 5), the domain told.
                if !self.alive(ev.target)
                    && let Some(d) = self.world.of_class(ev.key.class)
                {
                    let actions = d.undelivered(&DomainEvent {
                        key: ev.key,
                        target: ev.target,
                        payload: payload.clone(),
                    });
                    self.apply(instant, &ev, actions);
                } else if self.builtins.contains_key(&ev.target) {
                    self.activate_builtin(
                        ev.target,
                        ev.clone(),
                        &BuiltinEvent::Message {
                            port: *port,
                            payload,
                        },
                    );
                } else {
                    let vals = vec![Val::U64(now), Val::U32(*port), Val::bytes(payload)];
                    self.activate(ev.target, ev.clone(), ON_MESSAGE, &vals);
                }
            }
            EventKind::Intervention { payload } => self.handle_intervention(&ev, payload),
            EventKind::ScheduledChange { payload } => {
                // The core gives a change no meaning for an actor; a domain's connection or
                // world takes it (RFC 004 rule 20; RFC 008 rule 1, `change`).
                let map = florn_model::canonical::Decoder::new(payload).domain_map();
                if let Some(map) = map
                    && let Some(d) = self.world.of_target(ev.target)
                {
                    let actions = d.change(instant, ev.target, &map);
                    self.apply(instant, &ev, actions);
                }
            }
            EventKind::Domain { payload } => {
                let event = DomainEvent {
                    key: ev.key,
                    target: ev.target,
                    payload: payload.clone(),
                };
                if self.builtins.contains_key(&ev.target) && self.alive(ev.target) {
                    self.activate_builtin(ev.target, ev.clone(), &BuiltinEvent::Domain(&event));
                    return;
                }
                let alive = self.alive(ev.target);
                // A domain whose world declares a compound activation is lent out for the event
                // and given the call-in handle (RFC 012 finding F-3); any other is handled in
                // place, as it always was.
                if let Some((index, mut d)) = self.world.lend_class(ev.key.class) {
                    let actions = {
                        let mut calls = Calls::new(self, ev.clone());
                        let actions = d.event_calling(&event, alive, &mut calls);
                        calls.finish();
                        actions
                    };
                    self.world.take_back(index, d);
                    self.apply(instant, &ev, actions);
                } else if let Some(d) = self.world.of_class(ev.key.class) {
                    let actions = d.event(&event, alive);
                    self.apply(instant, &ev, actions);
                }
            }
        }
    }

    fn handle_intervention(&mut self, ev: &Event, payload: &[u8]) {
        let now = ev.key.tick;
        let instant = ev.key.instant();
        let applied = |kind: &str, scope: Scope| {
            Notification::new("intervention.applied", Severity::Info, scope, Ends::Nothing)
                .with("at", Value::Instant(at(instant)))
                .with("kind", Value::Name(kind.into()))
        };
        if let Some(Intervention::Message {
            actor,
            port,
            payload,
        }) = Intervention::decode(payload)
        {
            self.notify(applied("message", Scope::actor(actor)));
            if self.builtins.contains_key(&actor) {
                self.activate_builtin(
                    actor,
                    ev.clone(),
                    &BuiltinEvent::Message {
                        port,
                        payload: &payload,
                    },
                );
            } else {
                let vals = vec![Val::U64(now), Val::U32(port), Val::bytes(&payload)];
                self.activate(actor, ev.clone(), ON_MESSAGE, &vals);
            }
            return;
        }
        // The kernel addresses every intervention event to the interventions' origin; a
        // domain kind's target travels in the content (`encode_targeted`).
        let decoded = decode_intervention(payload);
        let Some((kind, Some(target), map)) = decoded else {
            self.notify(applied("unknown", Scope::run()));
            return;
        };
        let Some(d) = self.world.of_kind_mut(&kind) else {
            self.notify(applied("unknown", Scope::run()));
            return;
        };
        let actions = d.intervene(instant, target, &kind, &map);
        self.notify(applied(&kind, Scope::run()));
        self.apply(instant, ev, actions);
    }

    /// What a domain asked for (RFC 008 rule 1): calls into actors, events of its classes,
    /// notes for the log.
    fn apply(&mut self, instant: Instant, ev: &Event, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Call {
                    actor,
                    export,
                    values,
                } => {
                    let which = self
                        .actors
                        .get(&actor)
                        .and_then(|s| s.world.export_index(&export));
                    if let Some(which) = which {
                        self.activate(actor, ev.clone(), which, &values);
                    }
                }
                Action::Schedule {
                    origin,
                    delay,
                    class,
                    target,
                    payload,
                } => {
                    if let Some(at) = instant.after(delay) {
                        let seq = self.world.next_seq(origin);
                        self.kernel
                            .schedule_domain(at, class, origin, seq, target, payload);
                    }
                }
                Action::Notify(n) => self.notify(n.at(at(instant))),
            }
        }
    }
}
