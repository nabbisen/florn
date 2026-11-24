//! The exports: a run record, a diagnostic log, a package, a checkpoint (RFC 005 rules 29 and 30).

use super::*;

impl<E: Engine> Controller<E> {
    pub(super) fn export(&mut self, kind: ExportKind, confirmed: bool, out: &mut Vec<Message>) {
        // A record is a run at rest: not while a paced advance is in progress.
        if self.state == RunState::Running {
            return self.notify(out, self.not_now("export"));
        }
        let file_kind = kind.file_kind();
        let built: Built = match kind {
            ExportKind::RunRecord { trace, payloads } => match &mut self.run {
                None => return self.notify(out, self.not_now("export")),
                Some(run) => {
                    // The whole trace, assembled by replay where the Host kept only the records
                    // since the last checkpoint (DATA-008).
                    let records = if trace { run.full_trace() } else { None };
                    let contents = Contents {
                        run: Some(run.run_toml()),
                        trace: records,
                        payloads: if payloads {
                            Some(run.payloads().clone())
                        } else {
                            None
                        },
                        log: run.log().clone(),
                        readme: None,
                    };
                    let modules = contents
                        .run
                        .as_ref()
                        .map(|r| {
                            r.inputs
                                .modules
                                .iter()
                                .map(|m| (m.name.clone(), hex(&m.source_sha256)))
                                .collect()
                        })
                        .unwrap_or_default();
                    record::write(&contents).map(|b| {
                        (
                            format!("{}.{}", run.run_id(), file_kind.extension()),
                            b,
                            payloads,
                            modules,
                        )
                    })
                }
            },
            ExportKind::DiagnosticLog => match &self.run {
                None => return self.notify(out, self.not_now("export")),
                Some(run) => Ok((
                    format!("{}.{}", run.run_id(), file_kind.extension()),
                    run.log().to_jsonl().into_bytes(),
                    false,
                    Vec::new(),
                )),
            },
            ExportKind::Checkpoint { index } => match &self.run {
                None => return self.notify(out, self.not_now("export")),
                Some(run) => match run.kept().get(index as usize) {
                    None => {
                        return self
                            .notify(out, control_invalid("export", "out-of-range", "checkpoint"));
                    }
                    Some(k) => {
                        let modules = run
                            .inputs()
                            .modules
                            .iter()
                            .map(|m| (m.name.clone(), hex(&m.source_sha256)))
                            .collect();
                        Ok((
                            format!(
                                "{}-{}.{}",
                                run.run_id(),
                                k.instant.tick,
                                file_kind.extension()
                            ),
                            k.checkpoint().encode(),
                            false,
                            modules,
                        ))
                    }
                },
            },
            ExportKind::ParentRecord => match &self.parent_record {
                None => return self.notify(out, self.not_now("export")),
                Some(contents) => {
                    let run = contents.run.as_ref().expect("a kept record has run.toml");
                    let modules = run
                        .inputs
                        .modules
                        .iter()
                        .map(|m| (m.name.clone(), hex(&m.source_sha256)))
                        .collect();
                    record::write(contents).map(|b| {
                        (
                            format!("{}.{}", run.run_id, file_kind.extension()),
                            b,
                            false,
                            modules,
                        )
                    })
                }
            },
            ExportKind::Package => match &self.scenario {
                None => return self.notify(out, self.not_now("export")),
                Some(l) => {
                    let (found, _) = self.modules_for(&l.manifest);
                    let by_hash: BTreeMap<String, Vec<u8>> = found
                        .values()
                        .map(|b| (hex(&florn_admission::sha256(b)), b.clone()))
                        .collect();
                    let meta = PackageMeta {
                        partial: by_hash.len() < l.manifest.modules.len(),
                        created_by: Some(format!("florn {}", env!("CARGO_PKG_VERSION"))),
                        notes: None,
                    };
                    let modules = l
                        .manifest
                        .modules
                        .iter()
                        .map(|(n, m)| (n.clone(), hex(&m.sha256)))
                        .collect();
                    Package::write(&l.text, &l.manifest, &by_hash, &meta, None)
                        .map(|b| {
                            (
                                format!(
                                    "{}.{}",
                                    l.manifest.name.replace(' ', "-"),
                                    file_kind.extension()
                                ),
                                b,
                                false,
                                modules,
                            )
                        })
                        .map_err(|e| e.codes().join(","))
                }
            },
        };
        match built {
            Ok((name, bytes, payloads, modules)) => {
                let preview = ExportPreview {
                    kind: file_kind,
                    bytes: bytes.len() as u64,
                    payloads,
                    modules,
                };
                out.push(Message::Notification(Code::ExportPreview.notification(
                    Scope::file(&name),
                    vec![
                        Value::Enum(file_kind.as_str().into()),
                        Value::Bytes(bytes.len() as u64),
                        Value::Enum(if payloads { "yes" } else { "no" }.into()),
                    ],
                )));
                if confirmed {
                    out.push(Message::Answer(Answer::Exported {
                        kind: file_kind,
                        name,
                        bytes,
                    }));
                } else {
                    out.push(Message::Answer(Answer::Preview(preview)));
                }
            }
            Err(reason) => out.push(Message::Notification(Code::FileUnreadable.notification(
                Scope::file(file_kind.as_str()),
                vec![Value::Enum(file_kind.as_str().into()), Value::Text(reason)],
            ))),
        }
    }
}
