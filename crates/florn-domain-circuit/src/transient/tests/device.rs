//! RFC 012 rules 19 to 21 and RFC 013 rules 5, 8, 9 and 16: the solver with a guest device model,
//! played by a native model that answers what a guest would: the same stamps as a built-in, or a
//! loss at a chosen call.

use crate::dc::{Stage, initial_solve, initial_solve_with};
use crate::device::{Device, Lost, Stamp};
use crate::diode::Junction;
use crate::plant::Plant;
use crate::tests::Deck;
use crate::transient::Transient;

use florn_kernel::FaultKind;

/// What the solver asked of the device, in order.
#[derive(Clone, Debug, PartialEq)]
enum Call {
    Begin(u64, f64),
    Evaluate(Vec<f64>),
    Accept(Vec<f64>),
    Reject,
}

/// What the model is.
#[derive(Clone, Copy)]
enum Model {
    /// A conductance between its two pins.
    Conductance(f64),
    /// The Shockley junction, limited as the built-in diode's is.
    Diode(Junction),
    /// A conductance whose current flips sign at every evaluation from the third on: Newton never
    /// converges.
    Flip,
}

/// A loss the model answers at a call: the `n`th call of a kind.
#[derive(Clone, Copy, PartialEq)]
enum On {
    Begin,
    Evaluate,
    Accept,
    Reject,
}

struct Native {
    model: Model,
    calls: Vec<Call>,
    /// The junction voltage the last evaluation used, committed at an accepted step.
    committed: f64,
    trial: f64,
    lose: Option<(On, usize, Lost)>,
    seen: [usize; 4],
}

impl Native {
    fn new(model: Model) -> Self {
        Self {
            model,
            calls: Vec::new(),
            committed: 0.0,
            trial: 0.0,
            lose: None,
            seen: [0; 4],
        }
    }

    fn losing(mut self, on: On, nth: usize, lost: Lost) -> Self {
        self.lose = Some((on, nth, lost));
        self
    }

    fn check(&mut self, on: On) -> Result<(), Lost> {
        let k = on as usize;
        self.seen[k] += 1;
        match &self.lose {
            Some((at, nth, lost)) if *at == on && *nth == self.seen[k] => Err(lost.clone()),
            _ => Ok(()),
        }
    }

    fn count(&self, f: impl Fn(&Call) -> bool) -> usize {
        self.calls.iter().filter(|c| f(c)).count()
    }
}

impl Device for Native {
    fn present(&self) -> bool {
        true
    }
    fn begin(&mut self, start: u64, h: f64) -> Result<(), Lost> {
        self.calls.push(Call::Begin(start, h));
        self.check(On::Begin)?;
        self.trial = self.committed;
        Ok(())
    }
    fn evaluate(&mut self, trial: &[f64]) -> Result<Stamp, Lost> {
        self.calls.push(Call::Evaluate(trial.to_vec()));
        self.check(On::Evaluate)?;
        let v = trial[0] - trial[1];
        Ok(match self.model {
            Model::Conductance(g) => {
                let i = g * v;
                Stamp {
                    currents: vec![i, -i],
                    jacobian: vec![g, -g, -g, g],
                    limited: false,
                }
            }
            Model::Flip => {
                let calls = self
                    .calls
                    .iter()
                    .filter(|c| matches!(c, Call::Evaluate(_)))
                    .count();
                let g = 1e-3;
                let i = if calls < 3 {
                    g * v
                } else if calls % 2 == 0 {
                    1.0
                } else {
                    -1.0
                };
                Stamp {
                    currents: vec![i, -i],
                    jacobian: vec![g, -g, -g, g],
                    limited: false,
                }
            }
            Model::Diode(j) => {
                let e = j.evaluate(v, self.trial);
                self.trial = e.v_used;
                Stamp {
                    currents: vec![e.i, -e.i],
                    jacobian: vec![e.g, -e.g, -e.g, e.g],
                    limited: e.limited,
                }
            }
        })
    }
    fn accept(&mut self, solution: &[f64]) -> Result<(), Lost> {
        self.calls.push(Call::Accept(solution.to_vec()));
        self.check(On::Accept)?;
        self.committed = solution[0] - solution[1];
        Ok(())
    }
    fn reject(&mut self) -> Result<(), Lost> {
        self.calls.push(Call::Reject);
        self.check(On::Reject)?;
        self.trial = self.committed;
        Ok(())
    }
}

