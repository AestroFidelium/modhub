//! `ModName` and `ModVersion` parsing: every publish and download request parses both
//! from the URL before it reaches the store. Also sorting a mod's release history, which
//! is how a version listing is ordered.
#![expect(
    clippy::unwrap_used,
    reason = "bench setup: an input that fails to parse is a broken bench"
)]

use divan::{Bencher, black_box};
use modhub_core::{ModName, ModVersion};

fn main() {
    divan::main();
}

/// A typical name, the longest accepted one, and one rejected at its last byte.
const NAMES: &[&str] = &[
    "gpu-cloth",
    "a123456789-b123456789-c123456789-d123456789-e123456789-f12345678",
    "a123456789-b123456789-c123456789-d123456789-e123456789-f1234567-",
];

#[divan::bench(args = NAMES)]
fn mod_name(text: &str) -> bool {
    black_box(text).parse::<ModName>().is_ok()
}

/// A release, a pre-release, and a version refused for its build metadata.
const VERSIONS: &[&str] = &["1.2.0", "12.40.3-rc.1.beta-2", "1.2.0+nightly.20260101"];

#[divan::bench(args = VERSIONS)]
fn mod_version(text: &str) -> bool {
    black_box(text).parse::<ModVersion>().is_ok()
}

/// Sorting a shuffled release history of `count` versions, a quarter of them pre-releases.
#[divan::bench(args = [16, 256, 4096])]
fn sort_versions(bencher: Bencher, count: usize) {
    let versions = release_history(count);
    bencher
        .with_inputs(|| versions.clone())
        .bench_local_refs(|versions| versions.sort_unstable());
}

/// `count` distinct versions in a fixed pseudo-random order.
fn release_history(count: usize) -> Vec<ModVersion> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut versions: Vec<ModVersion> = (0..count)
        .map(|i| {
            let (major, minor, patch) = (i / 256, (i / 16) % 16, i % 16);
            let text = if i % 4 == 0 {
                format!("{major}.{minor}.{patch}-rc.{}", i % 7)
            } else {
                format!("{major}.{minor}.{patch}")
            };
            text.parse().unwrap()
        })
        .collect();
    for i in (1..versions.len()).rev() {
        let j = usize::try_from(next() % (i as u64 + 1)).unwrap();
        versions.swap(i, j);
    }
    versions
}
