//! Stage 1, validation (RFC 003 §2): the exact feature set, the structure, the imports and
//! exports of the world, the maxima, no start section, no table mutation.

use std::ops::Range;

use florn_abi::FlatType;
use wasmparser::{
    ExternalKind, FuncType, Imports, Operator, Parser, Payload, TypeRef, ValType, Validator,
    WasmFeatures,
};

use crate::error::{Error, Errors, Kind};
use crate::world::{FuncSig, WorldSpec, realloc_sig};
use crate::{BUDGET_IMPORT_MODULE, BUDGET_IMPORT_NAME, RESERVED_PREFIX};

/// The WebAssembly page, in bytes.
pub const PAGE: u64 = 65_536;

/// The Host maxima on a module (RFC 003 rule 5, counts row; rule 19, size row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleMaxima {
    pub bytes: u64,
    pub functions: u64,
    pub globals: u64,
    pub tables: u64,
    /// The sum of the tables' declared initial sizes.
    pub table_elements: u64,
    /// Data and element segments together.
    pub segments: u64,
    /// The sum of the data segments' bytes.
    pub segment_bytes: u64,
    pub imports: u64,
    pub exports: u64,
    pub custom_sections: u64,
}

impl Default for ModuleMaxima {
    fn default() -> Self {
        Self {
            bytes: florn_model::package::MAX_MODULE_BYTES,
            functions: 100_000,
            globals: 10_000,
            tables: 16,
            table_elements: 1_000_000,
            segments: 10_000,
            segment_bytes: 32 * 1024 * 1024,
            imports: 1_000,
            exports: 1_000,
            custom_sections: 64,
        }
    }
}

/// The feature set of RFC 003 rule 5: what Rust's default `wasm32-unknown-unknown` target emits.
pub fn admitted_features() -> WasmFeatures {
    WasmFeatures::FLOATS
        | WasmFeatures::GC_TYPES
        | WasmFeatures::MUTABLE_GLOBAL
        | WasmFeatures::SIGN_EXTENSION
        | WasmFeatures::SATURATING_FLOAT_TO_INT
        | WasmFeatures::MULTI_VALUE
        | WasmFeatures::BULK_MEMORY
        | WasmFeatures::REFERENCE_TYPES
}

/// The proposals that are refused by name (RFC 003 rule 5, feature row).
const REFUSED_FEATURES: &[(&str, WasmFeatures)] = &[
    ("threads", WasmFeatures::THREADS),
    ("simd", WasmFeatures::SIMD),
    (
        "relaxed-simd",
        WasmFeatures::SIMD.union(WasmFeatures::RELAXED_SIMD),
    ),
    ("exception-handling", WasmFeatures::EXCEPTIONS),
    (
        "legacy-exception-handling",
        WasmFeatures::EXCEPTIONS.union(WasmFeatures::LEGACY_EXCEPTIONS),
    ),
    ("tail-call", WasmFeatures::TAIL_CALL),
    ("memory64", WasmFeatures::MEMORY64),
    ("multi-memory", WasmFeatures::MULTI_MEMORY),
    ("function-references", WasmFeatures::FUNCTION_REFERENCES),
    (
        "gc",
        WasmFeatures::FUNCTION_REFERENCES.union(WasmFeatures::GC),
    ),
    ("custom-page-sizes", WasmFeatures::CUSTOM_PAGE_SIZES),
    ("extended-const", WasmFeatures::EXTENDED_CONST),
    ("wide-arithmetic", WasmFeatures::WIDE_ARITHMETIC),
    (
        "shared-everything-threads",
        WasmFeatures::THREADS.union(WasmFeatures::SHARED_EVERYTHING_THREADS),
    ),
    (
        "stack-switching",
        WasmFeatures::EXCEPTIONS
            .union(WasmFeatures::FUNCTION_REFERENCES)
            .union(WasmFeatures::GC)
            .union(WasmFeatures::STACK_SWITCHING),
    ),
    ("compact-imports", WasmFeatures::COMPACT_IMPORTS),
];

