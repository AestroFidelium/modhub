//! Why a store operation failed.

use modhub_core::{ModName, ModVersion, ModVersionError};

/// A failed store operation: a domain refusal, corrupt data, or the database itself.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The name already belongs to a mod.
    #[error("mod name `{name}` is already taken")]
    NameTaken {
        /// The requested name.
        name: ModName,
    },
    /// No mod has this name.
    #[error("mod `{name}` does not exist")]
    UnknownMod {
        /// The requested name.
        name: ModName,
    },
    /// The mod exists but has no such version.
    #[error("`{name}` has no version {version}")]
    UnknownVersion {
        /// The mod.
        name: ModName,
        /// The requested version.
        version: ModVersion,
    },
    /// The version is already published; releases are immutable.
    #[error("`{name}` {version} is already published, and releases are immutable")]
    VersionExists {
        /// The mod.
        name: ModName,
        /// The version published before.
        version: ModVersion,
    },
    /// A stored version is not a valid [`ModVersion`]: the row was written past the store.
    #[error("stored version {text:?} of `{name}` is not a valid mod version")]
    CorruptVersion {
        /// The mod the row belongs to.
        name: ModName,
        /// The stored text.
        text: String,
        /// Why it does not parse.
        source: ModVersionError,
    },
    /// A stored module no longer matches the digest it was stored with.
    #[error("stored module of `{name}` {version} does not match its SHA-256")]
    CorruptModule {
        /// The mod.
        name: ModName,
        /// The version.
        version: ModVersion,
    },
    /// The database failed or is unreachable.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

impl StoreError {
    /// `domain()` when `error` violates `constraint`, otherwise a database error.
    pub(super) fn on_violation(
        error: sqlx::Error,
        constraint: &str,
        domain: impl FnOnce() -> Self,
    ) -> Self {
        let violated = error
            .as_database_error()
            .and_then(|database| database.constraint())
            == Some(constraint);
        if violated {
            domain()
        } else {
            Self::Database(error)
        }
    }
}
