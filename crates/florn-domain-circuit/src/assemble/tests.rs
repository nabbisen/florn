//! RFC 013 rules 3 to 5: the equations at a trial point. The residual is zero at the solution and
//! the Jacobian is the derivative of the residual, checked by finite differences.

use super::*;

/// The system of the initial solve at `x`, the sources scaled by `lambda`.
fn assemble(plant: &Plant, x: &[f64], gmin: f64, lambda: f64, memory: &mut Memory) -> System {
    assemble_form(plant, x, gmin, Form::Dc { lambda }, memory)
}
use crate::dc::initial_solve;
use crate::tests::{Deck, divider};

#[test]
fn the_residual_is_zero_at_the_operating_point() {
    let plant = divider().plant();
    let s = initial_solve(&plant).unwrap();
    let mut mem = memory(&plant);
    let sys = assemble(&plant, &s.x, plant.profile.gmin, 1.0, &mut mem);
    for (i, f) in sys.f.iter().enumerate() {
        assert!(f.abs() < 1e-12, "row {i}: {f}");
    }
}

#[test]
fn the_jacobian_is_the_derivative_of_the_residual() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "l1", "inductor", "l = \"1mH\"")
        .builtin(4, "r2", "resistor", "r = \"2kohm\"")
        .net("top", &["v1.p", "r1.a"])
        .net("a", &["r1.b", "l1.a", "r2.a"])
        .ground("gnd", &["v1.n", "l1.b", "r2.b"])
        .observed();
    let plant = deck.plant();
    let n = plant.size();
    let x: Vec<f64> = (0..n).map(|i| 0.3 + 0.1 * i as f64).collect();
    let mut mem = memory(&plant);
    let base = assemble(&plant, &x, 1e-12, 1.0, &mut mem);
    for col in 0..n {
        let h = 1e-6;
        let mut xp = x.clone();
        xp[col] += h;
        let mut mem = memory(&plant);
        let up = assemble(&plant, &xp, 1e-12, 1.0, &mut mem);
        for row in 0..n {
            let numeric = (up.f[row] - base.f[row]) / h;
            let analytic = base.j.get(row, col);
            assert!(
                (numeric - analytic).abs() <= 1e-6 * (1.0 + analytic.abs()),
                "J[{row}][{col}] {numeric} vs {analytic}"
            );
        }
    }
}

#[test]
fn source_stepping_scales_the_independent_sources_only() {
    let plant = divider().plant();
    let mut mem = memory(&plant);
    let x = vec![0.0; plant.size()];
    let full = assemble(&plant, &x, 1e-12, 1.0, &mut mem);
    let tenth = assemble(&plant, &x, 1e-12, 0.1, &mut mem);
    // The source's branch row holds −v: −10 at full scale, −1 at a tenth.
    let branch = plant.nets.len();
    assert_eq!(full.f[branch], -10.0);
    assert_eq!(tenth.f[branch], -1.0);
}

#[test]
fn the_scale_of_a_net_is_the_sum_of_the_magnitudes_of_its_branch_currents() {
    let plant = divider().plant();
    let mut mem = memory(&plant);
    // top = 10, mid = 4: r1 carries 6 mA, r2 4/3 mA; at `mid` both flow.
    let x = vec![10.0, 4.0, 0.0];
    let sys = assemble(&plant, &x, 0.0, 1.0, &mut mem);
    let mid = 1;
    assert!((sys.scale[mid] - (6e-3 + 4.0 / 3000.0)).abs() < 1e-15);
}

#[test]
fn a_diode_remembers_the_junction_voltage_it_used() {
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "d1", "diode", "")
        .net("top", &["v1.p", "d1.a"])
        .ground("gnd", &["v1.n", "d1.k"])
        .observed();
    let plant = deck.plant();
    let mut mem = memory(&plant);
    let d = plant
        .devices
        .iter()
        .position(|d| d.kind == "diode")
        .unwrap();
    assert_eq!(mem[d], 0.0);
    let sys = assemble(&plant, &[5.0, 0.0], 1e-12, 1.0, &mut mem);
    assert!(sys.limited);
    assert!(mem[d] > 0.0 && mem[d] < 5.0, "{}", mem[d]);
}

#[test]
fn the_jacobian_of_a_step_is_the_derivative_of_its_residual_for_every_dynamic_element() {
    // A capacitor, an inductor and a motor, each with the equation `x' = α·x − history` of
    // rule 6; the finite differences of the residual are the Jacobian.
    let deck = Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"5V\"")
        .builtin(2, "l1", "inductor", "l = \"2mH\"")
        .builtin(3, "c1", "capacitor", "c = \"3uF\"")
        .builtin_opt(
            4,
            "m1",
            "dc-motor",
            "r = \"2ohm\"\nl = \"3mH\"\nkt = \"0.1Nm/A\"\nke = \"0.2Vs/rad\"\nj = \"0.01kgm2\"\nb = \"0.001Nms\"\nperiod = \"1ms\"",
        )
        .builtin(5, "r1", "resistor", "r = \"1kohm\"")
        .net("top", &["v1.p", "l1.a", "m1.a+"])
        .net("mid", &["l1.b", "c1.a", "r1.a"])
        .ground("g", &["v1.n", "c1.b", "r1.b", "m1.a-"])
        .observed();
    let plant = deck.plant();
    let n = plant.transient_unknowns;
    let companion = Companion {
        alpha: 7.0e4,
        history: (0..4).map(|k| 1.0 + 0.5 * k as f64).collect(),
    };
    let x: Vec<f64> = (0..n).map(|i| 0.3 + 0.1 * i as f64).collect();
    let at = |x: &[f64]| {
        let mut mem = memory(&plant);
        assemble_form(&plant, x, 1e-12, Form::Step(&companion), &mut mem)
    };
    let base = at(&x);
    assert_eq!((base.f.len(), base.j.size()), (n, n));
    for col in 0..n {
        let h = 1e-6;
        let mut xp = x.clone();
        xp[col] += h;
        let up = at(&xp);
        for row in 0..n {
            let numeric = (up.f[row] - base.f[row]) / h;
            let analytic = base.j.get(row, col);
            assert!(
                (numeric - analytic).abs() <= 1e-5 * (1.0 + analytic.abs()),
                "J[{row}][{col}] {numeric} vs {analytic}"
            );
        }
    }
}
