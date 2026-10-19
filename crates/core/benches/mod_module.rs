//! `ModModule::inspect` on what uploads look like: the real example mod, and generated
//! code-heavy modules up to the 8 MiB limit. Validation cost follows the code; custom
//! sections are skipped, so padding with them would measure nothing.
#![expect(
    clippy::unwrap_used,
    reason = "bench setup: a module that fails to build or check is a broken bench"
)]

use divan::counter::BytesCount;
use divan::{Bencher, black_box};
use modhub_core::ModModule;
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, MemorySection, MemoryType,
    Module, TypeSection, ValType,
};

const EXAMPLE_MOD: &[u8] = include_bytes!("../tests/fixtures/example_mod.wasm");

fn main() {
    divan::main();
}

#[divan::bench]
fn example_mod(bencher: Bencher) {
    bencher
        .counter(BytesCount::of_slice(EXAMPLE_MOD))
        .bench(|| ModModule::inspect(black_box(EXAMPLE_MOD)));
}

/// Target sizes in bytes; the last stays just under the 8 MiB limit.
#[divan::bench(args = [64 << 10, 1 << 20, (8 << 20) - (64 << 10)])]
fn code_heavy(bencher: Bencher, size: usize) {
    let module = code_module(size);
    assert!(module.len() <= ModModule::MAX_BYTES);
    ModModule::inspect(&module).unwrap();
    bencher
        .counter(BytesCount::of_slice(&module))
        .bench(|| ModModule::inspect(black_box(&module)));
}

/// A valid mod of about `size` bytes: memory, `mod_register`, and as many functions of
/// ~3 KiB of integer arithmetic as fit.
fn code_module(size: usize) -> Vec<u8> {
    const CHAIN: usize = 1000; // (i32.const 1, i32.add) pairs per function, ~3 bytes each
    let helpers = u32::try_from(size / (3 * CHAIN + 8)).unwrap();

    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I64]);
    types.ty().function([], [ValType::I32]);
    let mut functions = FunctionSection::new();
    let mut code = CodeSection::new();

    functions.function(0);
    let mut register = Function::new([]);
    register.instructions().i64_const(0).end();
    code.function(&register);

    for _ in 0..helpers {
        functions.function(1);
        let mut body = Function::new([]);
        let mut sink = body.instructions();
        sink.i32_const(0);
        for _ in 0..CHAIN {
            sink.i32_const(1).i32_add();
        }
        sink.end();
        code.function(&body);
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 17,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("mod_register", ExportKind::Func, 0);

    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&memories);
    module.section(&exports);
    module.section(&code);
    module.finish()
}
