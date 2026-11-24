//! The canonical binary encoding of the control interface's messages (RFC 005 rule 25): the
//! envelope, a command, a query, a subscription, a telemetry record and the small pieces they share.
//! The answers and their summaries are in `answer`.

use super::answer::{answer_r, answer_w};
use super::*;

pub(super) fn at_w(e: &mut Encoder, at: At) {
    e.u64(at.tick);
    e.u32(at.microstep);
}
pub(super) fn at_r(d: &mut Decoder) -> Option<At> {
    Some(At {
        tick: d.u64()?,
        microstep: d.u32()?,
    })
}
pub(super) fn file_kind_w(e: &mut Encoder, k: FileKind) {
    e.u8(match k {
        FileKind::Manifest => 0,
        FileKind::Package => 1,
        FileKind::Module => 2,
        FileKind::RunRecord => 3,
        FileKind::Checkpoint => 4,
        FileKind::DiagnosticLog => 5,
    });
}
pub(super) fn file_kind_r(d: &mut Decoder) -> Option<FileKind> {
    Some(match d.u8()? {
        0 => FileKind::Manifest,
        1 => FileKind::Package,
        2 => FileKind::Module,
        3 => FileKind::RunRecord,
        4 => FileKind::Checkpoint,
        5 => FileKind::DiagnosticLog,
        _ => return None,
    })
}
pub(super) fn run_state_w(e: &mut Encoder, s: RunState) {
    e.u8(match s {
        RunState::NoScenario => 0,
        RunState::Invalid => 1,
        RunState::Ready => 2,
        RunState::Running => 3,
        RunState::Paused => 4,
        RunState::Ended => 5,
    });
}
pub(super) fn run_state_r(d: &mut Decoder) -> Option<RunState> {
    Some(match d.u8()? {
        0 => RunState::NoScenario,
        1 => RunState::Invalid,
        2 => RunState::Ready,
        3 => RunState::Running,
        4 => RunState::Paused,
        5 => RunState::Ended,
        _ => return None,
    })
}
pub(super) fn pacing_w(e: &mut Encoder, p: Pacing) {
    match p {
        Pacing::Fastest => e.u8(0),
        Pacing::TicksPerSecond(t) => {
            e.u8(1);
            e.u64(t);
        }
    }
}
pub(super) fn pacing_r(d: &mut Decoder) -> Option<Pacing> {
    Some(match d.u8()? {
        0 => Pacing::Fastest,
        1 => Pacing::TicksPerSecond(d.u64()?),
        _ => return None,
    })
}
pub(super) fn bool_w(e: &mut Encoder, b: bool) {
    e.u8(b as u8);
}
pub(super) fn bool_r(d: &mut Decoder) -> Option<bool> {
    match d.u8()? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn command_w(e: &mut Encoder, c: &Command) {
    match c {
        Command::LoadScenario { bytes } => {
            e.u8(0);
            e.bytes(bytes);
        }
        Command::LoadModule { bytes } => {
            e.u8(1);
            e.bytes(bytes);
        }
        Command::Admit { bytes } => {
            e.u8(2);
            e.bytes(bytes);
        }
        Command::NewRun { seed, residency } => {
            e.u8(3);
            match seed {
                SeedChoice::Given(s) => {
                    e.u8(0);
                    e.u64(*s);
                }
                SeedChoice::Choose => e.u8(1),
            }
            // `resident` is the 0.2 form, unchanged; `swap` is one byte more (RFC 009 rule 14).
            if *residency == Residency::Swap {
                e.u8(1);
            }
        }
        Command::EndRun => e.u8(4),
        Command::Advance { unit } => {
            e.u8(5);
            match unit {
                Unit::OneEvent => e.u8(0),
                Unit::OneInstant => e.u8(1),
                Unit::OneTick => e.u8(2),
                Unit::ToTick(t) => {
                    e.u8(3);
                    e.u64(*t);
                }
            }
        }
        Command::Pause => e.u8(6),
        Command::SetPacing { pacing } => {
            e.u8(7);
            pacing_w(e, *pacing);
        }
        Command::Intervene {
            target,
            content,
            at,
        } => {
            e.u8(8);
            e.u32(*target);
            e.bytes(content);
            e.option(*at, Encoder::u64);
        }
        Command::Export { kind, confirmed } => {
            e.u8(9);
            match kind {
                ExportKind::RunRecord { trace, payloads } => {
                    e.u8(0);
                    bool_w(e, *trace);
                    bool_w(e, *payloads);
                }
                ExportKind::DiagnosticLog => e.u8(1),
                ExportKind::Package => e.u8(2),
                ExportKind::Checkpoint { index } => {
                    e.u8(3);
                    e.u32(*index);
                }
                ExportKind::ParentRecord => e.u8(4),
            }
            bool_w(e, *confirmed);
        }
        Command::Replay { record } => {
            e.u8(10);
            e.bytes(record);
        }
        Command::Checkpoint => e.u8(11),
        Command::Restore { checkpoint } => {
            e.u8(12);
            e.u32(*checkpoint);
        }
        Command::LoadCheckpoint { bytes } => {
            e.u8(13);
            e.bytes(bytes);
        }
        Command::SetCostModel { version } => {
            e.u8(14);
            e.u32(*version);
        }
        Command::Branch => e.u8(15),
    }
}

fn command_r(d: &mut Decoder) -> Option<Command> {
    Some(match d.u8()? {
        0 => Command::LoadScenario { bytes: d.bytes()? },
        1 => Command::LoadModule { bytes: d.bytes()? },
        2 => Command::Admit { bytes: d.bytes()? },
        3 => Command::NewRun {
            seed: match d.u8()? {
                0 => SeedChoice::Given(d.u64()?),
                1 => SeedChoice::Choose,
                _ => return None,
            },
            // The 0.2 form ends here and means `resident`; `swap` is one byte more (0.3).
            residency: if d.remaining() == 0 {
                Residency::Resident
            } else {
                match d.u8()? {
                    1 => Residency::Swap,
                    _ => return None,
                }
            },
        },
        4 => Command::EndRun,
        5 => Command::Advance {
            unit: match d.u8()? {
                0 => Unit::OneEvent,
                1 => Unit::OneInstant,
                2 => Unit::OneTick,
                3 => Unit::ToTick(d.u64()?),
                _ => return None,
            },
        },
        6 => Command::Pause,
        7 => Command::SetPacing {
            pacing: pacing_r(d)?,
        },
        8 => Command::Intervene {
            target: d.u32()?,
            content: d.bytes()?,
            at: d.option(Decoder::u64)?,
        },
        9 => Command::Export {
            kind: match d.u8()? {
                0 => ExportKind::RunRecord {
                    trace: bool_r(d)?,
                    payloads: bool_r(d)?,
                },
                1 => ExportKind::DiagnosticLog,
                2 => ExportKind::Package,
                3 => ExportKind::Checkpoint { index: d.u32()? },
                4 => ExportKind::ParentRecord,
                _ => return None,
            },
            confirmed: bool_r(d)?,
        },
        10 => Command::Replay { record: d.bytes()? },
        11 => Command::Checkpoint,
        12 => Command::Restore {
            checkpoint: d.u32()?,
        },
        13 => Command::LoadCheckpoint { bytes: d.bytes()? },
        14 => Command::SetCostModel { version: d.u32()? },
        15 => Command::Branch,
        _ => return None,
    })
}

fn query_w(e: &mut Encoder, q: &Query) {
    match q {
        Query::Scenario => e.u8(0),
        Query::RunState => e.u8(1),
        Query::Actor { id } => {
            e.u8(2);
            e.u32(*id);
        }
        Query::TraceWindow { from, count } => {
            e.u8(3);
            e.u64(*from);
            e.u32(*count);
        }
        Query::Capacity => e.u8(4),
        Query::Support => e.u8(5),
        Query::Decode { protocol, payload } => {
            e.u8(6);
            e.text(protocol);
            e.bytes(payload);
        }
        Query::Payload { digest } => {
            e.u8(7);
            e.text(digest);
        }
        Query::Checkpoints => e.u8(8),
        Query::Inspect { id } => {
            e.u8(9);
            e.u32(*id);
        }
    }
}
fn query_r(d: &mut Decoder) -> Option<Query> {
    Some(match d.u8()? {
        0 => Query::Scenario,
        1 => Query::RunState,
        2 => Query::Actor { id: d.u32()? },
        3 => Query::TraceWindow {
            from: d.u64()?,
            count: d.u32()?,
        },
        4 => Query::Capacity,
        5 => Query::Support,
        6 => Query::Decode {
            protocol: d.text()?,
            payload: d.bytes()?,
        },
        7 => Query::Payload { digest: d.text()? },
        8 => Query::Checkpoints,
        9 => Query::Inspect { id: d.u32()? },
        _ => return None,
    })
}
fn subscription_w(e: &mut Encoder, s: &Subscription) {
    match s {
        Subscription::Telemetry {
            channels,
            keep_one_in,
        } => {
            e.u8(0);
            e.list(channels, |e, c| e.u32(*c));
            e.u32(*keep_one_in);
        }
        Subscription::Events => e.u8(1),
        Subscription::Progress => e.u8(2),
        Subscription::Display => e.u8(3),
    }
}
fn subscription_r(d: &mut Decoder) -> Option<Subscription> {
    Some(match d.u8()? {
        0 => Subscription::Telemetry {
            channels: d.list(Decoder::u32)?,
            keep_one_in: d.u32()?,
        },
        1 => Subscription::Events,
        2 => Subscription::Progress,
        3 => Subscription::Display,
        _ => return None,
    })
}
fn kind_w(e: &mut Encoder, k: SubscriptionKind) {
    e.u8(match k {
        SubscriptionKind::Telemetry => 0,
        SubscriptionKind::Events => 1,
        SubscriptionKind::Progress => 2,
        SubscriptionKind::Display => 3,
    });
}
fn kind_r(d: &mut Decoder) -> Option<SubscriptionKind> {
    Some(match d.u8()? {
        0 => SubscriptionKind::Telemetry,
        1 => SubscriptionKind::Events,
        2 => SubscriptionKind::Progress,
        3 => SubscriptionKind::Display,
        _ => return None,
    })
}
pub(super) fn telemetry_w(e: &mut Encoder, t: &TelemetryRecord) {
    at_w(e, t.at);
    e.u32(t.actor);
    e.u32(t.channel);
    e.bytes(&t.payload);
}
pub(super) fn telemetry_r(d: &mut Decoder) -> Option<TelemetryRecord> {
    Some(TelemetryRecord {
        at: at_r(d)?,
        actor: d.u32()?,
        channel: d.u32()?,
        payload: d.bytes()?,
    })
}
pub(super) fn opt_text_w(e: &mut Encoder, t: &Option<String>) {
    e.option(t.as_deref(), Encoder::text);
}
pub(super) fn opt_text_r(d: &mut Decoder) -> Option<Option<String>> {
    d.option(Decoder::text)
}
pub(super) fn pairs_w(e: &mut Encoder, pairs: &[(String, String)]) {
    e.list(pairs, |e, (k, v)| {
        e.text(k);
        e.text(v);
    });
}
pub(super) fn pairs_r(d: &mut Decoder) -> Option<Vec<(String, String)>> {
    d.list(|d| Some((d.text()?, d.text()?)))
}

/// The canonical encoding of an envelope: the version, a tag, the message.
pub fn encode(env: &Envelope) -> Vec<u8> {
    let mut e = Encoder::default();
    e.u32(env.version);
    match &env.message {
        Message::Command(c) => {
            e.u8(0);
            command_w(&mut e, c);
        }
        Message::Query(q) => {
            e.u8(1);
            query_w(&mut e, q);
        }
        Message::Subscribe(s) => {
            e.u8(2);
            subscription_w(&mut e, s);
        }
        Message::Unsubscribe(k) => {
            e.u8(3);
            kind_w(&mut e, *k);
        }
        Message::Notification(n) => {
            e.u8(4);
            n.write(&mut e);
        }
        Message::Answer(a) => {
            e.u8(5);
            answer_w(&mut e, a);
        }
        Message::Telemetry(t) => {
            e.u8(6);
            telemetry_w(&mut e, t);
        }
        Message::Progress(p) => {
            e.u8(7);
            run_state_w(&mut e, p.state);
            at_w(&mut e, p.now);
            e.u64(p.handled);
            e.u64(p.records);
        }
    }
    e.finish()
}

/// Decode an envelope; a version other than [`CONTROL_VERSION`] is refused before anything else.
pub fn decode(bytes: &[u8]) -> Result<Envelope, DecodeError> {
    let mut d = Decoder::new(bytes);
    let version = d.u32().ok_or(DecodeError::Malformed("version"))?;
    if version != CONTROL_VERSION {
        return Err(DecodeError::Version(version));
    }
    let tag = d.u8().ok_or(DecodeError::Malformed("message"))?;
    let message = match tag {
        0 => Message::Command(command_r(&mut d).ok_or(DecodeError::Malformed("command"))?),
        1 => Message::Query(query_r(&mut d).ok_or(DecodeError::Malformed("query"))?),
        2 => Message::Subscribe(
            subscription_r(&mut d).ok_or(DecodeError::Malformed("subscription"))?,
        ),
        3 => Message::Unsubscribe(kind_r(&mut d).ok_or(DecodeError::Malformed("subscription"))?),
        4 => Message::Notification(
            Notification::read(&mut d).ok_or(DecodeError::Malformed("notification"))?,
        ),
        5 => Message::Answer(answer_r(&mut d).ok_or(DecodeError::Malformed("answer"))?),
        6 => Message::Telemetry(telemetry_r(&mut d).ok_or(DecodeError::Malformed("telemetry"))?),
        7 => Message::Progress(Progress {
            state: run_state_r(&mut d).ok_or(DecodeError::Malformed("progress"))?,
            now: at_r(&mut d).ok_or(DecodeError::Malformed("progress"))?,
            handled: d.u64().ok_or(DecodeError::Malformed("progress"))?,
            records: d.u64().ok_or(DecodeError::Malformed("progress"))?,
        }),
        _ => return Err(DecodeError::Malformed("message")),
    };
    if !d.done() {
        return Err(DecodeError::Malformed("trailing bytes"));
    }
    Ok(Envelope { version, message })
}
