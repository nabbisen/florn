//! The run record (RFC 005 rule 29): a ZIP archive, `.florn-run`, with `run.toml`, the trace in
//! chunks, payloads by digest, the diagnostic log, and a readme. It contains no scenario text and
//! no module bytes.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use florn_kernel::{Digest, Record};
use florn_model::canonical::hex;

use crate::identity::RunToml;
use crate::log::DiagnosticLog;

pub const EXTENSION: &str = "florn-run";
/// The largest record the reader accepts, and its largest entry.
pub const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 1_000_000;

/// What goes into a run record (RFC 005 rule 30: the preview lists the same).
#[derive(Clone, Debug, Default)]
pub struct Contents {
    pub run: Option<RunToml>,
    /// The trace records, chunked by the chain interval, when kept.
    pub trace: Option<Vec<Record>>,
    /// Payload bytes by digest, when included.
    pub payloads: Option<BTreeMap<Digest, Vec<u8>>>,
    pub log: DiagnosticLog,
    pub readme: Option<String>,
}

/// The preview of RFC 005 rule 30: what the file will contain and its size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub modules: Vec<(String, String)>,
    pub payloads: bool,
    pub trace_records: u64,
    pub bytes: u64,
}

pub fn write(c: &Contents) -> Result<Vec<u8>, String> {
    let run = c.run.as_ref().ok_or("a run record needs run.toml")?;
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let put = |zip: &mut zip::ZipWriter<&mut Cursor<Vec<u8>>>,
                   name: &str,
                   bytes: &[u8]|
         -> Result<(), String> {
            zip.start_file(name, opts).map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())
        };
        put(&mut zip, "run.toml", run.to_toml().as_bytes())?;
        if let Some(records) = &c.trace {
            let interval = run.chain_interval.max(1) as usize;
            for (n, chunk) in records.chunks(interval).enumerate() {
                let mut bytes = Vec::new();
                for r in chunk {
                    let enc = florn_kernel::trace::encode(r);
                    bytes.extend_from_slice(&(enc.len() as u32).to_le_bytes());
                    bytes.extend_from_slice(&enc);
                }
                put(&mut zip, &format!("trace/{n}.bin"), &bytes)?;
            }
        }
        if let Some(payloads) = &c.payloads {
            for (d, bytes) in payloads {
                put(&mut zip, &format!("payloads/{}", hex(d)), bytes)?;
            }
        }
        put(&mut zip, "diagnostic.log", c.log.to_jsonl().as_bytes())?;
        put(&mut zip, "diagnostic.bin", &c.log.encode())?;
        let readme = match &c.readme {
            Some(r) => r.clone(),
            None => default_readme(run),
        };
        put(&mut zip, "README.md", readme.as_bytes())?;
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(cursor.into_inner())
}

pub fn preview(c: &Contents) -> Result<Preview, String> {
    let run = c.run.as_ref().ok_or("a run record needs run.toml")?;
    let bytes = write(c)?;
    Ok(Preview {
        modules: run
            .inputs
            .modules
            .iter()
            .map(|m| (m.name.clone(), hex(&m.source_sha256)))
            .collect(),
        payloads: c.payloads.is_some(),
        trace_records: c.trace.as_ref().map_or(0, |t| t.len() as u64),
        bytes: bytes.len() as u64,
    })
}

fn default_readme(run: &RunToml) -> String {
    let mut s = String::from("# florn run record\n\n");
    s.push_str("What happened in one run of one scenario: the run's inputs and identity in `run.toml`, the stored chain values, the diagnostic log, and optionally the trace and payloads. It references the scenario and the modules by hash and contains neither.\n\n");
    s.push_str(&format!(
        "- scenario: `{}`\n- seed: `{}`\n- records: {}\n",
        hex(&run.inputs.scenario_sha256),
        run.inputs.root_seed,
        run.records
    ));
    for m in &run.inputs.modules {
        s.push_str(&format!(
            "- module `{}`: `{}`\n",
            m.name,
            hex(&m.source_sha256)
        ));
    }
    s
}

/// What a reader finds in a run record.
#[derive(Clone, Debug)]
pub struct Read_ {
    pub run: RunToml,
    pub trace: Option<Vec<Record>>,
    pub payloads: BTreeMap<Digest, Vec<u8>>,
    pub log_jsonl: String,
    pub entries: Vec<String>,
}

pub fn read(bytes: &[u8]) -> Result<Read_, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("file.too-large".into());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if archive.len() > MAX_ENTRIES {
        return Err("file.too-large".into());
    }
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let f = archive.by_index(i).map_err(|e| e.to_string())?;
        if f.size() > MAX_ENTRY_BYTES {
            return Err("file.too-large".into());
        }
        entries.push(f.name().map_err(|e| e.to_string())?.into_owned());
    }
    let mut get = |name: &str| -> Result<Option<Vec<u8>>, String> {
        let Ok(mut f) = archive.by_name(name) else {
            return Ok(None);
        };
        let mut v = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut v).map_err(|e| e.to_string())?;
        Ok(Some(v))
    };
    let run_text = get("run.toml")?.ok_or("run.toml missing")?;
    let run_text = String::from_utf8(run_text).map_err(|e| e.to_string())?;
    let run = RunToml::parse(&run_text).ok_or("run.toml is not a run record of this version")?;
    let log_jsonl = get("diagnostic.log")?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let mut trace: Option<Vec<Record>> = None;
    let mut n = 0;
    while let Some(chunk) = get(&format!("trace/{n}.bin"))? {
        let records = trace.get_or_insert_with(Vec::new);
        let mut pos = 0usize;
        while pos + 4 <= chunk.len() {
            let len = u32::from_le_bytes(chunk[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            let end = pos.checked_add(len).ok_or("trace chunk malformed")?;
            let enc = chunk.get(pos..end).ok_or("trace chunk malformed")?;
            records.push(florn_kernel::trace::decode(enc).ok_or("trace record malformed")?);
            pos = end;
        }
        n += 1;
    }
    let mut payloads = BTreeMap::new();
    for e in entries.clone() {
        if let Some(h) = e.strip_prefix("payloads/")
            && let Some(d) = crate::world::unhex(h).and_then(|v| v.try_into().ok())
            && let Some(b) = get(&e)?
        {
            payloads.insert(d, b);
        }
    }
    Ok(Read_ {
        run,
        trace,
        payloads,
        log_jsonl,
        entries,
    })
}