/// A source through a resistor into a node `mid` that has a voltmeter and either a built-in `rd`
/// (identity 10) or the guest device `d` (identity 10), from `mid` to the ground.
fn plant_with(guest: bool, element: &str, config: &str, profile: &str) -> Plant {
    plant_through(guest, element, config, profile, "1kohm")
}

/// As [`plant_with`], with the series resistance given.
fn plant_through(guest: bool, element: &str, config: &str, profile: &str, ohms: &str) -> Plant {
    let mut deck = Deck::new()
        .builtin(1, "vs", "voltage-source", "v = \"5V\"")
        .builtin(2, "r", "resistor", &format!("r = \"{ohms}\""))
        .builtin(3, "vm", "voltmeter", "period = \"10us\"")
        .profile(profile);
    deck = if guest {
        deck.guest(10, "d", &[("a", "circuit/pin"), ("k", "circuit/pin")])
    } else {
        deck.builtin(10, "d", element, config)
    };
    let pins: [&str; 2] = if element == "diode" {
        ["d.a", "d.k"]
    } else {
        ["d.a", "d.b"]
    };
    let (a, k) = if guest {
        ("d.a", "d.k")
    } else {
        (pins[0], pins[1])
    };
    let mut plant = deck
        .net("in", &["vs.p", "r.a"])
        .net("mid", &["r.b", a, "vm.p"])
        .ground("gnd", &["vs.n", k, "vm.n"])
        .plant();
    if guest {
        plant.set_guest(10);
    }
    plant
}

fn conductance() -> Plant {
    plant_with(true, "resistor", "", "")
}

fn resistor() -> Plant {
    plant_with(false, "resistor", "r = \"1kohm\"", "")
}

fn diode_plant(guest: bool, profile: &str) -> Plant {
    plant_with(guest, "diode", "", profile)
}

fn junction() -> Junction {
    Junction::new(1e-14, 1.0, 1e-12)
}

/// The transient solver at the operating point of the plant with the device given.
fn started(plant: &Plant, device: &mut dyn Device) -> Transient {
    let solved = initial_solve_with(plant, device).expect("the operating point");
    Transient::new(plant, &solved.solution)
}

/// Equal to within the `gmin` of the plant (1 pS from every net to the ground).
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} against {b}");
}

fn bits(x: &[f64]) -> Vec<u64> {
    x.iter().map(|v| v.to_bits()).collect()
}

#[test]
fn the_plant_with_a_guest_has_its_pins_and_its_place_in_the_order() {
    let p = conductance();
    let g = p.guest.as_ref().expect("a guest device");
    assert_eq!((g.actor, g.name.as_str()), (10, "d"));
    // The pins in ascending port identity: `a` is on `mid`, `k` on the ground.
    assert_eq!(g.nodes.len(), 2);
    assert!(g.nodes[0].is_some() && g.nodes[1].is_none());
    // Every built-in has a lower identity: the guest is stamped after all of them.
    assert_eq!(g.before, p.devices.len());
    // A guest of lower identity than some built-in is stamped before it (rule 5: ascending actor
    // identity, built-ins and the guest alike).
    let mut early = Deck::new()
        .builtin(1, "vs", "voltage-source", "v = \"5V\"")
        .guest(2, "d", &[("a", "circuit/pin"), ("k", "circuit/pin")])
        .builtin(3, "r", "resistor", "r = \"1kohm\"")
        .builtin(4, "vm", "voltmeter", "period = \"10us\"")
        .net("in", &["vs.p", "r.a", "d.a", "vm.p"])
        .ground("gnd", &["vs.n", "r.b", "d.k", "vm.n"])
        .plant();
    early.set_guest(2);
    let g = early.guest.as_ref().unwrap();
    assert_eq!(
        g.before, 1,
        "after the source (identity 1), before the resistor (3)"
    );
    assert_eq!(early.devices[g.before].actor, 3);
    // A scenario without the guest has none.
    assert!(resistor().guest.is_none());
    let mut none = conductance();
    none.set_guest(77);
    assert!(none.guest.is_none(), "an actor with no pins is no device");
}

