//! Persistence of mods and their releases in PostgreSQL.

mod digest;
mod error;
mod stored;

use modhub_core::{CheckedModule, ModName, ModVersion};
use sqlx::PgPool;

pub use digest::Sha256;
pub use error::StoreError;
pub use stored::StoredModule;

/// The registry's tables behind typed operations. Cheap to clone: clones share the pool.
#[derive(Debug, Clone)]
pub struct Store {
    pool: PgPool,
}

impl Store {
    /// A store over an existing connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Brings the schema up to date with the migrations embedded at build time.
    ///
    /// # Errors
    ///
    /// When a migration fails or the database is unreachable.
    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!().run(&self.pool).await
    }

    /// Claims `name` for a new mod with no releases.
    ///
    /// # Errors
    ///
    /// [`StoreError::NameTaken`] when the name belongs to another mod.
    pub async fn register(&self, name: &ModName) -> Result<(), StoreError> {
        sqlx::query!("INSERT INTO mods (name) VALUES ($1)", name.as_str())
            .execute(&self.pool)
            .await
            .map_err(|error| {
                StoreError::on_violation(error, "mods_name_key", || StoreError::NameTaken {
                    name: name.clone(),
                })
            })?;
        Ok(())
    }

    /// Publishes `module` as `version` of the mod `name`, and returns its digest.
    /// Releases are immutable: a published version can never be published again.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnknownMod`] when no mod is called `name`,
    /// [`StoreError::VersionExists`] when this version is already published.
    pub async fn publish(
        &self,
        name: &ModName,
        version: &ModVersion,
        module: &CheckedModule<'_>,
    ) -> Result<Sha256, StoreError> {
        let sha256 = Sha256::of(module.bytes());
        // One statement: the mod lookup and the insert cannot race a concurrent delete.
        let inserted = sqlx::query!(
            "INSERT INTO releases (mod_id, version, module, sha256)
             SELECT id, $2, $3, $4 FROM mods WHERE name = $1",
            name.as_str(),
            version.to_string(),
            module.bytes(),
            sha256.as_bytes().as_slice(),
        )
        .execute(&self.pool)
        .await
        .map_err(|error| {
            StoreError::on_violation(error, "releases_mod_version_key", || {
                StoreError::VersionExists {
                    name: name.clone(),
                    version: version.clone(),
                }
            })
        })?;
        if inserted.rows_affected() == 0 {
            return Err(StoreError::UnknownMod { name: name.clone() });
        }
        Ok(sha256)
    }

    /// The module published as `version` of the mod `name`, after checking it still
    /// matches the digest it was stored with.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnknownMod`], [`StoreError::UnknownVersion`], or
    /// [`StoreError::CorruptModule`] when the stored bytes no longer match.
    pub async fn download(
        &self,
        name: &ModName,
        version: &ModVersion,
    ) -> Result<StoredModule, StoreError> {
        // LEFT JOIN: a known mod without this version yields a row of NULLs, which
        // tells "unknown version" apart from "unknown mod" (no row).
        let row = sqlx::query!(
            r#"SELECT r.module AS "module?", r.sha256 AS "sha256?"
               FROM mods m LEFT JOIN releases r ON r.mod_id = m.id AND r.version = $2
               WHERE m.name = $1"#,
            name.as_str(),
            version.to_string(),
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::UnknownMod { name: name.clone() })?;
        let (Some(bytes), Some(stored)) = (row.module, row.sha256) else {
            return Err(StoreError::UnknownVersion {
                name: name.clone(),
                version: version.clone(),
            });
        };
        let sha256 = Sha256::of(&bytes);
        if sha256.as_bytes().as_slice() != stored {
            return Err(StoreError::CorruptModule {
                name: name.clone(),
                version: version.clone(),
            });
        }
        Ok(StoredModule::new(bytes, sha256))
    }

    /// Every published version of the mod `name`, lowest precedence first.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnknownMod`] when no mod is called `name`,
    /// [`StoreError::CorruptVersion`] when a stored version does not parse.
    pub async fn versions(&self, name: &ModName) -> Result<Vec<ModVersion>, StoreError> {
        // LEFT JOIN: a registered mod without releases yields one row with a NULL
        // version, which tells it apart from an unknown mod (no rows at all).
        let rows = sqlx::query_scalar!(
            r#"SELECT r.version AS "version?"
               FROM mods m LEFT JOIN releases r ON r.mod_id = m.id
               WHERE m.name = $1"#,
            name.as_str(),
        )
        .fetch_all(&self.pool)
        .await?;
        if rows.is_empty() {
            return Err(StoreError::UnknownMod { name: name.clone() });
        }
        // Precedence is semver's, not text order ("1.10.0" > "1.9.0"), so sort in Rust.
        let mut versions = rows
            .into_iter()
            .flatten()
            .map(|text| match text.parse() {
                Ok(version) => Ok(version),
                Err(source) => Err(StoreError::CorruptVersion {
                    name: name.clone(),
                    text,
                    source,
                }),
            })
            .collect::<Result<Vec<ModVersion>, _>>()?;
        versions.sort_unstable();
        Ok(versions)
    }
}
