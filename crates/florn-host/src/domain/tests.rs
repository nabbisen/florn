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
        ("worlds", Box::new(|d| drop(d.worlds()))),
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

/// A world of one import interface and one export, to compose and to register.
fn world(name: &str, import: Option<&str>, export: &str) -> DomainWorld {
    let function = |name: &'static str| Signature {
        name,
        params: vec![("now", Ty::U64)],
        result: None,
    };
    DomainWorld {
        name: name.to_string(),
        imports_ids: import.iter().map(|i| i.to_string()).collect(),
        exports_ids: vec![export.to_string()],
        imports: import
            .iter()
            .map(|i| (i.to_string(), vec![function("look")]))
            .collect(),
        exports: vec![(export.to_string(), vec![function("on-event")])],
        compounds: vec![],
    }
}

#[test]
fn composing_worlds_gives_every_interface_once() {
    // RFC 002 rule 48; RFC 012 finding F-9: a world of two domains imports and exports what both
    // do, an interface that both have only once.
    let a = world("a:w/one@0.1.0", Some("a:w/look@0.1.0"), "a:w/events@0.1.0");
    let b = world("b:w/two@0.1.0", Some("a:w/look@0.1.0"), "b:w/events@0.1.0");
    let both = DomainWorld::compose("c:w/both@0.1.0", &[&a, &b]).expect("they agree");
    assert_eq!(both.name, "c:w/both@0.1.0");
    assert_eq!(both.imports_ids, vec!["a:w/look@0.1.0"]);
    assert_eq!(
        both.exports_ids,
        vec!["a:w/events@0.1.0", "b:w/events@0.1.0"]
    );
    assert_eq!(both.imports.len(), 1);
    assert_eq!(both.exports.len(), 2);
    // The admission specification carries the core's and both parts' functions.
    let spec = both.spec();
    assert_eq!(spec.name, "c:w/both@0.1.0");
    assert!(
        spec.interfaces
            .exports
            .contains(&"b:w/events@0.1.0".to_string())
    );
    assert!(
        spec.exports
            .iter()
            .any(|f| f.name == "a:w/events@0.1.0#on-event")
    );
    assert!(
        spec.exports
            .iter()
            .any(|f| f.name == "b:w/events@0.1.0#on-event")
    );
}

#[test]
fn worlds_that_disagree_about_an_interface_do_not_compose() {
    let a = world("a:w/one@0.1.0", Some("a:w/look@0.1.0"), "a:w/events@0.1.0");
    let mut b = world("b:w/two@0.1.0", Some("a:w/look@0.1.0"), "b:w/events@0.1.0");
    b.imports[0].1[0].params.push(("more", Ty::U32));
    let refused = DomainWorld::compose("c:w/both@0.1.0", &[&a, &b]);
    assert!(refused.unwrap_err().contains("a:w/look@0.1.0"));
}

/// A domain that holds its worlds as data.
struct Holder {
    name: &'static str,
    worlds: Vec<DomainWorld>,
}

