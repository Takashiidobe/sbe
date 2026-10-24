use miette::Diagnostic;
use thiserror::Error;

use crate::sir::VerifyErrors;

#[derive(Debug, Error, Diagnostic)]
pub enum CompileError {
    #[error("function is invalid")]
    #[diagnostic(code(wasm::invalid_function))]
    Invalid(#[from] VerifyErrors),

    #[error("failed to print wat: {0}")]
    #[diagnostic(code(wasm::print))]
    Print(String),
}