#[test]
fn a_guest_conductance_is_a_resistor_to_the_last_bit() {
    let g = conductance();
    let r = resistor();
    let mut model = Native::new(Model::Conductance(1.0 / 1000.0));
    let solved = initial_solve_with(&g, &mut model).unwrap();
    assert_eq!(solved.lost, None);
    let reference = initial_solve(&r).unwrap();
    assert_eq!(bits(&solved.solution.x), bits(&reference.x));
    let mut t = Transient::new(&g, &solved.solution);
    let mut tr = Transient::new(&r, &reference);
    for until in [10_000, 25_000, 40_000, 100_000] {
        assert!(t.advance_with(until, &mut model).stuck.is_none());
        assert!(tr.advance(until).stuck.is_none());
        assert_eq!(bits(t.x()), bits(tr.x()), "at {until}");
    }
    near(t.voltage("mid").unwrap(), 2.5);
}

/// The source, a guest or a resistor of identity 2 (a built-in, `r2`, follows it with identity 3),
/// and a voltmeter: the device sits **between** built-ins in the order of the evaluation.
fn early_plant(guest: bool) -> Plant {
    let deck = Deck::new().builtin(1, "vs", "voltage-source", "v = \"5V\"");
    let deck = if guest {
        deck.guest(2, "d", &[("a", "circuit/pin"), ("k", "circuit/pin")])
    } else {
        deck.builtin(2, "d", "resistor", "r = \"1kohm\"")
    };
    let other = if guest { "d.k" } else { "d.b" };
    let mut plant = deck
        .builtin(3, "r", "resistor", "r = \"1kohm\"")
        .builtin(4, "vm", "voltmeter", "period = \"10us\"")
        .net("in", &["vs.p", "r.a"])
        .net("mid", &["r.b", "d.a", "vm.p"])
        .ground("gnd", &["vs.n", other, "vm.n"])
        .plant();
    if guest {
        plant.set_guest(2);
    }
    plant
}

#[test]
fn a_guest_of_lower_identity_than_a_built_in_is_stamped_in_its_place_to_the_last_bit() {
    let (guest, builtin) = (early_plant(true), early_plant(false));
    assert_eq!(guest.guest.as_ref().unwrap().before, 1);
    let mut model = Native::new(Model::Conductance(1.0 / 1000.0));
    let solved = initial_solve_with(&guest, &mut model).unwrap();
    let reference = initial_solve(&builtin).unwrap();
    assert_eq!(bits(&solved.solution.x), bits(&reference.x));
    let mut t = Transient::new(&guest, &solved.solution);
    let mut tr = Transient::new(&builtin, &reference);
    assert!(t.advance_with(60_000, &mut model).stuck.is_none());
    assert!(tr.advance(60_000).stuck.is_none());
    assert_eq!(bits(t.x()), bits(tr.x()));
    near(t.voltage("mid").unwrap(), 2.5);
}

#[test]
fn a_guest_diode_is_the_built_in_diode_to_the_last_bit() {
    let guest = diode_plant(true, "");
    let builtin = diode_plant(false, "");
    let mut model = Native::new(Model::Diode(junction()));
    let solved = initial_solve_with(&guest, &mut model).unwrap();
    let reference = initial_solve(&builtin).unwrap();
    assert_eq!(solved.solution.stage, reference.stage);
    assert_eq!(solved.solution.iterations, reference.iterations);
    assert_eq!(bits(&solved.solution.x), bits(&reference.x));
    let mut t = Transient::new(&guest, &solved.solution);
    let mut tr = Transient::new(&builtin, &reference);
    // The source stepped to −5 V and back, as `diode-clip` does: the limiter and the restarts.
    for (until, volts) in [
        (10_000u64, None),
        (50_000, Some(-5.0)),
        (90_000, Some(5.0)),
        (130_000, None),
    ] {
        if let Some(v) = volts {
            t.set(1, v);
            t.cut();
            tr.set(1, v);
            tr.cut();
        }
        let (a, b) = (t.advance_with(until, &mut model), tr.advance(until));
        assert!(a.stuck.is_none() && b.stuck.is_none());
        assert_eq!(a.report.steps, b.report.steps, "at {until}");
        assert_eq!(a.report.iterations, b.report.iterations, "at {until}");
        assert_eq!(bits(t.x()), bits(tr.x()), "at {until}");
    }
}

