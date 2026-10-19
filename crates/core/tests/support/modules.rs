//! WebAssembly modules built from a scenario. The verdict each one should get lives in
//! `contract.rs`.

use bolero::{TypeGenerator, produce};
use wasm_encoder::{
    CodeSection, ConstExpr, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    GlobalSection, GlobalType, ImportSection, MemorySection, MemoryType, Module, RefType,
    TypeSection, ValType,
};

/// A module the way a mod author might build it, correctly or not.
#[derive(Debug, TypeGenerator)]
pub(crate) struct ModuleSpec {
    #[generator(produce::<Vec<ImportSpec>>().with().len(0usize..=2))]
    pub(crate) imports: Vec<ImportSpec>,
    pub(crate) memory: MemorySpec,
    #[generator(produce::<Vec<ExportSpec>>().with().len(0usize..=6))]
    pub(crate) exports: Vec<ExportSpec>,
    /// An `i32` global exported under a name the host expects a function or memory at.
    pub(crate) global: Option<ExportName>,
}

/// A function import of type `() -> ()`: the host provides none, so any is fatal.
#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum ImportSpec {
    WasiFdWrite,
    EnvAbort,
}

impl ImportSpec {
    pub(crate) fn module_and_name(self) -> (&'static str, &'static str) {
        match self {
            Self::WasiFdWrite => ("wasi_snapshot_preview1", "fd_write"),
            Self::EnvAbort => ("env", "abort"),
        }
    }
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum MemorySpec {
    Absent,
    Defined { pages: Pages, export: MemoryExport },
}

/// Initial pages: typical sizes, or around the host's limit of 1024.
#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum Pages {
    Typical(#[generator(0u64..=32)] u64),
    NearLimit(#[generator(1020u64..=1030)] u64),
}

impl Pages {
    pub(crate) fn count(self) -> u64 {
        match self {
            Self::Typical(pages) | Self::NearLimit(pages) => pages,
        }
    }
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum MemoryExport {
    AsMemory,
    NotExported,
}

/// A function export.
#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) struct ExportSpec {
    pub(crate) name: ExportName,
    pub(crate) signature: SignatureSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, TypeGenerator)]
pub(crate) enum ExportName {
    Register,
    Alloc,
    Handle,
    Trigger,
    Tick,
    /// A function under the name the host expects a memory at.
    Memory,
    /// Anything else: helpers are allowed.
    Helper,
}

impl ExportName {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Register => "mod_register",
            Self::Alloc => "mod_alloc",
            Self::Handle => "mod_handle",
            Self::Trigger => "mod_trigger",
            Self::Tick => "mod_tick",
            Self::Memory => "memory",
            Self::Helper => "helper",
        }
    }

    /// The signature the host calls it with, as (params, results).
    pub(crate) fn expected(self) -> (&'static [ValType], &'static [ValType]) {
        use ValType::{I32, I64};
        match self {
            Self::Register => (&[], &[I64]),
            Self::Alloc => (&[I32], &[I32]),
            Self::Handle | Self::Trigger => (&[I32, I32, I32], &[I64]),
            Self::Tick => (&[I32, I32], &[I64]),
            Self::Memory | Self::Helper => (&[], &[]),
        }
    }
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum SignatureSpec {
    /// Half the time (enum variants are drawn evenly), so modules often get past the
    /// signature checks to the rules after them.
    AsExpected,
    Wrong(WrongShape),
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum WrongShape {
    NoResult,
    ExtraParam,
    I32Result,
    /// The wrong shapes below also cover every value type a signature can show.
    FloatParam,
    DoubleResult,
    VectorParam,
    ReferenceParam,
}

impl SignatureSpec {
    pub(crate) fn apply(self, name: ExportName) -> (Vec<ValType>, Vec<ValType>) {
        let (params, results) = name.expected();
        let (mut params, mut results) = (params.to_vec(), results.to_vec());
        match self {
            Self::AsExpected => {}
            Self::Wrong(WrongShape::NoResult) => results.clear(),
            Self::Wrong(WrongShape::ExtraParam) => params.push(ValType::I64),
            Self::Wrong(WrongShape::I32Result) => results = vec![ValType::I32],
            Self::Wrong(WrongShape::FloatParam) => params.push(ValType::F32),
            Self::Wrong(WrongShape::DoubleResult) => results = vec![ValType::F64],
            Self::Wrong(WrongShape::VectorParam) => params.push(ValType::V128),
            Self::Wrong(WrongShape::ReferenceParam) => {
                params.push(ValType::Ref(RefType::EXTERNREF));
            }
        }
        (params, results)
    }
}

impl ModuleSpec {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut types = TypeSection::new();
        let mut imports = ImportSection::new();
        let mut functions = FunctionSection::new();
        let mut memories = MemorySection::new();
        let mut exports = ExportSection::new();
        let mut globals = GlobalSection::new();
        let mut code = CodeSection::new();

        types.ty().function([], []);
        for import in &self.imports {
            let (module, name) = import.module_and_name();
            imports.import(module, name, EntityType::Function(0));
        }
        let imported = u32::try_from(self.imports.len()).unwrap();

        if let MemorySpec::Defined { pages, export } = self.memory {
            memories.memory(MemoryType {
                minimum: pages.count(),
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            if let MemoryExport::AsMemory = export {
                exports.export("memory", ExportKind::Memory, 0);
            }
        }

        for (index, export) in self.exports.iter().enumerate() {
            let (params, results) = export.signature.apply(export.name);
            let type_index = u32::try_from(index).unwrap() + 1;
            types.ty().function(params, results.clone());
            functions.function(type_index);
            exports.export(
                export.name.text(),
                ExportKind::Func,
                imported + u32::try_from(index).unwrap(),
            );
            let mut body = Function::new([]);
            let mut sink = body.instructions();
            for result in results {
                match result {
                    ValType::I64 => sink.i64_const(0),
                    ValType::F64 => sink.f64_const(0.0.into()),
                    _ => sink.i32_const(0),
                };
            }
            sink.end();
            code.function(&body);
        }

        if let Some(name) = self.global {
            globals.global(
                GlobalType {
                    val_type: ValType::I32,
                    mutable: false,
                    shared: false,
                },
                &ConstExpr::i32_const(0),
            );
            exports.export(name.text(), ExportKind::Global, 0);
        }

        let mut module = Module::new();
        module.section(&types);
        module.section(&imports);
        module.section(&functions);
        module.section(&memories);
        module.section(&globals);
        module.section(&exports);
        module.section(&code);
        module.finish()
    }
}