/// What stage 1 learned about a valid source module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceInfo {
    pub memory_initial_pages: u64,
    pub memory_maximum_pages: Option<u64>,
    /// Every custom section, in order: name and the byte range of its payload in the source.
    pub custom_sections: Vec<(String, Range<usize>)>,
    pub functions: u64,
    pub defined_globals: u64,
    pub tables: u64,
    pub imports: Vec<(String, String)>,
    pub exports: Vec<String>,
}

fn flat(t: &ValType) -> Option<FlatType> {
    Some(match t {
        ValType::I32 => FlatType::I32,
        ValType::I64 => FlatType::I64,
        ValType::F32 => FlatType::F32,
        ValType::F64 => FlatType::F64,
        _ => return None,
    })
}

fn same_sig(ty: &FuncType, sig: &FuncSig) -> bool {
    let params: Option<Vec<FlatType>> = ty.params().iter().map(flat).collect();
    let results: Option<Vec<FlatType>> = ty.results().iter().map(flat).collect();
    params.as_deref() == Some(&sig.params[..]) && results.as_deref() == Some(&sig.results[..])
}

/// The custom sections of any parseable module, before validation: name and payload range.
pub fn custom_sections(bytes: &[u8]) -> Result<Vec<(String, Range<usize>)>, Error> {
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CustomSection(c) = payload.map_err(|e| {
            Error::new(Kind::Invalid {
                reason: e.to_string(),
            })
        })? {
            let start = c.data_offset() as usize;
            out.push((c.name().to_string(), start..start + c.data().len()));
        }
    }
    Ok(out)
}

