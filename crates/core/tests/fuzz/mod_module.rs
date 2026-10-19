//! `ModModule::inspect` on untrusted uploads: arbitrary bytes, and real modules with
//! corrupted bytes. It never panics.

use bolero::{TypeGenerator, check};
use modhub_core::ModModule;

use crate::modules::ModuleSpec;

#[derive(Debug, TypeGenerator)]
struct RawUpload {
    bytes: Vec<u8>,
}

#[test]
fn inspecting_arbitrary_bytes_never_panics() {
    check!()
        .with_type::<RawUpload>()
        .for_each(|RawUpload { bytes }| {
            let _ = ModModule::inspect(bytes);
        });
}

/// A well-formed module with one byte overwritten: past the header, into the sections.
#[derive(Debug, TypeGenerator)]
struct Corrupted {
    module: ModuleSpec,
    at: u16,
    byte: u8,
}

#[test]
fn inspecting_corrupted_modules_never_panics() {
    check!().with_type::<Corrupted>().for_each(|corrupted| {
        let mut bytes = corrupted.module.encode();
        let at = usize::from(corrupted.at) % bytes.len();
        bytes[at] = corrupted.byte;
        let _ = ModModule::inspect(&bytes);
    });
}
