//! Stage 4, the transformation (RFC 003 §3): the passes in their fixed order, each preserving
//! custom sections; the state layout of the result (§6). The only code that calls the
//! instrumentation library (rule 9).

use fvm_wasm_instrument::gas_metering::{self, ConstantCostRules};
use fvm_wasm_instrument::stack_limiter;
use wasm_encoder::reencode::{self, Reencode, RoundtripReencoder};
use wasm_encoder::{CustomSection, ExportKind, ExportSection, Module, RawSection, Section};
use wasmparser::{ExternalKind, Parser, Payload, TypeRef, ValType, Validator};

use crate::error::{Error, Kind};
use crate::estimate::ClassLimits;
use crate::validate::admitted_features;
use crate::{BUDGET_IMPORT_MODULE, GLOBAL_EXPORT_PREFIX, STACK_EXPORT, TABLE_EXPORT_PREFIX};

fn failed(pass: &str, e: impl std::fmt::Display) -> Error {
    Error::new(Kind::TransformFailed {
        reason: format!("{pass}: {e}"),
    })
}

/// Pass 1: the memory's maximum becomes the class limit in pages; a lower declared maximum is kept.
fn cap_memory(bytes: &[u8], pages: u64) -> Result<Vec<u8>, Error> {
    struct Cap(u64);
    impl Reencode for Cap {
        type Error = core::convert::Infallible;
        fn memory_type(
            &mut self,
            m: wasmparser::MemoryType,
        ) -> Result<wasm_encoder::MemoryType, reencode::Error<Self::Error>> {
            let mut t = RoundtripReencoder.memory_type(m)?;
            t.maximum = Some(t.maximum.map_or(self.0, |x| x.min(self.0)));
            Ok(t)
        }
    }
    let mut module = Module::new();
    Cap(pages)
        .parse_core_module(&mut module, Parser::new(0), bytes)
        .map_err(|e| failed("cap memory", e))?;
    Ok(module.finish())
}

/// The counts an export pass needs: imported and defined globals, imported and defined tables,
/// existing exports (name, kind, index), every defined global's mutability.
struct Shape {
    imported_globals: u32,
    defined_globals: Vec<bool>,
    imported_tables: u32,
    defined_tables: u32,
    exports: Vec<(String, ExternalKind, u32)>,
}

fn shape(bytes: &[u8]) -> Result<Shape, Error> {
    let mut s = Shape {
        imported_globals: 0,
        defined_globals: Vec::new(),
        imported_tables: 0,
        defined_tables: 0,
        exports: Vec::new(),
    };
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| failed("shape", e))? {
            Payload::ImportSection(r) => {
                for g in r {
                    if let wasmparser::Imports::Single(_, imp) =
                        g.map_err(|e| failed("shape", e))?
                    {
                        match imp.ty {
                            TypeRef::Global(_) => s.imported_globals += 1,
                            TypeRef::Table(_) => s.imported_tables += 1,
                            _ => {}
                        }
                    }
                }
            }
            Payload::GlobalSection(r) => {
                for g in r {
                    s.defined_globals
                        .push(g.map_err(|e| failed("shape", e))?.ty.mutable);
                }
            }
            Payload::TableSection(r) => s.defined_tables = r.count(),
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.map_err(|e| failed("shape", e))?;
                    s.exports.push((e.name.to_string(), e.kind, e.index));
                }
            }
            _ => {}
        }
    }
    Ok(s)
}

/// Rewrite the export section: shift global indices by `global_shift`, then add `extra`.
fn rewrite_exports(
    bytes: &[u8],
    global_shift: u32,
    extra: Vec<(String, ExportKind, u32)>,
) -> Result<Vec<u8>, Error> {
    struct Exporter {
        shift: u32,
        extra: Vec<(String, ExportKind, u32)>,
    }
    impl Reencode for Exporter {
        type Error = core::convert::Infallible;
        fn parse_export_section(
            &mut self,
            exports: &mut ExportSection,
            section: wasmparser::ExportSectionReader<'_>,
        ) -> Result<(), reencode::Error<Self::Error>> {
            for e in section {
                let e = e?;
                let index = if e.kind == ExternalKind::Global {
                    e.index + self.shift
                } else {
                    e.index
                };
                exports.export(e.name, self.export_kind(e.kind)?, index);
            }
            for (name, kind, idx) in &self.extra {
                exports.export(name, *kind, *idx);
            }
            Ok(())
        }
    }
    let mut module = Module::new();
    Exporter {
        shift: global_shift,
        extra,
    }
    .parse_core_module(&mut module, Parser::new(0), bytes)
    .map_err(|e| failed("exports", e))?;
    Ok(module.finish())
}

/// Passes 2 and 3: the library's stack limiter, then its metering with the budget as an imported
/// global of `env`. The library leaves exported globals' indices unshifted after it inserts the
/// imported global at index 0; florn corrects them and exports the height global as `florn.stack`.
fn limit_and_meter(bytes: &[u8], limits: &ClassLimits) -> Result<Vec<u8>, Error> {
    let before = shape(bytes)?;
    if before.imported_globals != 0 {
        return Err(failed("limit stack", "the source imports a global"));
    }
    let stack = u32::try_from(limits.stack)
        .map_err(|_| failed("limit stack", "the stack limit exceeds 32 bits"))?;
    let limited =
        stack_limiter::inject(bytes, stack).map_err(|e| failed("limit stack", format!("{e:#}")))?;
    let height_global = before.defined_globals.len() as u32; // appended as the last defined global
    let metered = gas_metering::inject(
        &limited,
        &ConstantCostRules::new(1, 0),
        BUDGET_IMPORT_MODULE,
    )
    .map_err(|e| failed("meter", format!("{e:#}")))?;
    // The budget global is import 0 of the global index space: every other global moved up by one.
    rewrite_exports(
        &metered,
        1,
        vec![(
            STACK_EXPORT.to_string(),
            ExportKind::Global,
            height_global + 1,
        )],
    )
}

