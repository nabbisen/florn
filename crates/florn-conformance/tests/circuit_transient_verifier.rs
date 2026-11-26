//! The transient solver against an independent model of RFC 013 (checklist B-45). The checks are
//! the verifier's of PR-45, written from the RFC before the unit was read; the expectations come
//! from a model of rules 6, 7 and 10 to 14 in `circuit_transient_verifier/refmodel.py`, which
//! `gen_cases.py` runs over 42 scripts into `cases.rs` (4,600 accepted steps and their samples).
//! The two places where the review ruled on the verifier's findings (a retried step is never
//! longer than the attempt it replaces; no Gear 2 step with omega above 2) are marked `Ruling`.
use florn_conformance::circuit::Deck;
use florn_domain_circuit::integrate::Formula;
use florn_domain_circuit::transient::{StepRecord, Why};
use florn_domain_circuit::{CircuitDomain, Limits};
use florn_host::compound::NoCalls;
use florn_host::domain::Domain;

include!("circuit_transient_verifier/cases.rs");

fn deck(c: &Case) -> Deck {
    let mut d = Deck::new("verifier", "1s")
        .profile(c.profile)
        .part(1, "vs", "voltage-source", &[], "v = \"1V\"")
        .part(99, "vm", "voltmeter", &[], "period = \"100us\"");
    let mut in_pins = vec!["vs.p".to_string()];
    let mut gnd = vec!["vs.n".to_string(), "vm.n".to_string()];
    if c.br.is_empty() {
        d = d
            .part(10, "r10", "resistor", &[], "r = \"1kohm\"")
            .part(11, "r11", "resistor", &[], "r = \"1kohm\"")
            .net("mid", &["r10.b", "r11.a", "vm.p"]);
        in_pins.push("r10.a".into());
        gnd.push("r11.b".into());
    }
    for (i, b) in c.br.iter().enumerate() {
        let (r, cc) = (format!("r{}", 10 + i), format!("c{}", 20 + i));
        d = d
            .part(
                10 + i as u32,
                &r,
                "resistor",
                &[],
                &format!("r = \"{}\"", b.r),
            )
            .part(
                20 + i as u32,
                &cc,
                "capacitor",
                &[],
                &format!("c = \"{}\"\nic = \"0V\"", b.c),
            );
        in_pins.push(format!("{r}.a"));
        gnd.push(format!("{cc}.b"));
        let mut pins = vec![format!("{r}.b"), format!("{cc}.a")];
        if i == 0 {
            pins.push("vm.p".into());
        }
        let refs: Vec<&str> = pins.iter().map(|s| s.as_str()).collect();
        d = d.net(&format!("cap{i}"), &refs);
    }
    let ip: Vec<&str> = in_pins.iter().map(|s| s.as_str()).collect();
    let gp: Vec<&str> = gnd.iter().map(|s| s.as_str()).collect();
    d.net("in", &ip).ground("gnd", &gp)
}

fn started(deck: &Deck) -> CircuitDomain {
    let mut d = CircuitDomain::new(Limits::native());
    d.compose(&deck.manifest(), 1);
    d.start_calling(&mut NoCalls, &[])
        .expect("the plant starts");
    d
}

struct Ran {
    steps: Vec<StepRecord>,
    cuts: Vec<usize>,
    volts: Vec<Vec<f64>>,
    minimum: usize,
    stuck: Option<(usize, u64, Why, Option<u32>)>,
    stuck_rejected: u32,
}

