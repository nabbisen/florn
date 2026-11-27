//! PR-48 (checklist B-48): the circuit's results across engines and the controls of RFC 013 rules
//! 19 to 21. Every circuit scenario run here is compared with the results file the kit recorded on
//! x86-64 (`scenarios/results/<name>.results`) by the comparison of rule 20 as a second run, and the
//! table of `bit_identity` with its file; the browsers do the same through the Host's own
//! `compare::golden` (`ci/browser/run.js`). Every control is shown to fail on a planted violation.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use florn_conformance::goldens::{self, CIRCUIT};
use florn_conformance::shipped::{self, Digests};
use florn_domain_circuit::bits;
use florn_host::compare::golden::{Golden, against};
use florn_host::compare::results;

fn file_of(name: &str) -> String {
    std::fs::read_to_string(goldens::results_file(name))
        .unwrap_or_else(|e| panic!("{name}.results: {e}"))
}

fn recorded_on() -> String {
    Digests::read(&shipped::directory()).unwrap().arch
}

// --- every circuit scenario against its file -------------------------------------------------

#[test]
fn every_circuit_scenario_agrees_with_its_results_file_and_matches_where_it_was_recorded() {
    for s in goldens::standings(&goldens::directory()) {
        assert!(s.golden.agrees(), "{}: {}", s.name, s.golden.line());
        // The kit expects bit-identical results everywhere (rule 19): a `within-tolerance` on an
        // architecture is a figure to look at, and the table prints it.
        if std::env::consts::ARCH == recorded_on() {
            assert!(
                matches!(s.golden, Golden::Matched { .. }),
                "{}: {}",
                s.name,
                s.golden.line()
            );
        }
    }
}

#[test]
fn every_circuit_scenario_has_a_results_file_and_every_file_has_a_scenario() {
    // The circuit scenarios that ship: every scenario file whose domains include `circuit`.
    let mut circuit = BTreeSet::new();
    for entry in std::fs::read_dir(shipped::directory()).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "toml") {
            let text = std::fs::read_to_string(&path).unwrap();
            let domains = text
                .lines()
                .find(|l| l.starts_with("domains"))
                .unwrap_or_default();
            if domains.contains("\"circuit\"") {
                circuit.insert(path.file_stem().unwrap().to_string_lossy().to_string());
            }
        }
    }
    let listed: BTreeSet<String> = CIRCUIT.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        circuit, listed,
        "a circuit scenario without a results file, or one that is gone"
    );
    let mut files = BTreeSet::new();
    for entry in std::fs::read_dir(goldens::directory()).unwrap().flatten() {
        files.insert(entry.file_name().to_string_lossy().to_string());
    }
    let mut want: BTreeSet<String> = CIRCUIT.iter().map(|n| format!("{n}.results")).collect();
    want.insert(goldens::BITS_FILE.to_string());
    assert_eq!(
        files, want,
        "scenarios/results holds the files of the circuit and nothing else"
    );
}

#[test]
fn a_results_file_is_the_trace_of_its_digests_run() {
    let digests = Digests::read(&shipped::directory()).unwrap();
    for name in CIRCUIT {
        let text = file_of(name);
        assert_eq!(text.lines().next(), Some("florn-results 1"));
        // One line per record of the run `digests.toml` records.
        assert_eq!(
            text.lines().count() as u64 - 1,
            digests.runs[*name].records,
            "{name}"
        );
        let rows = results::parse(&text).unwrap();
        assert!(
            results::write(&rows) == text,
            "{name}: the file is the writer's text"
        );
    }
}

#[test]
fn the_recorded_results_are_what_the_native_host_gives_here_when_it_is_the_recording_arch() {
    if std::env::consts::ARCH != recorded_on() {
        return;
    }
    for name in CIRCUIT {
        // (A boolean: the files are a megabyte, and a failure should not print them.)
        assert!(
            goldens::results_of(name) == file_of(name),
            "{name}: the recorded file is not this run's text; write the files with `scenarios`"
        );
    }
}

// --- the sensors' numbers are compared as numbers ----------------------------------------------