#[test]
fn the_dc_solve_opens_the_device_in_dc_form_and_the_solve_that_gives_the_result_accepts() {
    let p = diode_plant(true, "");
    let mut model = Native::new(Model::Diode(junction()));
    let solved = initial_solve_with(&p, &mut model).unwrap();
    assert_eq!(solved.solution.stage, Stage::Plain);
    // One compound activation: `begin-step(0, 0.0)`, one evaluation per iteration, one accept
    // with the pin voltages of the operating point.
    assert_eq!(model.calls[0], Call::Begin(0, 0.0));
    let evaluations = model.count(|c| matches!(c, Call::Evaluate(_)));
    assert_eq!(evaluations as u32, solved.solution.iterations);
    assert_eq!(model.count(|c| matches!(c, Call::Begin(..))), 1);
    assert_eq!(model.count(|c| matches!(c, Call::Reject)), 0);
    let Some(Call::Accept(pins)) = model.calls.last() else {
        panic!("{:?}", model.calls.last())
    };
    let a = solved.solution.x[p.nets.iter().position(|n| n.name == "mid").unwrap()];
    assert_eq!(pins, &vec![a, 0.0]);
}

#[test]
fn gmin_stepping_accepts_the_last_level_only() {
    // With a series resistance of 100 kohm and a bound of 7 iterations `gmin` stepping gets there
    // (the figures of `dc`'s tests): every level but the last is a solve that does not give the
    // result, and so is the plain solve that failed before them.
    let p = plant_through(true, "diode", "", "newton.max-iterations = 7", "100kohm");
    let mut model = Native::new(Model::Diode(junction()));
    let solved = initial_solve_with(&p, &mut model).unwrap();
    assert_eq!(solved.solution.stage, Stage::GminStepping);
    let begins = model.count(|c| matches!(c, Call::Begin(..)));
    let accepts = model.count(|c| matches!(c, Call::Accept(_)));
    let rejects = model.count(|c| matches!(c, Call::Reject));
    assert_eq!(accepts, 1);
    assert_eq!(begins, accepts + rejects);
    assert!(
        rejects >= 2,
        "the failed plain solve and the levels before the last: {rejects}"
    );
    assert!(matches!(model.calls.last(), Some(Call::Accept(_))));
}

#[test]
fn the_current_into_each_pin_leaves_its_own_net() {
    // A device between two nets that are neither the ground: a source into 1 kohm into `a`, the
    // device from `a` to `b`, 1 kohm from `b` to the ground. The guest is a resistor to the last
    // bit, the current of the second pin leaving the second net.
    let build = |guest: bool| {
        let deck = Deck::new()
            .builtin(1, "vs", "voltage-source", "v = \"5V\"")
            .builtin(2, "r1", "resistor", "r = \"1kohm\"")
            .builtin(3, "r2", "resistor", "r = \"1kohm\"")
            .builtin(4, "vm", "voltmeter", "period = \"10us\"");
        let deck = if guest {
            deck.guest(10, "d", &[("a", "circuit/pin"), ("k", "circuit/pin")])
        } else {
            deck.builtin(10, "d", "resistor", "r = \"1kohm\"")
        };
        let other = if guest { "d.k" } else { "d.b" };
        let mut plant = deck
            .net("in", &["vs.p", "r1.a"])
            .net("a", &["r1.b", "d.a", "vm.p"])
            .net("b", &["r2.a", other])
            .ground("gnd", &["vs.n", "r2.b", "vm.n"])
            .plant();
        if guest {
            plant.set_guest(10);
        }
        plant
    };
    let (guest, builtin) = (build(true), build(false));
    let g = guest.guest.as_ref().unwrap();
    assert!(g.nodes.iter().all(Option::is_some));
    let mut model = Native::new(Model::Conductance(1.0 / 1000.0));
    let solved = initial_solve_with(&guest, &mut model).unwrap();
    let reference = initial_solve(&builtin).unwrap();
    assert_eq!(bits(&solved.solution.x), bits(&reference.x));
    let b = guest.nets.iter().position(|n| n.name == "b").unwrap();
    near(solved.solution.x[b], 5.0 / 3.0);
}

