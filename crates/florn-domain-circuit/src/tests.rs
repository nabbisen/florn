//! The domain as the Host composes it, and the helpers the tests of the modules share: a deck of
//! built-ins and connections written as the manifest text of RFC 004, read by the real parser.

use florn_control::Notification;
use florn_model::{HostMaxima, Manifest};

use crate::analyse::{Analysis, analyse};
use crate::kinds::{self, Role};
use crate::plant::Plant;
use crate::profile::Limits;

/// A scenario under construction: actors, nets and signals, and a `[circuit]` table.
#[derive(Clone, Debug)]
pub(crate) struct Deck {
    actors: String,
    /// (name, type, pins, attributes) in the order written.
    connections: Vec<(String, String, Vec<String>, String)>,
    profile: String,
    observed: bool,
    guests: bool,
    pub limits: Limits,
}

impl Deck {
    pub fn new() -> Self {
        Self {
            actors: String::new(),
            connections: Vec::new(),
            profile: String::new(),
            observed: false,
            guests: false,
            limits: Limits::native(),
        }
    }

    /// A built-in with the ports of its kind but its optional ones (`cmd`, `load`).
    pub fn builtin(self, id: u32, name: &str, kind: &str, config: &str) -> Self {
        self.ported(id, name, kind, config, false)
    }

    /// A built-in with every port of its kind, the optional signal inputs included.
    pub fn builtin_opt(self, id: u32, name: &str, kind: &str, config: &str) -> Self {
        self.ported(id, name, kind, config, true)
    }

    fn ported(self, id: u32, name: &str, kind: &str, config: &str, optional: bool) -> Self {
        let k = kinds::by_builtin(&format!("circuit/{kind}")).expect("a kind of the table");
        let ports: Vec<(String, &str)> = k
            .ports
            .iter()
            .filter(|p| optional || !p.optional)
            .map(|p| (p.name.to_string(), p.role.port_type()))
            .collect();
        let ports: Vec<(&str, &str)> = ports.iter().map(|(n, t)| (n.as_str(), *t)).collect();
        self.builtin_with(id, name, kind, &ports, config)
    }

    /// A built-in with the ports given: a name and a type each.
    pub fn builtin_with(
        mut self,
        id: u32,
        name: &str,
        kind: &str,
        ports: &[(&str, &str)],
        config: &str,
    ) -> Self {
        let list: Vec<String> = ports
            .iter()
            .enumerate()
            .map(|(i, (n, t))| format!("{{ id = {i}, name = \"{n}\", type = \"{t}\" }}"))
            .collect();
        self.actors.push_str(&format!(
            "\n[[actors]]\nid = {id}\nname = \"{name}\"\nkind = \"builtin\"\nbuiltin = \"circuit/{kind}\"\nports = [{}]\n[actors.config]\n{config}\n",
            list.join(", ")
        ));
        self
    }

    /// A guest actor (a module of the scenario) with the ports given: a name and a type each. The
    /// domain reads its ports and signals only; its module is never admitted in these tests.
    pub fn guest(mut self, id: u32, name: &str, ports: &[(&str, &str)]) -> Self {
        let list: Vec<String> = ports
            .iter()
            .enumerate()
            .map(|(i, (n, t))| format!("{{ id = {i}, name = \"{n}\", type = \"{t}\" }}"))
            .collect();
        self.actors.push_str(&format!(
            "\n[[actors]]\nid = {id}\nname = \"{name}\"\nkind = \"module\"\nmodule = \"guest\"\nports = [{}]\n",
            list.join(", ")
        ));
        self.guests = true;
        self
    }

    fn connection(mut self, name: &str, ty: &str, pins: &[&str], attrs: &str) -> Self {
        self.connections.push((
            name.to_string(),
            ty.to_string(),
            pins.iter().map(|p| p.to_string()).collect(),
            attrs.to_string(),
        ));
        self
    }

    /// Add a voltmeter `vm` across the first net and the ground, so that the plant is observed
    /// (RFC 012 rule 28: `circuit.plant-unobserved`) and the deck builds.
    pub fn observed(mut self) -> Self {
        self.observed = true;
        self
    }

    /// A net joining the pins (`"actor.port"`).
    pub fn net(self, name: &str, pins: &[&str]) -> Self {
        self.connection(name, "circuit/net", pins, "")
    }

    /// The ground net.
    pub fn ground(self, name: &str, pins: &[&str]) -> Self {
        self.connection(name, "circuit/net", pins, "ground = true")
    }

