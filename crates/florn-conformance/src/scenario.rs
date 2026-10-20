//! A scenario over the kit's guests, written as RFC 004's manifest text so that the same
//! scenario runs under the Host here and through the headless runner from a file.

use std::collections::BTreeMap;

use florn_model::{HostMaxima, Manifest};

use crate::guests;

/// One actor: its identity, the guest (by example name), its ports, and its configuration as
/// TOML values.
#[derive(Clone, Debug)]
pub struct Actor {
    pub id: u32,
    pub module: &'static str,
    pub ports: u32,
    pub config: Vec<(&'static str, String)>,
}

pub fn actor(id: u32, module: &'static str, ports: u32) -> Actor {
    Actor {
        id,
        module,
        ports,
        config: Vec::new(),
    }
}

impl Actor {
    /// A configuration value, as TOML text (`"16"`, `"true"`, `"\"name\""`).
    pub fn with(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.config.push((key, value.into()));
        self
    }
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
    pub actors: Vec<Actor>,
    /// Connections as lists of (actor, port).
    pub connections: Vec<Vec<(u32, u32)>>,
    /// Modules given as bytes rather than read from the built guests (the kit's text guests).
    pub extra: BTreeMap<&'static str, Vec<u8>>,
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
            actors: Vec::new(),
            connections: Vec::new(),
            extra: BTreeMap::new(),
        }
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

    pub fn connect(mut self, ports: &[(u32, u32)]) -> Self {
        self.connections.push(ports.to_vec());
        self
    }

    /// The distinct module names, in first-use order.
    pub fn module_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Vec::new();
        for a in &self.actors {
            if !names.contains(&a.module) {
                names.push(a.module);
            }
        }
        names
    }

    /// The manifest text, with the module hashes given.
    pub fn text(&self, hashes: &BTreeMap<String, String>) -> String {
        let mut t = format!(
            "florn = 1\nname = \"{}\"\ndomains = [\"core\"]\n\n[time]\ntick = \"1ns\"\nend = \"{}ns\"\n\n[order]\ntie = \"{}\"\n",
            self.name, self.end, self.tie
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
            t.push_str(&format!(
                "\n[modules.{n}]\nsha256 = \"{hash}\"\ncontract = \"florn:core/guest@0.1.0\"\n"
            ));
        }
        for a in &self.actors {
            let ports: Vec<String> = (0..a.ports)
                .map(|p| format!("{{ id = {p}, name = \"p{p}\", type = \"core/port\" }}"))
                .collect();
            t.push_str(&format!(
                "\n[[actors]]\nid = {}\nname = \"a{}\"\nkind = \"module\"\nmodule = \"{}\"\nports = [{}]\n",
                a.id,
                a.id,
                a.module,
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
            let ports: Vec<String> = c.iter().map(|(a, p)| format!("\"a{a}.p{p}\"")).collect();
            t.push_str(&format!(
                "\n[[connections]]\nid = {}\nname = \"c{i}\"\nports = [{}]\n",
                1000 + i as u32,
                ports.join(", ")
            ));
        }
        t
    }

    /// The hashes of the built guests and of the extra modules.
    pub fn hashes(&self) -> BTreeMap<String, String> {
        let mut h = guests::hashes();
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
                let bytes = self
                    .extra
                    .get(n)
                    .cloned()
                    .unwrap_or_else(|| guests::bytes(n));
                (n.to_string(), bytes)
            })
            .collect()
    }
}
