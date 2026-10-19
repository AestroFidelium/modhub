//! `ModVersion` parsing on arbitrary text, as it arrives from an upload request.

use bolero::{TypeGenerator, check};
use modhub_core::ModVersion;

/// Whatever a client sent as a version.
#[derive(Debug, TypeGenerator)]
struct RawVersion {
    text: String,
}

#[test]
fn parsing_is_total_and_accepts_only_canonical_text() {
    check!()
        .with_type::<RawVersion>()
        .for_each(|RawVersion { text }| {
            if let Ok(version) = text.parse::<ModVersion>() {
                // Canonical: no second spelling of an accepted version exists.
                assert_eq!(version.to_string(), *text);
                assert!(!text.contains('+'), "accepted build metadata in {text:?}");
            }
        });
}
