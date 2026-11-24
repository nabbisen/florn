//! The canonical binary encoding of an answer and of the summaries it holds (RFC 005 rule 25): the
//! scenario, an actor, a module, a trace entry. The pieces they share are in `codec`.

use super::codec::*;
use super::*;

fn actor_summary_w(e: &mut Encoder, a: &ActorSummary) {
    e.u32(a.id);
    e.text(&a.name);
    opt_text_w(e, &a.module);
    e.text(&a.class);
    e.list(&a.ports, |e, p| {
        e.u32(p.id);
        e.text(&p.name);
        e.text(&p.ty);
    });
    pairs_w(e, &a.config);
}
fn actor_summary_r(d: &mut Decoder) -> Option<ActorSummary> {
    Some(ActorSummary {
        id: d.u32()?,
        name: d.text()?,
        module: opt_text_r(d)?,
        class: d.text()?,
        ports: d.list(|d| {
            Some(PortSummary {
                id: d.u32()?,
                name: d.text()?,
                ty: d.text()?,
            })
        })?,
        config: pairs_r(d)?,
    })
}
fn module_summary_w(e: &mut Encoder, m: &ModuleSummary) {
    e.text(&m.name);
    e.text(&m.source_sha256);
    e.text(&m.contract);
    e.text(&m.kind);
    opt_text_w(e, &m.description);
    e.list(&m.port_kinds, |e, (k, t, min, max)| {
        e.text(k);
        e.text(t);
        e.u64(*min);
        e.u64(*max);
    });
    e.list(&m.params, |e, p| {
        e.text(&p.name);
        e.text(&p.ty);
        bool_w(e, p.required);
        opt_text_w(e, &p.default);
        opt_text_w(e, &p.unit);
    });
    e.list(&m.channels, |e, c| {
        e.u32(c.channel);
        e.text(&c.name);
        pairs_w(e, &c.fields);
    });
    e.list(&m.needs, |e, (n, v)| {
        e.text(n);
        e.u64(*v);
    });
    e.list(&m.tables, |e, (n, fields)| {
        e.text(n);
        pairs_w(e, fields);
    });
}
fn module_summary_r(d: &mut Decoder) -> Option<ModuleSummary> {
    Some(ModuleSummary {
        name: d.text()?,
        source_sha256: d.text()?,
        contract: d.text()?,
        kind: d.text()?,
        description: opt_text_r(d)?,
        port_kinds: d.list(|d| Some((d.text()?, d.text()?, d.u64()?, d.u64()?)))?,
        params: d.list(|d| {
            Some(ParamSummary {
                name: d.text()?,
                ty: d.text()?,
                required: bool_r(d)?,
                default: opt_text_r(d)?,
                unit: opt_text_r(d)?,
            })
        })?,
        channels: d.list(|d| {
            Some(ChannelSummary {
                channel: d.u32()?,
                name: d.text()?,
                fields: pairs_r(d)?,
            })
        })?,
        needs: d.list(|d| Some((d.text()?, d.u64()?)))?,
        tables: d.list(|d| Some((d.text()?, pairs_r(d)?)))?,
    })
}
fn scenario_w(e: &mut Encoder, s: &ScenarioSummary) {
    e.text(&s.name);
    e.text(&s.sha256);
    e.text(&s.tick);
    e.u64(s.end);
    e.text(&s.tie);
    e.u64(s.seed);
    e.list(&s.actors, actor_summary_w);
    e.list(&s.connections, |e, c| {
        e.u32(c.id);
        e.text(&c.name);
        e.list(&c.ports, |e, (a, p)| {
            e.u32(*a);
            e.u32(*p);
        });
        opt_text_w(e, &c.ty);
        pairs_w(e, &c.attrs);
        e.list(&c.per_port, |e, ((a, p), fields)| {
            e.u32(*a);
            e.u32(*p);
            pairs_w(e, fields);
        });
    });
    e.list(&s.modules, module_summary_w);
    e.list(&s.missing_modules, |e, m| e.text(m));
    e.list(&s.domains, |e, m| e.text(m));
    e.list(&s.changes, |e, c| {
        e.u64(c.at);
        e.u32(c.target);
        pairs_w(e, &c.payload);
    });
    e.list(&s.classes, |e, (c, n)| {
        e.u8(*c);
        e.text(n);
    });
    e.list(&s.limits, |e, (class, fields)| {
        e.text(class);
        pairs_w(e, fields);
    });
}
fn scenario_r(d: &mut Decoder) -> Option<ScenarioSummary> {
    Some(ScenarioSummary {
        name: d.text()?,
        sha256: d.text()?,
        tick: d.text()?,
        end: d.u64()?,
        tie: d.text()?,
        seed: d.u64()?,
        actors: d.list(actor_summary_r)?,
        connections: d.list(|d| {
            Some(ConnectionSummary {
                id: d.u32()?,
                name: d.text()?,
                ports: d.list(|d| Some((d.u32()?, d.u32()?)))?,
                ty: opt_text_r(d)?,
                attrs: pairs_r(d)?,
                per_port: d.list(|d| Some(((d.u32()?, d.u32()?), pairs_r(d)?)))?,
            })
        })?,
        modules: d.list(module_summary_r)?,
        missing_modules: d.list(Decoder::text)?,
        domains: d.list(Decoder::text)?,
        changes: d.list(|d| {
            Some(ChangeSummary {
                at: d.u64()?,
                target: d.u32()?,
                payload: pairs_r(d)?,
            })
        })?,
        classes: d.list(|d| Some((d.u8()?, d.text()?)))?,
        limits: d.list(|d| Some((d.text()?, pairs_r(d)?)))?,
    })
}
fn trace_entry_w(e: &mut Encoder, t: &TraceEntry) {
    match t {
        TraceEntry::Handled {
            at,
            class,
            origin,
            seq,
            target,
            metered,
            draws,
            effects,
            sends,
        } => {
            e.u8(1);
            at_w(e, *at);
            e.u8(*class);
            e.u32(*origin);
            e.u64(*seq);
            e.u32(*target);
            e.u64(*metered);
            e.u32(*draws);
            e.u32(*effects);
            e.list(sends, |e, s| {
                e.u64(s.seq);
                e.u32(s.port);
                e.u32(s.len);
                e.text(&s.digest);
            });
        }
        TraceEntry::Fault {
            at,
            class,
            origin,
            seq,
            actor,
            code,
            parameter,
            metered,
        } => {
            e.u8(2);
            at_w(e, *at);
            e.u8(*class);
            e.u32(*origin);
            e.u64(*seq);
            e.u32(*actor);
            e.text(code);
            e.u32(*parameter);
            e.u64(*metered);
        }
        TraceEntry::Intervention { at, seq, len } => {
            e.u8(3);
            at_w(e, *at);
            e.u64(*seq);
            e.u32(*len);
        }
    }
}
fn trace_entry_r(d: &mut Decoder) -> Option<TraceEntry> {
    Some(match d.u8()? {
        1 => TraceEntry::Handled {
            at: at_r(d)?,
            class: d.u8()?,
            origin: d.u32()?,
            seq: d.u64()?,
            target: d.u32()?,
            metered: d.u64()?,
            draws: d.u32()?,
            effects: d.u32()?,
            sends: d.list(|d| {
                Some(SendSummary {
                    seq: d.u64()?,
                    port: d.u32()?,
                    len: d.u32()?,
                    digest: d.text()?,
                })
            })?,
        },
        2 => TraceEntry::Fault {
            at: at_r(d)?,
            class: d.u8()?,
            origin: d.u32()?,
            seq: d.u64()?,
            actor: d.u32()?,
            code: d.text()?,
            parameter: d.u32()?,
            metered: d.u64()?,
        },
        3 => TraceEntry::Intervention {
            at: at_r(d)?,
            seq: d.u64()?,
            len: d.u32()?,
        },
        _ => return None,
    })
}
pub(super) fn answer_w(e: &mut Encoder, a: &Answer) {
    match a {
        Answer::Scenario(s) => {
            e.u8(0);
            e.option(s.as_ref(), scenario_w);
        }
        Answer::RunState(r) => {
            e.u8(1);
            run_state_w(e, r.state);
            e.option(r.now, at_w);
            e.u64(r.handled);
            e.u64(r.records);
            opt_text_w(e, &r.chain);
            e.option(r.seed, Encoder::u64);
            pacing_w(e, r.pacing);
            opt_text_w(e, &r.ended);
            bool_w(e, r.reproducible);
            opt_text_w(e, &r.run_id);
        }
        Answer::Actor(a) => {
            e.u8(2);
            e.option(a.as_ref(), |e, a| {
                actor_summary_w(e, &a.summary);
                bool_w(e, a.alive);
                e.list(&a.faults, |e, f| {
                    at_w(e, f.at);
                    e.text(&f.code);
                    e.u64(f.metered);
                    e.u32(f.parameter);
                });
                e.list(&a.recent_telemetry, telemetry_w);
            });
        }
        Answer::TraceWindow(t) => {
            e.u8(3);
            e.list(t, trace_entry_w);
        }
        Answer::Capacity(c) => {
            e.u8(4);
            e.u64(c.instances);
            e.u64(c.memory_bytes);
            e.option(c.swap_cost_us_per_kb, Encoder::u64);
        }
        Answer::Support(s) => {
            e.u8(5);
            e.u32(s.control_version);
            e.text(&s.florn_version);
            e.text(&s.host_build);
            e.text(&s.engine);
            e.text(&s.engine_version);
            e.text(&s.arch);
            e.list(&s.features, |e, f| e.text(f));
        }
        Answer::Admitted {
            module,
            admitted_sha256,
        } => {
            e.u8(6);
            module_summary_w(e, module);
            e.text(admitted_sha256);
        }
        Answer::Preview(p) => {
            e.u8(7);
            file_kind_w(e, p.kind);
            e.u64(p.bytes);
            bool_w(e, p.payloads);
            e.list(&p.modules, |e, (n, h)| {
                e.text(n);
                e.text(h);
            });
        }
        Answer::Exported { kind, name, bytes } => {
            e.u8(8);
            file_kind_w(e, *kind);
            e.text(name);
            e.bytes(bytes);
        }
        Answer::Done => e.u8(9),
        Answer::Decoded(fields) => {
            e.u8(10);
            e.option(fields.as_deref(), pairs_w);
        }
        Answer::Payload(bytes) => {
            e.u8(11);
            e.option(bytes.as_deref(), Encoder::bytes);
        }
        Answer::Checkpoints(list) => {
            e.u8(12);
            e.list(list, |e, c| {
                e.u32(c.index);
                at_w(e, c.instant);
                e.u64(c.records);
                e.u64(c.handled);
                e.u64(c.bytes);
            });
        }
    }
}
pub(super) fn answer_r(d: &mut Decoder) -> Option<Answer> {
    Some(match d.u8()? {
        0 => Answer::Scenario(d.option(scenario_r)?),
        1 => Answer::RunState(RunStateInfo {
            state: run_state_r(d)?,
            now: d.option(at_r)?,
            handled: d.u64()?,
            records: d.u64()?,
            chain: opt_text_r(d)?,
            seed: d.option(Decoder::u64)?,
            pacing: pacing_r(d)?,
            ended: opt_text_r(d)?,
            reproducible: bool_r(d)?,
            run_id: opt_text_r(d)?,
        }),
        2 => Answer::Actor(d.option(|d| {
            Some(ActorInfo {
                summary: actor_summary_r(d)?,
                alive: bool_r(d)?,
                faults: d.list(|d| {
                    Some(FaultInfo {
                        at: at_r(d)?,
                        code: d.text()?,
                        metered: d.u64()?,
                        parameter: d.u32()?,
                    })
                })?,
                recent_telemetry: d.list(telemetry_r)?,
            })
        })?),
        3 => Answer::TraceWindow(d.list(trace_entry_r)?),
        4 => Answer::Capacity(CapacityInfo {
            instances: d.u64()?,
            memory_bytes: d.u64()?,
            swap_cost_us_per_kb: d.option(Decoder::u64)?,
        }),
        5 => Answer::Support(SupportInfo {
            control_version: d.u32()?,
            florn_version: d.text()?,
            host_build: d.text()?,
            engine: d.text()?,
            engine_version: d.text()?,
            arch: d.text()?,
            features: d.list(Decoder::text)?,
        }),
        6 => Answer::Admitted {
            module: module_summary_r(d)?,
            admitted_sha256: d.text()?,
        },
        7 => Answer::Preview(ExportPreview {
            kind: file_kind_r(d)?,
            bytes: d.u64()?,
            payloads: bool_r(d)?,
            modules: d.list(|d| Some((d.text()?, d.text()?)))?,
        }),
        8 => Answer::Exported {
            kind: file_kind_r(d)?,
            name: d.text()?,
            bytes: d.bytes()?,
        },
        9 => Answer::Done,
        10 => Answer::Decoded(d.option(pairs_r)?),
        11 => Answer::Payload(d.option(Decoder::bytes)?),
        12 => Answer::Checkpoints(d.list(|d| {
            Some(CheckpointSummary {
                index: d.u32()?,
                instant: at_r(d)?,
                records: d.u64()?,
                handled: d.u64()?,
                bytes: d.u64()?,
            })
        })?),
        _ => return None,
    })
}