/// Validate a source module against the world and the maxima. `memory_limit_pages` is the actor
/// class's memory limit, for the initial-memory row; the estimate repeats that check per actor.
pub fn validate(
    bytes: &[u8],
    world: &WorldSpec,
    maxima: &ModuleMaxima,
    memory_limit_pages: u64,
) -> Result<SourceInfo, Errors> {
    let mut errors = Errors::default();
    if bytes.len() as u64 > maxima.bytes {
        return Err(Error::new(Kind::TooLarge {
            what: "bytes",
            found: bytes.len() as u64,
            maximum: maxima.bytes,
        })
        .into());
    }

    // The structure, without validation: counts, memories, imports, exports, the start section,
    // table mutation, custom sections. A malformed binary stops here as `module.invalid`.
    let mut info = SourceInfo::default();
    let mut types: Vec<FuncType> = Vec::new();
    let mut func_types: Vec<u32> = Vec::new(); // every function's type index, imports first
    let mut memories = 0u64;
    let mut memory: Option<wasmparser::MemoryType> = None;
    let mut imported_memory = false;
    let mut start = false;
    let mut segments = 0u64;
    let mut segment_bytes = 0u64;
    let mut data_end = 0u64;
    let mut table_elements = 0u64;
    let mut table_mutation: Option<&'static str> = None;
    let mut exports: Vec<(String, ExternalKind, u32)> = Vec::new();
    let mut compact_imports = false;
    let mut import_is_func: Vec<bool> = Vec::new();
    let structure = (|| -> Result<(), wasmparser::BinaryReaderError> {
        for payload in Parser::new(0).parse_all(bytes) {
            match payload? {
                Payload::TypeSection(s) => {
                    for t in s.into_iter_err_on_gc_types() {
                        types.push(t?);
                    }
                }
                Payload::ImportSection(s) => {
                    for group in s {
                        let imp = match group? {
                            Imports::Single(_, imp) => imp,
                            _ => {
                                compact_imports = true;
                                continue;
                            }
                        };
                        info.imports
                            .push((imp.module.to_string(), imp.name.to_string()));
                        import_is_func.push(matches!(imp.ty, TypeRef::Func(_)));
                        match imp.ty {
                            TypeRef::Func(t) => func_types.push(t),
                            TypeRef::Memory(_) => {
                                imported_memory = true;
                                memories += 1;
                            }
                            TypeRef::Table(_) => info.tables += 1,
                            _ => {}
                        }
                    }
                }
                Payload::FunctionSection(s) => {
                    for t in s {
                        func_types.push(t?);
                    }
                }
                Payload::TableSection(s) => {
                    for t in s {
                        table_elements = table_elements.saturating_add(t?.ty.initial);
                        info.tables += 1;
                    }
                }
                Payload::MemorySection(s) => {
                    for m in s {
                        let m = m?;
                        memories += 1;
                        if memory.is_none() {
                            memory = Some(m);
                        }
                    }
                }
                Payload::GlobalSection(s) => info.defined_globals = s.count() as u64,
                Payload::ExportSection(s) => {
                    for e in s {
                        let e = e?;
                        exports.push((e.name.to_string(), e.kind, e.index));
                    }
                }
                Payload::StartSection { .. } => start = true,
                Payload::ElementSection(s) => segments += s.count() as u64,
                Payload::DataSection(s) => {
                    for d in s {
                        let d = d?;
                        segments += 1;
                        segment_bytes = segment_bytes.saturating_add(d.data.len() as u64);
                        if let wasmparser::DataKind::Active { offset_expr, .. } = d.kind {
                            let mut r = offset_expr.get_operators_reader();
                            if let Ok(Operator::I32Const { value }) = r.read() {
                                data_end = data_end
                                    .max((value as u32 as u64).saturating_add(d.data.len() as u64));
                            }
                        }
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    info.functions += 1;
                    for op in body.get_operators_reader()? {
                        let name = match op? {
                            Operator::TableSet { .. } => "table.set",
                            Operator::TableGrow { .. } => "table.grow",
                            Operator::TableFill { .. } => "table.fill",
                            Operator::TableCopy { .. } => "table.copy",
                            Operator::TableInit { .. } => "table.init",
                            Operator::ElemDrop { .. } => "elem.drop",
                            _ => continue,
                        };
                        if table_mutation.is_none() {
                            table_mutation = Some(name);
                        }
                    }
                }
                Payload::CustomSection(c) => {
                    let start = c.data_offset() as usize;
                    info.custom_sections
                        .push((c.name().to_string(), start..start + c.data().len()));
                }
                _ => {}
            }
        }
        Ok(())
    })();
    if let Err(e) = structure {
        return Err(Error::new(Kind::Invalid {
            reason: e.to_string(),
        })
        .into());
    }

    // The validator, with the admitted features and nothing else; a refused feature is named.
    if let Err(e) = Validator::new_with_features(admitted_features()).validate_all(bytes) {
        if memories > 1 {
            return Err(Error::new(Kind::Memory { reason: "count" }).into());
        }
        if compact_imports {
            return Err(Error::new(Kind::FeatureRefused {
                feature: "compact-imports",
            })
            .into());
        }
        for (name, feature) in REFUSED_FEATURES {
            if Validator::new_with_features(admitted_features() | *feature)
                .validate_all(bytes)
                .is_ok()
            {
                return Err(Error::new(Kind::FeatureRefused { feature: name }).into());
            }
        }
        return Err(Error::new(Kind::Invalid {
            reason: e.to_string(),
        })
        .into());
    }

    // Counts (rule 4: before anything proportional to them is allocated).
    let counts: [(&str, u64, u64); 8] = [
        ("functions", func_types.len() as u64, maxima.functions),
        ("globals", info.defined_globals, maxima.globals),
        ("tables", info.tables, maxima.tables),
        ("table-elements", table_elements, maxima.table_elements),
        ("segments", segments, maxima.segments),
        ("segment-bytes", segment_bytes, maxima.segment_bytes),
        ("imports", info.imports.len() as u64, maxima.imports),
        ("exports", exports.len() as u64, maxima.exports),
    ];
    for (what, found, maximum) in counts {
        if found > maximum {
            errors.push(Error::new(Kind::TooLarge {
                what,
                found,
                maximum,
            }));
        }
    }
    if info.custom_sections.len() as u64 > maxima.custom_sections {
        errors.push(Error::new(Kind::TooLarge {
            what: "custom-sections",
            found: info.custom_sections.len() as u64,
            maximum: maxima.custom_sections,
        }));
    }

    // The memory: exactly one, defined, not shared, 32-bit, exported as `memory`.
    match (memories, imported_memory, memory) {
        (1, false, Some(m)) => {
            if m.shared {
                errors.push(Error::new(Kind::Memory { reason: "shared" }));
            } else if m.memory64 {
                errors.push(Error::new(Kind::Memory { reason: "64-bit" }));
            } else if !exports
                .iter()
                .any(|(n, k, _)| n == "memory" && *k == ExternalKind::Memory)
            {
                errors.push(Error::new(Kind::Memory {
                    reason: "not-exported",
                }));
            }
            info.memory_initial_pages = m.initial;
            info.memory_maximum_pages = m.maximum;
        }
        (_, true, _) => errors.push(Error::new(Kind::Memory { reason: "imported" })),
        _ => errors.push(Error::new(Kind::Memory { reason: "count" })),
    }
    if start {
        errors.push(Error::new(Kind::StartSection));
    }
    if let Some(instruction) = table_mutation {
        errors.push(Error::new(Kind::TableMutation { instruction }));
    }

    // Imports: functions of the world's interfaces with the canonical signature; nothing else.
    let mut import_funcs = 0usize;
    for (k, (module, name)) in info.imports.iter().enumerate() {
        let refused = Error::new(Kind::ImportRefused {
            module: module.clone(),
            name: name.clone(),
        });
        let reserved = module.starts_with(RESERVED_PREFIX)
            || name.starts_with(RESERVED_PREFIX)
            || (module == BUDGET_IMPORT_MODULE && name == BUDGET_IMPORT_NAME);
        match (reserved, import_is_func[k], world.import(module, name)) {
            (false, true, Some(sig)) => {
                let ok = func_types
                    .get(import_funcs)
                    .and_then(|t| types.get(*t as usize))
                    .is_some_and(|ty| same_sig(ty, sig));
                if !ok {
                    errors.push(refused);
                }
            }
            _ => errors.push(refused),
        }
        if import_is_func[k] {
            import_funcs += 1;
        }
    }

    // Exports: every required one, with its signature; nothing under `florn.`.
    let func_of = |index: u32| -> Option<&FuncType> {
        func_types
            .get(index as usize)
            .and_then(|t| types.get(*t as usize))
    };
    let mut required: Vec<FuncSig> = world.exports.clone();
    required.push(realloc_sig());
    for sig in &required {
        let present = exports.iter().any(|(n, k, i)| {
            n == &sig.name
                && *k == ExternalKind::Func
                && func_of(*i).is_some_and(|ty| same_sig(ty, sig))
        });
        if !present {
            errors.push(Error::new(Kind::ExportMissing {
                name: sig.name.clone(),
            }));
        }
    }
    for (name, _, _) in &exports {
        if name.starts_with(RESERVED_PREFIX) {
            errors.push(Error::new(Kind::ExportReserved { name: name.clone() }));
        }
    }
    info.exports = exports.into_iter().map(|(n, _, _)| n).collect();

    // Data segments fit the initial memory; the initial memory fits the class limit.
    if memory.is_some() && !imported_memory && data_end > info.memory_initial_pages * PAGE {
        errors.push(Error::new(Kind::Invalid {
            reason: format!(
                "a data segment ends at byte {data_end}, beyond the initial memory of {} pages",
                info.memory_initial_pages
            ),
        }));
    }
    if info.memory_initial_pages > memory_limit_pages {
        errors.push(Error::new(Kind::InitialMemoryAboveLimit {
            pages: info.memory_initial_pages,
            limit: memory_limit_pages,
        }));
    }
    errors.into_result(info)
}
