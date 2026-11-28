use std::collections::BTreeSet;

use super::*;

fn answers(table: &[Entry], function: &str, inputs: &[f64]) -> u64 {
    let bits: Vec<u64> = inputs.iter().map(|x| x.to_bits()).collect();
    table
        .iter()
        .find(|e| e.function == function && e.inputs == bits)
        .unwrap_or_else(|| panic!("no entry {function} {inputs:?}"))
        .output
}

#[test]
fn the_table_is_fixed_and_runs_every_function_the_solver_uses() {
    let a = table();
    assert_eq!(a, table(), "the table is a function of nothing");
    let names: BTreeSet<&str> = a.iter().map(|e| e.function.as_str()).collect();
    for want in [
        "exp",
        "ln",
        "cbrt",
        "sqrt",
        "abs",
        "floor",
        "ceil",
        "trunc",
        "neg",
        "add",
        "sub",
        "mul",
        "div",
        "fmin",
        "fmax",
        "copysign",
        "diode.i",
        "diode.g",
        "pnjlim",
        "pnjlim.flag",
        "lu.tie.x0",
        "lu.swap.x3",
        "lu.sweep.x5",
        "lu.singular.err",
    ] {
        assert!(names.contains(want), "the table lacks {want}");
    }
    // Every function the crate calls from `libm` has a name in the table.
    for (_, name) in LIBM {
        assert!(names.contains(name), "no row for {name}");
    }
    assert!(a.len() > 2_000, "{} entries", a.len());
}

#[test]
fn the_inputs_include_the_cases_engines_treat_differently() {
    let inputs = specials();
    let has = |bits: u64| inputs.contains(&bits);
    assert!(
        has(0.0f64.to_bits()) && has((-0.0f64).to_bits()),
        "signed zeros"
    );
    assert!(has(f64::INFINITY.to_bits()) && has(f64::NEG_INFINITY.to_bits()));
    assert!(has(1) && has(0x000f_ffff_ffff_ffff), "subnormals");
    assert!(
        has(0x7ff8_0000_0000_0001) && has(0x7ff0_0000_0000_0001),
        "NaN payloads"
    );
    assert!(has(0xfff8_0000_0000_0000), "a negative NaN");
}

#[test]
fn every_answer_leaves_the_solver_canonical() {
    for e in table() {
        let x = f64::from_bits(e.output);
        if x.is_nan() {
            assert_eq!(
                e.output, 0x7ff8_0000_0000_0000,
                "{}: a NaN that is not canonical",
                e.function
            );
        }
    }
    // A NaN made by arithmetic, which the targets sign differently, is one NaN in the table.
    let t = table();
    let nan = answers(&t, "sub", &[f64::NEG_INFINITY, f64::NEG_INFINITY]);
    assert_eq!(nan, 0x7ff8_0000_0000_0000);
    assert_eq!(answers(&t, "ln", &[-1.0]), 0x7ff8_0000_0000_0000);
}

#[test]
fn the_rules_the_table_freezes_are_in_it() {
    let t = table();
    assert_eq!(answers(&t, "exp", &[0.0]), 1.0f64.to_bits());
    assert_eq!(answers(&t, "ln", &[1.0]), 0.0f64.to_bits());
    assert_eq!(answers(&t, "cbrt", &[-8.0]), (-2.0f64).to_bits());
    assert_eq!(answers(&t, "exp", &[f64::NEG_INFINITY]), 0);
    assert_eq!(answers(&t, "exp", &[710.0]), f64::INFINITY.to_bits());
    // `fmin` and `fmax` are by comparison: the first operand stands for a pair of zeros.
    assert_eq!(answers(&t, "fmin", &[-0.0, 0.0]), (-0.0f64).to_bits());
    assert_eq!(answers(&t, "fmin", &[0.0, -0.0]), 0.0f64.to_bits());
    assert_eq!(answers(&t, "fmax", &[-0.0, 0.0]), (-0.0f64).to_bits());
    assert_eq!(answers(&t, "fmax", &[0.0, -0.0]), 0.0f64.to_bits());
    // The limiter limits a jump to 5 V and not one to 0.3 V.
    assert_eq!(answers(&t, "pnjlim.flag", &[5.0, 0.0]), 1.0f64.to_bits());
    assert_eq!(answers(&t, "pnjlim.flag", &[0.3, 0.0]), 0.0f64.to_bits());
    // The singular matrix names the column of its zero pivot.
    let e = t.iter().find(|e| e.function == "lu.singular.err").unwrap();
    assert_eq!(f64::from_bits(e.output), 2.0);
    // A tie for the pivot is solved by the lowest index: 2x - 2y + z = 1, 2x + 2y - z = 2, ...
    let x: Vec<f64> = (0..3)
        .map(|k| {
            let name = format!("lu.tie.x{k}");
            f64::from_bits(t.iter().find(|e| e.function == name).unwrap().output)
        })
        .collect();
    let residual = [
        2.0 * x[0] - 2.0 * x[1] + x[2] - 1.0,
        2.0 * x[0] + 2.0 * x[1] - x[2] - 2.0,
        -2.0 * x[0] + x[1] + 3.0 * x[2] - 3.0,
    ];
    assert!(residual.iter().all(|r| r.abs() < 1e-12), "{residual:?}");
}

