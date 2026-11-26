use super::*;

fn from(rows: &[&[f64]]) -> Matrix {
    let mut m = Matrix::new(rows.len());
    for (i, r) in rows.iter().enumerate() {
        for (j, x) in r.iter().enumerate() {
            m.set(i, j, *x);
        }
    }
    m
}

#[test]
fn a_two_by_two_system_has_its_hand_solution() {
    // 2x + y = 5, x + 3y = 10 → x = 1, y = 3.
    let mut m = from(&[&[2.0, 1.0], &[1.0, 3.0]]);
    let mut b = [5.0, 10.0];
    m.solve(&mut b).unwrap();
    assert!(
        (b[0] - 1.0).abs() < 1e-15 && (b[1] - 3.0).abs() < 1e-15,
        "{b:?}"
    );
}

#[test]
fn a_zero_on_the_diagonal_is_pivoted_away() {
    // y = 2, x + y = 5 → x = 3; the first column's largest entry is in the second row.
    let mut m = from(&[&[0.0, 1.0], &[1.0, 1.0]]);
    let mut b = [2.0, 5.0];
    m.solve(&mut b).unwrap();
    assert_eq!(b, [3.0, 2.0]);
}

#[test]
fn the_pivot_is_the_largest_magnitude_and_the_lowest_row_on_a_tie() {
    // Both rows have magnitude 1 in column 0: the first row stays, so the result is the one a
    // pivot of the first row gives. With rows swapped in the input the bits of a system whose
    // rows are not symmetric differ only if the rule were not value-only: solve both orders and
    // compare with the hand solution.
    let mut m = from(&[&[1.0, 2.0], &[-1.0, 1.0]]);
    let mut b = [5.0, 1.0];
    m.solve(&mut b).unwrap();
    // x + 2y = 5, −x + y = 1 → y = 2, x = 1.
    assert_eq!(b, [1.0, 2.0]);
    let mut m = from(&[&[-1.0, 1.0], &[1.0, 2.0]]);
    let mut b = [1.0, 5.0];
    m.solve(&mut b).unwrap();
    assert_eq!(b, [1.0, 2.0]);
}

#[test]
fn a_singular_matrix_names_the_column_of_the_first_zero_pivot() {
    let mut m = from(&[&[1.0, 1.0], &[1.0, 1.0]]);
    assert_eq!(m.solve(&mut [1.0, 2.0]), Err(1));
    let mut m = from(&[&[0.0, 0.0], &[0.0, 1.0]]);
    assert_eq!(m.solve(&mut [1.0, 2.0]), Err(0));
    // A pivot that is not a number is no pivot: the column is named.
    let mut m = from(&[&[f64::NAN, 1.0], &[f64::NAN, 1.0]]);
    assert_eq!(m.solve(&mut [1.0, 2.0]), Err(0));
}

#[test]
fn an_empty_system_solves() {
    let mut m = Matrix::new(0);
    assert_eq!(m.solve(&mut []), Ok(()));
}

#[test]
fn a_larger_system_residual_is_small() {
    // A diagonally dominant tridiagonal system of 20 unknowns.
    let n = 20;
    let mut m = Matrix::new(n);
    let mut b = vec![0.0; n];
    for (i, bi) in b.iter_mut().enumerate() {
        m.set(i, i, 4.0);
        if i > 0 {
            m.set(i, i - 1, -1.0);
        }
        if i + 1 < n {
            m.set(i, i + 1, -1.0);
        }
        *bi = (i + 1) as f64;
    }
    let original = m.clone();
    let rhs = b.clone();
    m.solve(&mut b).unwrap();
    for (i, want) in rhs.iter().enumerate() {
        let mut r = -want;
        for (j, x) in b.iter().enumerate() {
            r += original.get(i, j) * x;
        }
        assert!(r.abs() < 1e-12, "row {i}: {r}");
    }
}

#[test]
fn the_same_system_gives_the_same_bits() {
    let build = || from(&[&[3.0, 1.0, 2.0], &[1.0, 5.0, 1.0], &[2.0, 1.0, 4.0]]);
    let (mut a, mut b) = ([1.0, 2.0, 3.0], [1.0, 2.0, 3.0]);
    build().solve(&mut a).unwrap();
    build().solve(&mut b).unwrap();
    assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits));
}

#[test]
fn a_tie_in_a_column_keeps_the_lowest_row_and_the_bits_show_it() {
    // The first column holds 0.3 and -0.3: equal magnitudes. The lowest row is the pivot (RFC 013
    // rule 19), and the rounding of what follows depends on it: the last bits of the solution are
    // those of this rule and of no other (pivoting on the last row of the tie gives other bits).
    let mut m = from(&[&[0.3, 2.3, 0.1], &[-0.3, 0.7, 0.1], &[0.1, 0.1, 0.9]]);
    let mut b = [2.3, 0.1, 1.1];
    m.solve(&mut b).unwrap();
    assert_eq!(
        b.map(f64::to_bits),
        [
            0x3ffb_3d30_b3d3_0b43,
            0x3fe7_921b_7921_b791,
            0x3fee_7063_e706_3e70
        ]
    );
}

/// RFC 013 rule 18: the time of one factorisation at each Host maximum and the memory of the
/// matrix, for the figures of a release. Run in release mode:
/// `cargo test -p florn-domain-circuit --release -- --ignored --nocapture lu::tests::measure`.
#[test]
#[ignore = "a measurement, not a check"]
fn measure_one_factorisation_at_each_host_maximum() {
    for n in [64usize, 256, 512, 1024] {
        let mut m = Matrix::new(n);
        let mut seed = 12345u64;
        for i in 0..n {
            for j in 0..n {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let r = ((seed >> 33) as f64) / f64::from(1u32 << 31) - 0.5;
                m.set(i, j, if i == j { r + 2.0 * n as f64 } else { r });
            }
        }
        let mut b = vec![1.0; n];
        let start = std::time::Instant::now();
        m.solve(&mut b).unwrap();
        let ms = start.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "MEASURE n = {n}: {ms:.2} ms, matrix {} KiB",
            8 * n * n / 1024
        );
    }
}
