//! Mod modules against the Stormlight mod contract: what the game's host would load is
//! accepted, everything it would refuse is refused with the reason.

use bolero::{TypeGenerator, check};
use modhub_core::{EntryPoint, ExportKind, ModModule, ModuleError};

use crate::contract::Verdict;
use crate::modules::ModuleSpec;

const EXAMPLE_MOD: &[u8] = include_bytes!("../fixtures/example_mod.wasm");

fn verdict(result: Result<ModModule, ModuleError>) -> Verdict {
    match result {
        Ok(module) => Verdict::Accepted {
            entry_points: module.entry_points().collect(),
            memory_pages: module.memory_pages(),
        },
        Err(ModuleError::Invalid(_)) => Verdict::Invalid,
        Err(ModuleError::Import { module, name }) => Verdict::Import {
            module: module.into(),
            name: name.into(),
        },
        Err(ModuleError::MissingExport { name }) => Verdict::MissingExport(name),
        Err(
            error @ ModuleError::WrongKind {
                name: "memory",
                expected: ExportKind::Memory,
            },
        ) => {
            assert!(error.to_string().ends_with("must be a memory"), "{error}");
            Verdict::NotAMemory
        }
        Err(
            error @ ModuleError::WrongKind {
                name,
                expected: ExportKind::Func,
            },
        ) => {
            assert!(error.to_string().ends_with("must be a function"), "{error}");
            Verdict::NotAFunction(name)
        }
        Err(ModuleError::MemoryTooLarge { pages, max_pages }) => {
            assert_eq!(max_pages, ModModule::MAX_MEMORY_PAGES);
            Verdict::MemoryTooLarge(pages)
        }
        Err(error @ ModuleError::WrongSignature { name, .. }) => {
            // The message names the export and both signatures, so an author can fix it.
            let message = error.to_string();
            assert!(
                message.contains(name) && message.contains("->"),
                "{message}"
            );
            Verdict::WrongSignature(name)
        }
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn verdict_follows_the_mod_contract() {
    check!().with_type::<ModuleSpec>().for_each(|spec| {
        let result = ModModule::inspect(&spec.encode());
        // The message shows what the module declared, exactly, so an author can fix it.
        if let Err(ModuleError::WrongSignature { name, found, .. }) = &result {
            assert_eq!(found.to_string(), spec.signature_text(name), "{spec:?}");
        }
        assert_eq!(verdict(result), spec.expected(), "{spec:?}");
    });
}

/// Where to cut the real example mod: anywhere, or exactly at a section boundary
/// (rare by chance, and where the interesting verdicts are).
#[derive(Debug, TypeGenerator)]
enum Cut {
    Anywhere(u32),
    AtBoundary(u8),
}

/// Byte offsets where each section of `module` ends, with the section id.
fn section_ends(module: &[u8]) -> Vec<(usize, u8)> {
    wasmparser::Parser::new(0)
        .parse_all(module)
        .filter_map(|payload| payload.unwrap().as_section())
        .map(|(id, range)| (range.end, id))
        .collect()
}

const CODE_SECTION: u8 = 10;
/// `\0asm` and the version: on its own, a valid empty module.
const HEADER_LEN: usize = 8;

#[test]
fn the_real_example_mod_is_accepted_and_no_mid_section_cut_is() {
    let full = ModModule::inspect(EXAMPLE_MOD).unwrap();
    assert_eq!(
        full.entry_points().collect::<Vec<_>>(),
        [EntryPoint::Handle, EntryPoint::Trigger, EntryPoint::Tick]
    );
    // Cut mid-section, a module is broken. Cut at a section boundary after the code, it
    // is a smaller valid module: the sections after code (data, custom metadata) are
    // optional in WebAssembly. A validator cannot tell that one lost its data; catching
    // a truncated upload is the transport's job (length, checksum).
    let ends = section_ends(EXAMPLE_MOD);
    let code_end = ends.iter().find(|(_, id)| *id == CODE_SECTION).unwrap().0;
    let boundary = |len: usize| len == HEADER_LEN || ends.iter().any(|(end, _)| *end == len);

    let mut boundaries: Vec<usize> = ends.iter().map(|(end, _)| *end).collect();
    boundaries.push(HEADER_LEN);
    boundaries.retain(|&end| end < EXAMPLE_MOD.len());

    check!().with_type::<Cut>().for_each(|cut| {
        let len = match *cut {
            Cut::Anywhere(at) => usize::try_from(at).unwrap() % EXAMPLE_MOD.len(),
            Cut::AtBoundary(index) => boundaries[usize::from(index) % boundaries.len()],
        };
        match ModModule::inspect(&EXAMPLE_MOD[..len]) {
            Ok(module) => {
                assert!(
                    boundary(len) && len >= code_end,
                    "accepted a cut at {len} bytes"
                );
                assert_eq!(module, full, "{len} bytes");
            }
            // At a boundary, what is left can be a smaller valid module that breaks the
            // contract instead (the bare 8-byte header exports no memory).
            Err(_) if boundary(len) => {}
            Err(error) => assert!(
                matches!(error, ModuleError::Invalid(_)),
                "{len} bytes: {error}"
            ),
        }
    });
}

/// A size around the upload limit. `Exactly` is its own case: drawn from a range, the
/// one value on the boundary would almost never come up.
#[derive(Debug, Clone, Copy, TypeGenerator)]
enum NearLimit {
    Exactly,
    Below(#[generator(1usize..=4096)] usize),
    Above(#[generator(1usize..=4096)] usize),
}

impl NearLimit {
    fn len(self) -> usize {
        match self {
            Self::Exactly => ModModule::MAX_BYTES,
            Self::Below(by) => ModModule::MAX_BYTES - by,
            Self::Above(by) => ModModule::MAX_BYTES + by,
        }
    }
}

/// `module` grown to exactly `len` bytes with a custom section, which the host ignores.
fn padded(module: &[u8], len: usize) -> Vec<u8> {
    // Section id, then the section size as 4-byte LEB128 (the size is between 2^21
    // and 2^28 near the limit), then a 1-byte name length, the name, the padding.
    const OVERHEAD: usize = 1 + 4 + 1 + 3;
    let data = len - module.len() - OVERHEAD;
    let size = u32::try_from(1 + 3 + data).unwrap();
    let mut bytes = module.to_vec();
    bytes.push(0);
    bytes.extend((0..4).map(|i| {
        let byte = u8::try_from((size >> (7 * i)) & 0x7f).unwrap();
        if i < 3 { byte | 0x80 } else { byte }
    }));
    bytes.push(3);
    bytes.extend_from_slice(b"pad");
    bytes.resize(len, 0);
    bytes
}

#[test]
fn the_size_limit_is_inclusive() {
    check!().with_type::<NearLimit>().for_each(|near| {
        let len = near.len();
        let bytes = padded(EXAMPLE_MOD, len);
        assert_eq!(bytes.len(), len);
        let result = ModModule::inspect(&bytes);
        if len <= ModModule::MAX_BYTES {
            assert!(result.is_ok(), "{len} bytes: {result:?}");
        } else {
            assert!(
                matches!(result, Err(ModuleError::TooLarge { len: l }) if l == len),
                "{result:?}"
            );
        }
    });
}

/// The limits mod authors are told about (README, API errors). Changing one is a
/// decision, so it changes this test too. A plain test: there is no input to vary.
#[test]
fn advertised_limits() {
    assert_eq!(ModModule::MAX_BYTES, 8 * 1024 * 1024);
    // The host's default memory cap: 64 MiB in 64 KiB pages.
    assert_eq!(ModModule::MAX_MEMORY_PAGES * 64 * 1024, 64 * 1024 * 1024);
}
