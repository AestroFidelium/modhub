//! The name a mod is published under.

use std::fmt;
use std::str::FromStr;

/// A validated mod name: hyphen-separated runs of `[a-z0-9]` that start with a letter,
/// at most [`ModName::MAX_LEN`] bytes.
///
/// Lowercase only, with a single separator, so two names that look the same to a
/// player are the same name: `Cloth_Sim`, `cloth_sim` and `cloth-sim` cannot coexist.
///
/// ```
/// use modhub_core::{ModName, ModNameError};
///
/// let name: ModName = "gpu-cloth".parse()?;
/// assert_eq!(name.as_str(), "gpu-cloth");
/// assert_eq!("Gpu-cloth".parse::<ModName>(), Err(ModNameError::BadStart { found: 'G' }));
/// # Ok::<(), ModNameError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModName(Box<str>);

impl ModName {
    /// The longest accepted name, in bytes. Every valid name is ASCII, so also in characters.
    pub const MAX_LEN: usize = 64;

    /// The name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a text is not a valid [`ModName`]. Byte offsets point into the rejected text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModNameError {
    /// The text is empty.
    #[error("mod name is empty")]
    Empty,
    /// The text is longer than [`ModName::MAX_LEN`] bytes.
    #[error("mod name is {len} bytes long, the limit is {max}", max = ModName::MAX_LEN)]
    TooLong {
        /// Length of the rejected text in bytes.
        len: usize,
    },
    /// The first character is not a lowercase ASCII letter.
    #[error("mod name must start with a lowercase ASCII letter, found {found:?}")]
    BadStart {
        /// The offending first character.
        found: char,
    },
    /// A character outside `a-z`, `0-9` and `-`.
    #[error("mod name may contain only a-z, 0-9 and '-', found {found:?} at byte {at}")]
    BadChar {
        /// The offending character.
        found: char,
        /// Its byte offset.
        at: usize,
    },
    /// Two hyphens in a row.
    #[error("mod name must not contain \"--\", found at byte {at}")]
    DoubleHyphen {
        /// Byte offset of the first of the two hyphens.
        at: usize,
    },
    /// The text ends with a hyphen.
    #[error("mod name must not end with '-'")]
    TrailingHyphen,
}

impl FromStr for ModName {
    type Err = ModNameError;

    /// Checks run in a fixed order, so a text breaking several rules always reports
    /// the same error: length, first character, then the body from left to right.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut chars = text.char_indices();
        let Some((_, first)) = chars.next() else {
            return Err(ModNameError::Empty);
        };
        if text.len() > Self::MAX_LEN {
            return Err(ModNameError::TooLong { len: text.len() });
        }
        if !first.is_ascii_lowercase() {
            return Err(ModNameError::BadStart { found: first });
        }
        let mut previous = first;
        for (at, found) in chars {
            match found {
                '-' if previous == '-' => return Err(ModNameError::DoubleHyphen { at: at - 1 }),
                'a'..='z' | '0'..='9' | '-' => {}
                _ => return Err(ModNameError::BadChar { found, at }),
            }
            previous = found;
        }
        if previous == '-' {
            return Err(ModNameError::TrailingHyphen);
        }
        Ok(Self(text.into()))
    }
}

impl fmt::Display for ModName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
