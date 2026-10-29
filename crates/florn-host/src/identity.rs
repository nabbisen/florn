//! The run inputs and the run identity (RFC 001 rules 40 to 42; RFC 003 rule 25), and their
//! form in `run.toml` of the run record (RFC 005 rule 29).

use florn_kernel::{AppliedIntervention, Digest, Instant};
use florn_model::canonical::hex;
use toml::{Table, Value};

use crate::world::unhex;

/// The environment part of the identity (RFC 001 rule 41): decides the tier of a comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment {
    pub florn_version: String,
    /// `native`, `browser`
    pub host_build: String,
    pub engine: String,
    pub engine_version: String,
    pub arch: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleInput {
    /// The `[modules]` name.
    pub name: String,
    pub source_sha256: [u8; 32],
    /// The check value of RFC 003 rule 25.
    pub admitted_sha256: [u8; 32],
}

/// The run inputs (RFC 001 rule 40): together they determine the trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunInputs {
    pub scenario_sha256: [u8; 32],
    pub modules: Vec<ModuleInput>,
    pub admission_version: u32,
    pub cost_model_version: u32,
    pub cost_table: String,
    pub contracts: Vec<String>,
    pub root_seed: u64,
    pub interventions: Vec<AppliedIntervention>,
    pub generator: String,
    pub derivation: String,
    pub digest: String,
}

/// Why a run ended (`enum(end-reason)` of RFC 007).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    EndReached,
    User,
    HostFault,
}

impl EndReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EndReached => "end-reached",
            Self::User => "user",
            Self::HostFault => "host-fault",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "end-reached" => Self::EndReached,
            "user" => Self::User,
            "host-fault" => Self::HostFault,
            _ => return None,
        })
    }
}

/// The parent of a branch (RFC 001 rule 54; RFC 009 rules 10 and 12): the parent's chain value,
/// record count and instant at the branching. Provenance: compared by nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parent {
    pub chain: Digest,
    pub records: u64,
    pub instant: Instant,
}

/// `run.toml`: the identity, the inputs, the stored chain, the parent of a branch, and the
/// metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunToml {
    pub environment: Environment,
    pub inputs: RunInputs,
    /// `[parent]` when the run is a branch (RFC 009 rule 12).
    pub parent: Option<Parent>,
    /// (records, chain value) at every stored point, the end included.
    pub chain: Vec<(u64, Digest)>,
    pub chain_interval: u64,
    pub records: u64,
    pub ended: Option<EndReason>,
    pub reproducible: bool,
    /// Metadata (RFC 001 rule 42).
    pub run_id: String,
    pub created_ms: u64,
}

pub const RUN_RECORD_VERSION: i64 = 1;

fn digest_of(s: &str) -> Option<Digest> {
    unhex(s)?.try_into().ok()
}

impl RunToml {
    pub fn to_toml(&self) -> String {
        let mut root = Table::new();
        root.insert("florn-run".into(), Value::Integer(RUN_RECORD_VERSION));
        let mut id = Table::new();
        id.insert(
            "florn".into(),
            Value::String(self.environment.florn_version.clone()),
        );
        id.insert(
            "host".into(),
            Value::String(self.environment.host_build.clone()),
        );
        id.insert(
            "engine".into(),
            Value::String(self.environment.engine.clone()),
        );
        id.insert(
            "engine-version".into(),
            Value::String(self.environment.engine_version.clone()),
        );
        id.insert("arch".into(), Value::String(self.environment.arch.clone()));
        root.insert("identity".into(), Value::Table(id));
        let i = &self.inputs;
        let mut inputs = Table::new();
        inputs.insert("scenario".into(), Value::String(hex(&i.scenario_sha256)));
        inputs.insert(
            "admission".into(),
            Value::Integer(i.admission_version as i64),
        );
        inputs.insert(
            "cost-model".into(),
            Value::Integer(i.cost_model_version as i64),
        );
        inputs.insert("cost-table".into(), Value::String(i.cost_table.clone()));
        inputs.insert(
            "contracts".into(),
            Value::Array(
                i.contracts
                    .iter()
                    .map(|c| Value::String(c.clone()))
                    .collect(),
            ),
        );
        inputs.insert("seed".into(), Value::String(i.root_seed.to_string()));
        inputs.insert("generator".into(), Value::String(i.generator.clone()));
        inputs.insert("derivation".into(), Value::String(i.derivation.clone()));
        inputs.insert("digest".into(), Value::String(i.digest.clone()));
        inputs.insert(
            "modules".into(),
            Value::Array(
                i.modules
                    .iter()
                    .map(|m| {
                        let mut t = Table::new();
                        t.insert("name".into(), Value::String(m.name.clone()));
                        t.insert("source".into(), Value::String(hex(&m.source_sha256)));
                        t.insert("admitted".into(), Value::String(hex(&m.admitted_sha256)));
                        Value::Table(t)
                    })
                    .collect(),
            ),
        );
        inputs.insert(
            "interventions".into(),
            Value::Array(
                i.interventions
                    .iter()
                    .map(|iv| {
                        let mut t = Table::new();
                        t.insert("tick".into(), Value::String(iv.instant.tick.to_string()));
                        t.insert(
                            "microstep".into(),
                            Value::Integer(iv.instant.microstep as i64),
                        );
                        t.insert("content".into(), Value::String(hex(&iv.content)));
                        Value::Table(t)
                    })
                    .collect(),
            ),
        );
        root.insert("inputs".into(), Value::Table(inputs));
        let mut chain = Table::new();
        chain.insert(
            "interval".into(),
            Value::String(self.chain_interval.to_string()),
        );
        chain.insert("records".into(), Value::String(self.records.to_string()));
        chain.insert(
            "stored".into(),
            Value::Array(
                self.chain
                    .iter()
                    .map(|(n, d)| {
                        let mut t = Table::new();
                        t.insert("records".into(), Value::String(n.to_string()));
                        t.insert("digest".into(), Value::String(hex(d)));
                        Value::Table(t)
                    })
                    .collect(),
            ),
        );
        root.insert("chain".into(), Value::Table(chain));
        if let Some(p) = &self.parent {
            let mut parent = Table::new();
            parent.insert("chain".into(), Value::String(hex(&p.chain)));
            parent.insert("records".into(), Value::String(p.records.to_string()));
            parent.insert("instant".into(), Value::String(p.instant.tick.to_string()));
            parent.insert(
                "microstep".into(),
                Value::Integer(p.instant.microstep as i64),
            );
            root.insert("parent".into(), Value::Table(parent));
        }
        let mut meta = Table::new();
        meta.insert("run-id".into(), Value::String(self.run_id.clone()));
        meta.insert(
            "created-ms".into(),
            Value::String(self.created_ms.to_string()),
        );
        if let Some(e) = self.ended {
            meta.insert("ended".into(), Value::String(e.as_str().into()));
        }
        meta.insert("reproducible".into(), Value::Boolean(self.reproducible));
        root.insert("metadata".into(), Value::Table(meta));
        root.to_string()
    }

