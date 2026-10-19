//! The core's world: delivery over the scenario's connections, and the core intervention. A domain
//! (RFC 005 rule 3) replaces this with its own semantics; the core attaches no meaning to bytes
//! (RFC 002 rule 24) and delivers a committed `send` to every other port of the connection, at the
//! next microstep, as a message.

use std::collections::BTreeMap;

use florn_kernel::{Delivery, EventKind, Identity, Instant, World};
use florn_model::Manifest;
use florn_model::canonical::Encoder;

/// The class of timer events in the order key (RFC 002 rule 29), assigned by the core.
pub const CLASS_TIMER: u8 = 1;
/// The class of message events.
pub const CLASS_MESSAGE: u8 = 2;

/// Every port of the scenario and the ports its connection reaches.
#[derive(Clone, Debug, Default)]
pub struct CoreWorld {
    peers: BTreeMap<(Identity, u32), Vec<(Identity, u32)>>,
}

impl CoreWorld {
    pub fn from_scenario(scenario: &Manifest) -> Self {
        let mut peers: BTreeMap<(Identity, u32), Vec<(Identity, u32)>> = BTreeMap::new();
        for c in &scenario.connections {
            for p in &c.ports {
                let others = c
                    .ports
                    .iter()
                    .filter(|q| *q != p)
                    .map(|q| (q.actor, q.port))
                    .collect();
                peers.insert((p.actor, p.port), others);
            }
        }
        Self { peers }
    }

    pub fn peers(&self, actor: Identity, port: u32) -> &[(Identity, u32)] {
        self.peers
            .get(&(actor, port))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

impl World for CoreWorld {
    fn deliver(
        &mut self,
        _: Instant,
        origin: Identity,
        _: u64,
        port: u32,
        payload: &[u8],
    ) -> Vec<Delivery> {
        self.peers(origin, port)
            .iter()
            .map(|(target, port)| Delivery {
                target: *target,
                class: CLASS_MESSAGE,
                delay: 0,
                kind: EventKind::Message {
                    port: *port,
                    payload: payload.to_vec(),
                },
            })
            .collect()
    }
}

/// The core's interventions (RFC 001 rules 33 to 38; RFC 005 rule 26). The content is a canonical
/// map (RFC 005 rule 20): `kind`, then the fields of the kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intervention {
    /// Deliver a message to an actor's port, as if a peer had sent it.
    Message {
        actor: Identity,
        port: u32,
        payload: Vec<u8>,
    },
}

impl Intervention {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Message { .. } => "message",
        }
    }

    /// The canonical map: entries sorted by key, each as text key then a tagged value (RFC 004
    /// rule 32: an integer, a text, or bytes as text of hexadecimal digits).
    pub fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::default();
        match self {
            Self::Message {
                actor,
                port,
                payload,
            } => {
                e.u32(4);
                e.text("actor");
                e.u8(0);
                e.i64(*actor as i64);
                e.text("kind");
                e.u8(2);
                e.text("message");
                e.text("payload");
                e.u8(2);
                e.text(&florn_model::canonical::hex(payload));
                e.text("port");
                e.u8(0);
                e.i64(*port as i64);
            }
        }
        e.finish()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut d = florn_model::canonical::Decoder::new(bytes);
        let n = d.u32()?;
        let mut actor = None;
        let mut port = None;
        let mut kind = None;
        let mut payload = None;
        for _ in 0..n {
            let key = d.text()?;
            let tag = d.u8()?;
            match (key.as_str(), tag) {
                ("actor", 0) => actor = Some(u32::try_from(d.i64()?).ok()?),
                ("port", 0) => port = Some(u32::try_from(d.i64()?).ok()?),
                ("kind", 2) => kind = Some(d.text()?),
                ("payload", 2) => payload = Some(unhex(&d.text()?)?),
                _ => return None,
            }
        }
        match kind.as_deref() {
            Some("message") => Some(Self::Message {
                actor: actor?,
                port: port?,
                payload: payload?,
            }),
            _ => None,
        }
    }
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
