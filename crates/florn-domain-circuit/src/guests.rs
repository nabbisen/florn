//! What the start hook reads of the guest actors (RFC 012 finding F-18): the `[circuit]` kind of
//! each module and the telemetry channels it declares. The Host gives them read-only; the domain
//! keeps what it needs. `circuit.device-limit` (rule 19) is judged from them, and a guest's
//! `control` series index (RFC 014 rule 3: the actor's declared `f64` fields, counted in channel
//! order and then field order) is computed from them (PR-47).

use std::collections::BTreeMap;

use florn_admission::manifest::{Channel, ModuleManifest, ParamType};
use florn_host::domain::Guest;
use florn_kernel::Identity;
use florn_model::manifest::DomainValue;

use crate::NAME;

/// The kind a module declares in its `[circuit]` table (rule 18, rule 19).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestKind {
    Controller,
    Device,
}

/// One guest actor of the circuit domain: its kind and its declared channels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuestInfo {
    pub kind: GuestKind,
    pub channels: Vec<Channel>,
}

/// The circuit guests of a run, by actor identity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Guests(BTreeMap<Identity, GuestInfo>);

impl Guests {
    /// The guests that declare a `[circuit]` table with a kind the domain knows; the kind an
    /// unknown module declares was already refused at admission (`circuit.kind-unknown`).
    pub fn read(guests: &[Guest]) -> Self {
        Self::from_manifests(guests.iter().map(|g| (g.actor, g.manifest())))
    }

    /// As [`Self::read`], from the manifests alone.
    pub fn from_manifests<'a>(
        manifests: impl IntoIterator<Item = (Identity, &'a ModuleManifest)>,
    ) -> Self {
        let mut out = BTreeMap::new();
        for (actor, manifest) in manifests {
            let kind = match manifest.domain_tables.get(NAME).and_then(|t| t.get("kind")) {
                Some(DomainValue::Text(t)) if t == "controller" => GuestKind::Controller,
                Some(DomainValue::Text(t)) if t == "device" => GuestKind::Device,
                _ => continue,
            };
            let mut channels = manifest.telemetry.clone();
            channels.sort_by_key(|c| c.channel);
            out.insert(actor, GuestInfo { kind, channels });
        }
        Self(out)
    }

    /// The guest actors that are device models (RFC 012 rule 19: at most one).
    pub fn devices(&self) -> usize {
        self.0
            .values()
            .filter(|g| g.kind == GuestKind::Device)
            .count()
    }

    pub fn get(&self, actor: Identity) -> Option<&GuestInfo> {
        self.0.get(&actor)
    }

    /// The index of the first `f64` field of `channel` among the actor's declared `f64` fields,
    /// counted in channel order and then field order (RFC 014 rule 3); `None` for an actor or a
    /// channel it does not declare.
    pub fn series_base(&self, actor: Identity, channel: u32) -> Option<usize> {
        let info = self.0.get(&actor)?;
        let mut before = 0;
        for c in &info.channels {
            if c.channel == channel {
                return Some(before);
            }
            before += c
                .fields
                .iter()
                .filter(|(_, t)| *t == ParamType::F64)
                .count();
        }
        None
    }
}

#[cfg(test)]
mod tests;
