//! The verdict the Stormlight mod contract gives a [`ModuleSpec`], written out
//! independently of the validator: the oracle of the property tests.

use std::collections::BTreeSet;

use modhub_core::EntryPoint;
use wasm_encoder::ValType;

use crate::modules::{ExportName, MemoryExport, MemorySpec, ModuleSpec, SignatureSpec};

/// What the contract says about a module, in the validator's check order.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Accepted {
        entry_points: BTreeSet<EntryPoint>,
        memory_pages: u64,
    },
    Invalid,
    Import {
        module: String,
        name: String,
    },
    MissingExport(&'static str),
    NotAMemory,
    MemoryTooLarge(u64),
    WrongSignature(&'static str),
    NotAFunction(&'static str),
}

impl ModuleSpec {
    pub(crate) fn expected(&self) -> Verdict {
        // Two exports under one name do not form a valid module.
        let mut names: Vec<&str> = self.exports.iter().map(|e| e.name.text()).collect();
        names.extend(self.global.map(ExportName::text));
        if let MemorySpec::Defined {
            export: MemoryExport::AsMemory,
            ..
        } = self.memory
        {
            names.push("memory");
        }
        let unique: BTreeSet<&str> = names.iter().copied().collect();
        if unique.len() != names.len() {
            return Verdict::Invalid;
        }

        if let Some(import) = self.imports.first() {
            let (module, name) = import.module_and_name();
            return Verdict::Import {
                module: module.into(),
                name: name.into(),
            };
        }

        let memory_pages = match self.memory {
            MemorySpec::Defined {
                pages,
                export: MemoryExport::AsMemory,
            } => pages.count(),
            _ if self.has(ExportName::Memory) || self.global == Some(ExportName::Memory) => {
                return Verdict::NotAMemory;
            }
            _ => return Verdict::MissingExport("memory"),
        };
        if memory_pages > 1024 {
            return Verdict::MemoryTooLarge(memory_pages);
        }

        if !self.has(ExportName::Register) && self.global != Some(ExportName::Register) {
            return Verdict::MissingExport("mod_register");
        }
        for name in [
            ExportName::Register,
            ExportName::Alloc,
            ExportName::Handle,
            ExportName::Trigger,
            ExportName::Tick,
        ] {
            if self.global == Some(name) {
                return Verdict::NotAFunction(name.text());
            }
            let wrong = self
                .exports
                .iter()
                .any(|e| e.name == name && !e.signature.matches(name));
            if wrong {
                return Verdict::WrongSignature(name.text());
            }
        }

        let entry_points: BTreeSet<EntryPoint> = self
            .exports
            .iter()
            .filter_map(|e| e.name.entry_point())
            .collect();
        if !entry_points.is_empty() && !self.has(ExportName::Alloc) {
            return Verdict::MissingExport("mod_alloc");
        }
        Verdict::Accepted {
            entry_points,
            memory_pages,
        }
    }

    fn has(&self, name: ExportName) -> bool {
        self.exports.iter().any(|e| e.name == name)
    }
}

impl ExportName {
    pub(crate) fn entry_point(self) -> Option<EntryPoint> {
        match self {
            Self::Handle => Some(EntryPoint::Handle),
            Self::Trigger => Some(EntryPoint::Trigger),
            Self::Tick => Some(EntryPoint::Tick),
            _ => None,
        }
    }
}

impl SignatureSpec {
    /// Whether the signature is the one the host calls. A "wrong" variant can land
    /// on the expected one (an `I32Result` for `mod_alloc`).
    pub(crate) fn matches(self, name: ExportName) -> bool {
        let (params, results) = name.expected();
        let (actual_params, actual_results) = self.apply(name);
        actual_params == params && actual_results == results
    }
}

impl ModuleSpec {
    /// How the signature of the function exported as `name` should read in an error
    /// message, rendered here independently of `Signature`'s `Display`.
    pub(crate) fn signature_text(&self, name: &str) -> String {
        let export = self.exports.iter().find(|e| e.name.text() == name).unwrap();
        let (params, results) = export.signature.apply(export.name);
        let names = |types: &[ValType]| {
            types
                .iter()
                .map(|ty| match ty {
                    ValType::I32 => "i32",
                    ValType::I64 => "i64",
                    ValType::F32 => "f32",
                    ValType::F64 => "f64",
                    ValType::V128 => "v128",
                    ValType::Ref(_) => "ref",
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let results = if results.len() == 1 {
            names(&results)
        } else {
            format!("({})", names(&results))
        };
        format!("({}) -> {results}", names(&params))
    }
}
