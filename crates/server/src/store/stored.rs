//! A module read back from storage.

use super::Sha256;

/// A published module, verified against the digest it was stored with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredModule {
    bytes: Vec<u8>,
    sha256: Sha256,
}

impl StoredModule {
    pub(super) fn new(bytes: Vec<u8>, sha256: Sha256) -> Self {
        Self { bytes, sha256 }
    }

    /// The module's digest.
    pub fn sha256(&self) -> Sha256 {
        self.sha256
    }

    /// The module's bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
