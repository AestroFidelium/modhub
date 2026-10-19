//! Persistence of mods and their releases in PostgreSQL.

mod error;

use modhub_core::{ModName, ModVersion};
use sqlx::PgPool;

pub use error::StoreError;

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

    /// Publishes `version` of the mod `name`. Releases are immutable: a published
    /// version can never be published again.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnknownMod`] when no mod is called `name`,
    /// [`StoreError::VersionExists`] when this version is already published.
    pub async fn publish(&self, name: &ModName, version: &ModVersion) -> Result<(), StoreError> {
        // One statement: the mod lookup and the insert cannot race a concurrent delete.
        let inserted = sqlx::query!(
            "INSERT INTO releases (mod_id, version) SELECT id, $2 FROM mods WHERE name = $1",
            name.as_str(),
            version.to_string(),
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
        Ok(())
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
