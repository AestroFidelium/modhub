//! Generators of mod versions: canonical semver text without build metadata, built
//! from parts so a test can compare against the parts instead of re-parsing.

use std::fmt;

use bolero::{Driver, TypeGenerator, ValueGenerator};

/// A canonical version, `MAJOR.MINOR.PATCH[-PRE]`.
#[derive(Debug, Clone, TypeGenerator)]
pub(crate) struct ValidVersion {
    // Small ranges so two generated versions often share a prefix and the
    // tie-breaking rules get exercised; `u64::MAX` is covered by the fuzz target.
    #[generator(0u64..=3)]
    pub(crate) major: u64,
    #[generator(0u64..=3)]
    pub(crate) minor: u64,
    #[generator(0u64..=3)]
    pub(crate) patch: u64,
    pub(crate) pre: Option<PreRelease>,
}

/// A non-empty pre-release: up to four dot-separated identifiers.
#[derive(Debug, Clone)]
pub(crate) struct PreRelease(pub(crate) Vec<Identifier>);

impl TypeGenerator for PreRelease {
    fn generate<D: Driver>(driver: &mut D) -> Option<Self> {
        let count = (1usize..=4).generate(driver)?;
        let identifiers = (0..count)
            .map(|_| Identifier::generate(driver))
            .collect::<Option<_>>()?;
        Some(Self(identifiers))
    }
}

/// One pre-release identifier. Semver orders numeric ones numerically, below any
/// alphanumeric one, which is ordered as ASCII text.
#[derive(Debug, Clone, PartialEq, Eq, TypeGenerator)]
pub(crate) enum Identifier {
    Numeric(#[generator(0u64..=12)] u64),
    Alpha(Word),
}

/// One to four letters from a small alphabet, so equal and prefix words are common.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Word(pub(crate) String);

impl TypeGenerator for Word {
    fn generate<D: Driver>(driver: &mut D) -> Option<Self> {
        let len = (1usize..=4).generate(driver)?;
        let word = (0..len)
            .map(|_| Some(char::from((b'a'..=b'c').generate(driver)?)))
            .collect::<Option<_>>()?;
        Some(Self(word))
    }
}

impl fmt::Display for ValidVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(PreRelease(identifiers)) = &self.pre {
            for (index, identifier) in identifiers.iter().enumerate() {
                f.write_str(if index == 0 { "-" } else { "." })?;
                match identifier {
                    Identifier::Numeric(number) => write!(f, "{number}")?,
                    Identifier::Alpha(Word(word)) => f.write_str(word)?,
                }
            }
        }
        Ok(())
    }
}