    /// A net with attributes as TOML lines.
    pub fn net_with(self, name: &str, pins: &[&str], attrs: &str) -> Self {
        self.connection(name, "circuit/net", pins, attrs)
    }

    /// A signal from its one output to its inputs.
    pub fn signal(self, name: &str, ports: &[&str], attrs: &str) -> Self {
        self.connection(name, "circuit/signal", ports, attrs)
    }

    /// The `[circuit]` table.
    pub fn profile(mut self, text: &str) -> Self {
        self.profile = text.to_string();
        self
    }

    fn rendered(&self) -> (String, String) {
        let mut actors = self.actors.clone();
        let mut connections = self.connections.clone();
        if self.observed {
            actors.push_str(
                "\n[[actors]]\nid = 900\nname = \"vm\"\nkind = \"builtin\"\nbuiltin = \"circuit/voltmeter\"\nports = [{ id = 0, name = \"p\", type = \"circuit/pin\" }, { id = 1, name = \"n\", type = \"circuit/pin\" }, { id = 2, name = \"v\", type = \"circuit/signal-out\" }]\n[actors.config]\nperiod = \"1ms\"\n",
            );
            let first = connections
                .iter()
                .position(|c| c.1 == "circuit/net" && !c.3.contains("ground = true"));
            let ground = connections
                .iter()
                .position(|c| c.3.contains("ground = true"));
            if let (Some(f), Some(g)) = (first, ground) {
                connections[f].2.push("vm.p".into());
                connections[g].2.push("vm.n".into());
            }
        }
        let mut text = String::new();
        for (i, (name, ty, pins, attrs)) in connections.iter().enumerate() {
            let ports: Vec<String> = pins.iter().map(|p| format!("\"{p}\"")).collect();
            let attrs = if attrs.is_empty() {
                String::new()
            } else {
                format!("[connections.attrs]\n{attrs}\n")
            };
            text.push_str(&format!(
                "\n[[connections]]\nid = {}\nname = \"{name}\"\ntype = \"{ty}\"\nports = [{}]\n{attrs}",
                100 + i,
                ports.join(", ")
            ));
        }
        (actors, text)
    }

    pub fn text(&self) -> String {
        let (actors, connections) = self.rendered();
        let modules = if self.guests {
            format!(
                "\n[modules.guest]\nsha256 = \"{}\"\ncontract = \"florn:circuit/controller@0.1.0\"\n",
                "0".repeat(64)
            )
        } else {
            String::new()
        };
        format!(
            "florn = 1\nname = \"deck\"\ndomains = [\"core\", \"circuit\"]\n\n[time]\ntick = \"1ns\"\nend = \"10ms\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 1\n\n[limits.default]\nmemory = \"2MiB\"\n{modules}\n[circuit]\n{}\n{}{}",
            self.profile, actors, connections
        )
    }

    pub fn manifest(&self) -> Manifest {
        let text = self.text();
        Manifest::parse(&text, &HostMaxima::native())
            .unwrap_or_else(|e| panic!("the deck is a manifest: {e:?}\n{text}"))
    }

    pub fn analyse(&self) -> Analysis {
        analyse(&self.manifest(), self.limits)
    }

    pub fn notes(&self) -> Vec<Notification> {
        self.analyse().notes
    }

    /// The codes of every notification, in order.
    pub fn codes(&self) -> Vec<String> {
        self.notes().iter().map(|n| n.code.clone()).collect()
    }

    /// The plant of a deck that has no error.
    pub fn plant(&self) -> Plant {
        let a = self.analyse();
        a.plant
            .unwrap_or_else(|| panic!("the deck has errors: {:?}", a.notes))
    }
}

/// Every pin of a kind's row, for decks that wire a device by hand.
#[allow(dead_code)]
pub(crate) fn pins_of(kind: &str) -> Vec<&'static str> {
    kinds::by_builtin(&format!("circuit/{kind}"))
        .unwrap()
        .ports
        .iter()
        .filter(|p| p.role == Role::Pin)
        .map(|p| p.name)
        .collect()
}

/// A resistor divider: 10 V across 1 kohm over 3 kohm, the middle net `mid`. Its operating point
/// is a hand solution: `top` 10 V, `mid` 7.5 V, the source's current −2.5 mA (it supplies it).
pub(crate) fn divider() -> Deck {
    Deck::new()
        .builtin(1, "v1", "voltage-source", "v = \"10V\"")
        .builtin(2, "r1", "resistor", "r = \"1kohm\"")
        .builtin(3, "r2", "resistor", "r = \"3kohm\"")
        .builtin(4, "vm", "voltmeter", "period = \"1ms\"")
        .net("top", &["v1.p", "r1.a"])
        .net("mid", &["r1.b", "r2.a", "vm.p"])
        .ground("gnd", &["v1.n", "r2.b", "vm.n"])
}

