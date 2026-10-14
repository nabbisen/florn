//! The kernel: the queue in key order, activations that record and then commit or discard,
//! timers with replace and cancel, interventions at instant boundaries as recorded inputs, the
//! advance units, the limits of RFC 001 rule 13, and the trace.
//!
//! The Host drives it: `next_event` hands out the next event in order; the Host runs the guest
//! with a [`Sink`] and then calls `commit` or `fault`; the world the Host owns turns committed
//! sends into events through [`World`]. Nothing here reads a clock or runs a thread.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use crate::effect::{Effect, Sink, SinkRefusal};
use crate::key::{EventKey, Identity, TieHasher, TiePolicy};
use crate::stream::Stream;
use crate::time::{Instant, Tick};
use crate::trace::{CommittedEffect, Digest, FaultKind, Record, Trace};
use crate::{CLASS_CONTROL, ORIGIN_INTERVENTIONS, ORIGIN_SCHEDULED_CHANGES};

/// What an event is (RFC 002 §5; RFC 001 §7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    Timer {
        timer: u32,
    },
    Message {
        port: u32,
        payload: Vec<u8>,
    },
    /// A scheduled change of the scenario (origin `ORIGIN_SCHEDULED_CHANGES`, class 0).
    ScheduledChange {
        payload: Vec<u8>,
    },
    /// An intervention (origin `ORIGIN_INTERVENTIONS`, class 0).
    Intervention {
        payload: Vec<u8>,
    },
    /// A domain's own event: the class the domain assigned, an opaque payload.
    Domain {
        payload: Vec<u8>,
    },
}

/// An event in the queue: its key, its target, and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub key: EventKey,
    pub target: Identity,
    pub kind: EventKind,
}

impl PartialOrd for Event {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Event {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.key.cmp(&o.key)
    }
}

/// The world's part of delivery (RFC 002 rule 24): a committed send becomes zero or more events.
/// The domain owns the connection's semantics; the kernel owns the order.
pub trait World {
    /// Called at commit for every committed `send`, in commit order. Returns the events to
    /// schedule: for each, the target, the class, the delay from `now` (RFC 001 rule 11), and the
    /// kind. The origin of each event is `origin` and its sequence number is `seq` (rule 17);
    /// several events from one send share them and are told apart by target.
    fn deliver(
        &mut self,
        now: Instant,
        origin: Identity,
        seq: u64,
        port: u32,
        payload: &[u8],
    ) -> Vec<Delivery>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub target: Identity,
    pub class: u8,
    pub delay: Tick,
    pub kind: EventKind,
}

/// A world that delivers nothing: for tests and for scenarios without connections.
pub struct NoWorld;
impl World for NoWorld {
    fn deliver(&mut self, _: Instant, _: Identity, _: u64, _: u32, _: &[u8]) -> Vec<Delivery> {
        Vec::new()
    }
}

/// The run's configuration from the scenario (RFC 004 §3.3) and the Host.
#[derive(Clone, Debug)]
pub struct Config {
    pub root_seed: u64,
    pub tie: TiePolicy,
    /// The first event at or after `end` is not handled (RFC 004 rule 14).
    pub end: Tick,
    pub microsteps_per_tick: u32,
    pub events_per_instant: u64,
    /// The class of timer events, assigned by the domain (RFC 002 rule 29).
    pub timer_class: u8,
    pub chain_interval: u64,
    pub keep_records: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root_seed: 0,
            tie: TiePolicy::ByIdentity,
            end: Tick::MAX,
            microsteps_per_tick: 1_000,
            events_per_instant: 100_000,
            timer_class: 1,
            chain_interval: Trace::DEFAULT_INTERVAL,
            keep_records: false,
        }
    }
}

/// Why the run ended other than by reaching `end`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunFault {
    /// RFC 001 rule 13: the microstep limit of a tick was exceeded; the origins of the last instant.
    MicrostepLimit {
        tick: Tick,
        limit: u32,
        origins: Vec<Identity>,
    },
    /// RFC 001 rule 13: the events-per-instant limit was exceeded.
    EventsPerInstantLimit {
        instant: Instant,
        limit: u64,
        origins: Vec<Identity>,
    },
}

