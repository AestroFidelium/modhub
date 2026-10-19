//! Why a module is refused.

use std::fmt;

use super::{ModModule, Signature};

/// What an export should have been.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// A function.
    Func,
    /// A linear memory.
    Memory,
}

impl fmt::Display for ExportKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Func => "function",
            Self::Memory => "memory",
        })
    }
}

/// A module the Stormlight host would refuse to load, and why. Messages are written for
/// the mod's author.
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    /// The upload is larger than [`ModModule::MAX_BYTES`].
    #[error("module is {len} bytes, the limit is {max}", max = ModModule::MAX_BYTES)]
    TooLarge {
        /// Its size in bytes.
        len: usize,
    },
    /// Not a valid WebAssembly 2.0 module: malformed, truncated, a component, or using
    /// a feature the host does not run (threads, shared memory, …).
    #[error("not a valid WebAssembly 2.0 module: {0}")]
    Invalid(#[from] wasmparser::BinaryReaderError),
    /// The module imports something; mods run with no imports at all.
    #[error(
        "module imports `{module}::{name}`, but mods run with no imports (no WASI, no host calls)"
    )]
    Import {
        /// The import's module.
        module: Box<str>,
        /// The import's name.
        name: Box<str>,
    },
    /// A required export is missing.
    #[error("module must export `{name}`")]
    MissingExport {
        /// The export.
        name: &'static str,
    },
    /// An export the host uses is the wrong kind of item.
    #[error("export `{name}` must be a {expected}")]
    WrongKind {
        /// The export.
        name: &'static str,
        /// What it must be.
        expected: ExportKind,
    },
    /// A function the host calls has a different signature.
    #[error("export `{name}` must be {expected}, found {found}")]
    WrongSignature {
        /// The export.
        name: &'static str,
        /// The signature the host calls it with.
        expected: Signature,
        /// The signature the module declares.
        found: Signature,
    },
    /// The initial memory is larger than the host allows.
    #[error("module starts with {pages} memory pages, the host allows {max_pages}")]
    MemoryTooLarge {
        /// Initial pages, 64 KiB each.
        pages: u64,
        /// [`ModModule::MAX_MEMORY_PAGES`].
        max_pages: u64,
    },
}
