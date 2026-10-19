//! The version a mod release is published under.

use std::fmt;
use std::str::FromStr;

/// A semver version without build metadata, `MAJOR.MINOR.PATCH[-PRE]`.
///
/// Ordering is semver precedence: a pre-release sorts below its release. Build
/// metadata is refused because semver ignores it in precedence, so `1.0.0+a` and
/// `1.0.0+b` would be two releases the registry could not tell apart in order.
///
/// ```
/// use modhub_core::{ModVersion, ModVersionError};
///
/// let release: ModVersion = "1.2.0".parse()?;
/// let candidate: ModVersion = "1.2.0-rc.1".parse()?;
/// assert!(candidate < release);
/// assert!(matches!("1.2.0+nightly".parse::<ModVersion>(), Err(ModVersionError::BuildMetadata)));
/// # Ok::<(), ModVersionError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModVersion(semver::Version);

/// Why a text is not a valid [`ModVersion`].
#[derive(Debug, thiserror::Error)]
pub enum ModVersionError {
    /// The text is not semver.
    #[error("mod version is not valid semver: {0}")]
    Syntax(#[from] semver::Error),
    /// The text carries `+build` metadata.
    #[error("mod version must not carry build metadata (the part after '+')")]
    BuildMetadata,
}

impl FromStr for ModVersion {
    type Err = ModVersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let version = semver::Version::parse(text)?;
        if !version.build.is_empty() {
            return Err(ModVersionError::BuildMetadata);
        }
        Ok(Self(version))
    }
}

impl fmt::Display for ModVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
