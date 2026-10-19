//! Valid mod modules for storage and API tests: the real example mod, made distinct
//! per version with a custom section (which the host ignores).

use crate::slots::VersionSlot;

/// The release build of stormlight-example-mod (see `crates/core/tests/fixtures`).
pub(crate) const EXAMPLE_MOD: &[u8] =
    include_bytes!("../../../core/tests/fixtures/example_mod.wasm");

/// The example mod tagged with `version`, so every version publishes different bytes
/// and a download can be told apart from any other release.
pub(crate) fn module_for(version: VersionSlot) -> Vec<u8> {
    const NAME: &[u8] = b"modhub-test";
    let tag = version.text();
    // Custom section: id 0, size, then a length-prefixed name and the payload. Every
    // length here is below 128, so each LEB128 is one byte.
    let size = 1 + NAME.len() + tag.len();
    let mut bytes = EXAMPLE_MOD.to_vec();
    bytes.push(0);
    bytes.push(u8::try_from(size).unwrap());
    bytes.push(u8::try_from(NAME.len()).unwrap());
    bytes.extend_from_slice(NAME);
    bytes.extend_from_slice(tag.as_bytes());
    bytes
}
