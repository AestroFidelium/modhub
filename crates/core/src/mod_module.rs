//! A mod's WebAssembly module, checked against the contract of the Stormlight host.

mod checked;
mod error;
mod signature;

use std::collections::BTreeSet;

use stormlight_mod_abi::runtime::{ALLOC_EXPORT, HANDLE_EXPORT, TICK_EXPORT, TRIGGER_EXPORT};
use wasmparser::types::{EntityType, TypesRef};
use wasmparser::{
    BinaryReaderError, CompositeInnerType, Import, Parser, Payload, Validator, WasmFeatures,
};

pub use checked::CheckedModule;
pub use error::{ExportKind, ModuleError};
pub use signature::{Signature, ValType};

/// The export the host calls once at load to read the mod's registration. The host
/// names it by this literal (stormlight `shared/modloader/src/host.rs`); the ABI crate
/// has no constant for it.
const REGISTER_EXPORT: &str = "mod_register";
/// The guest's linear memory, where the host reads results and writes inputs.
const MEMORY_EXPORT: &str = "memory";

/// An optional runtime entry point a mod may export besides `mod_register`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryPoint {
    /// `mod_handle`: custom impact handlers.
    Handle,
    /// `mod_trigger`: reactions to game events.
    Trigger,
    /// `mod_tick`: per-tick logic.
    Tick,
}

impl EntryPoint {
    /// Every entry point, in check order.
    pub const ALL: [Self; 3] = [Self::Handle, Self::Trigger, Self::Tick];

    /// The export the host calls.
    pub fn export_name(self) -> &'static str {
        match self {
            Self::Handle => HANDLE_EXPORT,
            Self::Trigger => TRIGGER_EXPORT,
            Self::Tick => TICK_EXPORT,
        }
    }

    /// The signature the host calls it with: `(selector, ptr, len)` or `(ptr, len)`,
    /// returning a packed `(ptr, len)` of the result.
    fn signature(self) -> Signature {
        use ValType::{I32, I64};
        match self {
            Self::Handle | Self::Trigger => Signature::new(&[I32, I32, I32], &[I64]),
            Self::Tick => Signature::new(&[I32, I32], &[I64]),
        }
    }
}

/// A module the Stormlight host can load: valid WebAssembly 2.0, no imports, an
/// exported memory within the host's limit, `mod_register`, and every entry point it
/// exports with the signature the host calls (plus `mod_alloc` when it has any).
///
/// ```
/// use modhub_core::{ModModule, ModuleError};
///
/// // The smallest valid module: a header and nothing else. No memory, no mod_register.
/// let empty = b"\0asm\x01\0\0\0";
/// assert!(matches!(
///     ModModule::inspect(empty),
///     Err(ModuleError::MissingExport { name: "memory" })
/// ));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModModule {
    entry_points: BTreeSet<EntryPoint>,
    memory_pages: u64,
}

impl ModModule {
    /// The largest accepted module. Real mods are a few hundred KiB.
    pub const MAX_BYTES: usize = 8 * 1024 * 1024;

    /// The largest initial memory, in 64 KiB pages: the host's default memory cap
    /// (`Limits::max_memory_bytes`, 64 MiB). A larger module fails at instantiation.
    pub const MAX_MEMORY_PAGES: u64 = 1024;

