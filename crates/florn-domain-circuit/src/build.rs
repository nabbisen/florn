//! From the read scenario to the [`Plant`]: the unknowns in the order of RFC 013 rule 3, the
//! element of every built-in with the nodes of its pins and the signals it reads, and the nets
//! that have no direct-current path to ground (RFC 013 rule 9's list of the elements that conduct).

use std::collections::BTreeMap;

use florn_model::manifest::{Actor, ActorKind};
use florn_model::{Manifest, TickLength};

use crate::analyse::Member;
use crate::plant::{Device, Element, Net, Node, Plant, Signal, Unknown, UnknownKind};
use crate::profile::Profile;

/// The unknowns' counts, in the order of rule 3.
pub(crate) struct Layout {
    pub nodes: usize,
    pub branches: usize,
    /// Rule 3's unknowns: nodes, branches, states.
    pub transient: usize,
}

fn owns_branch(kind: &str) -> bool {
    matches!(kind, "voltage-source" | "inductor" | "ammeter" | "dc-motor")
}

pub(crate) fn unknowns(nets: &[Net], members: &[Member]) -> Layout {
    let nodes = nets.iter().filter(|n| !n.ground).count();
    let branches = members.iter().filter(|m| owns_branch(m.kind.name)).count();
    let states = members.iter().filter(|m| m.kind.name == "dc-motor").count();
    Layout {
        nodes,
        branches,
        transient: nodes + branches + states,
    }
}

/// What the build needs to find a pin's net and a port's signal.
pub(crate) struct Wiring<'a> {
    pub scenario: &'a Manifest,
    pub nets: &'a [Net],
    pub net_of_port: &'a BTreeMap<(u32, u32), usize>,
    pub signal_of_port: &'a BTreeMap<(u32, u32), usize>,
}

pub(crate) struct Built {
    pub plant: Plant,
    /// One net of every set that has no direct-current path to ground, the lowest identity.
    pub floating: Vec<u32>,
}

impl Wiring<'_> {
    /// The node of an actor's pin by name; the ground for a pin on the ground net.
    fn node(&self, actor: &Actor, pin: &str) -> Node {
        let port = actor.ports.iter().find(|p| p.name == pin)?;
        let net = *self.net_of_port.get(&(actor.id, port.id))?;
        self.node_of_net(net)
    }

    fn node_of_net(&self, net: usize) -> Node {
        if self.nets[net].ground {
            None
        } else {
            Some(self.nets[..net].iter().filter(|n| !n.ground).count())
        }
    }

    /// The identity of an actor's port by name.
    fn port_id(&self, actor: &Actor, port: &str) -> Option<u32> {
        actor.ports.iter().find(|p| p.name == port).map(|p| p.id)
    }

    fn signal(&self, actor: &Actor, port: &str) -> Option<usize> {
        let p = actor.ports.iter().find(|p| p.name == port)?;
        self.signal_of_port.get(&(actor.id, p.id)).copied()
    }
}