/// The line of `text` that holds the first numeric field named `field`, the line number and the
/// text with the last hex digit of that field's value moved by one.
fn nudge(text: &str, field: &str, by: i8) -> (usize, String) {
    let mut out = Vec::new();
    let mut done = None;
    for (n, line) in text.lines().enumerate() {
        let marker = format!("+{field}|");
        if done.is_none()
            && let Some(at) = line.find(&marker)
        {
            // `name|unit|factor|value`, the value being the last 16 hex digits of the token.
            let end = line[at..].find(' ').map_or(line.len(), |e| at + e);
            let last = line.as_bytes()[end - 1] as char;
            let digit = last.to_digit(16).unwrap() as i8;
            let moved = char::from_digit(((digit + by).rem_euclid(16)) as u32, 16).unwrap();
            let mut l = line.to_string();
            l.replace_range(end - 1..end, &moved.to_string());
            out.push(l);
            done = Some(n + 1);
            continue;
        }
        out.push(line.to_string());
    }
    (done.expect("a field of that name"), out.join("\n") + "\n")
}

#[test]
fn the_controls_of_the_comparison_a_last_bit_is_within_tolerance_and_a_real_difference_is_not() {
    for (name, field) in [("rc-step", "value"), ("motor-loop", "value")] {
        let run = goldens::run(name);
        let rows = run.rows().unwrap();
        let reading = run.reading();
        let text = file_of(name);
        // The file as it is: matched.
        assert_eq!(against(&text, &rows, &reading).word(), "matched", "{name}");
        // The last bit of one sample: within tolerance, the one pair differing.
        let (_, doctored) = nudge(&text, field, 1);
        let g = against(&doctored, &rows, &reading);
        assert_eq!(g.word(), "within-tolerance", "{name}: {}", g.line());
        assert_eq!(g.differing(), 1, "{name}");
        // A sample whose value is another number entirely: diverged, naming the record.
        let mut far = text.clone();
        let at = far.find(&format!("+{field}|")).unwrap();
        let value_at = far[at..].find(' ').map(|e| at + e).unwrap() - 16;
        far.replace_range(value_at..value_at + 16, "4000000000000000");
        let g = against(&far, &rows, &reading);
        assert_eq!(g.word(), "diverged", "{name}: {}", g.line());
        assert!(g.line().contains("diverged at record"), "{}", g.line());
        // A record fewer: diverged, by the length.
        let shorter: Vec<&str> = text.lines().collect();
        let shorter = shorter[..shorter.len() - 1].join("\n") + "\n";
        let g = against(&shorter, &rows, &reading);
        assert_eq!(g.word(), "diverged");
        assert!(g.line().ends_with("length"), "{}", g.line());
    }
}

#[test]
fn a_sensors_send_is_a_number_even_where_it_is_connected_to_nothing() {
    // `rc-step`'s voltmeter sends its sample on a port that is on no signal; the comparison
    // reads the eight bytes as a number (rule 20, item 2), so a last bit there is not a payload
    // that differs.
    let text = file_of("rc-step");
    assert!(
        text.contains("+output|V|"),
        "the send is a numeric field in the file"
    );
    let run = goldens::run("rc-step");
    let rows = run.rows().unwrap();
    let (_, doctored) = nudge(&text, "output", 1);
    let g = against(&doctored, &rows, &run.reading());
    assert_eq!(g.word(), "within-tolerance", "{}", g.line());
}

// --- bit_identity ------------------------------------------------------------------------------

#[test]
fn bit_identity_agrees_with_the_recorded_table() {
    let n = goldens::bit_identity(&goldens::directory()).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(n, bits::table().len());
}

fn crate_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    let root = shipped::directory().join("../crates/florn-domain-circuit/src");
    walk(&root, &mut out);
    out
}

/// The `libm` functions a source text calls, and whether it names `libm` in a form that cannot be
/// read call by call (`use libm`, `libm::{`).
fn libm_calls(text: &str) -> (BTreeSet<String>, bool) {
    let mut calls = BTreeSet::new();
    let mut unreadable = false;
    for line in text.lines() {
        let code = line.split("//").next().unwrap_or("");
        if code.contains("use libm")
            || code.contains("libm::{")
            || code.contains("extern crate libm")
        {
            unreadable = true;
        }
        let mut rest = code;
        while let Some(at) = rest.find("libm::") {
            let after = &rest[at + 6..];
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                unreadable = true;
            } else {
                calls.insert(name);
            }
            rest = after;
        }
    }
    (calls, unreadable)
}

