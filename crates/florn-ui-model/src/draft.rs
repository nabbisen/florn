//! Authoring (external design §3.4): a scenario being written, as fields, and its manifest
//! text (RFC 004 §3). The draft is loaded through the control interface like any manifest,
//! and the Host's validation comes back as notifications that attach to the fields they
//! name; a scenario with errors cannot run, and the run control says why. The draft itself
//! checks only what a field can tell on its own.
use florn_control::Notification;
use florn_control::message::{Fields, ModuleSummary, ScenarioSummary};

#[cfg(test)]
use crate::fields::Problem;
use crate::fields::{self, Field, Kind};
use crate::inspector::{kind_of, kind_of_text};

/// A module the draft may place: named, by hash, with its contract and what it declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftModule {
    pub name: String,
    pub sha256: String,
    pub contract: String,
    pub summary: Option<ModuleSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftActor {
    pub id: u32,
    pub name: Field,
    pub module: String,
    /// (id, name, type)
    pub ports: Vec<(u32, String, String)>,
    pub config: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftConnection {
    pub id: u32,
    pub name: Field,
    pub ty: String,
    pub ports: Vec<(u32, u32)>,
    pub attrs: Vec<Field>,
    /// `[connections.attrs.per-port]` as text, by (actor, port), kept as loaded.
    pub per_port: Vec<((u32, u32), Fields)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftChange {
    pub at: Field,
    pub target: u32,
    pub payload: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub name: Field,
    pub tick: Field,
    pub end: Field,
    pub tie: String,
    pub seed: Field,
    /// `[limits.<class>]` as text, by class; a new draft has the default class's memory.
    pub limits: Vec<(String, Vec<(String, String)>)>,
    pub modules: Vec<DraftModule>,
    pub actors: Vec<DraftActor>,
    pub connections: Vec<DraftConnection>,
    pub changes: Vec<DraftChange>,
    /// The Host's refusals of the last load that name no field, and every one in order.
    pub errors: Vec<Notification>,
    next_actor: u32,
    next_connection: u32,
}

pub const FIRST_CONNECTION_ID: u32 = 1000;

impl Default for Draft {
    fn default() -> Self {
        Self::new("scenario")
    }
}

impl Draft {
    /// An empty scenario: every scenario has an `end` (RFC 004 rule 14), asked for at once.
    pub fn new(name: &str) -> Self {
        Self {
            name: Field::new("name", Kind::Name, name),
            tick: Field::new("tick", Kind::Quantity, "1ns"),
            end: Field::new("end", Kind::Quantity, ""),
            tie: "by-identity".into(),
            seed: Field::new("root", Kind::Integer, "1"),
            limits: vec![("default".into(), vec![("memory".into(), "2MiB".into())])],
            modules: Vec::new(),
            actors: Vec::new(),
            connections: Vec::new(),
            changes: Vec::new(),
            errors: Vec::new(),
            next_actor: 1,
            next_connection: FIRST_CONNECTION_ID,
        }
    }

    /// The draft of a loaded scenario, to edit it.
    pub fn from_scenario(s: &ScenarioSummary) -> Self {
        let mut d = Self::new(&s.name);
        d.tick = Field::new("tick", Kind::Quantity, &s.tick);
        d.end = Field::new(
            "end",
            Kind::Quantity,
            &format!("{}{}", s.end, unit_of(&s.tick)),
        );
        d.tie = s.tie.clone();
        d.seed = Field::new("root", Kind::Integer, &s.seed.to_string());
        d.limits = s.limits.clone();
        for m in &s.modules {
            d.modules.push(DraftModule {
                name: m.name.clone(),
                sha256: m.source_sha256.clone(),
                contract: m.contract.clone(),
                summary: Some(m.clone()),
            });
        }
        for a in &s.actors {
            let Some(module) = &a.module else { continue };
            let summary = s.modules.iter().find(|m| m.name == *module);
            // What the manifest sets, as set (the file round-trips to the same scenario);
            // the declared parameters it leaves to the module, as empty optional fields.
            let mut config: Vec<Field> = a
                .config
                .iter()
                .map(|(k, v)| {
                    let declared = summary.and_then(|m| m.params.iter().find(|p| p.name == *k));
                    let kind = declared.map_or_else(|| kind_of_text(v), |p| kind_of(&p.ty));
                    Field::new(k, kind, v).with_unit(declared.and_then(|p| p.unit.clone()))
                })
                .collect();
            if let Some(m) = summary {
                for p in &m.params {
                    if !config.iter().any(|f| f.name == p.name) {
                        let f = Field::new(&p.name, kind_of(&p.ty), "").with_unit(p.unit.clone());
                        config.push(if p.required { f } else { f.optional() });
                    }
                }
            }
            d.actors.push(DraftActor {
                id: a.id,
                name: Field::new("name", Kind::Name, &a.name),
                module: module.clone(),
                ports: a
                    .ports
                    .iter()
                    .map(|p| (p.id, p.name.clone(), p.ty.clone()))
                    .collect(),
                config,
            });
            d.next_actor = d.next_actor.max(a.id + 1);
        }
        for c in &s.connections {
            d.connections.push(DraftConnection {
                id: c.id,
                name: Field::new("name", Kind::Name, &c.name),
                ty: c.ty.clone().unwrap_or_default(),
                ports: c.ports.clone(),
                attrs: crate::inspector::attr_fields(c),
                per_port: c.per_port.clone(),
            });
            d.next_connection = d.next_connection.max(c.id + 1);
        }
        for c in &s.changes {
            d.changes.push(DraftChange {
                at: Field::new(
                    "at",
                    Kind::Quantity,
                    &format!("{}{}", c.at, unit_of(&s.tick)),
                ),
                target: c.target,
                payload: c
                    .payload
                    .iter()
                    .map(|(k, v)| Field::new(k, kind_of_text(v), v))
                    .collect(),
            });
        }
        d
    }

    pub fn add_module(&mut self, m: DraftModule) {
        self.modules.retain(|x| x.name != m.name);
        self.modules.push(m);
    }

    /// Place an actor of a module with `ports` ports of `port_type` (RFC 004 rule 7: the
    /// interface assigns the identity, and a name the person can change). The configuration
    /// fields are the module's parameters: a required one empty and to be filled, an optional
    /// one empty and left to the module's default unless the person sets it.
    pub fn place(&mut self, module: &str, port_type: &str, ports: u32) -> Option<u32> {
        let m = self.modules.iter().find(|m| m.name == module)?;
        let id = self.next_actor;
        self.next_actor += 1;
        let config = m
            .summary
            .as_ref()
            .map(|s| {
                s.params
                    .iter()
                    .map(|p| {
                        let f = Field::new(&p.name, kind_of(&p.ty), "").with_unit(p.unit.clone());
                        if p.required { f } else { f.optional() }
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.actors.push(DraftActor {
            id,
            name: Field::new("name", Kind::Name, &format!("a{id}")),
            module: module.into(),
            ports: (0..ports)
                .map(|p| (p, format!("p{p}"), port_type.to_string()))
                .collect(),
            config,
        });
        Some(id)
    }

    pub fn remove_actor(&mut self, id: u32) {
        self.actors.retain(|a| a.id != id);
        self.connections
            .retain(|c| !c.ports.iter().any(|(a, _)| *a == id));
    }

    /// Connect ports (two or more where the domain allows, RFC 004 rule 2): the connection's
    /// identity is the interface's.
    pub fn connect(&mut self, ty: &str, ports: &[(u32, u32)]) -> u32 {
        let id = self.next_connection;
        self.next_connection += 1;
        self.connections.push(DraftConnection {
            id,
            name: Field::new(
                "name",
                Kind::Name,
                &format!("c{}", id - FIRST_CONNECTION_ID),
            ),
            ty: ty.into(),
            ports: ports.to_vec(),
            attrs: Vec::new(),
            per_port: Vec::new(),
        });
        id
    }

    pub fn remove_connection(&mut self, id: u32) {
        self.connections.retain(|c| c.id != id);
        self.changes.retain(|c| c.target != id);
    }

    /// Set or add a connection attribute; the field's kind follows the value's form.
    pub fn set_attr(&mut self, connection: u32, key: &str, value: &str) -> Option<&Field> {
        let c = self.connections.iter_mut().find(|c| c.id == connection)?;
        set_field(&mut c.attrs, key, value);
        c.attrs.iter().find(|f| f.name == key)
    }

    pub fn set_config(&mut self, actor: u32, key: &str, value: &str) -> Option<&Field> {
        let a = self.actors.iter_mut().find(|a| a.id == actor)?;
        set_field(&mut a.config, key, value);
        a.config.iter().find(|f| f.name == key)
    }

    /// Schedule a change (RFC 004 rule 20): "at 30 s, take this link down".
    pub fn schedule(&mut self, at: &str, target: u32, payload: &[(&str, &str)]) {
        self.changes.push(DraftChange {
            at: Field::new("at", Kind::Quantity, at),
            target,
            payload: payload
                .iter()
                .map(|(k, v)| Field::new(k, kind_of_text(v), v))
                .collect(),
        });
    }

    /// The domains the draft uses: the core, then those of its ports and connection types in
    /// first-seen order.
    pub fn domains(&self) -> Vec<String> {
        let mut out = vec!["core".to_string()];
        let mut add = |ty: &str| {
            if let Some((d, _)) = ty.split_once('/')
                && !out.iter().any(|x| x == d)
            {
                out.push(d.to_string());
            }
        };
        for a in &self.actors {
            for (_, _, ty) in &a.ports {
                add(ty);
            }
        }
        for c in &self.connections {
            add(&c.ty);
        }
        out
    }

    /// Every field, with its manifest path, for a form that lists them.
    pub fn problems(&self) -> usize {
        let mut n = usize::from(!self.name.is_valid())
            + usize::from(!self.tick.is_valid())
            + usize::from(!self.end.is_valid())
            + usize::from(!self.seed.is_valid());
        for a in &self.actors {
            n += usize::from(!a.name.is_valid());
            n += a.config.iter().filter(|f| !f.is_valid()).count();
        }
        for c in &self.connections {
            n += usize::from(!c.name.is_valid());
            n += c.attrs.iter().filter(|f| !f.is_valid()).count();
        }
        for c in &self.changes {
            n += usize::from(!c.at.is_valid());
            n += c.payload.iter().filter(|f| !f.is_valid()).count();
        }
        n
    }

    /// The Host refused the draft: each refusal goes to the field it names; the rest stay in
    /// `errors`. Returns how many named a field.
    pub fn apply_errors(&mut self, errors: &[Notification]) -> usize {
        self.errors = errors.to_vec();
        let mut n = 0;
        let top = |f: &mut Field, path: &str| {
            f.errors = errors
                .iter()
                .filter(|e| e.scope.field.as_deref() == Some(path))
                .cloned()
                .collect();
            f.errors.len()
        };
        n += top(&mut self.name, "name");
        n += top(&mut self.tick, "time.tick");
        n += top(&mut self.end, "time.end");
        n += top(&mut self.seed, "seed.root");
        for (i, a) in self.actors.iter_mut().enumerate() {
            n += top(&mut a.name, &format!("actors[{i}].name"));
            n += fields::attach(&mut a.config, &format!("actors[{i}].config"), errors);
        }
        for (i, c) in self.connections.iter_mut().enumerate() {
            n += top(&mut c.name, &format!("connections[{i}].name"));
            n += fields::attach(&mut c.attrs, &format!("connections[{i}].attrs"), errors);
        }
        for (i, c) in self.changes.iter_mut().enumerate() {
            n += top(&mut c.at, &format!("changes[{i}].at"));
            n += fields::attach(&mut c.payload, &format!("changes[{i}].payload"), errors);
        }
        n
    }

    /// Commit every field's text; `true` when any value changed.
    pub fn commit_all(&mut self) -> bool {
        let mut changed = false;
        for f in [
            &mut self.name,
            &mut self.tick,
            &mut self.end,
            &mut self.seed,
        ] {
            changed |= f.commit();
        }
        for a in &mut self.actors {
            changed |= a.name.commit();
            for f in &mut a.config {
                changed |= f.commit();
            }
        }
        for c in &mut self.connections {
            changed |= c.name.commit();
            for f in &mut c.attrs {
                changed |= f.commit();
            }
        }
        for c in &mut self.changes {
            changed |= c.at.commit();
            for f in &mut c.payload {
                changed |= f.commit();
            }
        }
        changed
    }

    /// The manifest (RFC 004 §3) from the committed values, readable in a diff.
    pub fn to_toml(&self) -> String {
        let mut t = String::new();
        t.push_str(&format!(
            "florn = 1\nname = {}\ndomains = [{}]\n\n[time]\ntick = {}\nend = {}\n\n[order]\ntie = {}\n\n[seed]\nroot = {}\n",
            quote(&self.name.committed),
            self.domains()
                .iter()
                .map(|d| quote(d))
                .collect::<Vec<_>>()
                .join(", "),
            quote(&self.tick.committed),
            quote(&self.end.committed),
            quote(&self.tie),
            self.seed.committed,
        ));
        for (class, fields) in &self.limits {
            t.push_str(&format!("\n[limits.{class}]\n"));
            for (k, v) in fields {
                t.push_str(&format!("{k} = {}\n", toml_value(v)));
            }
        }
        let used: Vec<&DraftModule> = self
            .modules
            .iter()
            .filter(|m| self.actors.iter().any(|a| a.module == m.name))
            .collect();
        for m in used {
            t.push_str(&format!(
                "\n[modules.{}]\nsha256 = {}\ncontract = {}\n",
                m.name,
                quote(&m.sha256),
                quote(&m.contract)
            ));
        }
        for a in &self.actors {
            let ports: Vec<String> = a
                .ports
                .iter()
                .map(|(id, name, ty)| {
                    format!(
                        "{{ id = {id}, name = {}, type = {} }}",
                        quote(name),
                        quote(ty)
                    )
                })
                .collect();
            t.push_str(&format!(
                "\n[[actors]]\nid = {}\nname = {}\nkind = \"module\"\nmodule = {}\nports = [{}]\n",
                a.id,
                quote(&a.name.committed),
                quote(&a.module),
                ports.join(", ")
            ));
            let set: Vec<&Field> = a
                .config
                .iter()
                .filter(|f| !f.committed.is_empty())
                .collect();
            if !set.is_empty() {
                t.push_str("[actors.config]\n");
                for f in set {
                    t.push_str(&format!("{} = {}\n", f.name, toml_value(&f.committed)));
                }
            }
        }
        for c in &self.connections {
            let ports: Vec<String> = c
                .ports
                .iter()
                .map(|(actor, port)| {
                    let a = self.actors.iter().find(|a| a.id == *actor);
                    let an = a.map(|a| a.name.committed.clone()).unwrap_or_default();
                    let pn = a
                        .and_then(|a| a.ports.iter().find(|p| p.0 == *port))
                        .map(|p| p.1.clone())
                        .unwrap_or_default();
                    quote(&format!("{an}.{pn}"))
                })
                .collect();
            t.push_str(&format!(
                "\n[[connections]]\nid = {}\nname = {}\n{}ports = [{}]\n",
                c.id,
                quote(&c.name.committed),
                if c.ty.is_empty() {
                    String::new()
                } else {
                    format!("type = {}\n", quote(&c.ty))
                },
                ports.join(", ")
            ));
            let set: Vec<&Field> = c.attrs.iter().filter(|f| !f.committed.is_empty()).collect();
            if !set.is_empty() {
                t.push_str("[connections.attrs]\n");
                for f in set {
                    t.push_str(&format!("{} = {}\n", f.name, toml_value(&f.committed)));
                }
            }
            if !c.per_port.is_empty() {
                t.push_str("[connections.attrs.per-port]\n");
                for ((actor, port), fields) in &c.per_port {
                    let a = self.actors.iter().find(|a| a.id == *actor);
                    let an = a.map(|a| a.name.committed.clone()).unwrap_or_default();
                    let pn = a
                        .and_then(|a| a.ports.iter().find(|p| p.0 == *port))
                        .map(|p| p.1.clone())
                        .unwrap_or_default();
                    let inner: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| format!("{k} = {}", toml_value(v)))
                        .collect();
                    t.push_str(&format!(
                        "{} = {{ {} }}\n",
                        quote(&format!("{an}.{pn}")),
                        inner.join(", ")
                    ));
                }
            }
        }
        for c in &self.changes {
            let target = self
                .connections
                .iter()
                .find(|x| x.id == c.target)
                .map(|x| x.name.committed.clone())
                .or_else(|| {
                    self.actors
                        .iter()
                        .find(|a| a.id == c.target)
                        .map(|a| a.name.committed.clone())
                })
                .unwrap_or_default();
            t.push_str(&format!(
                "\n[[changes]]\nat = {}\ntarget = {}\n[changes.payload]\n",
                quote(&c.at.committed),
                quote(&target)
            ));
            for f in &c.payload {
                t.push_str(&format!("{} = {}\n", f.name, toml_value(&f.committed)));
            }
        }
        t
    }
}

fn set_field(fields: &mut Vec<Field>, key: &str, value: &str) {
    match fields.iter_mut().find(|f| f.name == key) {
        Some(f) => {
            f.type_text(value);
            f.commit();
        }
        None => {
            let mut f = Field::new(key, kind_of_text(value), value);
            f.type_text(value);
            f.commit();
            fields.push(f);
        }
    }
}

/// The unit of a quantity as written (`1ns` gives `ns`).
pub fn unit_of(q: &str) -> &str {
    q.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-')
}

pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A value as the manifest writes it: a boolean or a number bare, anything else quoted.
pub fn toml_value(text: &str) -> String {
    let number = text.parse::<f64>().is_ok()
        && text.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.');
    if text == "true" || text == "false" || number {
        text.to_string()
    } else {
        quote(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_writes_its_manifest() {
        let mut d = Draft::new("lab");
        d.end.type_text("3000ns");
        d.end.commit();
        d.add_module(DraftModule {
            name: "distance_vector".into(),
            sha256: "ab".repeat(32),
            contract: "florn:network/router@0.1.0".into(),
            summary: None,
        });
        let a = d.place("distance_vector", "network/interface", 2).unwrap();
        let b = d.place("distance_vector", "network/interface", 2).unwrap();
        assert_eq!((a, b), (1, 2));
        assert!(d.place("nothing", "network/interface", 1).is_none());
        let c = d.connect("network/link", &[(a, 0), (b, 0)]);
        assert_eq!(c, FIRST_CONNECTION_ID);
        d.set_attr(c, "delay", "2ns");
        d.set_attr(c, "loss", "0.1");
        d.set_config(a, "infinity", "16");
        d.schedule("1000ns", c, &[("enabled", "false")]);
        assert_eq!(d.domains(), vec!["core", "network"]);
        assert_eq!(d.problems(), 0);
        let t = d.to_toml();
        assert!(t.contains("domains = [\"core\", \"network\"]"));
        assert!(t.contains("[modules.distance_vector]\nsha256 = \"abab"));
        assert!(t.contains("ports = [{ id = 0, name = \"p0\", type = \"network/interface\" }, { id = 1, name = \"p1\", type = \"network/interface\" }]"));
        assert!(t.contains("[actors.config]\ninfinity = 16\n"));
        assert!(t.contains("type = \"network/link\"\nports = [\"a1.p0\", \"a2.p0\"]\n[connections.attrs]\ndelay = \"2ns\"\nloss = 0.1\n"));
        assert!(t.contains(
            "[[changes]]\nat = \"1000ns\"\ntarget = \"c0\"\n[changes.payload]\nenabled = false\n"
        ));
        // A bare number where a quantity goes is a problem the draft tells itself.
        d.set_attr(c, "delay", "5");
        assert_eq!(d.problems(), 1);
        assert!(d.to_toml().contains("delay = \"2ns\""), "not committed");
    }

    #[test]
    fn placed_actors_leave_optional_parameters_to_the_module() {
        use florn_control::message::{ModuleSummary, ParamSummary};
        let param = |name: &str, required: bool| ParamSummary {
            name: name.into(),
            ty: "u64".into(),
            required,
            default: (!required).then(|| "7".to_string()),
            unit: None,
        };
        let summary = ModuleSummary {
            name: "m".into(),
            source_sha256: String::new(),
            contract: String::new(),
            kind: "k".into(),
            description: None,
            port_kinds: Vec::new(),
            params: vec![param("size", true), param("period", false)],
            channels: Vec::new(),
            needs: Vec::new(),
            tables: Vec::new(),
        };
        let mut d = Draft::new("x");
        d.add_module(DraftModule {
            name: "m".into(),
            sha256: String::new(),
            contract: String::new(),
            summary: Some(summary),
        });
        let a = d.place("m", "network/interface", 1).unwrap();
        let actor = &d.actors[0];
        assert_eq!(
            actor.config[0].problem,
            Some(Problem::Empty),
            "required, to be filled"
        );
        assert!(actor.config[1].optional && actor.config[1].is_valid());
        assert!(
            !d.to_toml().contains("period"),
            "left to the module's default"
        );
        d.set_config(a, "size", "3");
        assert!(d.to_toml().contains("[actors.config]\nsize = 3\n"));
    }

    #[test]
    fn the_hosts_refusals_attach_to_their_fields() {
        use florn_control::{Ends, Notification, Scope, Severity};
        let mut d = Draft::new("lab");
        d.add_module(DraftModule {
            name: "m".into(),
            sha256: String::new(),
            contract: String::new(),
            summary: None,
        });
        let a = d.place("m", "network/interface", 1).unwrap();
        let c = d.connect("network/link", &[(a, 0)]);
        d.set_attr(c, "delay", "2ns");
        let at = |field: &str| {
            let s = Scope {
                field: Some(field.into()),
                ..Scope::default()
            };
            Notification::new("network.attr-type", Severity::Error, s, Ends::Command)
        };
        let n = d.apply_errors(&[
            at("connections[0].attrs.delay"),
            at("time.end"),
            at("connections[0].name"),
            at("nowhere"),
        ]);
        assert_eq!(n, 3);
        assert_eq!(d.connections[0].attrs[0].errors.len(), 1);
        assert_eq!(d.end.errors.len(), 1);
        assert_eq!(d.connections[0].name.errors.len(), 1);
        assert_eq!(d.errors.len(), 4);
        assert_eq!(
            d.problems(),
            3,
            "the three refused fields, the empty end among them"
        );
    }
}