/// The harness plays PR-46's handler: limits at breakpoints, `set` and `cut` there.
fn run(c: &Case, inspect: bool) -> Ran {
    let mut d = started(&deck(c));
    let t = d.transient_mut().unwrap();
    t.trace_steps();
    let mut pending: Vec<(u64, f64)> = Vec::new();
    let (mut cuts, mut volts, mut minimum, mut stuck) = (vec![0usize], vec![], 0usize, None);
    let mut stuck_rejected = 0u32;
    let mut all: Vec<StepRecord> = Vec::new();
    for (idx, op) in c.ops.iter().enumerate() {
        match op {
            Op::Bp(tk, v) => pending.push((*tk, *v)),
            Op::Adv(target) => {
                let mut fail = None;
                // breakpoints not after `solved` act first; later ones before the target split it
                loop {
                    pending.sort_by_key(|(tk, _)| *tk);
                    let next = pending
                        .iter()
                        .position(|(tk, _)| *tk <= t.now() || *tk < *target);
                    let Some(k) = next else { break };
                    let (tk, v) = pending.remove(k);
                    if tk > t.now() {
                        let o = t.advance(tk);
                        minimum += o.report.minimum.len();
                        if let Some(s) = o.stuck {
                            stuck_rejected = o.report.rejected;
                            fail = Some(s);
                            break;
                        }
                    }
                    assert!(t.set(1, v));
                    t.cut();
                    all.extend(t.take_trace());
                    cuts.push(all.len());
                }
                if fail.is_none() {
                    let o = t.advance(*target);
                    minimum += o.report.minimum.len();
                    if o.stuck.is_some() {
                        stuck_rejected = o.report.rejected;
                    }
                    fail = o.stuck;
                }
                if inspect {
                    let _ = (t.x().to_vec(), t.proposal(), t.since_cut(), t.now());
                }
                if let Some(s) = fail {
                    let owner = s.owner.map(|i| t.plant().devices[i].actor);
                    stuck = Some((idx, s.tick, s.why, owner));
                    break;
                }
                let vs: Vec<f64> = (0..c.br.len())
                    .map(|i| t.voltage(&format!("cap{i}")).unwrap())
                    .collect();
                volts.push(vs);
            }
        }
    }
    all.extend(t.take_trace());
    Ran {
        steps: all,
        cuts,
        volts,
        minimum,
        stuck,
        stuck_rejected,
    }
}

fn letter(f: Formula) -> char {
    match f {
        Formula::BackwardEuler => 'B',
        Formula::Trapezoidal => 'T',
        Formula::Gear2 => 'G',
    }
}

fn profile_of(c: &Case) -> (u64, u64, u64, bool) {
    // step, min-step, max-step in ticks, fixed: read back from the profile text (defaults 1000, 1, 100*step)
    let get = |k: &str| {
        c.profile.lines().find_map(|l| {
            let l = l.trim();
            l.strip_prefix(&format!("{k} = \""))
                .and_then(|r| r.strip_suffix("ns\""))
                .and_then(|n| n.parse::<u64>().ok())
        })
    };
    let step = get("step").unwrap_or(1000);
    (
        step,
        get("min-step").unwrap_or(1),
        get("max-step").unwrap_or(100 * step),
        c.profile.contains("fixed"),
    )
}