#[test]
fn an_attempt_that_does_not_converge_is_closed_by_a_reject_like_any_other() {
    // Stamps that flip from the third evaluation: Newton fails at every size, and the solver gives
    // up at the smallest step. Every attempt that began was closed once, by `reject-step`.
    let p = conductance();
    let mut model = Native::new(Model::Flip);
    // The initial solve takes two evaluations and is accepted; the flip begins with the third.
    let mut t = started(&p, &mut model);
    let out = t.advance_with(100_000, &mut model);
    assert!(out.stuck.is_some() && !out.ended, "{out:?}");
    assert!(out.report.attempts > 1, "{:?}", out.report);
    let begins = model.count(|c| matches!(c, Call::Begin(..)));
    let (accepts, rejects) = (
        model.count(|c| matches!(c, Call::Accept(_))),
        model.count(|c| matches!(c, Call::Reject)),
    );
    assert_eq!(accepts, 1, "the initial solve's, and no step");
    assert_eq!(
        rejects, out.report.attempts as usize,
        "each attempt is discarded"
    );
    assert_eq!(begins, accepts + rejects);
    // The state is that of the last accepted step: none was taken.
    assert_eq!(t.now(), 0);
}

#[test]
fn a_failed_stage_is_rejected_and_only_the_solve_that_gives_the_result_is_accepted() {
    // A bound of 6 iterations fails the plain solve and gmin stepping, and source stepping
    // follows (the figures of `dc`'s tests): every solve but the last of the last stage is
    // discarded.
    let p = diode_plant(true, "newton.max-iterations = 6");
    let mut model = Native::new(Model::Diode(junction()));
    let solved = initial_solve_with(&p, &mut model).unwrap();
    assert_eq!(solved.solution.stage, Stage::SourceStepping);
    let begins = model.count(|c| matches!(c, Call::Begin(..)));
    let accepts = model.count(|c| matches!(c, Call::Accept(_)));
    let rejects = model.count(|c| matches!(c, Call::Reject));
    assert_eq!(accepts, 1, "one solve gives the result");
    assert_eq!(begins, accepts + rejects, "every activation is closed once");
    assert!(
        rejects >= 10,
        "the plain solve, the gmin levels and nine of ten source levels: {rejects}"
    );
    assert!(matches!(model.calls.last(), Some(Call::Accept(_))));
    // The same plant with the built-in diode gives the same point.
    let reference = initial_solve(&diode_plant(false, "newton.max-iterations = 6")).unwrap();
    assert_eq!(reference.stage, Stage::SourceStepping);
    for (a, b) in solved.solution.x.iter().zip(&reference.x) {
        assert!((a - b).abs() < 1e-9, "{a} against {b}");
    }
}