// The domain as the Host reads it (RFC 008 rule 1).

mod as_a_domain {
    use florn_admission::manifest::{ModuleManifest, PortKind, Resources};
    use florn_control::Value;
    use florn_host::FaultPolicy;
    use florn_host::domain::Domain;
    use florn_model::manifest::{DomainMap, DomainValue};

    use super::{Deck, divider};
    use crate::{CircuitDomain, Limits, kinds};

    fn composed(deck: &Deck) -> CircuitDomain {
        let mut d = CircuitDomain::new(Limits::native());
        d.compose(&deck.manifest(), 1);
        d
    }

    fn module(table: &[(&str, &str)], ports: &[(&str, u64, u64)]) -> ModuleManifest {
        let mut domain_tables = std::collections::BTreeMap::new();
        let map: DomainMap = table
            .iter()
            .map(|(k, v)| (k.to_string(), DomainValue::Text(v.to_string())))
            .collect();
        domain_tables.insert("circuit".to_string(), map);
        ModuleManifest {
            world: "florn:circuit/device@0.1.0".into(),
            kind: "device".into(),
            description: None,
            sdk: None,
            ports: ports
                .iter()
                .map(|(ty, min, max)| PortKind {
                    kind: "pins".into(),
                    ty: ty.to_string(),
                    min: *min,
                    max: *max,
                })
                .collect(),
            params: Vec::new(),
            telemetry: Vec::new(),
            resources: Resources::default(),
            domain_tables,
        }
    }

    #[test]
    fn it_owns_the_types_the_kinds_and_the_classes_of_rfc_012() {
        let d = CircuitDomain::default();
        assert_eq!(d.name(), "circuit");
        assert_eq!(d.connection_types(), ["circuit/net", "circuit/signal"]);
        assert_eq!(
            d.port_types(),
            ["circuit/pin", "circuit/signal-out", "circuit/signal-in"]
        );
        assert_eq!(d.builtin_kinds().len(), 12);
        for k in kinds::KINDS {
            assert!(d.builtin_kinds().contains(&k.full_name()), "{}", k.name);
            assert_eq!(
                kinds::by_builtin(k.full_name()).map(|x| x.name),
                Some(k.name)
            );
        }
        assert_eq!(
            d.classes(),
            [(6, "advance"), (7, "sample"), (8, "condition")]
        );
        assert_eq!(d.units().len(), 95);
    }

    #[test]
    fn the_channels_of_the_builtins_are_frozen() {
        // RFC 012 rule 24: `probe` is channel 1 and `condition` is 2, for every kind that has them.
        let d = CircuitDomain::default();
        for kind in ["voltmeter", "ammeter", "dc-motor"] {
            assert_eq!(
                d.builtin_channels(&format!("circuit/{kind}")),
                [1],
                "{kind}"
            );
        }
        assert_eq!(d.builtin_channels("circuit/comparator"), [2]);
        assert!(d.builtin_channels("circuit/resistor").is_empty());
        let telemetry = d.telemetry();
        let layout = |name: &str| {
            telemetry
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap()
                .1
                .clone()
        };
        assert_eq!(layout("probe"), [("value", "f64")]);
        assert_eq!(layout("condition"), [("output", "f64"), ("located", "u64")]);
        assert_eq!(
            layout("control"),
            [("input", "f64"), ("output", "f64"), ("error", "f64")]
        );
    }

    #[test]
    fn a_clean_scenario_validates_with_nothing_and_composes_a_plant() {
        let deck = divider();
        let d = composed(&deck);
        assert!(d.validate(&deck.manifest()).is_empty());
        assert_eq!(d.plant().unwrap().size(), 3);
        // A scenario with an error composes no plant.
        let bad = divider().profile("colour = 1");
        assert!(composed(&bad).plant().is_none());
    }

    #[test]
    fn the_profile_gives_the_policy_and_the_tolerance_after_compose() {
        let d = composed(&divider().profile(
            "faulted-policy = \"stop-run\"\ntolerance.atol = 1e-6\ntolerance.rtol = 1e-3",
        ));
        assert_eq!(d.faulted_policy(), FaultPolicy::StopRun);
        let t = d.tolerance().unwrap();
        assert_eq!((t.atol, t.rtol), (1e-6, 1e-3));
        let d = composed(&divider());
        assert_eq!(d.faulted_policy(), FaultPolicy::Drop);
        let t = d.tolerance().unwrap();
        assert_eq!((t.atol, t.rtol), (1e-9, 1e-6));
        // Before a plant there is nothing to compare.
        assert!(CircuitDomain::default().tolerance().is_none());
    }