/// Build the plant from a scenario that has no error.
pub(crate) fn plant(
    w: &Wiring,
    members: &[Member],
    signals: Vec<Signal>,
    profile: Profile,
) -> Built {
    let layout = unknowns(w.nets, members);
    let mut unknown_list: Vec<Unknown> = w
        .nets
        .iter()
        .filter(|n| !n.ground)
        .map(|n| Unknown {
            name: n.name.clone(),
            kind: UnknownKind::Voltage,
        })
        .collect();
    let named = |kind: UnknownKind, keep: &dyn Fn(&Member) -> bool| -> Vec<Unknown> {
        members
            .iter()
            .filter(|m| keep(m))
            .map(|m| Unknown {
                name: m.actor.name.clone(),
                kind,
            })
            .collect()
    };
    unknown_list.extend(named(UnknownKind::Current, &|m| owns_branch(m.kind.name)));
    unknown_list.extend(named(UnknownKind::State, &|m| m.kind.name == "dc-motor"));
    unknown_list.extend(named(UnknownKind::Current, &|m| {
        m.kind.name == "capacitor" && m.params.opt("ic").is_some()
    }));

    let (mut branch, mut state, mut extra) = (
        layout.nodes,
        layout.nodes + layout.branches,
        layout.transient,
    );
    let mut devices = Vec::new();
    let mut dsu = Dsu::new(layout.nodes + 1);
    let ground = layout.nodes;
    let idx = |n: Node| n.unwrap_or(ground);
    for m in members {
        let a = m.actor;
        let p = &m.params;
        let pin = |s: &str| w.node(a, s);
        let mut conduct = |x: Node, y: Node| dsu.union(idx(x), idx(y));
        let element = match m.kind.name {
            "resistor" => {
                let (x, y) = (pin("a"), pin("b"));
                conduct(x, y);
                Element::Resistor {
                    a: x,
                    b: y,
                    g: 1.0 / p.real("r"),
                }
            }
            "capacitor" => {
                let (x, y) = (pin("a"), pin("b"));
                let ic = p.opt("ic");
                let dc = ic.map(|_| {
                    extra += 1;
                    extra - 1
                });
                if ic.is_some() {
                    conduct(x, y);
                }
                Element::Capacitor {
                    a: x,
                    b: y,
                    c: p.real("c"),
                    ic,
                    dc,
                }
            }
            "inductor" => {
                let (x, y) = (pin("a"), pin("b"));
                let ic = p.opt("ic");
                if ic.is_none() {
                    conduct(x, y);
                }
                branch += 1;
                Element::Inductor {
                    a: x,
                    b: y,
                    l: p.real("l"),
                    ic,
                    branch: branch - 1,
                }
            }
            "voltage-source" => {
                let (x, y) = (pin("p"), pin("n"));
                conduct(x, y);
                branch += 1;
                Element::VoltageSource {
                    p: x,
                    n: y,
                    v: p.real("v"),
                    gain: p.real("gain"),
                    cmd: w.signal(a, "cmd"),
                    branch: branch - 1,
                }
            }
            "current-source" => Element::CurrentSource {
                p: pin("p"),
                n: pin("n"),
                i: p.real("i"),
                gain: p.real("gain"),
                cmd: w.signal(a, "cmd"),
            },
            "diode" => {
                let (x, y) = (pin("a"), pin("k"));
                conduct(x, y);
                Element::Diode {
                    a: x,
                    k: y,
                    is: p.real("is"),
                    n: p.real("n"),
                    gmin: p.real("gmin"),
                }
            }
            "switch" => {
                let (x, y) = (pin("a"), pin("b"));
                conduct(x, y);
                Element::Switch {
                    a: x,
                    b: y,
                    ron: p.real("ron"),
                    roff: p.real("roff"),
                    threshold: p.real("threshold"),
                    ctl: w.signal(a, "ctl"),
                }
            }
            "voltmeter" => Element::Voltmeter {
                p: pin("p"),
                n: pin("n"),
                out: w.signal(a, "v"),
                out_port: w.port_id(a, "v"),
                period: p.ticks("period"),
                phase: p.ticks("phase"),
            },
            "ammeter" => {
                let (x, y) = (pin("a"), pin("b"));
                conduct(x, y);
                branch += 1;
                Element::Ammeter {
                    a: x,
                    b: y,
                    out: w.signal(a, "i"),
                    out_port: w.port_id(a, "i"),
                    period: p.ticks("period"),
                    phase: p.ticks("phase"),
                    branch: branch - 1,
                }
            }
            "constant" => {
                let out = w.signal(a, "y");
                let value = out.map_or(0.0, |s| signals[s].held);
                Element::Constant { out, value }
            }
            "comparator" => Element::Comparator {
                p: pin("p"),
                n: pin("n"),
                out: w.signal(a, "y"),
                threshold: p.real("threshold"),
                hysteresis: p.real("hysteresis"),
            },
            _ => {
                let (x, y) = (pin("a+"), pin(crate::kinds::MOTOR_NEGATIVE));
                conduct(x, y);
                branch += 1;
                state += 1;
                Element::Motor {
                    p: x,
                    n: y,
                    r: p.real("r"),
                    l: p.real("l"),
                    kt: p.real("kt"),
                    ke: p.real("ke"),
                    j: p.real("j"),
                    b: p.real("b"),
                    load: w.signal(a, "load"),
                    out: w.signal(a, "speed"),
                    out_port: w.port_id(a, "speed"),
                    period: p.ticks("period"),
                    phase: p.ticks("phase"),
                    branch: branch - 1,
                    state: state - 1,
                }
            }
        };
        devices.push(Device {
            actor: a.id,
            name: a.name.clone(),
            kind: m.kind.name,
            element,
        });
    }
    // The pins of a guest device conduct (rule 9): every pin of one actor is one set. The nodes of
    // the pins of every guest actor are kept, in ascending port identity (the order of a device's
    // trial and result, RFC 012 rule 19), for the one of them the start hook finds is a device.
    let mut module_pins = std::collections::BTreeMap::new();
    let mut module_names = std::collections::BTreeMap::new();
    for a in &w.scenario.actors {
        if matches!(a.kind, ActorKind::Module(_)) {
            let mut pins: Vec<(u32, Node)> = a
                .ports
                .iter()
                .filter(|p| p.r#type == crate::PIN)
                .filter_map(|p| {
                    w.net_of_port
                        .get(&(a.id, p.id))
                        .map(|&n| (p.id, w.node_of_net(n)))
                })
                .collect();
            pins.sort_by_key(|(id, _)| *id);
            let nodes: Vec<usize> = pins.iter().map(|(_, n)| idx(*n)).collect();
            for pair in nodes.windows(2) {
                dsu.union(pair[0], pair[1]);
            }
            module_pins.insert(
                a.id,
                pins.into_iter().map(|(_, n)| n).collect::<Vec<Node>>(),
            );
            module_names.insert(a.id, a.name.clone());
        }
    }
    let mut floating = Vec::new();
    let mut seen = Vec::new();
    for (k, net) in w.nets.iter().filter(|n| !n.ground).enumerate() {
        let root = dsu.find(k);
        if root != dsu.find(ground) && !seen.contains(&root) {
            seen.push(root);
            floating.push(net.id);
        }
    }
    let plant = Plant {
        profile,
        tick: match w.scenario.time.tick {
            TickLength::Ns => 1e-9,
            TickLength::Ps => 1e-12,
            TickLength::Fs => 1e-15,
        },
        nets: w.nets.iter().filter(|n| !n.ground).cloned().collect(),
        signals,
        devices,
        unknowns: unknown_list,
        transient_unknowns: layout.transient,
        port_signal: w.signal_of_port.clone(),
        module_pins,
        module_names,
        guest: None,
        end: w.scenario.time.end,
        world: w.scenario.world_identity(crate::NAME).unwrap_or(0),
    };
    Built { plant, floating }
}

/// Disjoint sets over the nodes, the ground last.
struct Dsu(Vec<usize>);

impl Dsu {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}