#[test]
fn every_attempted_step_is_one_activation_closed_by_accept_or_reject() {
    // An RC plant under a diode-less source step: the error test rejects attempts, so both closes
    // occur. The device is a conductance in parallel with the capacitor.
    let deck = Deck::new()
        .builtin(1, "vs", "voltage-source", "v = \"1V\"")
        .builtin(2, "r", "resistor", "r = \"1kohm\"")
        .builtin(3, "c", "capacitor", "c = \"1uF\"\nic = \"0V\"")
        .builtin(4, "vm", "voltmeter", "period = \"100us\"")
        .profile("ltol = 1e-7")
        .guest(10, "d", &[("a", "circuit/pin"), ("k", "circuit/pin")])
        .net("in", &["vs.p", "r.a"])
        .net("cap", &["r.b", "c.a", "d.a", "vm.p"])
        .ground("gnd", &["vs.n", "c.b", "d.k", "vm.n"]);
    let mut plant = deck.plant();
    plant.set_guest(10);
    let mut model = Native::new(Model::Conductance(1e-9));
    let mut t = started(&plant, &mut model);
    model.calls.clear();
    let out = t.advance_with(2_000_000, &mut model);
    assert!(out.stuck.is_none() && !out.ended);
    assert!(
        out.report.rejected > 0,
        "the error test rejects attempts: {:?}",
        out.report
    );
    // Walk the calls: each attempt is a `Begin`, evaluations, and exactly one close.
    let (mut open, mut accepts, mut rejects, mut begins) = (false, 0, 0, 0);
    for c in &model.calls {
        match c {
            Call::Begin(..) => {
                assert!(!open, "an activation is open at a begin");
                open = true;
                begins += 1;
            }
            Call::Evaluate(_) => assert!(open, "an evaluation outside an activation"),
            Call::Accept(_) => {
                assert!(open, "an accept with nothing open");
                open = false;
                accepts += 1;
            }
            Call::Reject => {
                assert!(open, "a reject with nothing open");
                open = false;
                rejects += 1;
            }
        }
    }
    assert!(!open);
    // The step record: one accept per accepted step, one reject per rejected attempt, one begin
    // per attempt.
    assert_eq!(accepts, out.report.steps as usize);
    assert_eq!(rejects, out.report.rejected as usize);
    assert_eq!(begins, out.report.attempts as usize);
    // The step length in seconds is `n × tick`: the first begin of the advance carries the tick it
    // starts at and the length of its first attempt.
    let Call::Begin(start, h) = model.calls[0] else {
        panic!()
    };
    assert_eq!(start, 0);
    let ticks = h / plant.tick;
    assert!(
        h > 0.0 && crate::math::fmin(ticks - ticks.floor(), ticks.ceil() - ticks) < 1e-6,
        "{h} is not a whole number of ticks"
    );
}

#[test]
fn a_device_lost_in_an_evaluation_is_absent_from_then_on_and_the_step_is_made_again() {
    let p = conductance();
    // The third evaluation of the run: the first iterations of the initial solve took two.
    let lost = Lost::Faulted {
        kind: FaultKind::Contract,
        parameter: 12_020,
        invalid: Some("a value of the result is not finite"),
    };
    let mut model = Native::new(Model::Conductance(0.001)).losing(On::Evaluate, 3, lost.clone());
    let mut t = started(&p, &mut model);
    let out = t.advance_with(100_000, &mut model);
    assert!(out.stuck.is_none() && !out.ended);
    assert_eq!(out.report.lost, Some(lost));
    // The loss is no rejection (rule 16).
    assert_eq!(out.report.rejected, 0);
    // The plant has no device from then on: the node is the source's voltage, and nothing of the
    // model was called again.
    assert!(t.plant().guest.is_none());
    near(t.voltage("mid").unwrap(), 5.0);
    let after = model.calls.len();
    t.advance_with(200_000, &mut model);
    assert_eq!(model.calls.len(), after, "an absent device is not called");
    // The call that lost it was the last the model saw in that advance.
    assert!(matches!(model.calls.last(), Some(Call::Evaluate(_))));
}

#[test]
fn a_device_lost_in_begin_in_accept_or_in_reject_is_the_same_loss() {
    for (on, nth) in [(On::Begin, 2), (On::Accept, 2)] {
        let p = conductance();
        let lost = Lost::Faulted {
            kind: FaultKind::Trap,
            parameter: 0,
            invalid: None,
        };
        let mut model = Native::new(Model::Conductance(0.001)).losing(on, nth, lost.clone());
        let mut t = started(&p, &mut model);
        let out = t.advance_with(100_000, &mut model);
        assert!(out.stuck.is_none() && !out.ended);
        assert_eq!(out.report.lost, Some(lost));
        near(t.voltage("mid").unwrap(), 5.0);
        assert_eq!(t.now(), 100_000, "the advance reaches its tick");
    }
    // A reject lost: an RC plant whose first rejection loses it.
    let deck = Deck::new()
        .builtin(1, "vs", "voltage-source", "v = \"1V\"")
        .builtin(2, "r", "resistor", "r = \"1kohm\"")
        .builtin(3, "c", "capacitor", "c = \"1uF\"\nic = \"0V\"")
        .builtin(4, "vm", "voltmeter", "period = \"100us\"")
        .profile("ltol = 1e-7")
        .guest(10, "d", &[("a", "circuit/pin"), ("k", "circuit/pin")])
        .net("in", &["vs.p", "r.a"])
        .net("cap", &["r.b", "c.a", "d.a", "vm.p"])
        .ground("gnd", &["vs.n", "c.b", "d.k", "vm.n"]);
    let mut plant = deck.plant();
    plant.set_guest(10);
    let mut model = Native::new(Model::Conductance(1e-9)).losing(On::Reject, 1, Lost::Gone);
    let mut t = started(&plant, &mut model);
    let out = t.advance_with(2_000_000, &mut model);
    assert_eq!(out.report.lost, Some(Lost::Gone));
    assert!(out.stuck.is_none() && t.plant().guest.is_none());
}

