//! PR-45 (checklist B-45): the transient solver of RFC 013 rules 6, 7 and 10 to 14, through the
//! domain as the Host composes and starts it. `rc-step`'s goldens (rule 21) and the order of the
//! methods (section 11), written from the RFC's figures. The advance on demand, with its events
//! and breakpoints, is PR-46's: here the harness plays the part of its handler, taking the plant
//! to each sample tick, applying the scheduled change at 2 ms and cutting the history there.
use florn_conformance::circuit::{Deck, rc_step};
use florn_domain_circuit::transient::Transient;
use florn_domain_circuit::{CircuitDomain, Limits};
use florn_host::compound::NoCalls;
use florn_host::domain::Domain;

/// The domain started on `deck`: composed, the start hook run, the solver at tick 0.
fn started(deck: &Deck) -> CircuitDomain {
    let mut d = CircuitDomain::new(Limits::native());
    d.compose(&deck.manifest(), 1);
    d.start_calling(&mut NoCalls, &[])
        .expect("the plant starts");
    d
}

/// The analytic voltage across the capacitor (RC = 1 ms), the source stepped from 1 V to 0 V at
/// `fall` seconds.
fn analytic(t: f64, fall: f64) -> f64 {
    if t <= fall {
        1.0 - (-t / 1e-3).exp()
    } else {
        (1.0 - (-fall / 1e-3).exp()) * (-(t - fall) / 1e-3).exp()
    }
}

/// The voltmeter's samples every 100 us to 5 ms against the analytic solution; the largest error.
/// The source goes to 0 V at 2 ms, after the sample of that tick has been read.
fn worst_sample_error(t: &mut Transient) -> f64 {
    let mut worst = 0.0f64;
    for k in 1..=50u64 {
        let tick = k * 100_000;
        let out = t.advance(tick);
        assert!(out.stuck.is_none(), "{out:?}");
        let v = t.voltage("cap").expect("the capacitor's net");
        worst = worst.max((v - analytic(tick as f64 * 1e-9, 2e-3)).abs());
        if k == 20 {
            // A source step at T leaves the sample at T as it was (the limit from the left) and
            // acts from the step that starts at T (RFC 012 rule 9).
            assert!(t.set(1, 0.0));
            t.cut();
        }
    }
    worst
}

#[test]
fn rc_step_is_within_four_ltol_v_under_both_methods_at_the_default_profile_and_at_rc_over_ten() {
    let ltol_v = 1e-3 * 1.0;
    for method in ["trapezoidal", "gear2"] {
        for extra in ["", "max-step = \"100us\""] {
            let mut d = started(&rc_step().profile(&format!("method = \"{method}\"\n{extra}")));
            let e = worst_sample_error(d.transient_mut().unwrap());
            assert!(
                e <= 4.0 * ltol_v,
                "{method} {extra:?}: {e} against {}",
                4.0 * ltol_v
            );
        }
    }
}

#[test]
fn halving_a_fixed_step_divides_the_error_at_2ms_by_a_factor_between_3_8_and_4_2() {
    for method in ["trapezoidal", "gear2"] {
        let errors: Vec<f64> = [125_000u64, 62_500, 31_250, 15_625]
            .iter()
            .map(|n| {
                let profile = format!(
                    "method = \"{method}\"\ncontrol = \"fixed\"\nstep = \"{n}ns\"\nmin-step = \"{n}ns\"\nmax-step = \"{}ns\"",
                    2 * n
                );
                // No scheduled change and a voltmeter period of 1 ms: no step is shortened.
                let deck = rc_step().profile(&profile);
                let mut d = started(&deck);
                let t = d.transient_mut().unwrap();
                assert!(t.advance(2_000_000).stuck.is_none());
                (t.voltage("cap").unwrap() - analytic(2e-3, f64::INFINITY)).abs()
            })
            .collect();
        for pair in errors.windows(2) {
            let ratio = pair[0] / pair[1];
            assert!(
                (3.8..=4.2).contains(&ratio),
                "{method}: {ratio} from {errors:?}"
            );
        }
    }
}

#[test]
fn a_source_step_acts_from_the_step_that_starts_at_its_tick_and_the_sample_at_it_does_not_move() {
    let mut d = started(&rc_step());
    let t = d.transient_mut().unwrap();
    t.advance(2_000_000);
    let at = t.voltage("cap").unwrap();
    assert!(t.set(1, 0.0));
    // The state at `solved` is the limit from the left: setting the input changed nothing.
    assert_eq!(t.voltage("cap").unwrap(), at);
    t.cut();
    t.advance(2_100_000);
    assert!(
        t.voltage("cap").unwrap() < at,
        "the capacitor discharges from the step that starts at 2 ms"
    );
}

#[test]
fn the_solver_is_a_deterministic_function_of_the_run() {
    let run = || {
        let mut d = started(&rc_step().profile("method = \"gear2\""));
        let t = d.transient_mut().unwrap();
        worst_sample_error(t);
        t.x().iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}