#[test]
fn v3_the_step_sequence_and_the_samples_equal_the_independent_model() {
    let mut findings: Vec<String> = Vec::new();
    let mut bit_equal = 0usize;
    let mut total = 0usize;
    let mut worst_dev = 0.0f64;
    for c in cases() {
        let r = run(&c, false);
        // steps
        let n = r.steps.len().min(c.steps.len());
        let mut bad = r.steps.len() != c.steps.len();
        for i in 0..n {
            let (a, e) = (&r.steps[i], &c.steps[i]);
            if a.start != e.start
                || a.ticks != e.ticks
                || letter(a.formula) != e.f
                || a.attempts != e.tries
            {
                findings.push(format!(
                    "{}: step {i}: code start={} ticks={} {} attempts={} / model start={} ticks={} {} tries={}",
                    c.name, a.start, a.ticks, letter(a.formula), a.attempts, e.start, e.ticks, e.f, e.tries
                ));
                bad = true;
                break;
            }
        }
        if r.steps.len() != c.steps.len() && !bad {
            bad = true;
        }
        if bad && r.steps.len() != c.steps.len() {
            findings.push(format!(
                "{}: {} steps in the code, {} in the model",
                c.name,
                r.steps.len(),
                c.steps.len()
            ));
        }
        // voltages
        for (k, (a, e)) in r.volts.iter().zip(c.volts.iter()).enumerate() {
            for (x, y) in a.iter().zip(e.iter()) {
                total += 1;
                if x.to_bits() == y.to_bits() {
                    bit_equal += 1;
                }
                worst_dev = worst_dev.max((x - y).abs() / y.abs().max(1e-3));
                if (x - y).abs() > 1e-9 * y.abs().max(1e-3) {
                    findings.push(format!("{}: advance {k}: code {x:e} model {y:e}", c.name));
                    break;
                }
            }
        }
        if r.volts.len() != c.volts.len() {
            findings.push(format!(
                "{}: {} advances completed, model {}",
                c.name,
                r.volts.len(),
                c.volts.len()
            ));
        }
        if r.minimum != c.minimum {
            findings.push(format!(
                "{}: step-minimum count code {} model {}",
                c.name, r.minimum, c.minimum
            ));
        }
        match (&r.stuck, c.stuck) {
            (None, None) => {}
            (Some((i, tk, why, owner)), Some((mi, mtk, mb))) => {
                let max_rej: u32 = c
                    .profile
                    .lines()
                    .find_map(|l| l.strip_prefix("max-rejections = "))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60);
                if r.stuck_rejected != max_rej + 1 {
                    findings.push(format!("{}: {} rejections before non-convergence, the rule says more than max-rejections ({max_rej}): {}", c.name, r.stuck_rejected, max_rej + 1));
                }
                if *i != mi
                    || *tk != mtk
                    || *why != Why::TooManyRejections
                    || *owner != Some(20 + mb as u32)
                {
                    findings.push(format!("{}: stuck code ({i}, {tk}, {why:?}, {owner:?}) model ({mi}, {mtk}, branch {mb})", c.name));
                }
            }
            (a, b) => findings.push(format!("{}: stuck code {a:?} model {b:?}", c.name)),
        }
    }
    eprintln!("largest relative deviation of a sample from the model: {worst_dev:e}");
    eprintln!("sample bits equal to the model's literal-formula bits: {bit_equal} of {total}");
    assert!(
        findings.is_empty(),
        "{} finding(s):\n{}",
        findings.len(),
        findings.join("\n")
    );
}

#[test]
fn v4_order_test_ratios_between_3_8_and_4_2_and_equal_to_the_model() {
    for m in ["trapezoidal", "gear2"] {
        let errs: Vec<f64> = [8, 16, 32, 64]
            .iter()
            .map(|d| {
                let c = cases()
                    .into_iter()
                    .find(|c| c.name == format!("order-{m}-{d}"))
                    .unwrap();
                let r = run(&c, false);
                (r.volts.last().unwrap()[0] - (1.0 - (-2.0f64).exp())).abs()
            })
            .collect();
        for w in errs.windows(2) {
            let q = w[0] / w[1];
            assert!((3.8..=4.2).contains(&q), "{m}: {q} from {errs:?}");
        }
    }
}

#[test]
fn v1_rc_step_samples_are_within_four_ltol_v_of_the_analytic_solution() {
    for c in cases().into_iter().filter(|c| c.name.starts_with("rcstep")) {
        let r = run(&c, false);
        let mut worst = 0.0f64;
        for (k, v) in r.volts.iter().enumerate() {
            let t = (k as f64 + 1.0) * 100_000.0 * 1e-9;
            let want = if t <= 2e-3 {
                1.0 - (-t / 1e-3).exp()
            } else {
                (1.0 - (-2.0f64).exp()) * (-(t - 2e-3) / 1e-3).exp()
            };
            worst = worst.max((v[0] - want).abs());
        }
        eprintln!("{}: worst {:.4} ltol*V", c.name, worst / 1e-3);
        assert!(worst <= 4e-3, "{}: {worst}", c.name);
    }
}