#[test]
fn a_run_that_ends_in_a_call_abandons_the_advance_at_the_last_accepted_step() {
    let p = conductance();
    let mut model = Native::new(Model::Conductance(0.001)).losing(On::Evaluate, 6, Lost::Ended);
    let mut t = started(&p, &mut model);
    let before = t.now();
    let out = t.advance_with(100_000, &mut model);
    assert!(out.ended && out.stuck.is_none());
    assert!(t.now() >= before && t.now() < 100_000, "{}", t.now());
    // The state is that of the last accepted step: the device was lost before its first of them or
    // after some, and the node is the plant's state there.
    assert!(
        t.plant().guest.is_some(),
        "an ended run does not drop the device from the plant"
    );
}

#[test]
fn a_device_lost_in_the_initial_solve_is_solved_again_without_it() {
    let p = conductance();
    let lost = Lost::Faulted {
        kind: FaultKind::Trap,
        parameter: 0,
        invalid: None,
    };
    let mut model = Native::new(Model::Conductance(0.001)).losing(On::Evaluate, 1, lost.clone());
    let solved = initial_solve_with(&p, &mut model).unwrap();
    assert_eq!(solved.lost, Some(lost));
    // The operating point is the plant's without the device.
    let mid = p.nets.iter().position(|n| n.name == "mid").unwrap();
    near(solved.solution.x[mid], 5.0);
    assert_eq!(
        model.calls.len(),
        2,
        "begin and the evaluation that lost it"
    );
}

#[test]
fn a_run_that_ends_in_the_initial_solve_gives_no_point() {
    let p = conductance();
    let mut model = Native::new(Model::Conductance(0.001)).losing(On::Begin, 1, Lost::Ended);
    assert_eq!(
        initial_solve_with(&p, &mut model),
        Err(crate::dc::Stop::Ended)
    );
}

#[test]
fn a_device_that_limits_takes_one_more_iteration_than_one_that_does_not() {
    // A conductance that reports `limited` at the second evaluation of a solve, with the update
    // and the residual within their criteria there: the solve does not converge in that iteration
    // (rule 8), so it takes one iteration more.
    struct Limiting(Native);
    impl Device for Limiting {
        fn present(&self) -> bool {
            true
        }
        fn begin(&mut self, s: u64, h: f64) -> Result<(), Lost> {
            self.0.begin(s, h)
        }
        fn evaluate(&mut self, trial: &[f64]) -> Result<Stamp, Lost> {
            // The second evaluation since the last `begin`: the update and the residual are within
            // their criteria there, and only the flag keeps the iteration from converging.
            let since = self
                .0
                .calls
                .iter()
                .rev()
                .take_while(|c| !matches!(c, Call::Begin(..)))
                .filter(|c| matches!(c, Call::Evaluate(_)))
                .count();
            let mut s = self.0.evaluate(trial)?;
            s.limited = since == 1;
            Ok(s)
        }
        fn accept(&mut self, x: &[f64]) -> Result<(), Lost> {
            self.0.accept(x)
        }
        fn reject(&mut self) -> Result<(), Lost> {
            self.0.reject()
        }
    }
    let p = conductance();
    let plain = initial_solve_with(&p, &mut Native::new(Model::Conductance(0.001))).unwrap();
    let limiting =
        initial_solve_with(&p, &mut Limiting(Native::new(Model::Conductance(0.001)))).unwrap();
    assert_eq!(
        limiting.solution.iterations,
        plain.solution.iterations + 1,
        "the iteration that limited does not converge"
    );
    assert_eq!(bits(&limiting.solution.x), bits(&plain.solution.x));
}
