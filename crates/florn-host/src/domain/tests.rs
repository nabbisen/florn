use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn a_lent_domain_answers_no_import_and_panics_when_asked_for_anything_else() {
    // RFC 012 finding F-3, as ruled for PR-38: while a domain is lent out for a call into a guest,
    // reaching it is a defect of the Host. `answer` is the one thing a guest can ask of it, and it
    // is refused; every other method fails at once and does not give a default.
    let mut lent = Lent { name: "test" };
    assert_eq!(lent.name(), "test");
    assert_eq!(lent.answer(1, "probe", &[]), None);
    let now = Instant::new(0, 0);
    type Ask = Box<dyn FnMut(&mut Lent)>;
    let mut calls: Vec<(&str, Ask)> = vec![
        ("connection_types", Box::new(|d| drop(d.connection_types()))),
        ("port_types", Box::new(|d| drop(d.port_types()))),
        ("builtin_kinds", Box::new(|d| drop(d.builtin_kinds()))),
        (
            "builtin_channels",
            Box::new(|d| drop(d.builtin_channels("k"))),
        ),
        (
            "intervention_kinds",
            Box::new(|d| drop(d.intervention_kinds())),
        ),
        ("classes", Box::new(|d| drop(d.classes()))),
        ("units", Box::new(|d| drop(d.units()))),
        ("world", Box::new(|d| drop(d.world()))),
        (
            "deliver",
            Box::new(move |d| drop(d.deliver(now, 1, 1, 0, &[], &BTreeSet::new()))),
        ),
        (
            "change",
            Box::new(move |d| drop(d.change(now, 1, &DomainMap::new()))),
        ),
        (
            "intervene",
            Box::new(move |d| drop(d.intervene(now, 1, "k", &DomainMap::new()))),
        ),
        ("telemetry", Box::new(|d| drop(d.telemetry()))),
        ("export", Box::new(|d| drop(d.export()))),
        (
            "restore",
            Box::new(|d| {
                let _ = d.restore(&[]);
            }),
        ),
    ];
    for (what, call) in &mut calls {
        let r = catch_unwind(AssertUnwindSafe(|| call(&mut lent)));
        let message = r
            .err()
            .and_then(|p| p.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        assert!(
            message.contains("is lent out") && message.contains(*what),
            "{what}: {message:?}"
        );
    }
}