/// Why a commit was refused: the activation becomes a fault (RFC 002 rule 40).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitRefusal {
    /// RFC 001 rule 6: a timer beyond the largest tick.
    DelayBeyondLargestTick { timer: u32, delay: Tick },
    /// RFC 002 rule 27: pending timers after commit would exceed the quota.
    PendingTimers { limit: usize },
    /// The sink had refused a call: the activation already faulted.
    Refused(SinkRefusal),
}

/// The per-actor state the kernel keeps (RFC 003 rule 21's "kernel's own state for the actor").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorKernelState {
    pub seq: u64,
    pub stream: Stream,
    /// timer identity → generation; a timer event whose generation is older is stale (replaced or cancelled).
    pub timers: BTreeMap<u32, u64>,
    pub alive: bool,
}

/// An activation in progress: the event being handled and what must be restored on a fault.
#[derive(Clone, Debug)]
pub struct Activation {
    pub event: Event,
    pub actor: Identity,
    stream_at_start: Stream,
}

/// An applied intervention, as a run input (RFC 001 rule 35).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedIntervention {
    pub instant: Instant,
    pub content: Vec<u8>,
}

pub struct Kernel {
    config: Config,
    tie: TieHasher,
    queue: BinaryHeap<Reverse<Event>>,
    actors: BTreeMap<Identity, ActorKernelState>,
    now: Instant,
    /// Events handled in the current instant and the origins seen in it (rule 13).
    instant_events: u64,
    instant_origins: Vec<Identity>,
    /// The microstep count of the current tick's last instant.
    last_tick_microsteps: u32,
    /// Interventions waiting for the next boundary: (requested tick, content).
    pending_interventions: Vec<(Option<Tick>, Vec<u8>)>,
    applied_interventions: Vec<AppliedIntervention>,
    intervention_seq: u64,
    change_seq: u64,
    /// Replay: interventions to apply at their recorded instants (rule 36).
    replay: Vec<AppliedIntervention>,
    pub trace: Trace,
    in_activation: bool,
    fault: Option<RunFault>,
    handled: u64,
}

