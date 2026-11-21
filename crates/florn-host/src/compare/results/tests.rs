use super::*;

fn key(tick: u64) -> EventKey {
    EventKey {
        tick,
        microstep: 2,
        class: 7,
        tie: 4,
        origin: 9,
        seq: 1,
    }
}

fn rows() -> Vec<Row> {
    vec![
        Row::Handled {
            key: key(100),
            target: 3,
            work: 77,
            draws: 2,
            effects: vec![
                Effect::Send {
                    seq: 0,
                    port: 1,
                    len: 8,
                    payload: Payload::Numbers {
                        fields: vec![Number {
                            name: "level".into(),
                            unit: "kohm".into(),
                            factor: 1000.0f64.to_bits(),
                            value: 1.5f64.to_bits(),
                        }],
                        rest: [0xab; 32],
                    },
                },
                Effect::SetTimer {
                    seq: 1,
                    timer: 4,
                    delay: 500,
                },
                Effect::CancelTimer { seq: 2, timer: 4 },
                Effect::Report {
                    seq: 3,
                    channel: 2,
                    len: 5,
                    payload: Payload::Digest([0x11; 32]),
                },
            ],
        },
        Row::Fault {
            key: key(200),
            actor: 3,
            kind: 2,
            parameter: 12019,
            work: 99,
        },
        Row::Intervention {
            key: key(300),
            digest: [0x22; 32],
            len: 12,
        },
    ]
}

/// The syntax of the results file is frozen (RFC 013 rule 21): this text is what a file of these
/// rows is, byte for byte. An edit of the writer that changes it fails here, and must bump the
/// version on the first line.
#[test]
fn the_results_file_is_frozen() {
    let text = write(&rows());
    let expected = "florn-results 1\n\
h 100.2.7.4.9.1 3 77 2 s:0:1:8:n=abababababababababababababababababababababababababababababababab+level|kohm|408f400000000000|3ff8000000000000 t:1:4:500 c:2:4 r:3:2:5:d=1111111111111111111111111111111111111111111111111111111111111111\n\
f 200.2.7.4.9.1 3 2 12019 99\n\
i 300.2.7.4.9.1 2222222222222222222222222222222222222222222222222222222222222222 12\n";
    assert_eq!(text, expected);
    assert!(text.starts_with("florn-results 1\n"));
}

#[test]
fn a_results_file_reads_back_to_its_rows() {
    let text = write(&rows());
    assert_eq!(parse(&text).unwrap(), rows());
    // Writing what was read is the same text.
    assert_eq!(write(&parse(&text).unwrap()), text);
    // No records: the first line alone.
    assert_eq!(write(&[]), "florn-results 1\n");
    assert_eq!(parse("florn-results 1\n").unwrap(), Vec::<Row>::new());
}

#[test]
fn a_name_and_a_unit_are_escaped_so_that_no_separator_is_in_them() {
    let odd = vec![Row::Handled {
        key: key(1),
        target: 1,
        work: 0,
        draws: 0,
        effects: vec![Effect::Report {
            seq: 0,
            channel: 1,
            len: 8,
            payload: Payload::Numbers {
                fields: vec![Number {
                    name: "a b|c+d:e;f%g".into(),
                    unit: "Ω".into(),
                    factor: 1.0f64.to_bits(),
                    value: 0,
                }],
                rest: [0; 32],
            },
        }],
    }];
    let text = write(&odd);
    let line = text.lines().nth(1).unwrap();
    assert_eq!(
        line.matches(' ').count(),
        5,
        "five separators, none from a name: {line}"
    );
    assert!(line.contains("a%20b%7cc%2bd%3ae%3bf%25g|%ce%a9|"), "{line}");
    assert_eq!(parse(&text).unwrap(), odd);
}

#[test]
fn what_is_not_a_results_file_is_refused() {
    for (text, why) in [
        ("", "no first line"),
        ("florn-results 2\n", "another version"),
        ("florn-results\n", "no version"),
        ("florn-results 1\nx 1\n", "an unknown kind of line"),
        (
            "florn-results 1\nh 1.2.3.4.5\n",
            "a short key and no target",
        ),
        ("florn-results 1\nh 1.2.3.4.5.6 1 2\n", "a missing count"),
        (
            "florn-results 1\nh 1.2.3.4.5.6 1 2 3 q:1\n",
            "an unknown effect",
        ),
        (
            "florn-results 1\nh 1.2.3.4.5.6 1 2 3 t:1:2\n",
            "a short timer",
        ),
        (
            "florn-results 1\nh 1.2.3.4.5.6 1 2 3 s:0:1:8:d=ab\n",
            "a short digest",
        ),
        (
            "florn-results 1\nh 1.2.3.4.5.6 1 2 3 r:0:1:8:n=zz\n",
            "bad hex",
        ),
        ("florn-results 1\nf 1.2.3.4.5.6 1 2 3\n", "a short fault"),
        (
            "florn-results 1\nf 1.2.3.4.5.6 1 999 3 4\n",
            "a kind beyond a byte",
        ),
        (
            "florn-results 1\ni 1.2.3.4.5.6 ab 3\n",
            "a short intervention digest",
        ),
        (
            "florn-results 1\nh 1.2.3.4.5.6.7 1 2 3\n",
            "a key of seven parts",
        ),
    ] {
        assert!(parse(text).is_err(), "{why}: {text:?}");
    }
    // The line is named.
    let e = parse("florn-results 1\nh 1.2.3.4.5.6 1 2 3\nbad\n").unwrap_err();
    assert!(e.contains("line 3"), "{e}");
}
