//! Module bytes together with proof that they passed the contract.

use super::{ModModule, ModuleError};

/// Bytes that passed [`ModModule::check`], and what the check found. The only way to
/// make one is the check, so code that takes a `CheckedModule` (storage) can never be
/// handed unchecked bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedModule<'a> {
    bytes: &'a [u8],
    module: ModModule,
}

impl<'a> CheckedModule<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Result<Self, ModuleError> {
        let module = ModModule::inspect(bytes)?;
        Ok(Self { bytes, module })
    }

    /// The module's bytes, exactly as checked.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// What the check found.
    pub fn module(&self) -> &ModModule {
        &self.module
    }
}