#[test]
fn v6_after_every_cut_the_first_three_steps_are_backward_euler() {
    for c in cases()
        .into_iter()
        .filter(|c| c.name.starts_with("restart") || c.name.starts_with("rcstep"))
    {
        let r = run(&c, false);
        // a cut precedes steps[cut_index..]; check the formula run after each cut
        for &at in &r.cuts {
            let seg: String = r.steps[at.min(r.steps.len())..]
                .iter()
                .take(4)
                .map(|s| letter(s.formula))
                .collect();
            assert!(
                seg.starts_with("BBB"),
                "{}: after the cut at record {at}: {seg}",
                c.name
            );
        }
    }
}

#[test]
fn v8_a_fixed_step_is_min_step_d_never_lengthened() {
    for c in cases().into_iter().filter(|c| profile_of(c).3) {
        let (step, _, _, _) = profile_of(&c);
        let r = run(&c, false);
        for s in &r.steps {
            let to_limit = s.start + s.ticks == s.limit;
            assert!(
                s.ticks <= step && (s.ticks == step || to_limit),
                "{}: fixed step {} (step {step}) at {}",
                c.name,
                s.ticks,
                s.start
            );
        }
    }
}

#[test]
fn v9_no_step_is_below_min_step_except_the_whole_distance_or_above_max_step() {
    for c in cases().into_iter().filter(|c| !profile_of(c).3) {
        let (_, min, max, _) = profile_of(&c);
        let r = run(&c, false);
        for s in &r.steps {
            let to_limit = s.start + s.ticks == s.limit;
            assert!(
                s.ticks <= max,
                "{}: step {} above max-step {max}",
                c.name,
                s.ticks
            );
            assert!(
                s.ticks >= min || to_limit,
                "{}: step {} below min-step {min} at {} and not to a limit",
                c.name,
                s.ticks,
                s.start
            );
        }
    }
}

/// Ruling (RFC 013 rule 6, amended 2026-11-26): no Gear 2 step is taken with omega above 2, under
/// either control. After a step that was the whole distance to a limit and shorter than the next,
/// the next step is backward Euler, so no two consecutive Gear 2 steps have a ratio above 2.
#[test]
fn v13_no_gear2_step_has_omega_above_2_under_either_control() {
    let mut worst: (f64, String) = (0.0, String::new());
    let mut over = Vec::new();
    for c in cases() {
        let r = run(&c, false);
        for w in r.steps.windows(2) {
            if w[1].formula == Formula::Gear2 {
                let om = w[1].ticks as f64 / w[0].ticks as f64;
                if om > worst.0 {
                    worst = (
                        om,
                        format!(
                            "{} at tick {}: {} after {}",
                            c.name, w[1].start, w[1].ticks, w[0].ticks
                        ),
                    );
                }
                if om > 2.0 {
                    over.push(format!(
                        "{}: omega {om} ({} after {}) at {}",
                        c.name, w[1].ticks, w[0].ticks, w[1].start
                    ));
                }
            }
        }
    }
    eprintln!("largest omega observed: {:.3} ({})", worst.0, worst.1);
    assert!(
        over.is_empty(),
        "{} Gear steps with omega > 2, first: {}",
        over.len(),
        over.first().unwrap()
    );
}