    /// Read `run.toml`; `None` for anything that is not a run record of this version.
    pub fn parse(text: &str) -> Option<Self> {
        let root: Table = text.parse().ok()?;
        if root.get("florn-run")?.as_integer()? != RUN_RECORD_VERSION {
            return None;
        }
        let s = |t: &Table, k: &str| -> Option<String> { Some(t.get(k)?.as_str()?.to_string()) };
        let u = |t: &Table, k: &str| -> Option<u64> { t.get(k)?.as_str()?.parse().ok() };
        let id = root.get("identity")?.as_table()?;
        let environment = Environment {
            florn_version: s(id, "florn")?,
            host_build: s(id, "host")?,
            engine: s(id, "engine")?,
            engine_version: s(id, "engine-version")?,
            arch: s(id, "arch")?,
        };
        let i = root.get("inputs")?.as_table()?;
        let modules = i
            .get("modules")?
            .as_array()?
            .iter()
            .map(|m| {
                let t = m.as_table()?;
                Some(ModuleInput {
                    name: s(t, "name")?,
                    source_sha256: digest_of(&s(t, "source")?)?,
                    admitted_sha256: digest_of(&s(t, "admitted")?)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let interventions = i
            .get("interventions")?
            .as_array()?
            .iter()
            .map(|iv| {
                let t = iv.as_table()?;
                Some(AppliedIntervention {
                    instant: Instant::new(
                        u(t, "tick")?,
                        u32::try_from(t.get("microstep")?.as_integer()?).ok()?,
                    ),
                    content: unhex(&s(t, "content")?)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let inputs = RunInputs {
            scenario_sha256: digest_of(&s(i, "scenario")?)?,
            modules,
            admission_version: u32::try_from(i.get("admission")?.as_integer()?).ok()?,
            cost_model_version: u32::try_from(i.get("cost-model")?.as_integer()?).ok()?,
            cost_table: s(i, "cost-table")?,
            contracts: i
                .get("contracts")?
                .as_array()?
                .iter()
                .map(|c| Some(c.as_str()?.to_string()))
                .collect::<Option<Vec<_>>>()?,
            root_seed: u(i, "seed")?,
            interventions,
            generator: s(i, "generator")?,
            derivation: s(i, "derivation")?,
            digest: s(i, "digest")?,
        };
        let c = root.get("chain")?.as_table()?;
        let chain = c
            .get("stored")?
            .as_array()?
            .iter()
            .map(|p| {
                let t = p.as_table()?;
                Some((u(t, "records")?, digest_of(&s(t, "digest")?)?))
            })
            .collect::<Option<Vec<_>>>()?;
        let parent = match root.get("parent") {
            Some(v) => {
                let t = v.as_table()?;
                Some(Parent {
                    chain: digest_of(&s(t, "chain")?)?,
                    records: u(t, "records")?,
                    instant: Instant::new(
                        u(t, "instant")?,
                        u32::try_from(t.get("microstep")?.as_integer()?).ok()?,
                    ),
                })
            }
            None => None,
        };
        let m = root.get("metadata")?.as_table()?;
        Some(Self {
            environment,
            inputs,
            parent,
            chain,
            chain_interval: u(c, "interval")?,
            records: u(c, "records")?,
            ended: match m.get("ended") {
                Some(v) => Some(EndReason::parse(v.as_str()?)?),
                None => None,
            },
            reproducible: m.get("reproducible")?.as_bool()?,
            run_id: s(m, "run-id")?,
            created_ms: u(m, "created-ms")?,
        })
    }
}
