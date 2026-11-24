//! The results file of RFC 013 rule 21.

use super::values::last_bit;
use super::*;

// ---- Results file (rule 21) -------------------------------------------------------------------------

fn results_text(d: &Doc) -> String {
    let probe = go(
        &base(),
        Some(base().trace),
        base().payloads,
        host::options(),
    );
    let rows = florn_host::compare::rows(Some(&d.trace[..]), &d.payloads, &probe.run.reading())
        .expect("rows");
    florn_host::compare::results::write(&rows)
}

#[test]
fn f1_the_bits_of_a_float_are_written_in_hex() {
    let text = scenario("1e-9", "1e-6");
    let d = record_with(&text, [1.0, -0.0, f64::INFINITY, 2.5]);
    let out = results_text(&d);
    for want in [
        "3ff0000000000000",
        "8000000000000000",
        "7ff0000000000000",
        "4004000000000000",
    ] {
        assert!(
            out.contains(want),
            "{want} absent from\n{}",
            out.lines().take(3).collect::<Vec<_>>().join("\n")
        );
    }
    assert_eq!(out.lines().next(), Some("florn-results 1"));
    assert_eq!(out.lines().count(), d.trace.len() + 1);
}

#[test]
fn f3_every_input_of_rule_20_changes_the_line() {
    let d = base();
    let original = results_text(&d);
    let line_of = |t: &str, i: usize| t.lines().nth(1 + i).unwrap().to_string();
    let i = level_records(&d.trace)[1];
    let mut muts: Vec<(&str, Doc)> = vec![];
    let mk = |f: &dyn Fn(&mut Doc)| {
        let mut x = d.clone();
        f(&mut x);
        x
    };
    let key = |g: fn(&mut florn_kernel::EventKey)| {
        move |x: &mut Doc| {
            if let Record::Handled { key, .. } = &mut x.trace[i] {
                g(key)
            }
        }
    };
    muts.push(("tick", mk(&key(|k| k.tick += 1))));
    muts.push(("microstep", mk(&key(|k| k.microstep += 1))));
    muts.push(("class", mk(&key(|k| k.class += 1))));
    muts.push(("tie", mk(&key(|k| k.tie += 1))));
    muts.push(("origin", mk(&key(|k| k.origin += 1))));
    muts.push(("seq", mk(&key(|k| k.seq += 1))));
    muts.push((
        "target",
        mk(&|x| {
            if let Record::Handled { target, .. } = &mut x.trace[i] {
                *target += 1
            }
        }),
    ));
    muts.push(("work", mk(&|x| x.metered(i, 1))));
    muts.push(("draws", mk(&|x| x.draws(i, 1))));
    muts.push((
        "effect seq",
        mk(&|x| {
            if let Record::Handled { effects, .. } = &mut x.trace[i]
                && let CommittedEffect::Report { seq, .. } = &mut effects[0]
            {
                *seq += 1
            }
        }),
    ));
    muts.push((
        "effect channel",
        mk(&|x| {
            if let Record::Handled { effects, .. } = &mut x.trace[i]
                && let CommittedEffect::Report { channel, .. } = &mut effects[0]
            {
                *channel = 3
            }
        }),
    ));
    muts.push(("numeric bit", mk(&|x| x.set_level(i, last_bit(2.0)))));
    let j = frame_records(&d.trace)[0];
    muts.push((
        "other payload digest",
        mk(&|x| {
            let digest = match &x.trace[j] {
                Record::Handled { effects, .. } => effects
                    .iter()
                    .find_map(|e| match e {
                        CommittedEffect::Report {
                            channel: 1, digest, ..
                        } => Some(*digest),
                        _ => None,
                    })
                    .unwrap(),
                _ => unreachable!(),
            };
            let mut b = x.payloads[&digest].clone();
            b[0] ^= 1;
            x.set_report(j, b);
        }),
    ));
    for (name, m) in muts {
        let t = results_text(&m);
        let idx = if name == "other payload digest" { j } else { i };
        assert_ne!(
            line_of(&t, idx),
            line_of(&original, idx),
            "{name} does not change the line"
        );
    }
}

#[test]
fn f5_the_results_file_parses_back_to_the_rows_it_was_written_from() {
    let d = base();
    let t = results_text(&d);
    let rows = florn_host::compare::results::parse(&t).expect("parses");
    assert_eq!(florn_host::compare::results::write(&rows), t);
    // edits the parser must refuse
    assert!(
        florn_host::compare::results::parse(&t.replacen("florn-results 1", "florn-results 2", 1))
            .is_err()
    );
    assert!(florn_host::compare::results::parse("").is_err());
    assert!(florn_host::compare::results::parse("florn-results 1\nnot a row\n").is_err());
}
