//! Mod versions: canonical semver text round-trips, precedence follows the semver
//! spec, and build metadata is refused.

use std::cmp::Ordering;

use bolero::{TypeGenerator, check};
use modhub_core::{ModVersion, ModVersionError};

use crate::versions::{Identifier, PreRelease, ValidVersion, Word};

#[test]
fn canonical_versions_parse_and_display_unchanged() {
    check!().with_type::<ValidVersion>().for_each(|version| {
        let text = version.to_string();
        let parsed: ModVersion = text.parse().unwrap();
        assert_eq!(parsed.to_string(), text);
    });
}

/// Two versions to order against each other.
#[derive(Debug, TypeGenerator)]
struct Pair {
    left: ValidVersion,
    right: ValidVersion,
}

#[test]
fn ordering_follows_semver_precedence() {
    check!()
        .with_type::<Pair>()
        .for_each(|Pair { left, right }| {
            let parsed_left: ModVersion = left.to_string().parse().unwrap();
            let parsed_right: ModVersion = right.to_string().parse().unwrap();
            assert_eq!(parsed_left.cmp(&parsed_right), precedence(left, right));
            assert_eq!(
                parsed_left == parsed_right,
                left.to_string() == right.to_string()
            );
        });
}

/// Semver 2.0.0 §11, written out as the oracle.
fn precedence(left: &ValidVersion, right: &ValidVersion) -> Ordering {
    let core = |v: &ValidVersion| (v.major, v.minor, v.patch);
    core(left)
        .cmp(&core(right))
        .then_with(|| match (&left.pre, &right.pre) {
            (None, None) => Ordering::Equal,
            // A pre-release sorts below the release it leads up to.
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(PreRelease(l)), Some(PreRelease(r))) => l
                .iter()
                .zip(r)
                .map(|(a, b)| identifier_precedence(a, b))
                .find(|order| order.is_ne())
                .unwrap_or_else(|| l.len().cmp(&r.len())),
        })
}

fn identifier_precedence(left: &Identifier, right: &Identifier) -> Ordering {
    match (left, right) {
        (Identifier::Numeric(a), Identifier::Numeric(b)) => a.cmp(b),
        (Identifier::Numeric(_), Identifier::Alpha(_)) => Ordering::Less,
        (Identifier::Alpha(_), Identifier::Numeric(_)) => Ordering::Greater,
        (Identifier::Alpha(Word(a)), Identifier::Alpha(Word(b))) => a.cmp(b),
    }
}

/// A valid version with build metadata appended.
#[derive(Debug, TypeGenerator)]
struct WithBuild {
    version: ValidVersion,
    build: Word,
}

#[test]
fn build_metadata_is_refused() {
    check!()
        .with_type::<WithBuild>()
        .for_each(|WithBuild { version, build }| {
            let text = format!("{version}+{}", build.0);
            let parsed = text.parse::<ModVersion>();
            assert!(
                matches!(parsed, Err(ModVersionError::BuildMetadata)),
                "{text:?} gave {parsed:?}"
            );
        });
}