/// Pass 4: every defined mutable global except the height global as `florn.global.<n>`, every
/// table as `florn.table.<n>`, by absolute index; existing names are kept, none is duplicated.
fn export_state(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let s = shape(bytes)?;
    let height = s
        .exports
        .iter()
        .find(|(n, k, _)| n == STACK_EXPORT && *k == ExternalKind::Global)
        .map(|(_, _, i)| *i);
    let mut extra = Vec::new();
    for (i, mutable) in s.defined_globals.iter().enumerate() {
        let idx = s.imported_globals + i as u32;
        if !*mutable || Some(idx) == height {
            continue;
        }
        let name = format!("{GLOBAL_EXPORT_PREFIX}{idx}");
        if !s.exports.iter().any(|(n, _, _)| *n == name) {
            extra.push((name, ExportKind::Global, idx));
        }
    }
    for i in 0..(s.imported_tables + s.defined_tables) {
        let name = format!("{TABLE_EXPORT_PREFIX}{i}");
        if !s.exports.iter().any(|(n, _, _)| *n == name) {
            extra.push((name, ExportKind::Table, i));
        }
    }
    rewrite_exports(bytes, 0, extra)
}

/// Custom sections carry no code; the library keeps only one per module (it stores sections by
/// id). They are set aside before the passes and appended after, in their original order.
type CustomSections = Vec<(String, Vec<u8>)>;

fn split_custom(bytes: &[u8]) -> Result<(Vec<u8>, CustomSections), Error> {
    let mut module = Module::new();
    let mut custom = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| failed("custom sections", e))?;
        match payload {
            Payload::CustomSection(c) => custom.push((c.name().to_string(), c.data().to_vec())),
            other => {
                if let Some((id, range)) = other.as_section() {
                    module.section(&RawSection {
                        id,
                        data: &bytes[range.start as usize..range.end as usize],
                    });
                }
            }
        }
    }
    Ok((module.finish(), custom))
}

/// The whole transformation of RFC 003 rule 7, validated. Custom sections pass through every pass.
pub fn transform(source: &[u8], limits: &ClassLimits) -> Result<Vec<u8>, Error> {
    let (stripped, custom) = split_custom(source)?;
    let capped = cap_memory(&stripped, limits.memory_pages())?;
    let metered = limit_and_meter(&capped, limits)?;
    let mut exported = export_state(&metered)?;
    for (name, data) in &custom {
        CustomSection {
            name: name.as_str().into(),
            data: data.as_slice().into(),
        }
        .append_to(&mut exported);
    }
    Validator::new_with_features(admitted_features())
        .validate_all(&exported)
        .map_err(|e| failed("validate", e))?;
    Ok(exported)
}

/// A global's core type, for the state layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalKind {
    I32,
    I64,
    F32,
    F64,
}

/// The state of an admitted module as the Host reads and restores it (RFC 003 rule 21):
/// the exported globals with their types and the exported tables with their sizes, in export order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateLayout {
    pub globals: Vec<(String, GlobalKind)>,
    pub tables: Vec<(String, u64)>,
}

pub fn state_layout(admitted: &[u8]) -> Result<StateLayout, Error> {
    let mut global_types: Vec<ValType> = Vec::new();
    let mut table_sizes: Vec<u64> = Vec::new();
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(admitted) {
        match payload.map_err(|e| failed("layout", e))? {
            Payload::ImportSection(r) => {
                for g in r {
                    if let wasmparser::Imports::Single(_, imp) =
                        g.map_err(|e| failed("layout", e))?
                    {
                        match imp.ty {
                            TypeRef::Global(t) => global_types.push(t.content_type),
                            TypeRef::Table(t) => table_sizes.push(t.initial),
                            _ => {}
                        }
                    }
                }
            }
            Payload::GlobalSection(r) => {
                for g in r {
                    global_types.push(g.map_err(|e| failed("layout", e))?.ty.content_type);
                }
            }
            Payload::TableSection(r) => {
                for t in r {
                    table_sizes.push(t.map_err(|e| failed("layout", e))?.ty.initial);
                }
            }
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.map_err(|e| failed("layout", e))?;
                    exports.push((e.name.to_string(), e.kind, e.index));
                }
            }
            _ => {}
        }
    }
    let mut layout = StateLayout::default();
    for (name, kind, index) in exports {
        if name.starts_with(GLOBAL_EXPORT_PREFIX) && kind == ExternalKind::Global {
            let k = match global_types.get(index as usize) {
                Some(ValType::I32) => GlobalKind::I32,
                Some(ValType::I64) => GlobalKind::I64,
                Some(ValType::F32) => GlobalKind::F32,
                Some(ValType::F64) => GlobalKind::F64,
                _ => return Err(failed("layout", format!("global {name} has no core type"))),
            };
            layout.globals.push((name, k));
        } else if name.starts_with(TABLE_EXPORT_PREFIX) && kind == ExternalKind::Table {
            let size = *table_sizes
                .get(index as usize)
                .ok_or_else(|| failed("layout", format!("table {name} out of range")))?;
            layout.tables.push((name, size));
        }
    }
    Ok(layout)
}