    #[test]
    fn a_sensor_is_of_a_continuous_class_and_a_resistor_is_not() {
        let d = composed(&divider());
        assert!(d.continuous_actor(4), "the voltmeter");
        assert!(!d.continuous_actor(2), "a resistor");
        assert!(!d.continuous_actor(99));
    }

    #[test]
    fn a_module_table_with_a_kind_the_domain_lacks_is_kind_unknown() {
        let d = CircuitDomain::default();
        let n = d.validate_module("m", &module(&[("kind", "gadget")], &[]));
        assert_eq!(n[0].code, "circuit.kind-unknown");
        assert_eq!(n[0].params[1].1, Value::Text("gadget".into()));
        assert!(
            d.validate_module("m", &module(&[("kind", "controller")], &[]))
                .is_empty()
        );
        assert_eq!(
            d.validate_module("m", &module(&[], &[]))[0].code,
            "circuit.kind-unknown"
        );
    }

    #[test]
    fn a_device_module_declares_exactly_one_pin_kind_of_two_to_sixteen_pins() {
        let d = CircuitDomain::default();
        let table = [("kind", "device")];
        assert!(
            d.validate_module("m", &module(&table, &[("circuit/pin", 2, 2)]))
                .is_empty()
        );
        assert!(
            d.validate_module("m", &module(&table, &[("circuit/pin", 2, 16)]))
                .is_empty()
        );
        for ports in [
            vec![],
            vec![("circuit/pin", 1, 2)],
            vec![("circuit/pin", 2, 17)],
            vec![("circuit/pin", 2, 2), ("circuit/pin", 2, 2)],
            vec![("circuit/signal-in", 1, 1)],
        ] {
            let n = d.validate_module("m", &module(&table, &ports));
            assert_eq!(n.len(), 1, "{ports:?}");
            assert_eq!(n[0].code, "circuit.device-ports");
        }
        let n = d.validate_module(
            "m",
            &module(&table, &[("circuit/pin", 2, 2), ("circuit/pin", 2, 2)]),
        );
        assert_eq!(n[0].params[1].1, Value::Count(2));
    }

    #[test]
    fn the_index_of_a_series_is_the_position_among_the_declared_f64_fields() {
        // RFC 014 rule 3: each `f64` field the domain draws has its index among the actor's declared
        // `f64` fields, in channel order and then field order. A built-in declares the channels of
        // its kind, so the index is computed from the layouts the domain publishes.
        let d = composed(&divider());
        let value = 3.25f64.to_le_bytes().to_vec();
        assert_eq!(d.display_series(4, 1, &value), [(0, 3.25)]);
        // A payload of the wrong channel, or too short, draws nothing; neither does a resistor.
        assert!(d.display_series(4, 2, &value).is_empty());
        assert!(d.display_series(4, 1, &value[..4]).is_empty());
        assert!(d.display_series(2, 1, &value).is_empty());
        assert!(d.display_series(99, 1, &value).is_empty());
        // The comparator's `condition` is an output (f64) then a located tick (not drawn).
        let cmp = Deck::new()
            .builtin(1, "v1", "voltage-source", "v = \"1V\"")
            .builtin(2, "c1", "comparator", "")
            .net("a", &["v1.p", "c1.p"])
            .ground("g", &["v1.n", "c1.n"]);
        let d = composed(&cmp);
        let mut payload = 1.0f64.to_le_bytes().to_vec();
        payload.extend(77u64.to_le_bytes());
        assert_eq!(d.display_series(2, 2, &payload), [(0, 1.0)]);
        // The count of f64 fields per declared channel agrees with `telemetry()`.
        let telemetry = d.telemetry();
        for (kind, channels) in [
            ("voltmeter", vec!["probe"]),
            ("comparator", vec!["condition"]),
        ] {
            let declared = d.builtin_channels(&format!("circuit/{kind}"));
            assert_eq!(declared.len(), channels.len());
            let f64s: usize = channels
                .iter()
                .map(|c| {
                    telemetry
                        .iter()
                        .find(|(n, _)| n == c)
                        .unwrap()
                        .1
                        .iter()
                        .filter(|(_, t)| *t == "f64")
                        .count()
                })
                .sum();
            assert!(f64s >= 1);
        }
    }
}
