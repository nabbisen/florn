//! RFC 001 §14's properties that need no engine, driven through the kernel with scripted actors.
use florn_kernel::*;

/// A scripted actor: what it does when activated.
#[derive(Clone)]
enum Script {
    /// On every activation: send `bytes` through port 0 and re-arm timer 0 after `period`.
    Pinger { period: u64, bytes: Vec<u8> },
    /// On every activation: re-arm timer 0 with zero delay (an endless zero-delay chain).
    ZeroDelay,
    /// On every activation: draw `n` random values, report them, re-arm after `period`.
    Drawer { period: u64, n: u32 },
    /// Send, then trap.
    SendThenTrap,
    /// Do nothing.
    Idle,
}

/// A world with one connection: port 0 of every actor reaches every other actor's port 0 after `delay`.
struct Mesh {
    actors: Vec<Identity>,
    delay: u64,
}
impl World for Mesh {
    fn deliver(
        &mut self,
        _: Instant,
        origin: Identity,
        _: u64,
        _: u32,
        payload: &[u8],
    ) -> Vec<Delivery> {
        self.actors
            .iter()
            .filter(|a| **a != origin)
            .map(|a| Delivery {
                target: *a,
                class: 2,
                delay: self.delay,
                kind: EventKind::Message {
                    port: 0,
                    payload: payload.to_vec(),
                },
            })
            .collect()
    }
}

struct Run {
    kernel: Kernel,
    scripts: Vec<(Identity, Script)>,
    sinks: std::collections::BTreeMap<Identity, Sink>,
    world: Mesh,
    reports: Vec<(Identity, Vec<u8>)>,
}

impl Run {
    fn new(config: Config, scripts: Vec<(Identity, Script)>, delay: u64) -> Self {
        let mut kernel = Kernel::new(config);
        let mut sinks = std::collections::BTreeMap::new();
        for (id, _) in &scripts {
            kernel.add_actor(*id);
            sinks.insert(*id, Sink::new(Quotas::default(), vec![0], vec![1]));
        }
        let actors = scripts.iter().map(|(id, _)| *id).collect();
        Self {
            kernel,
            scripts,
            sinks,
            world: Mesh { actors, delay },
            reports: Vec::new(),
        }
    }

    /// `init` as the first activation (RFC 002 rule 36): each pinger arms its timer.
    fn init(&mut self) {
        let mut scripts = self.scripts.clone();
        scripts.sort_by_key(|(id, _)| *id); // RFC 002 rule 36: ascending identity order
        for (id, script) in scripts {
            let ev = Event {
                key: EventKey {
                    tick: 0,
                    microstep: 0,
                    class: 0,
                    tie: 0,
                    origin: id,
                    seq: 0,
                },
                target: id,
                kind: EventKind::Domain { payload: vec![] },
            };
            let act = self.kernel.begin(id, ev);
            let sink = self.sinks.get_mut(&id).unwrap();
            sink.reset();
            match script {
                Script::Pinger { period, .. } | Script::Drawer { period, .. } => {
                    sink.set_timer(0, period).unwrap()
                }
                Script::ZeroDelay => sink.set_timer(0, 0).unwrap(),
                Script::SendThenTrap => sink.set_timer(0, 10).unwrap(),
                Script::Idle => {}
            }
            self.kernel.commit(act, sink, 0, &mut self.world).unwrap();
        }
    }

    /// Handle events until the kernel has none, or `max` events.
    fn run(&mut self, max: u64) {
        let mut n = 0;
        while n < max {
            let Some(ev) = self.kernel.next_event() else {
                break;
            };
            n += 1;
            let id = ev.target;
            let script = self
                .scripts
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, s)| s.clone())
                .unwrap_or(Script::Idle);
            let alive = self.kernel.actor(id).map(|a| a.alive).unwrap_or(false);
            if !alive {
                continue; // the domain's policy here: drop
            }
            let act = self.kernel.begin(id, ev);
            let sink = self.sinks.get_mut(&id).unwrap();
            sink.reset();
            let mut trap = false;
            match script {
                Script::Pinger { period, bytes } => {
                    sink.send(0, bytes).unwrap();
                    sink.set_timer(0, period).unwrap();
                }
                Script::ZeroDelay => sink.set_timer(0, 0).unwrap(),
                Script::Drawer { period, n } => {
                    let mut out = Vec::new();
                    for _ in 0..n {
                        out.extend_from_slice(&self.kernel.draw(id).to_le_bytes());
                    }
                    self.reports.push((id, out.clone()));
                    sink.report(1, out).unwrap();
                    sink.set_timer(0, period).unwrap();
                }
                Script::SendThenTrap => {
                    sink.send(0, vec![9]).unwrap();
                    trap = true;
                }
                Script::Idle => {}
            }
            if trap {
                self.kernel.fault(act, trace::FaultKind::Trap, 0, 1);
            } else {
                self.kernel.commit(act, sink, 1, &mut self.world).unwrap();
            }
        }
    }
}