#[test]
fn the_text_is_read_back_as_the_table_and_a_difference_is_named() {
    let t = table();
    let text = render(&t);
    assert!(text.starts_with("florn-bits 1\n"));
    assert_eq!(text.lines().count(), t.len() + 1);
    assert_eq!(compare(&text, &t), Ok(t.len()));
    // One bit of one answer.
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let k = lines.iter().position(|l| l.starts_with("exp ")).unwrap();
    let last = lines[k].len() - 1;
    let flipped = if lines[k].ends_with('0') { '1' } else { '0' };
    lines[k].replace_range(last.., &flipped.to_string());
    let doctored = lines.join("\n") + "\n";
    let e = compare(&doctored, &t).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].starts_with(&format!("line {}: the file has `exp ", k + 1)),
        "{e:?}"
    );
    // A line fewer, a line more, and a file that is not one.
    let intact: Vec<&str> = text.lines().collect();
    let shorter = intact[..intact.len() - 1].join("\n") + "\n";
    assert!(compare(&shorter, &t).unwrap_err()[0].contains("the file has none"));
    let longer = text.clone() + "exp 0000000000000000 0000000000000000\n";
    assert!(compare(&longer, &t).unwrap_err()[0].contains("the file has"));
    assert!(compare("no\n", &t).unwrap_err()[0].contains("does not begin with"));
}

#[test]
fn a_difference_in_many_lines_is_counted_and_only_the_first_are_given() {
    let t = table();
    let text = render(&t).replace("exp ", "exq ");
    let e = compare(&text, &t).unwrap_err();
    assert_eq!(e.len(), 9);
    assert!(e[8].ends_with("differences in all"), "{e:?}");
}

#[test]
fn a_doctored_function_fails_the_comparison_with_the_file() {
    let recorded = render(&table());
    let one_ulp_up = |x: f64| {
        let y = exp(x);
        if y.is_finite() && y > 0.0 {
            f64::from_bits(y.to_bits() + 1)
        } else {
            y
        }
    };
    let doctored = Functions {
        exp: one_ulp_up,
        ..Functions::CRATE
    };
    let e = compare(&recorded, &table_using(&doctored)).unwrap_err();
    assert!(e.iter().any(|l| l.contains("the file has `exp ")), "{e:?}");
    // The functions' signs of zero: a `fmin` that treats the pair the other way round.
    let swapped = Functions {
        fmin: |a, b| if a < b { a } else { b },
        ..Functions::CRATE
    };
    let e = compare(&recorded, &table_using(&swapped)).unwrap_err();
    assert!(e.iter().any(|l| l.contains("fmin ")), "{e:?}");
    // The others are untouched: only the doctored function's lines differ.
    assert!(
        e.iter()
            .all(|l| l.contains("fmin ") || l.ends_with("differences in all")),
        "{e:?}"
    );
    // The crate's own functions agree with the file.
    assert_eq!(
        compare(&recorded, &table_using(&Functions::CRATE)),
        Ok(table().len())
    );
}

#[test]
fn no_line_of_the_table_is_written_twice() {
    let text = render(&table());
    let mut seen = std::collections::BTreeSet::new();
    for line in text.lines() {
        assert!(seen.insert(line), "a line written twice: {line}");
    }
}

#[test]
fn a_file_with_a_carriage_return_or_without_its_last_line_end_is_refused_not_read() {
    let t = table();
    let text = render(&t);
    // The same lines with CRLF ends are not the file the writer writes.
    let crlf = text.replace('\n', "\r\n");
    let e = compare(&crlf, &t).unwrap_err();
    assert!(e[0].contains("the form the writer writes"), "{e:?}");
    // Nor a file whose last line end is gone.
    let e = compare(text.trim_end_matches('\n'), &t).unwrap_err();
    assert!(e[0].contains("the form the writer writes"), "{e:?}");
    // Nor an upper-case digit: it is not the line rendered.
    let upper = text.replacen("3ff0000000000000", "3FF0000000000000", 1);
    assert!(compare(&upper, &t).is_err());
    assert_eq!(compare(&text, &t), Ok(t.len()));
}
