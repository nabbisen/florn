//! The package: a ZIP archive with the extension `.florn` (RFC 004 §6): `manifest.toml`,
//! `modules/<sha256>.wasm` for every referenced module unless the package is partial,
//! `package.toml`, and an optional `README.md`. On import every module file is hashed and the
//! package is refused if any hash differs from its name or from the manifest's declaration.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use sha2::{Digest, Sha256};

use crate::HostMaxima;
use crate::canonical::hex;
use crate::error::{Error, Errors, Kind};
use crate::manifest::Manifest;

/// The package schema version this crate reads and writes.
pub const PACKAGE_VERSION: u64 = 1;
/// Maxima for a package (RFC 006 §7): entries and total bytes.
pub const MAX_ENTRIES: usize = 10_256;
pub const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_MODULE_BYTES: u64 = 32 * 1024 * 1024;

/// `package.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageMeta {
    pub partial: bool,
    pub created_by: Option<String>,
    pub notes: Option<String>,
}

/// A read package: the manifest text and model, the module bytes by hash, and what is missing.
#[derive(Clone, Debug)]
pub struct Package {
    pub manifest_text: String,
    pub manifest: Manifest,
    pub meta: PackageMeta,
    /// Module bytes by their SHA-256 (hex), verified.
    pub modules: BTreeMap<String, Vec<u8>>,
    /// Hashes the manifest references that the package does not contain (a partial package, rule 35).
    pub missing: Vec<String>,
    pub readme: Option<String>,
}