#[test]
fn the_crate_calls_no_libm_function_the_table_does_not_list() {
    let listed: BTreeSet<String> = bits::LIBM.iter().map(|(l, _)| l.to_string()).collect();
    let sources = crate_sources();
    assert!(sources.len() > 20, "the crate's sources were found");
    let mut called = BTreeSet::new();
    for path in sources {
        let (calls, unreadable) = libm_calls(&std::fs::read_to_string(&path).unwrap());
        assert!(
            !unreadable,
            "{}: `libm` is named in a form the check cannot read",
            path.display()
        );
        called.extend(calls);
    }
    let missing: Vec<&String> = called.difference(&listed).collect();
    assert!(
        missing.is_empty(),
        "libm functions called and not in the table: {missing:?}"
    );
    // And the table lists nothing the crate does not call: a stale name is a table that proves less.
    assert_eq!(called, listed);
}

#[test]
fn the_check_of_the_sources_fails_on_a_planted_libm_call() {
    let (calls, _) = libm_calls("fn f(x: f64) -> f64 { libm::exp(x) + libm::sinh(x) }");
    let listed: BTreeSet<String> = bits::LIBM.iter().map(|(l, _)| l.to_string()).collect();
    assert_eq!(
        calls.difference(&listed).collect::<Vec<_>>(),
        [&"sinh".to_string()]
    );
    // A comment is not a call; a form that hides the name is refused.
    assert!(
        libm_calls("// libm::tanh is not used\nfn f() {}")
            .0
            .is_empty()
    );
    assert!(libm_calls("use libm::{exp, ln};").1);
    assert!(libm_calls("use libm;").1);
}

#[test]
fn bit_identity_fails_on_a_doctored_file_and_on_a_doctored_function() {
    let file = std::fs::read_to_string(goldens::directory().join(goldens::BITS_FILE)).unwrap();
    // A bit of one answer in the file.
    let mut lines: Vec<String> = file.lines().map(str::to_string).collect();
    let k = lines.iter().position(|l| l.starts_with("cbrt ")).unwrap();
    let flipped = if lines[k].ends_with('1') { "0" } else { "1" };
    let n = lines[k].len();
    lines[k].replace_range(n - 1.., flipped);
    let doctored = lines.join("\n") + "\n";
    let e = bits::compare(&doctored, &bits::table()).unwrap_err();
    assert!(e[0].contains("cbrt"), "{e:?}");
    // A function of the crate that returns the answer one unit in the last place off.
    let off = bits::Functions {
        ln: |x| {
            let y = florn_domain_circuit::math::ln(x);
            if y.is_finite() && y != 0.0 {
                f64::from_bits(y.to_bits() + 1)
            } else {
                y
            }
        },
        ..bits::Functions::CRATE
    };
    let e = bits::compare(&file, &bits::table_using(&off)).unwrap_err();
    assert!(e.iter().any(|l| l.contains("the file has `ln ")), "{e:?}");
    // And a file that is another version's.
    let e = bits::compare(
        &file.replacen("florn-bits 1", "florn-bits 2", 1),
        &bits::table(),
    )
    .unwrap_err();
    assert!(e[0].contains("does not begin"), "{e:?}");
}

// --- the table the figures print ---------------------------------------------------------------

#[test]
fn the_cross_engine_table_names_every_scenario_the_engine_and_the_figures() {
    let (text, good) = goldens::report(&goldens::directory());
    assert!(good, "{text}");
    assert!(
        text.starts_with(&format!("| {} |", goldens::engine_name())),
        "{text}"
    );
    for name in CIRCUIT {
        assert!(text.contains(&format!("| `{name}` | ")), "{text}");
    }
    assert!(text.contains("| `bit_identity` | matched | "), "{text}");
    assert!(
        text.contains("| matched |") || text.contains("matched |"),
        "{text}"
    );
    // A row that does not agree is said, with the reason.
    let standings = vec![goldens::Standing {
        name: "x".into(),
        golden: Golden::Unreadable("why".into()),
    }];
    let t = goldens::table("e", &standings, &Err(vec!["line 2".into()]));
    assert!(t.contains("| `x` | unreadable |"), "{t}");
    assert!(t.contains("| `bit_identity` | diverged |"), "{t}");
}