/// Ruling (RFC 013 rule 12, amended 2026-11-26): a retried step is never longer than the attempt it
/// replaces. The scripts that livelocked under the first text of rules 12 and 13 (`livelock-*`, a
/// step of 10,000, `min-step` 3,000, a limit 5,000 ticks away; and `atmin-gear2`, which ended in
/// non-convergence at tick 148,128) now run to the end: no step is stuck, the error test that fails
/// at the minimum is accepted and notified, as the model says.
#[test]
fn ruling_a_retry_is_never_longer_and_the_livelock_cases_run_to_the_end() {
    let mut seen = 0;
    for c in cases()
        .into_iter()
        .filter(|c| c.name.starts_with("livelock") || c.name == "atmin-gear2")
    {
        seen += 1;
        let r = run(&c, false);
        assert!(
            r.stuck.is_none(),
            "{}: the run ends normally, not in non-convergence: {:?}",
            c.name,
            r.stuck
        );
        assert!(
            r.minimum > 0,
            "{}: the failed error test at the minimum is notified",
            c.name
        );
        assert_eq!(
            r.minimum, c.minimum,
            "{}: as many as the model says",
            c.name
        );
    }
    assert_eq!(seen, 3);
}

#[test]
fn v21_deterministic_and_unchanged_by_inspection() {
    for c in cases()
        .into_iter()
        .filter(|c| c.name.starts_with("restart") || c.name.starts_with("tight"))
    {
        let a = run(&c, false);
        let b = run(&c, true);
        let bits = |r: &Ran| {
            r.volts
                .iter()
                .flatten()
                .map(|x| x.to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&a), bits(&b), "{}", c.name);
        assert_eq!(a.steps.len(), b.steps.len(), "{}", c.name);
        for (x, y) in a.steps.iter().zip(&b.steps) {
            assert_eq!(
                (x.start, x.ticks, x.attempts),
                (y.start, y.ticks, y.attempts),
                "{}",
                c.name
            );
        }
    }
}

fn diode_deck(profile: &str) -> Deck {
    Deck::new("verifier-diode", "1s")
        .profile(profile)
        .part(1, "vs", "voltage-source", &[], "v = \"0V\"")
        .part(10, "r10", "resistor", &[], "r = \"1kohm\"")
        .part(11, "d11", "diode", &[], "")
        .part(99, "vm", "voltmeter", &[], "period = \"100us\"")
        .net("in", &["vs.p", "r10.a"])
        .net("an", &["r10.b", "d11.a", "vm.p"])
        .ground("gnd", &["vs.n", "d11.k", "vm.n"])
}

/// V19: the diode stepped from 0 V to 5 V through 1 kohm: the scalar model needs 8 iterations.
#[test]
fn v19_the_diode_step_takes_the_models_iterations() {
    let mut d = started(&diode_deck("method = \"trapezoidal\""));
    let t = d.transient_mut().unwrap();
    assert!(t.set(1, 5.0));
    t.cut();
    let o = t.advance(1000);
    assert!(o.stuck.is_none(), "{o:?}");
    eprintln!(
        "diode 0->5 V: steps {} attempts {} iterations {} (model: 8 for the scalar plant)",
        o.report.steps, o.report.attempts, o.report.iterations
    );
    assert_eq!((o.report.steps, o.report.attempts), (1, 1));
    assert!(
        (7..=9).contains(&o.report.iterations),
        "iterations {}",
        o.report.iterations
    );
    assert!((t.voltage("an").unwrap() - 0.6925436331722674).abs() < 1e-9);
}

/// V17: a convergence failure is retried at floor(n/4) and at min-step it is non-convergence.
#[test]
fn v17_convergence_failure_retries_at_a_quarter_and_at_min_step_is_non_convergence() {
    let mut d = started(&diode_deck(
        "method = \"trapezoidal\"\nnewton.max-iterations = 3",
    ));
    let t = d.transient_mut().unwrap();
    t.set(1, 5.0);
    t.cut();
    let o = t.advance(1000);
    let s = o
        .stuck
        .expect("Newton cannot converge in 3 iterations at any step of this plant");
    eprintln!(
        "convergence failure: {:?} rejected {} first {:?}",
        s, o.report.rejected, o.report.first_cause
    );
    assert_eq!(s.why, Why::AtMinimumStep);
    assert_eq!(s.tick, 0);
    assert_eq!(
        o.report.rejected, 5,
        "1000, 250, 62, 15, 3 fail; the attempt at 1 tick is the non-convergence"
    );
    assert_eq!(o.report.attempts, 6);
    assert_eq!(
        o.report.first_cause,
        Some(florn_domain_circuit::transient::Cause::Convergence)
    );
    assert_eq!(t.now(), 0, "the state is that of the last accepted step");
}

