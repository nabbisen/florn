//! What a world means at the core level: the import interfaces with their flat signatures, the
//! required exports, and the reserved names. Release 0.1 supports the core world; a domain world
//! extends the core's spec with its interfaces.

use florn_abi::core_world;
use florn_abi::{FlatType, Ty};

/// A core function signature, flattened by the canonical ABI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncSig {
    pub name: String,
    pub params: Vec<FlatType>,
    pub results: Vec<FlatType>,
}

impl FuncSig {
    fn of(sig: &core_world::Signature) -> Self {
        let (params, results) = Ty::flat_signature(&sig.param_types(), sig.result.as_ref());
        FuncSig {
            name: sig.name.to_string(),
            params,
            results,
        }
    }
}

/// One import module of the canonical ABI (`florn:core/host@0.1.0`) with its functions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportInterface {
    pub module: String,
    pub functions: Vec<FuncSig>,
}

/// The interfaces a world imports and exports, by id, sorted: what the contract identity of a
/// module records (RFC 002 rule 11).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldInterfaces {
    pub imports: Vec<String>,
    pub exports: Vec<String>,
}

impl WorldInterfaces {
    pub fn new(imports: &[&str], exports: &[&str]) -> Self {
        let mut w = Self {
            imports: imports.iter().map(|s| s.to_string()).collect(),
            exports: exports.iter().map(|s| s.to_string()).collect(),
        };
        w.imports.sort();
        w.exports.sort();
        w
    }
}

impl std::fmt::Display for WorldInterfaces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "imports {}; exports {}",
            self.imports.join(", "),
            self.exports.join(", ")
        )
    }
}

/// A world, as admission checks it (RFC 003 rule 5, import and export rows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldSpec {
    /// `florn:core/guest@0.1.0`
    pub name: String,
    pub interfaces: WorldInterfaces,
    pub imports: Vec<ImportInterface>,
    /// Required function exports under their mangled names (`florn:core/actor@0.1.0#init`).
    pub exports: Vec<FuncSig>,
}

/// The core world's name (RFC 002 §4).
pub const CORE_WORLD: &str = "florn:core/guest@0.1.0";

/// `cabi_realloc(old_ptr, old_size, align, new_size) -> ptr` (RFC 002 rule 2).
pub fn realloc_sig() -> FuncSig {
    FuncSig {
        name: core_world::EXPORT_REALLOC.to_string(),
        params: vec![FlatType::I32; 4],
        results: vec![FlatType::I32],
    }
}

impl WorldSpec {
    /// The core world of RFC 002 §4.
    pub fn core() -> Self {
        WorldSpec {
            name: CORE_WORLD.to_string(),
            interfaces: WorldInterfaces::new(
                &["florn:core/types@0.1.0", core_world::HOST_MODULE],
                &["florn:core/actor@0.1.0"],
            ),
            imports: vec![ImportInterface {
                module: core_world::HOST_MODULE.to_string(),
                functions: core_world::host_functions()
                    .iter()
                    .map(FuncSig::of)
                    .collect(),
            }],
            exports: core_world::actor_functions()
                .iter()
                .map(|s| {
                    let mut f = FuncSig::of(s);
                    f.name = format!("{}{}", core_world::ACTOR_EXPORT_PREFIX, s.name);
                    f
                })
                .collect(),
        }
    }

    pub fn import(&self, module: &str, name: &str) -> Option<&FuncSig> {
        self.imports
            .iter()
            .find(|i| i.module == module)?
            .functions
            .iter()
            .find(|f| f.name == name)
    }
}

/// The worlds this Host admits (RFC 003 rule 2, stage 2).
pub fn supported_worlds() -> Vec<WorldSpec> {
    vec![WorldSpec::core()]
}
