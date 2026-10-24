use miette::Diagnostic;
use thiserror::Error;

use crate::sir::ModuleErrors;

#[derive(Debug, Error, Diagnostic)]
pub enum CompileError {
    #[error("module is invalid")]
    #[diagnostic(code(wasm::invalid_function))]
    Invalid(#[from] ModuleErrors),

    #[error("failed to print wat: {0}")]
    #[diagnostic(code(wasm::print))]
    Print(String),
}
