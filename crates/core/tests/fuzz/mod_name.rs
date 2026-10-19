//! `ModName` parsing on arbitrary text, as it arrives from an upload request.

use bolero::{TypeGenerator, check};
use modhub_core::ModName;

/// Whatever a client sent as a mod name.
#[derive(Debug, TypeGenerator)]
struct RawName {
    text: String,
}

/// The naming rule stated directly, as an oracle independent of the parser's error paths.
fn obeys_rule(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=ModName::MAX_LEN).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !text.ends_with('-')
        && !text.contains("--")
}

#[test]
fn parsing_is_total_and_accepts_only_rule_abiding_text() {
    check!()
        .with_type::<RawName>()
        .for_each(|RawName { text }| match text.parse::<ModName>() {
            Ok(name) => {
                assert!(obeys_rule(text), "accepted {text:?}");
                assert_eq!(name.as_str(), text);
            }
            Err(_) => assert!(!obeys_rule(text), "rejected {text:?}"),
        });
}