impl Domain for Holder {
    fn name(&self) -> &'static str {
        self.name
    }
    fn connection_types(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn port_types(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn worlds(&self) -> Vec<DomainWorld> {
        self.worlds.clone()
    }
    fn validate(&self, _: &Manifest) -> Vec<Notification> {
        Vec::new()
    }
    fn compose(&mut self, _: &Manifest, _: u64) {}
    fn deliver(
        &mut self,
        _: Instant,
        _: Identity,
        _: u64,
        _: u32,
        _: &[u8],
        _: &BTreeSet<Identity>,
    ) -> Vec<Delivery> {
        Vec::new()
    }
    fn change(&mut self, _: Instant, _: Identity, _: &DomainMap) -> Vec<Action> {
        Vec::new()
    }
    fn check_intervention(
        &self,
        _: Identity,
        _: &str,
        _: &DomainMap,
    ) -> Result<(), Box<Notification>> {
        Ok(())
    }
    fn intervene(&mut self, _: Instant, _: Identity, _: &str, _: &DomainMap) -> Vec<Action> {
        Vec::new()
    }
    fn event(&mut self, _: &DomainEvent, _: bool) -> Vec<Action> {
        Vec::new()
    }
    fn answer(&mut self, _: Identity, _: &str, _: &[Val]) -> Option<Val> {
        None
    }
    fn builtin(&mut self, _: Identity, _: &str, _: &BuiltinEvent<'_>, _: &mut Sink) {}
}

fn holder(name: &'static str, worlds: Vec<DomainWorld>) -> impl Fn() -> Holder {
    move || Holder {
        name,
        worlds: worlds.clone(),
    }
}

fn registry() -> Domains {
    // `a` has two worlds that import one interface (a controller and a device, say); `b` one.
    let a = holder(
        "a",
        vec![
            world("a:w/one@0.1.0", Some("a:w/look@0.1.0"), "a:w/events@0.1.0"),
            world("a:w/two@0.1.0", Some("a:w/look@0.1.0"), "a:w/other@0.1.0"),
        ],
    );
    let b = holder("b", vec![world("b:w/one@0.1.0", None, "b:w/events@0.1.0")]);
    Domains::core().with(a).with(b)
}

#[test]
fn a_composed_world_is_offered_only_when_the_host_holds_every_domain_it_spans() {
    // RFC 012 finding F-9: registered in `Controller::new`, where the domains' own worlds are,
    // when the Host holds every domain the world spans.
    let spans = [("a", "a:w/one@0.1.0"), ("b", "b:w/one@0.1.0")];
    let names = |d: &Domains| -> Vec<String> { d.worlds().into_iter().map(|w| w.name).collect() };

    let both = registry().with_composed("c:w/both@0.1.0", &spans);
    assert!(names(&both).contains(&"c:w/both@0.1.0".to_string()));
    assert_eq!(both.domains_of_world("c:w/both@0.1.0"), vec!["a", "b"]);
    assert_eq!(both.domains_of_world("a:w/two@0.1.0"), vec!["a"]);
    assert_eq!(
        both.domains_of_world("nobody:w/x@0.1.0"),
        Vec::<&str>::new()
    );

    // Without `b` the Host cannot offer it; the others are still there.
    let only_a = Domains::core()
        .with(holder(
            "a",
            vec![world(
                "a:w/one@0.1.0",
                Some("a:w/look@0.1.0"),
                "a:w/events@0.1.0",
            )],
        ))
        .with_composed("c:w/both@0.1.0", &spans);
    assert!(!names(&only_a).contains(&"c:w/both@0.1.0".to_string()));
    assert!(names(&only_a).contains(&"a:w/one@0.1.0".to_string()));

    // A world the domain does not have is as good as a domain that is missing.
    let wrong = registry().with_composed(
        "c:w/both@0.1.0",
        &[("a", "a:w/nope@0.1.0"), ("b", "b:w/one@0.1.0")],
    );
    assert!(!names(&wrong).contains(&"c:w/both@0.1.0".to_string()));

    // Registering a name again replaces it.
    let again = both.with_composed("c:w/both@0.1.0", &[("a", "a:w/two@0.1.0")]);
    assert_eq!(again.domains_of_world("c:w/both@0.1.0"), vec!["a"]);
}

#[test]
fn a_composition_whose_parts_disagree_is_not_offered() {
    let a = holder(
        "a",
        vec![world(
            "a:w/one@0.1.0",
            Some("a:w/look@0.1.0"),
            "a:w/events@0.1.0",
        )],
    );
    let mut clash = world("b:w/one@0.1.0", Some("a:w/look@0.1.0"), "b:w/events@0.1.0");
    clash.imports[0].1[0].params.push(("more", Ty::U32));
    let d = Domains::core()
        .with(a)
        .with(holder("b", vec![clash]))
        .with_composed(
            "c:w/both@0.1.0",
            &[("a", "a:w/one@0.1.0"), ("b", "b:w/one@0.1.0")],
        );
    assert!(!d.worlds().iter().any(|w| w.name == "c:w/both@0.1.0"));
}

#[test]
fn several_worlds_of_one_domain_may_import_one_interface_and_two_domains_may_not() {
    // `Composite` routes a guest's call of an imported interface to the domain that owns it.
    let scenario = Manifest::parse(
        "florn = 1\nname = \"w\"\ndomains = [\"a\"]\n\n[time]\ntick = \"1ns\"\nend = \"10ns\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 1\n",
        &florn_model::HostMaxima::native(),
    )
    .unwrap_or_else(|e| panic!("{:?}", e.codes()));
    let domains = |a: Holder, b: Option<Holder>| -> Vec<Box<dyn Domain>> {
        let mut v: Vec<Box<dyn Domain>> = vec![Box::new(a)];
        v.extend(b.map(|b| Box::new(b) as Box<dyn Domain>));
        v
    };
    let a = || Holder {
        name: "a",
        worlds: vec![
            world("a:w/one@0.1.0", Some("a:w/look@0.1.0"), "a:w/events@0.1.0"),
            world("a:w/two@0.1.0", Some("a:w/look@0.1.0"), "a:w/other@0.1.0"),
        ],
    };
    assert!(Composite::new(&scenario, domains(a(), None), 1).is_ok());
    let thief = Holder {
        name: "b",
        worlds: vec![world(
            "b:w/one@0.1.0",
            Some("a:w/look@0.1.0"),
            "b:w/events@0.1.0",
        )],
    };
    assert_eq!(
        Composite::new(&scenario, domains(a(), Some(thief)), 1).err(),
        Some("import interface assigned twice")
    );
}

#[test]
fn a_composition_the_host_holds_every_domain_of_and_cannot_build_is_a_defect() {
    // RFC 012 finding F-9, as ruled in the review of PR-39: the Host's own configuration is wrong,
    // and `Controller::new` stops on it. A domain the Host lacks is not a defect.
    let spans = [("a", "a:w/one@0.1.0"), ("b", "b:w/one@0.1.0")];
    // A composition that builds: no defect.
    assert!(
        registry()
            .with_composed("c:w/ok@0.1.0", &spans)
            .composition_defects()
            .is_empty()
    );
    // A domain that is not held: left out, no defect.
    let only_a = Domains::core()
        .with(holder(
            "a",
            vec![world("a:w/one@0.1.0", None, "a:w/events@0.1.0")],
        ))
        .with_composed("c:w/ok@0.1.0", &spans);
    assert!(only_a.composition_defects().is_empty());
    // A named world its domain does not have.
    let missing = registry().with_composed(
        "c:w/missing@0.1.0",
        &[("a", "a:w/nope@0.1.0"), ("b", "b:w/one@0.1.0")],
    );
    let defects = missing.composition_defects();
    assert_eq!(defects.len(), 1, "{defects:?}");
    assert!(defects[0].contains("c:w/missing@0.1.0") && defects[0].contains("a:w/nope@0.1.0"));
    // Parts that differ on one interface.
    let mut clash = world("b:w/one@0.1.0", Some("a:w/look@0.1.0"), "b:w/events@0.1.0");
    clash.imports[0].1[0].params.push(("more", Ty::U32));
    let differ = Domains::core()
        .with(holder(
            "a",
            vec![world(
                "a:w/one@0.1.0",
                Some("a:w/look@0.1.0"),
                "a:w/events@0.1.0",
            )],
        ))
        .with(holder("b", vec![clash]))
        .with_composed("c:w/differ@0.1.0", &spans);
    let defects = differ.composition_defects();
    assert_eq!(defects.len(), 1, "{defects:?}");
    assert!(defects[0].contains("c:w/differ@0.1.0") && defects[0].contains("a:w/look@0.1.0"));
}