impl Kernel {
    pub fn new(config: Config) -> Self {
        let tie = TieHasher::new(config.root_seed);
        let trace = Trace::new(config.chain_interval, config.keep_records);
        Self {
            config,
            tie,
            queue: BinaryHeap::new(),
            actors: BTreeMap::new(),
            now: Instant::ZERO,
            instant_events: 0,
            instant_origins: Vec::new(),
            last_tick_microsteps: 0,
            pending_interventions: Vec::new(),
            applied_interventions: Vec::new(),
            intervention_seq: 0,
            change_seq: 0,
            replay: Vec::new(),
            trace,
            in_activation: false,
            fault: None,
            handled: 0,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn now(&self) -> Instant {
        self.now
    }
    pub fn handled(&self) -> u64 {
        self.handled
    }
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
    pub fn run_fault(&self) -> Option<&RunFault> {
        self.fault.as_ref()
    }
    pub fn applied_interventions(&self) -> &[AppliedIntervention] {
        &self.applied_interventions
    }

    /// Register an actor: its stream is derived from the root seed and its identity (rule 29).
    pub fn add_actor(&mut self, id: Identity) {
        self.actors.insert(
            id,
            ActorKernelState {
                seq: 0,
                stream: Stream::for_actor(self.config.root_seed, id),
                timers: BTreeMap::new(),
                alive: true,
            },
        );
    }
    pub fn actor(&self, id: Identity) -> Option<&ActorKernelState> {
        self.actors.get(&id)
    }
    pub fn actor_mut(&mut self, id: Identity) -> Option<&mut ActorKernelState> {
        self.actors.get_mut(&id)
    }
    /// Restore the kernel's state for an actor (RFC 003 rule 22).
    pub fn restore_actor(&mut self, id: Identity, state: ActorKernelState) {
        self.actors.insert(id, state);
    }

    fn key(&self, instant: Instant, class: u8, origin: Identity, seq: u64) -> EventKey {
        let tie = match self.config.tie {
            TiePolicy::ByIdentity => 0,
            TiePolicy::BySeed => self.tie.tie(instant, class, origin),
        };
        EventKey {
            tick: instant.tick,
            microstep: instant.microstep,
            class,
            tie,
            origin,
            seq,
        }
    }

    /// A scheduled change of the scenario (RFC 004 rule 20): class 0 at `tick`, microstep 0.
    pub fn schedule_change(&mut self, at: Tick, target: Identity, payload: Vec<u8>) {
        self.change_seq += 1;
        let key = self.key(
            Instant::new(at, 0),
            CLASS_CONTROL,
            ORIGIN_SCHEDULED_CHANGES,
            self.change_seq,
        );
        self.queue.push(Reverse(Event {
            key,
            target,
            kind: EventKind::ScheduledChange { payload },
        }));
    }

    /// Schedule a domain event (RFC 001 rule 15): the domain names the class and the origin.
    pub fn schedule_domain(
        &mut self,
        at: Instant,
        class: u8,
        origin: Identity,
        seq: u64,
        target: Identity,
        payload: Vec<u8>,
    ) {
        let key = self.key(at, class, origin, seq);
        self.queue.push(Reverse(Event {
            key,
            target,
            kind: EventKind::Domain { payload },
        }));
    }

    /// Request an intervention (RFC 001 rules 33 to 38). Applied at the next instant boundary, at
    /// the current tick's next microstep or at `at` if later.
    pub fn intervene(&mut self, at: Option<Tick>, content: Vec<u8>) {
        self.pending_interventions.push((at, content));
    }

    /// Replay: the interventions to apply at their recorded instants (rule 36).
    pub fn set_replay(&mut self, interventions: Vec<AppliedIntervention>) {
        self.replay = interventions;
        self.replay.sort_by_key(|i| i.instant);
    }

    /// Apply pending interventions at a boundary (rule 33): they become class-0 events at the
    /// current tick and next microstep, or at the named later tick, microstep zero (rule 34).
    fn apply_interventions(&mut self, next_instant: Option<Instant>) -> bool {
        let mut applied = false;
        let pending = std::mem::take(&mut self.pending_interventions);
        for (at, content) in pending {
            applied = true;
            let instant = match at {
                Some(t) if t > self.now.tick => Instant::new(t, 0),
                _ => Instant::new(self.now.tick, self.now.microstep + 1),
            };
            self.apply_intervention_at(instant, content);
        }
        // Replayed interventions whose instant has come (rule 36).
        while let Some(first) = self.replay.first() {
            let due = next_instant.is_none_or(|n| first.instant <= n);
            if !due {
                break;
            }
            let i = self.replay.remove(0);
            self.apply_intervention_at(i.instant, i.content);
            applied = true;
        }
        applied
    }

    fn apply_intervention_at(&mut self, instant: Instant, content: Vec<u8>) {
        self.intervention_seq += 1;
        let key = self.key(
            instant,
            CLASS_CONTROL,
            ORIGIN_INTERVENTIONS,
            self.intervention_seq,
        );
        self.applied_interventions.push(AppliedIntervention {
            instant,
            content: content.clone(),
        });
        // The event's target is the world: the Host routes interventions by their content.
        self.queue.push(Reverse(Event {
            key,
            target: ORIGIN_INTERVENTIONS,
            kind: EventKind::Intervention { payload: content },
        }));
    }

    /// The next event in order, or `None` when the run is over: the queue is empty, `end` is
    /// reached, or a run fault occurred. Handles the instant boundary and the limits of rule 13.
    /// A stale timer event (replaced or cancelled) is skipped. An event for a dead actor is
    /// returned too: the Host applies the domain's policy (RFC 001 rule 26).
    pub fn next_event(&mut self) -> Option<Event> {
        assert!(!self.in_activation, "next_event during an activation");
        if self.fault.is_some() {
            return None;
        }
        loop {
            // Boundary: before looking at the next instant, apply interventions that are due.
            let next_instant = self.queue.peek().map(|Reverse(e)| e.key.instant());
            let boundary = match next_instant {
                Some(i) => i != self.now || self.instant_events == 0,
                None => true,
            };
            if boundary
                && (!self.pending_interventions.is_empty() || !self.replay.is_empty())
                && self.apply_interventions(next_instant)
            {
                continue;
            }
            let Reverse(ev) = self.queue.pop()?;
            if ev.key.tick >= self.config.end {
                self.queue.push(Reverse(ev));
                return None;
            }
            // A stale timer: its generation is older than the actor's current one for that identity.
            if let EventKind::Timer { timer } = ev.kind {
                let current = self
                    .actors
                    .get(&ev.target)
                    .and_then(|a| a.timers.get(&timer))
                    .copied();
                if current != Some(ev.key.seq) {
                    continue;
                }
            }
            let instant = ev.key.instant();
            if instant != self.now {
                if instant.tick != self.now.tick {
                    self.last_tick_microsteps = 0;
                }
                self.now = instant;
                self.instant_events = 0;
                self.instant_origins.clear();
                self.last_tick_microsteps = self.last_tick_microsteps.max(instant.microstep);
                if instant.microstep >= self.config.microsteps_per_tick {
                    let mut origins = self.collect_origins_at(instant);
                    if !origins.contains(&ev.key.origin) {
                        origins.push(ev.key.origin);
                        origins.sort_unstable();
                    }
                    self.fault = Some(RunFault::MicrostepLimit {
                        tick: instant.tick,
                        limit: self.config.microsteps_per_tick,
                        origins,
                    });
                    return None;
                }
            }
            self.instant_events += 1;
            if !self.instant_origins.contains(&ev.key.origin) {
                self.instant_origins.push(ev.key.origin);
            }
            if self.instant_events > self.config.events_per_instant {
                self.fault = Some(RunFault::EventsPerInstantLimit {
                    instant,
                    limit: self.config.events_per_instant,
                    origins: self.instant_origins.clone(),
                });
                return None;
            }
            self.handled += 1;
            if let EventKind::Intervention { payload } = &ev.kind {
                // RFC 001 rule 44: the intervention's record takes its place in handling order.
                let digest: Digest = *blake3::hash(payload).as_bytes();
                self.trace.push(Record::Intervention {
                    key: ev.key,
                    content_digest: digest,
                    content_len: payload.len() as u32,
                });
            }
            return Some(ev);
        }
    }

    fn collect_origins_at(&self, instant: Instant) -> Vec<Identity> {
        let mut origins: Vec<Identity> = self.instant_origins.clone();
        for Reverse(e) in self.queue.iter() {
            if e.key.instant() == instant && !origins.contains(&e.key.origin) {
                origins.push(e.key.origin);
            }
        }
        origins.sort_unstable();
        origins
    }

    /// Begin the activation of `actor` for `event`: the stream position is kept for rollback.
    pub fn begin(&mut self, actor: Identity, event: Event) -> Activation {
        assert!(!self.in_activation, "an activation is in progress");
        self.in_activation = true;
        let stream_at_start = self
            .actors
            .get(&actor)
            .map(|a| a.stream)
            .unwrap_or_else(|| Stream::for_actor(self.config.root_seed, actor));
        Activation {
            event,
            actor,
            stream_at_start,
        }
    }

    /// `random` during an activation (RFC 002 rule 30).
    pub fn draw(&mut self, actor: Identity) -> u64 {
        self.actors
            .get_mut(&actor)
            .map(|a| a.stream.next_u64())
            .unwrap_or(0)
    }

    /// Commit an activation (RFC 001 rule 23): sequence numbers assigned, timers scheduled, sends
    /// delivered by the world, the record appended. `metered` is the work the Host measured.
    pub fn commit(
        &mut self,
        act: Activation,
        sink: &mut Sink,
        metered: u64,
        world: &mut dyn World,
    ) -> Result<(), CommitRefusal> {
        let (effects, refusal) = sink.take();
        if let Some(r) = refusal {
            return self.fail_with(
                act,
                FaultKind::from_refusal(&r),
                0,
                metered,
                CommitRefusal::Refused(r),
            );
        }
        let now = self.now;
        let quota_timers = sink.quotas().pending_timers;
        // Validate before applying (RFC 002 rule 26; rule 27).
        let state = self
            .actors
            .get(&act.actor)
            .cloned()
            .unwrap_or(ActorKernelState {
                seq: 0,
                stream: act.stream_at_start,
                timers: BTreeMap::new(),
                alive: true,
            });
        let mut timers = state.timers.clone();
        for e in &effects {
            match e {
                Effect::SetTimer { timer, delay } => {
                    if now.after(*delay).is_none() {
                        return self.fail_with(
                            act,
                            FaultKind::Contract,
                            *timer,
                            metered,
                            CommitRefusal::DelayBeyondLargestTick {
                                timer: *timer,
                                delay: *delay,
                            },
                        );
                    }
                    timers.insert(*timer, 0);
                }
                Effect::CancelTimer { timer } => {
                    timers.remove(timer);
                }
                _ => {}
            }
        }
        if timers.len() > quota_timers {
            return self.fail_with(
                act,
                FaultKind::Quota,
                0,
                metered,
                CommitRefusal::PendingTimers {
                    limit: quota_timers,
                },
            );
        }
        // Apply, in order (rule 23), one sequence number per effect (rule 17).
        let draws = {
            let a = self
                .actors
                .get(&act.actor)
                .map(|a| a.stream.drawn())
                .unwrap_or(0);
            (a - act.stream_at_start.drawn()) as u32
        };
        let mut committed = Vec::with_capacity(effects.len());
        let origin = act.actor;
        let mut new_events: Vec<Event> = Vec::new();
        {
            let st = self.actors.entry(origin).or_insert(state);
            for e in &effects {
                st.seq += 1;
                let seq = st.seq;
                committed.push(CommittedEffect::from_effect(seq, e));
                match e {
                    Effect::SetTimer { timer, delay } => {
                        let instant = now.after(*delay).expect("validated");
                        st.timers.insert(*timer, seq);
                        new_events.push(Event {
                            key: EventKey {
                                tick: instant.tick,
                                microstep: instant.microstep,
                                class: self.config.timer_class,
                                tie: 0,
                                origin,
                                seq,
                            },
                            target: origin,
                            kind: EventKind::Timer { timer: *timer },
                        });
                    }
                    Effect::CancelTimer { timer } => {
                        st.timers.remove(timer);
                    }
                    Effect::Send { port, payload } => {
                        for d in world.deliver(now, origin, seq, *port, payload) {
                            if let Some(instant) = now.after(d.delay) {
                                new_events.push(Event {
                                    key: EventKey {
                                        tick: instant.tick,
                                        microstep: instant.microstep,
                                        class: d.class,
                                        tie: 0,
                                        origin,
                                        seq,
                                    },
                                    target: d.target,
                                    kind: d.kind,
                                });
                            }
                        }
                    }
                    Effect::Report { .. } => {}
                }
            }
        }
        for mut e in new_events {
            e.key.tie = match self.config.tie {
                TiePolicy::ByIdentity => 0,
                TiePolicy::BySeed => self.tie.tie(e.key.instant(), e.key.class, e.key.origin),
            };
            self.queue.push(Reverse(e));
        }
        self.trace.push(Record::Handled {
            key: act.event.key,
            target: act.actor,
            metered,
            draws,
            effects: committed,
        });
        self.in_activation = false;
        Ok(())
    }

    /// End an activation in a fault (RFC 001 rule 24): effects discarded, the stream restored,
    /// the actor dead until reset or restore, the fault recorded.
    pub fn fault(&mut self, act: Activation, kind: FaultKind, parameter: u32, metered: u64) {
        let _ = self.fail_with::<()>(
            act,
            kind,
            parameter,
            metered,
            CommitRefusal::PendingTimers { limit: 0 },
        );
    }

    fn fail_with<T>(
        &mut self,
        act: Activation,
        kind: FaultKind,
        parameter: u32,
        metered: u64,
        why: CommitRefusal,
    ) -> Result<T, CommitRefusal> {
        if let Some(a) = self.actors.get_mut(&act.actor) {
            a.stream = act.stream_at_start;
            a.alive = false;
        }
        self.trace.push(Record::Fault {
            key: act.event.key,
            actor: act.actor,
            kind,
            parameter,
            metered,
        });
        self.in_activation = false;
        Err(why)
    }

    /// Whether the queue holds any event before `end`.
    pub fn has_next(&self) -> bool {
        self.fault.is_none()
            && self
                .queue
                .peek()
                .is_some_and(|Reverse(e)| e.key.tick < self.config.end)
    }
}

impl FaultKind {
    pub fn from_refusal(r: &SinkRefusal) -> Self {
        match r {
            SinkRefusal::Quota { .. } => FaultKind::Quota,
            SinkRefusal::Contract { .. } => FaultKind::Contract,
        }
    }
}

/// The advance units of RFC 001 §2.1, as decisions the Host takes between events: whether the
/// event the kernel offers next is still within the unit that was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Advance {
    OneEvent,
    OneInstant,
    OneTick,
    ToTick(Tick),
}

impl Advance {
    /// Whether an event at `next` still belongs to a unit started at `start`, after `handled`
    /// events of that unit.
    pub fn includes(self, start: Instant, next: Instant, handled: u64) -> bool {
        match self {
            Advance::OneEvent => handled == 0,
            Advance::OneInstant => handled == 0 || next == start,
            Advance::OneTick => handled == 0 || next.tick == start.tick,
            Advance::ToTick(t) => next.tick < t,
        }
    }
}
