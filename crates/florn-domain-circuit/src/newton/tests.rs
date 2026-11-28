//! RFC 013 rule 8 at the plant: the criteria, the failure of a solve, the limiter inside the
//! device, and the bound on the iterations.

use super::*;
use crate::assemble::memory;
use crate::tests::{Deck, divider};

/// The iteration over a system that cannot lose a device.
fn iterate(
    plant: &Plant,
    x0: Vec<f64>,
    mut system: impl FnMut(&[f64]) -> System,
) -> Result<Converged, Failure> {
    iterate_with(plant, x0, |x| Ok(system(x)))
}

fn diode_deck(extra_profile: &str) -> Deck {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "d1", "diode", "")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "d1.a"])
        .ground("gnd", &["v1.n", "d1.k"])
        .observed()
        .profile(extra_profile)
}

fn run(plant: &Plant) -> Result<Converged, Failure> {
    let mut mem = memory(plant);
    solve(
        plant,
        vec![0.0; plant.size()],
        plant.profile.gmin,
        1.0,
        &mut mem,
    )
}

#[test]
fn a_linear_circuit_converges_in_two_iterations() {
    // The first step lands on the solution, the second sees an update below the tolerances.
    let plant = divider().plant();
    let c = run(&plant).unwrap();
    assert_eq!(c.iterations, 2);
    assert!((c.x[1] - 7.5).abs() < 1e-7);
}

#[test]
fn the_diode_circuit_converges_with_the_limiter_inside_the_device() {
    let plant = diode_deck("").plant();
    let c = run(&plant).unwrap();
    // The scalar model's 8, plus what the source's own branch adds at most a step or two.
    assert!((8..=10).contains(&c.iterations), "{}", c.iterations);
    assert!((c.x[1] - 0.692_543_633_176_399).abs() < 1e-9);
}

#[test]
fn more_iterations_than_the_bound_is_a_convergence_failure() {
    let plant = diode_deck("newton.max-iterations = 5").plant();
    let e = run(&plant).unwrap_err();
    assert!(!e.singular);
    assert_eq!(e.iterations, 5);
}

#[test]
fn a_singular_matrix_names_the_unknown_of_the_first_zero_pivot() {
    // Two voltage sources in parallel.
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "v2", "voltage-source", "v = \"5V\"")
        .builtin(3, "vm", "voltmeter", "period = \"1ms\"")
        .net("top", &["v1.p", "v2.p", "vm.p"])
        .ground("gnd", &["v1.n", "v2.n", "vm.n"]);
    let plant = deck.plant();
    let e = run(&plant).unwrap_err();
    assert!(e.singular);
    assert!(plant.unknowns[e.unknown].kind == crate::plant::UnknownKind::Current);
}

#[test]
fn the_result_is_the_same_bits_every_time() {
    let plant = diode_deck("").plant();
    let a = run(&plant).unwrap();
    let b = run(&plant).unwrap();
    assert_eq!(
        a.x.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        b.x.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    );
    assert_eq!(a.iterations, b.iterations);
}

#[test]
fn the_tolerances_of_the_profile_decide_when_it_stops() {
    let loose =
        diode_deck("newton.reltol = 0.1\nnewton.vntol = \"1mV\"\nnewton.abstol = \"1uA\"").plant();
    let tight = diode_deck("").plant();
    let (l, t) = (run(&loose).unwrap(), run(&tight).unwrap());
    assert!(
        l.iterations < t.iterations,
        "{} vs {}",
        l.iterations,
        t.iterations
    );
}

/// A system with the solution `target` that is linear in the trial: `f = x − target`, `J = I`.
fn linear<'a>(
    n: usize,
    target: &'a [f64],
    mut limited: impl FnMut(u32) -> bool + 'a,
) -> impl FnMut(&[f64]) -> crate::assemble::System + 'a {
    let mut call = 0;
    move |x: &[f64]| {
        call += 1;
        let mut j = crate::lu::Matrix::new(n);
        for i in 0..n {
            j.set(i, i, 1.0);
        }
        crate::assemble::System {
            j,
            f: x.iter().zip(target).map(|(x, t)| x - t).collect(),
            scale: vec![0.0; n],
            limited: limited(call),
        }
    }
}

#[test]
fn a_device_that_limited_in_an_iteration_does_not_converge_in_it() {
    // RFC 013 rule 8: converged only when no device reported limiting in the last iteration. The
    // trial is already the solution, so the update and residual criteria hold at once; the device
    // limited in the first iteration, so the solve takes a second, in which it did not.
    let plant = divider().plant();
    let n = plant.size();
    let target: Vec<f64> = (0..n).map(|i| 0.5 + i as f64).collect();
    let c = iterate(&plant, target.clone(), linear(n, &target, |call| call == 1)).unwrap();
    assert_eq!(c.iterations, 2);
    // The same without the report converges at once.
    let c = iterate(&plant, target.clone(), linear(n, &target, |_| false)).unwrap();
    assert_eq!(c.iterations, 1);
    // A device that limits in every iteration never converges: the bound is the failure.
    let failed = iterate(&plant, target.clone(), linear(n, &target, |_| true));
    assert_eq!(failed.unwrap_err().iterations, plant.profile.max_iterations);
}