#[test]
fn v17b_under_fixed_a_convergence_failure_is_non_convergence_at_once() {
    let mut d = started(&diode_deck(
        "method = \"trapezoidal\"\ncontrol = \"fixed\"\nnewton.max-iterations = 3",
    ));
    let t = d.transient_mut().unwrap();
    t.set(1, 5.0);
    t.cut();
    let o = t.advance(1000);
    let s = o.stuck.expect("stuck");
    assert_eq!(s.why, Why::Fixed);
    assert_eq!(o.report.attempts, 1);
}

/// V5: a prescribed pattern of step ratios from 0.5 to 2, halved: the error at 1.728 ms falls by
/// 3.8 to 4.2 at every halving, under both methods, and equals the model's.
#[test]
fn v5_prescribed_step_pattern_halving_gives_the_order() {
    for m in ["trapezoidal", "gear2"] {
        let errs: Vec<f64> = [16000, 8000, 4000, 2000, 1000]
            .iter()
            .map(|q| {
                let c = cases()
                    .into_iter()
                    .find(|c| c.name == format!("pattern-{m}-{q}"))
                    .unwrap();
                let r = run(&c, false);
                assert!(r.stuck.is_none());
                (r.volts.last().unwrap()[0] - (1.0 - (-1.728f64).exp())).abs()
            })
            .collect();
        let ratios: Vec<f64> = errs.windows(2).map(|w| w[0] / w[1]).collect();
        eprintln!("pattern {m}: errors {errs:?} ratios {ratios:?}");
        for q in &ratios {
            assert!((3.8..=4.2).contains(q), "{m}: {ratios:?}");
        }
    }
}

/// RFC 013 rule 6: "the coefficients are exact on quadratics at omega = 1/2, 1, 3/2 and 2".
#[test]
fn v5b_the_gear2_and_trapezoidal_coefficients_are_exact_on_quadratics() {
    use florn_domain_circuit::integrate::{alpha, history};
    let x = |t: f64| 3.0 * t * t - 2.0 * t + 5.0;
    let dx = |t: f64| 6.0 * t - 2.0;
    for w in [0.5, 1.0, 1.5, 2.0] {
        let (hn, h) = (1.0e-3, w * 1.0e-3);
        // t_{n-1} = -hn, t_n = 0, t_{n+1} = h
        let g = alpha(Formula::Gear2, h, hn) * x(h)
            - history(Formula::Gear2, h, hn, x(0.0), x(-hn), dx(0.0));
        assert!(
            (g - dx(h)).abs() <= 1e-9 * dx(h).abs().max(1.0),
            "gear2 omega {w}: {g} against {}",
            dx(h)
        );
        let tr = alpha(Formula::Trapezoidal, h, hn) * x(h)
            - history(Formula::Trapezoidal, h, hn, x(0.0), x(-hn), dx(0.0));
        assert!(
            (tr - dx(h)).abs() <= 1e-9 * dx(h).abs().max(1.0),
            "trapezoidal omega {w}"
        );
        // backward Euler is first order: it is NOT exact (the check can tell)
        let be = alpha(Formula::BackwardEuler, h, hn) * x(h)
            - history(Formula::BackwardEuler, h, hn, x(0.0), x(-hn), dx(0.0));
        assert!(
            (be - dx(h)).abs() > 1e-3,
            "backward Euler should not be exact on a quadratic"
        );
    }
}
