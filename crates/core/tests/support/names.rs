//! Generators of mod names. The naming rule lives here once: every scenario gets valid
//! names by construction, never by filtering.

use bolero::{Driver, TypeGenerator, ValueGenerator};
use modhub_core::ModName;

/// A string that obeys the naming rule: hyphen-separated runs of `[a-z0-9]` that start
/// with a letter, at most `ModName::MAX_LEN - SPARE` bytes long.
///
/// `SPARE` leaves room for a scenario that inserts bytes into the name and must not
/// trip the length limit by accident.
#[derive(Debug, Clone)]
pub(crate) struct ValidName<const SPARE: usize = 0>(pub(crate) String);

impl<const SPARE: usize> TypeGenerator for ValidName<SPARE> {
    fn generate<D: Driver>(driver: &mut D) -> Option<Self> {
        let max_len = ModName::MAX_LEN - SPARE;
        let mut name = String::with_capacity(max_len);
        name.push(char::from((b'a'..=b'z').generate(driver)?));
        let head_tail = (0usize..=7).generate(driver)?.min(max_len - 1);
        push_alphanumeric(driver, &mut name, head_tail)?;

        // Enough segments to reach the limit, so the longest legal names get exercised.
        for _ in 0..(0usize..=12).generate(driver)? {
            let segment = (1usize..=8).generate(driver)?;
            if name.len() + 1 + segment > max_len {
                break;
            }
            name.push('-');
            push_alphanumeric(driver, &mut name, segment)?;
        }
        Some(Self(name))
    }
}

fn push_alphanumeric<D: Driver>(driver: &mut D, name: &mut String, count: usize) -> Option<()> {
    for _ in 0..count {
        let index = (0u8..36).generate(driver)?;
        name.push(char::from(if index < 26 {
            b'a' + index
        } else {
            b'0' + index - 26
        }));
    }
    Some(())
}