fn config(seed: u64, tie: TiePolicy) -> Config {
    Config {
        root_seed: seed,
        tie,
        end: 1_000_000,
        chain_interval: 100,
        keep_records: true,
        ..Config::default()
    }
}

#[test]
fn tier_1_same_inputs_same_digest() {
    let mk = || {
        Run::new(
            config(42, TiePolicy::ByIdentity),
            vec![
                (
                    1,
                    Script::Pinger {
                        period: 1000,
                        bytes: vec![1],
                    },
                ),
                (
                    2,
                    Script::Pinger {
                        period: 700,
                        bytes: vec![2],
                    },
                ),
            ],
            300,
        )
    };
    let (mut a, mut b) = (mk(), mk());
    a.init();
    b.init();
    a.run(5_000);
    b.run(5_000);
    assert_eq!(a.kernel.trace.chain(), b.kernel.trace.chain());
    assert_eq!(
        a.kernel.trace.compare(&b.kernel.trace.stored()),
        Comparison::Matched {
            points: a.kernel.trace.stored().len() as u64
        }
    );
    assert!(a.kernel.handled() > 1_000);
}

#[test]
fn order_follows_the_policy_not_the_creation_order() {
    // Two origins produce events for the same instant; by-identity: the lower identity first,
    // whichever was created first.
    for tie in [TiePolicy::ByIdentity, TiePolicy::BySeed] {
        let scripts = vec![
            (
                1,
                Script::Pinger {
                    period: 1000,
                    bytes: vec![1],
                },
            ),
            (
                2,
                Script::Pinger {
                    period: 1000,
                    bytes: vec![2],
                },
            ),
        ];
        let mut forward = Run::new(config(7, tie), scripts.clone(), 1000);
        let mut reversed = Run::new(config(7, tie), scripts.into_iter().rev().collect(), 1000);
        forward.init();
        reversed.init();
        forward.run(2_000);
        reversed.run(2_000);
        assert_eq!(
            forward.kernel.trace.chain(),
            reversed.kernel.trace.chain(),
            "{tie:?}"
        );
        if tie == TiePolicy::ByIdentity {
            let records = forward.kernel.trace.kept_records().unwrap();
            // At tick 1000 both timers fire at microstep 0, class 1: origin 1 before origin 2.
            let at_1000: Vec<Identity> = records
                .iter()
                .filter_map(|r| match r {
                    Record::Handled { key, target, .. } if key.tick == 1000 && key.class == 1 => {
                        Some(*target)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(at_1000, vec![1, 2]);
        }
    }
}

#[test]
fn by_seed_is_fair_over_many_races() {
    // 10,000 equal-time races between two origins; each wins between 45% and 55% (RFC 001 §14).
    let mut r = Run::new(
        Config {
            root_seed: 99,
            tie: TiePolicy::BySeed,
            end: 10_001_000,
            chain_interval: 1_000_000,
            keep_records: true,
            ..Config::default()
        },
        vec![
            (
                1,
                Script::Pinger {
                    period: 1000,
                    bytes: vec![1],
                },
            ),
            (
                2,
                Script::Pinger {
                    period: 1000,
                    bytes: vec![2],
                },
            ),
        ],
        999_999_999,
    );
    r.init();
    r.run(30_000);
    let records = r.kernel.trace.kept_records().unwrap();
    let mut wins = [0u32; 2];
    let mut last_tick = u64::MAX;
    for rec in records {
        if let Record::Handled { key, target, .. } = rec
            && key.class == 1
            && key.tick != last_tick
        {
            last_tick = key.tick;
            wins[(*target - 1) as usize] += 1;
        }
    }
    let total = wins[0] + wins[1];
    assert!(total >= 10_000, "{total}");
    let share = wins[0] as f64 / total as f64;
    assert!((0.45..=0.55).contains(&share), "origin 1 won {share:.3}");
}

#[test]
fn zero_delay_chain_ends_with_the_microstep_fault_naming_the_origin() {
    let mut r = Run::new(
        Config {
            microsteps_per_tick: 50,
            ..config(1, TiePolicy::ByIdentity)
        },
        vec![(5, Script::ZeroDelay)],
        1,
    );
    r.init();
    r.run(1_000);
    assert_eq!(
        r.kernel.run_fault(),
        Some(&RunFault::MicrostepLimit {
            tick: 0,
            limit: 50,
            origins: vec![5]
        })
    );
    assert!(r.kernel.handled() <= 50);
}

#[test]
fn too_many_events_in_one_instant_end_the_run_with_the_fault_naming_the_origins() {
    // Three pingers with one period over a mesh without delay: at tick 10, three timers fire
    // and six messages arrive in the same instant, above a limit of five.
    let mut r = Run::new(
        Config {
            events_per_instant: 5,
            ..config(1, TiePolicy::ByIdentity)
        },
        vec![
            (
                1,
                Script::Pinger {
                    period: 10,
                    bytes: vec![1],
                },
            ),
            (
                2,
                Script::Pinger {
                    period: 10,
                    bytes: vec![2],
                },
            ),
            (
                3,
                Script::Pinger {
                    period: 10,
                    bytes: vec![3],
                },
            ),
        ],
        0,
    );
    r.init();
    r.run(1_000);
    match r.kernel.run_fault() {
        Some(RunFault::EventsPerInstantLimit {
            instant,
            limit,
            origins,
        }) => {
            assert_eq!(instant.tick, 10);
            assert_eq!(*limit, 5);
            assert!(
                !origins.is_empty() && origins.iter().all(|o| (1..=3).contains(o)),
                "{origins:?}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn activation_that_faults_leaves_no_message() {
    let mut r = Run::new(
        config(3, TiePolicy::ByIdentity),
        vec![(1, Script::SendThenTrap), (2, Script::Idle)],
        5,
    );
    r.init();
    r.run(100);
    let records = r.kernel.trace.kept_records().unwrap();
    assert!(records.iter().any(|x| matches!(
        x,
        Record::Fault {
            actor: 1,
            kind: trace::FaultKind::Trap,
            ..
        }
    )));
    assert!(
        !records
            .iter()
            .any(|x| matches!(x, Record::Handled { target: 2, key, .. } if key.class == 2)),
        "no message reached actor 2"
    );
    assert!(!r.kernel.actor(1).unwrap().alive);
}

#[test]
fn streams_are_unchanged_by_a_third_actor_and_rolled_back_on_a_fault() {
    let two = vec![
        (1, Script::Drawer { period: 100, n: 2 }),
        (2, Script::Drawer { period: 100, n: 2 }),
    ];
    let mut a = Run::new(config(11, TiePolicy::ByIdentity), two.clone(), 1);
    let mut three = two;
    three.push((3, Script::Drawer { period: 100, n: 2 }));
    let mut b = Run::new(config(11, TiePolicy::ByIdentity), three, 1);
    a.init();
    b.init();
    a.run(40);
    b.run(60);
    let draws = |r: &Run, id: Identity| {
        r.reports
            .iter()
            .filter(|(i, _)| *i == id)
            .map(|(_, v)| v.clone())
            .take(10)
            .collect::<Vec<_>>()
    };
    assert_eq!(draws(&a, 1), draws(&b, 1));
    assert_eq!(draws(&a, 2), draws(&b, 2));
    assert_ne!(draws(&b, 1), draws(&b, 3));
    // Rollback: draw, fault, the next draw after reset repeats the same values.
    let mut k = Kernel::new(config(5, TiePolicy::ByIdentity));
    k.add_actor(1);
    let ev = Event {
        key: EventKey {
            tick: 0,
            microstep: 0,
            class: 1,
            tie: 0,
            origin: 1,
            seq: 1,
        },
        target: 1,
        kind: EventKind::Timer { timer: 0 },
    };
    let act = k.begin(1, ev.clone());
    let first = k.draw(1);
    k.fault(act, trace::FaultKind::Trap, 0, 0);
    k.actor_mut(1).unwrap().alive = true; // reset
    let act = k.begin(1, ev);
    assert_eq!(k.draw(1), first);
    let mut sink = Sink::new(Quotas::default(), vec![], vec![]);
    k.commit(act, &mut sink, 0, &mut NoWorld).unwrap();
}

#[test]
fn interventions_are_recorded_and_replay_matches() {
    let scripts = vec![
        (
            1,
            Script::Pinger {
                period: 1000,
                bytes: vec![1],
            },
        ),
        (
            2,
            Script::Pinger {
                period: 700,
                bytes: vec![2],
            },
        ),
    ];
    let mut a = Run::new(config(42, TiePolicy::ByIdentity), scripts.clone(), 300);
    a.init();
    a.run(500);
    a.kernel.intervene(None, b"take l12 down".to_vec());
    a.run(500);
    a.kernel.intervene(Some(900_000), b"later".to_vec());
    a.run(100_000);
    let applied = a.kernel.applied_interventions().to_vec();
    assert_eq!(applied.len(), 2);
    assert!(applied[0].instant.tick < 900_000 && applied[1].instant == Instant::new(900_000, 0));
    // Replay from the inputs: the recorded interventions at their instants.
    let mut b = Run::new(config(42, TiePolicy::ByIdentity), scripts, 300);
    b.kernel.set_replay(applied);
    b.init();
    b.run(200_000);
    assert_eq!(a.kernel.trace.chain(), b.kernel.trace.chain());
    assert!(matches!(
        a.kernel.trace.compare(&b.kernel.trace.stored()),
        Comparison::Matched { .. }
    ));
    let records = a.kernel.trace.kept_records().unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|r| matches!(r, Record::Intervention { .. }))
            .count(),
        2
    );
}

#[test]
fn timers_replace_and_cancel() {
    let mut k = Kernel::new(config(1, TiePolicy::ByIdentity));
    k.add_actor(1);
    let mut sink = Sink::new(
        Quotas {
            pending_timers: 2,
            ..Quotas::default()
        },
        vec![0],
        vec![],
    );
    let ev = Event {
        key: EventKey {
            tick: 0,
            microstep: 0,
            class: 1,
            tie: 0,
            origin: 1,
            seq: 0,
        },
        target: 1,
        kind: EventKind::Timer { timer: 0 },
    };
    // set twice with one identity: fires once, at the second delay
    let act = k.begin(1, ev.clone());
    sink.reset();
    sink.set_timer(7, 10).unwrap();
    sink.set_timer(7, 20).unwrap();
    k.commit(act, &mut sink, 0, &mut NoWorld).unwrap();
    let e = k.next_event().unwrap();
    assert_eq!((e.key.tick, e.kind), (20, EventKind::Timer { timer: 7 }));
    assert!(k.next_event().is_none());
    // set then cancel in one activation leaves no timer
    let act = k.begin(1, ev.clone());
    sink.reset();
    sink.set_timer(8, 5).unwrap();
    sink.cancel_timer(8).unwrap();
    k.commit(act, &mut sink, 0, &mut NoWorld).unwrap();
    assert!(k.next_event().is_none());
    // the third pending timer under a quota of two faults
    let act = k.begin(1, ev.clone());
    sink.reset();
    sink.set_timer(1, 5).unwrap();
    sink.set_timer(2, 5).unwrap();
    sink.set_timer(3, 5).unwrap();
    assert_eq!(
        k.commit(act, &mut sink, 0, &mut NoWorld),
        Err(CommitRefusal::PendingTimers { limit: 2 })
    );
    // a delay beyond the largest tick is a contract fault
    k.actor_mut(1).unwrap().alive = true;
    let act = k.begin(1, ev);
    sink.reset();
    sink.set_timer(1, u64::MAX).unwrap();
    assert_eq!(
        k.commit(act, &mut sink, 0, &mut NoWorld),
        Err(CommitRefusal::DelayBeyondLargestTick {
            timer: 1,
            delay: u64::MAX
        })
    );
}

#[test]
fn end_is_respected_and_advance_units_classify() {
    let mut r = Run::new(
        Config {
            end: 2_500,
            ..config(1, TiePolicy::ByIdentity)
        },
        vec![(
            1,
            Script::Pinger {
                period: 1000,
                bytes: vec![],
            },
        )],
        1,
    );
    r.init();
    r.run(1_000);
    assert_eq!(r.kernel.now().tick, 2_000);
    assert!(!r.kernel.has_next());
    assert!(Advance::OneEvent.includes(Instant::new(5, 0), Instant::new(5, 0), 0));
    assert!(!Advance::OneEvent.includes(Instant::new(5, 0), Instant::new(5, 0), 1));
    assert!(Advance::OneInstant.includes(Instant::new(5, 0), Instant::new(5, 0), 3));
    assert!(!Advance::OneInstant.includes(Instant::new(5, 0), Instant::new(5, 1), 3));
    assert!(Advance::OneTick.includes(Instant::new(5, 0), Instant::new(5, 9), 3));
    assert!(!Advance::OneTick.includes(Instant::new(5, 0), Instant::new(6, 0), 3));
    assert!(Advance::ToTick(10).includes(Instant::new(5, 0), Instant::new(9, 0), 3));
    assert!(!Advance::ToTick(10).includes(Instant::new(5, 0), Instant::new(10, 0), 3));
}

#[test]
fn chacha8_matches_the_reference_implementation() {
    use rand_chacha::rand_core::{Rng, SeedableRng};
    let key = StreamKey::derive(42, 1, "actor");
    let mut ours = Stream::new(key);
    let mut theirs = rand_chacha::ChaCha8Rng::from_seed(key.0);
    for _ in 0..1000 {
        assert_eq!(ours.next_u64(), theirs.next_u64());
    }
}