    /// Checks `bytes` against the contract. Checks run in a fixed order, so a module
    /// breaking several rules always reports the same error.
    ///
    /// # Errors
    ///
    /// The first broken rule: size, validity, imports, memory, then exports.
    pub fn inspect(bytes: &[u8]) -> Result<Self, ModuleError> {
        use ValType::{I32, I64};

        if bytes.len() > Self::MAX_BYTES {
            return Err(ModuleError::TooLarge { len: bytes.len() });
        }
        // WebAssembly 2.0 and nothing newer: what the host's engine runs. No threads,
        // so shared memory is refused here too; no component model.
        let types = Validator::new_with_features(WasmFeatures::WASM2).validate_all(bytes)?;
        if let Some(import) = first_import(bytes)? {
            return Err(ModuleError::Import {
                module: import.module.into(),
                name: import.name.into(),
            });
        }
        let exports = Exports(types.as_ref());

        let memory_pages = match exports.get(MEMORY_EXPORT) {
            Some(EntityType::Memory(memory)) => memory.initial,
            Some(_) => {
                return Err(ModuleError::WrongKind {
                    name: MEMORY_EXPORT,
                    expected: ExportKind::Memory,
                });
            }
            None => {
                return Err(ModuleError::MissingExport {
                    name: MEMORY_EXPORT,
                });
            }
        };
        if memory_pages > Self::MAX_MEMORY_PAGES {
            return Err(ModuleError::MemoryTooLarge {
                pages: memory_pages,
                max_pages: Self::MAX_MEMORY_PAGES,
            });
        }

        exports
            .function(REGISTER_EXPORT, &Signature::new(&[], &[I64]))?
            .ok_or(ModuleError::MissingExport {
                name: REGISTER_EXPORT,
            })?;
        let alloc = exports.function(ALLOC_EXPORT, &Signature::new(&[I32], &[I32]))?;

        let mut entry_points = BTreeSet::new();
        for entry in EntryPoint::ALL {
            if exports
                .function(entry.export_name(), &entry.signature())?
                .is_some()
            {
                entry_points.insert(entry);
            }
        }
        // The host pushes each entry point's input through `mod_alloc`.
        if !entry_points.is_empty() && alloc.is_none() {
            return Err(ModuleError::MissingExport { name: ALLOC_EXPORT });
        }
        Ok(Self {
            entry_points,
            memory_pages,
        })
    }

    /// [`ModModule::inspect`] that keeps hold of the bytes, for code that stores them.
    ///
    /// # Errors
    ///
    /// As [`ModModule::inspect`].
    pub fn check(bytes: &[u8]) -> Result<CheckedModule<'_>, ModuleError> {
        CheckedModule::new(bytes)
    }

    /// The runtime entry points the mod exports, in [`EntryPoint::ALL`] order.
    pub fn entry_points(&self) -> impl Iterator<Item = EntryPoint> + '_ {
        self.entry_points.iter().copied()
    }

    /// The initial size of the mod's linear memory, in 64 KiB pages.
    pub fn memory_pages(&self) -> u64 {
        self.memory_pages
    }
}

/// The first import in section order. Only called on a validated module.
fn first_import(bytes: &[u8]) -> Result<Option<Import<'_>>, BinaryReaderError> {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(section) = payload? {
            return section.into_imports().next().transpose();
        }
    }
    Ok(None)
}

/// The exports of a validated module.
struct Exports<'a>(TypesRef<'a>);

impl Exports<'_> {
    fn get(&self, name: &str) -> Option<EntityType> {
        self.0
            .core_exports()
            .into_iter()
            .flatten()
            .find_map(|(export, ty)| (export == name).then_some(ty))
    }

    /// `Some(())` when `name` is a function exported with `expected`, `None` when it is
    /// not exported; an error when it is something else.
    fn function(
        &self,
        name: &'static str,
        expected: &Signature,
    ) -> Result<Option<()>, ModuleError> {
        let found = match self.get(name) {
            None => return Ok(None),
            Some(EntityType::Func(id) | EntityType::FuncExact(id)) => {
                self.0
                    .get(id)
                    .and_then(|ty| match &ty.composite_type.inner {
                        CompositeInnerType::Func(func) => Some(Signature::from_func_type(func)),
                        // A validated module's function index always has a function type.
                        _ => None,
                    })
            }
            Some(_) => None,
        };
        let found = found.ok_or(ModuleError::WrongKind {
            name,
            expected: ExportKind::Func,
        })?;
        if found != *expected {
            return Err(ModuleError::WrongSignature {
                name,
                expected: expected.clone(),
                found,
            });
        }
        Ok(Some(()))
    }
}
