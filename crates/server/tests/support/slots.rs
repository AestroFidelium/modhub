//! Small pools of names and versions for model-based tests: few enough that
//! registrations and publications collide often, which is where the rules live.

use bolero::TypeGenerator;
use modhub_core::{ModName, ModVersion};

/// One of four fixed valid names.
#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) struct NameSlot(#[generator(0u8..4)] u8);

impl NameSlot {
    pub(crate) fn name(self) -> ModName {
        self.text().parse().unwrap()
    }

    pub(crate) fn text(self) -> String {
        format!("mod-{}", char::from(b'a' + self.0))
    }
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Stage {
    Release,
    Beta,
}

/// A valid version from a small grid.
#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) struct VersionSlot {
    #[generator(0u64..=2)]
    major: u64,
    #[generator(0u64..=2)]
    minor: u64,
    stage: Stage,
}

impl VersionSlot {
    pub(crate) fn version(self) -> ModVersion {
        self.text().parse().unwrap()
    }

    pub(crate) fn text(self) -> String {
        let suffix = match self.stage {
            Stage::Release => "",
            Stage::Beta => "-beta",
        };
        format!("{}.{}.0{suffix}", self.major, self.minor)
    }
}

/// Text that is not a valid `ModVersion`, as a row written past the store could hold.
#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum BadVersion {
    Empty,
    NotSemver,
    LeadingZero,
    BuildMetadata,
}

impl BadVersion {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::NotSemver => "latest",
            Self::LeadingZero => "01.0.0",
            Self::BuildMetadata => "1.0.0+nightly",
        }
    }
}