impl Package {
    /// Read a package from its bytes (rules 33 to 35). Every error is reported.
    pub fn read(bytes: &[u8], host: &HostMaxima) -> Result<Package, Errors> {
        let mut errors = Errors::default();
        if bytes.len() as u64 > MAX_BYTES {
            errors.push(Error::new(Kind::ManifestTooLarge {
                what: "package-bytes",
                found: bytes.len() as u64,
                maximum: MAX_BYTES,
            }));
            return Err(errors);
        }
        let mut archive = match zip::ZipArchive::new(Cursor::new(bytes)) {
            Ok(a) => a,
            Err(e) => {
                errors.push(Error::new(Kind::FileUnreadable {
                    kind: "package",
                    reason: e.to_string(),
                }));
                return Err(errors);
            }
        };
        if archive.len() > MAX_ENTRIES {
            errors.push(Error::new(Kind::ManifestTooLarge {
                what: "package-entries",
                found: archive.len() as u64,
                maximum: MAX_ENTRIES as u64,
            }));
            return Err(errors);
        }
        fn read_entry(
            archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
            name: &str,
            max: u64,
        ) -> Option<Vec<u8>> {
            let mut f = archive.by_name(name).ok()?;
            if f.size() > max {
                return None;
            }
            let mut v = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut v).ok()?;
            Some(v)
        }
        let manifest_text = match read_entry(
            &mut archive,
            "manifest.toml",
            crate::maxima::MANIFEST_BYTES as u64,
        )
        .and_then(|b| String::from_utf8(b).ok())
        {
            Some(t) => t,
            None => {
                errors.push(Error::at(
                    Kind::ManifestMissingField {
                        field: "manifest.toml".into(),
                    },
                    "manifest.toml",
                ));
                return Err(errors);
            }
        };
        let meta = match read_entry(&mut archive, "package.toml", 64 * 1024)
            .and_then(|b| String::from_utf8(b).ok())
        {
            Some(t) => match parse_meta(&t) {
                Ok(m) => m,
                Err(e) => {
                    errors.push(*e);
                    PackageMeta::default()
                }
            },
            None => {
                errors.push(Error::at(
                    Kind::ManifestMissingField {
                        field: "package.toml".into(),
                    },
                    "package.toml",
                ));
                PackageMeta::default()
            }
        };
        let readme = read_entry(&mut archive, "README.md", 1024 * 1024)
            .and_then(|b| String::from_utf8(b).ok());
        let manifest = match Manifest::parse(&manifest_text, host) {
            Ok(m) => m,
            Err(e) => {
                errors.0.extend(e.0);
                return Err(errors);
            }
        };
        let mut modules = BTreeMap::new();
        let mut missing = Vec::new();
        let mut checked: Vec<String> = Vec::new();
        for m in manifest.modules.values() {
            let declared = hex(&m.sha256);
            checked.push(declared.clone());
            let entry = format!("modules/{declared}.wasm");
            match read_entry(&mut archive, &entry, MAX_MODULE_BYTES) {
                Some(b) => {
                    let actual = hex(&Sha256::digest(&b));
                    if actual != declared {
                        errors.push(Error::at(
                            Kind::ModuleHashMismatch {
                                module: m.name.clone(),
                                declared: declared.clone(),
                                actual,
                            },
                            &entry,
                        ));
                    } else {
                        modules.insert(declared, b);
                    }
                }
                None => missing.push(declared),
            }
        }
        // Every module file must be named by its own hash (rule 34), even ones the manifest does not reference.
        let names: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();
        for name in names {
            let Some(h) = name
                .strip_prefix("modules/")
                .and_then(|n| n.strip_suffix(".wasm"))
            else {
                continue;
            };
            if checked.iter().any(|c| c == h) {
                continue;
            }
            if let Some(b) = read_entry(&mut archive, &name, MAX_MODULE_BYTES) {
                let actual = hex(&Sha256::digest(&b));
                if actual != h {
                    errors.push(Error::at(
                        Kind::PackageEntryMismatch {
                            entry: name.clone(),
                            declared: h.into(),
                            actual,
                        },
                        &name,
                    ));
                }
            }
        }
        if !missing.is_empty() && !meta.partial {
            errors.push(Error::new(Kind::PackagePartial {
                missing: missing.len() as u64,
            }));
        }
        errors.into_result(Package {
            manifest_text,
            manifest,
            meta,
            modules,
            missing,
            readme,
        })
    }

    /// Write a package (rule 33). `modules` are the bytes of every module the manifest references,
    /// keyed by hash; an absent one makes the package partial, and `meta.partial` must say so.
    pub fn write(
        manifest_text: &str,
        manifest: &Manifest,
        modules: &BTreeMap<String, Vec<u8>>,
        meta: &PackageMeta,
        readme: Option<&str>,
    ) -> Result<Vec<u8>, Errors> {
        let mut errors = Errors::default();
        let mut missing = 0u64;
        for m in manifest.modules.values() {
            let h = hex(&m.sha256);
            match modules.get(&h) {
                Some(b) => {
                    let actual = hex(&Sha256::digest(b));
                    if actual != h {
                        errors.push(Error::new(Kind::ModuleHashMismatch {
                            module: m.name.clone(),
                            declared: h.clone(),
                            actual,
                        }));
                    }
                }
                None => missing += 1,
            }
        }
        if missing > 0 && !meta.partial {
            errors.push(Error::new(Kind::PackagePartial { missing }));
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            let put = |zip: &mut zip::ZipWriter<&mut Cursor<Vec<u8>>>,
                       name: &str,
                       data: &[u8]|
             -> Result<(), Errors> {
                zip.start_file(name, opts)
                    .and_then(|_| zip.write_all(data).map_err(Into::into))
                    .map_err(|e| {
                        Errors(vec![Error::new(Kind::FileUnreadable {
                            kind: "package",
                            reason: e.to_string(),
                        })])
                    })
            };
            put(&mut zip, "manifest.toml", manifest_text.as_bytes())?;
            let mut pt = format!(
                "florn-package = {PACKAGE_VERSION}\npartial = {}\n",
                meta.partial
            );
            if let Some(c) = &meta.created_by {
                pt.push_str(&format!("created-by = {}\n", toml_string(c)));
            }
            if let Some(n) = &meta.notes {
                pt.push_str(&format!("notes = {}\n", toml_string(n)));
            }
            put(&mut zip, "package.toml", pt.as_bytes())?;
            if let Some(r) = readme {
                put(&mut zip, "README.md", r.as_bytes())?;
            }
            for m in manifest.modules.values() {
                let h = hex(&m.sha256);
                if let Some(b) = modules.get(&h) {
                    put(&mut zip, &format!("modules/{h}.wasm"), b)?;
                }
            }
            zip.finish().map_err(|e| {
                Errors(vec![Error::new(Kind::FileUnreadable {
                    kind: "package",
                    reason: e.to_string(),
                })])
            })?;
        }
        Ok(cursor.into_inner())
    }
}

fn toml_string(s: &str) -> String {
    format!("{:?}", s) // Rust's debug escaping is valid TOML basic-string escaping for the characters that matter
}

fn parse_meta(text: &str) -> Result<PackageMeta, Box<Error>> {
    let t: toml::Table = text.parse().map_err(|e: toml::de::Error| {
        Box::new(Error::at(
            Kind::ManifestInvalid {
                reason: e.message().to_string(),
            },
            "package.toml",
        ))
    })?;
    let version = match t.get("florn-package") {
        Some(toml::Value::Integer(v)) => *v as u64,
        _ => {
            return Err(Box::new(Error::at(
                Kind::ManifestMissingField {
                    field: "package.toml:florn-package".into(),
                },
                "package.toml",
            )));
        }
    };
    if version != PACKAGE_VERSION {
        return Err(Box::new(Error::at(
            Kind::ManifestSchemaVersion {
                found: version,
                supported: PACKAGE_VERSION,
            },
            "package.toml",
        )));
    }
    let partial = matches!(t.get("partial"), Some(toml::Value::Boolean(true)));
    let text_of = |k: &str| match t.get(k) {
        Some(toml::Value::String(s)) => Some(s.clone()),
        _ => None,
    };
    Ok(PackageMeta {
        partial,
        created_by: text_of("created-by"),
        notes: text_of("notes"),
    })
}
