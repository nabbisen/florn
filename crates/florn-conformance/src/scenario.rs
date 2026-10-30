//! A scenario over the kit's guests, written as RFC 004's manifest text so that the same
//! scenario runs under the Host here and through the headless runner from a file.

use std::collections::BTreeMap;

use florn_model::{HostMaxima, Manifest};

use crate::guests;

/// The contract a guest example declares: the router world for the network guests, the core
/// world for the rest.
pub fn contract_of(module: &str) -> &'static str {
    match module {
        "net_echo" | "distance_vector" | "ospf" | "bgp" => "florn:network/router@0.1.0",
        _ => "florn:core/guest@0.1.0",
    }
}

/// One actor: its identity, the guest (by example name), its ports, and its configuration as
/// TOML values.
#[derive(Clone, Debug)]
pub struct Actor {
    pub id: u32,
    /// The guest's example name, or the built-in kind when `builtin` is set.
    pub module: &'static str,
    pub builtin: bool,
    pub ports: u32,
    /// The type of every port: `core/port`, or a domain's.
    pub port_type: &'static str,
    pub config: Vec<(&'static str, String)>,
}

pub fn actor(id: u32, module: &'static str, ports: u32) -> Actor {
    Actor {
        id,
        module,
        builtin: false,
        ports,
        port_type: "core/port",
        config: Vec::new(),
    }
}

/// A router of the network domain (RFC 008): a guest of the router world over interfaces.
pub fn router(id: u32, module: &'static str, interfaces: u32) -> Actor {
    Actor {
        port_type: "network/interface",
        ..actor(id, module, interfaces)
    }
}

/// A built-in actor of a domain, over ports of `port_type`.
pub fn builtin(id: u32, kind: &'static str, port_type: &'static str, ports: u32) -> Actor {
    Actor {
        builtin: true,
        port_type,
        ..actor(id, kind, ports)
    }
}

impl Actor {
    /// A configuration value, as TOML text (`"16"`, `"true"`, `"\"name\""`).
    pub fn with(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.config.push((key, value.into()));
        self
    }
}

/// Attribute lines: a key and its value as TOML text.
pub type Attrs = Vec<(&'static str, String)>;

/// A connection: its ports, its type when it has one, its attributes as TOML lines.
#[derive(Clone, Debug)]
pub struct Connection {
    pub ports: Vec<(u32, u32)>,
    pub r#type: Option<&'static str>,
    pub attrs: Attrs,
    pub per_port: Vec<((u32, u32), Attrs)>,
}

/// A scheduled change: the tick, the connection's index in the list, the payload lines.
#[derive(Clone, Debug)]
pub struct Change {
    pub at: u64,
    pub connection: usize,
    pub payload: Vec<(&'static str, String)>,
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: String,
    pub end: u64,
    pub tie: &'static str,
    pub seed: u64,
    /// `[limits.default]` entries as TOML text.
    pub limits: Vec<(&'static str, String)>,
    /// `[order]` entries besides `tie`.
    pub order: Vec<(&'static str, u64)>,
    /// The domains declared, `core` first.
    pub domains: Vec<&'static str>,
    pub actors: Vec<Actor>,
    pub connections: Vec<Connection>,
    pub changes: Vec<Change>,
    /// Modules given as bytes rather than read from the built guests (the kit's text guests).
    pub extra: BTreeMap<&'static str, Vec<u8>>,
    /// Whether the modules are the shipped ones of `scenarios/modules` rather than a fresh build.
    pub shipped: bool,
}

impl Scenario {
    pub fn new(name: &str, end: u64) -> Self {
        Self {
            name: name.to_string(),
            end,
            tie: "by-identity",
            seed: 1,
            limits: vec![("memory", "\"2MiB\"".into())],
            order: Vec::new(),
            domains: vec!["core"],
            actors: Vec::new(),
            connections: Vec::new(),
            changes: Vec::new(),
            extra: BTreeMap::new(),
            shipped: false,
        }
    }

    /// Over the shipped modules of `scenarios/modules`.
    pub fn shipped(mut self) -> Self {
        self.shipped = true;
        self
    }

    /// A module by bytes, under a name the actors use.
    pub fn module(mut self, name: &'static str, bytes: Vec<u8>) -> Self {
        self.extra.insert(name, bytes);
        self
    }

    pub fn tie(mut self, tie: &'static str) -> Self {
        self.tie = tie;
        self
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    pub fn limit(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.limits.retain(|(k, _)| *k != key);
        self.limits.push((key, value.into()));
        self
    }

    pub fn order(mut self, key: &'static str, value: u64) -> Self {
        self.order.push((key, value));
        self
    }

    pub fn actor(mut self, a: Actor) -> Self {
        self.actors.push(a);
        self
    }

    /// A core connection over (actor, port) pairs.
    pub fn connect(mut self, ports: &[(u32, u32)]) -> Self {
        self.connections.push(Connection {
            ports: ports.to_vec(),
            r#type: None,
            attrs: Vec::new(),
            per_port: Vec::new(),
        });
        self
    }

    /// A domain declared beside the core (RFC 004 rule 5).
    pub fn domain(mut self, name: &'static str) -> Self {
        if !self.domains.contains(&name) {
            self.domains.push(name);
        }
        self
    }

    /// A typed connection with attribute lines (`("delay", "\"20ns\"")`).
    pub fn link(
        mut self,
        r#type: &'static str,
        ports: &[(u32, u32)],
        attrs: &[(&'static str, &str)],
    ) -> Self {
        self.connections.push(Connection {
            ports: ports.to_vec(),
            r#type: Some(r#type),
            attrs: attrs.iter().map(|(k, v)| (*k, v.to_string())).collect(),
            per_port: Vec::new(),
        });
        self
    }

    /// Per-port attributes of the last connection.
    pub fn per_port(mut self, port: (u32, u32), attrs: &[(&'static str, &str)]) -> Self {
        let c = self.connections.last_mut().expect("a connection");
        c.per_port.push((
            port,
            attrs.iter().map(|(k, v)| (*k, v.to_string())).collect(),
        ));
        self
    }

    /// A scheduled change of the last connection at `at` ticks.
    pub fn change(mut self, at: u64, payload: &[(&'static str, &str)]) -> Self {
        self.changes.push(Change {
            at,
            connection: self.connections.len() - 1,
            payload: payload.iter().map(|(k, v)| (*k, v.to_string())).collect(),
        });
        self
    }

    /// The distinct module names, in first-use order.
    pub fn module_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Vec::new();
        for a in &self.actors {
            if a.builtin {
                continue;
            }
            if !names.contains(&a.module) {
                names.push(a.module);
            }
        }
        names
    }

    /// The manifest text, with the module hashes given.
    pub fn text(&self, hashes: &BTreeMap<String, String>) -> String {
        let domains: Vec<String> = self.domains.iter().map(|d| format!("\"{d}\"")).collect();
        let mut t = format!(
            "florn = 1\nname = \"{}\"\ndomains = [{}]\n\n[time]\ntick = \"1ns\"\nend = \"{}ns\"\n\n[order]\ntie = \"{}\"\n",
            self.name,
            domains.join(", "),
            self.end,
            self.tie
        );
        for (k, v) in &self.order {
            t.push_str(&format!("{k} = {v}\n"));
        }
        t.push_str(&format!("\n[seed]\nroot = {}\n", self.seed));
        if !self.limits.is_empty() {
            t.push_str("\n[limits.default]\n");
            for (k, v) in &self.limits {
                t.push_str(&format!("{k} = {v}\n"));
            }
        }
        for n in self.module_names() {
            let hash = hashes
                .get(n)
                .unwrap_or_else(|| panic!("no hash for module {n}"));
            let contract = contract_of(n);
            t.push_str(&format!(
                "\n[modules.{n}]\nsha256 = \"{hash}\"\ncontract = \"{contract}\"\n"
            ));
        }
        for a in &self.actors {
            let ports: Vec<String> = (0..a.ports)
                .map(|p| {
                    format!(
                        "{{ id = {p}, name = \"p{p}\", type = \"{}\" }}",
                        a.port_type
                    )
                })
                .collect();
            let kind = if a.builtin {
                format!("kind = \"builtin\"\nbuiltin = \"{}\"", a.module)
            } else {
                format!("kind = \"module\"\nmodule = \"{}\"", a.module)
            };
            t.push_str(&format!(
                "\n[[actors]]\nid = {}\nname = \"a{}\"\n{kind}\nports = [{}]\n",
                a.id,
                a.id,
                ports.join(", ")
            ));
            if !a.config.is_empty() {
                t.push_str("[actors.config]\n");
                for (k, v) in &a.config {
                    t.push_str(&format!("{k} = {v}\n"));
                }
            }
        }
        for (i, c) in self.connections.iter().enumerate() {
            let ports: Vec<String> = c
                .ports
                .iter()
                .map(|(a, p)| format!("\"a{a}.p{p}\""))
                .collect();
            let ty = c
                .r#type
                .map(|ty| format!("type = \"{ty}\"\n"))
                .unwrap_or_default();
            t.push_str(&format!(
                "\n[[connections]]\nid = {}\nname = \"c{i}\"\n{ty}ports = [{}]\n",
                1000 + i as u32,
                ports.join(", ")
            ));
            if !c.attrs.is_empty() {
                t.push_str("[connections.attrs]\n");
                for (k, v) in &c.attrs {
                    t.push_str(&format!("{k} = {v}\n"));
                }
            }
            if !c.per_port.is_empty() {
                t.push_str("[connections.attrs.per-port]\n");
                for ((a, p), attrs) in &c.per_port {
                    let lines: Vec<String> =
                        attrs.iter().map(|(k, v)| format!("{k} = {v}")).collect();
                    t.push_str(&format!("\"a{a}.p{p}\" = {{ {} }}\n", lines.join(", ")));
                }
            }
        }
        for ch in &self.changes {
            t.push_str(&format!(
                "\n[[changes]]\nat = \"{}ns\"\ntarget = \"c{}\"\n[changes.payload]\n",
                ch.at, ch.connection
            ));
            for (k, v) in &ch.payload {
                t.push_str(&format!("{k} = {v}\n"));
            }
        }
        t
    }

    /// The hashes of the built guests and of the extra modules.
    pub fn hashes(&self) -> BTreeMap<String, String> {
        let mut h = if self.shipped {
            crate::shipped::module_hashes()
        } else {
            guests::hashes()
        };
        for (n, b) in &self.extra {
            h.insert(n.to_string(), florn_host::run::sha256_hex(b));
        }
        h
    }

    /// The parsed manifest over the built guests' hashes.
    pub fn manifest(&self) -> Manifest {
        let text = self.text(&self.hashes());
        Manifest::parse(&text, &HostMaxima::native())
            .unwrap_or_else(|e| panic!("scenario {}: {:?}\n{text}", self.name, e.codes()))
    }

    /// The parsed manifest, or the refusal codes.
    pub fn try_manifest(&self) -> Result<Manifest, Vec<&'static str>> {
        Manifest::parse(&self.text(&self.hashes()), &HostMaxima::native()).map_err(|e| e.codes())
    }

    /// The module bytes by module name, from the built guests.
    pub fn modules(&self) -> BTreeMap<String, Vec<u8>> {
        self.module_names()
            .into_iter()
            .map(|n| {
                let bytes = self.extra.get(n).cloned().unwrap_or_else(|| {
                    if self.shipped {
                        crate::shipped::module_bytes(n)
                    } else {
                        guests::bytes(n)
                    }
                });
                (n.to_string(), bytes)
            })
            .collect()
    }
}
