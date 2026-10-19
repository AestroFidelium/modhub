//! Function signatures, as the host calls them and as a module declares them.

use std::fmt;

/// A WebAssembly value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    /// 32-bit integer.
    I32,
    /// 64-bit integer.
    I64,
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
    /// 128-bit vector.
    V128,
    /// Any reference type.
    Ref,
}

impl From<wasmparser::ValType> for ValType {
    fn from(ty: wasmparser::ValType) -> Self {
        match ty {
            wasmparser::ValType::I32 => Self::I32,
            wasmparser::ValType::I64 => Self::I64,
            wasmparser::ValType::F32 => Self::F32,
            wasmparser::ValType::F64 => Self::F64,
            wasmparser::ValType::V128 => Self::V128,
            wasmparser::ValType::Ref(_) => Self::Ref,
        }
    }
}

impl fmt::Display for ValType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::V128 => "v128",
            Self::Ref => "ref",
        })
    }
}

/// Parameter and result types of a function, displayed as `(i32, i32) -> i64`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    params: Box<[ValType]>,
    results: Box<[ValType]>,
}

impl Signature {
    pub(crate) fn new(params: &[ValType], results: &[ValType]) -> Self {
        Self {
            params: params.into(),
            results: results.into(),
        }
    }

    pub(crate) fn from_func_type(ty: &wasmparser::FuncType) -> Self {
        Self {
            params: ty.params().iter().copied().map(ValType::from).collect(),
            results: ty.results().iter().copied().map(ValType::from).collect(),
        }
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |types: &[ValType]| {
            types
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let results = match &*self.results {
            [single] => single.to_string(),
            results => format!("({})", list(results)),
        };
        write!(f, "({}) -> {results}", list(&self.params))
    }
}
