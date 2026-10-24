use miette::Diagnostic;
use thiserror::Error;

use crate::sir::VerifyErrors;

#[derive(Debug, Error, Diagnostic)]
pub enum CompileError {
    #[error("function is invalid")]
    #[diagnostic(code(wasm::invalid_function))]
    Invalid(#[from] VerifyErrors),

    #[error("wasm does not support functions with more than one block yet")]
    #[diagnostic(
        code(wasm::multiple_blocks),
        help("arbitrary control flow needs a stackifier or relooper")
    )]
    MultipleBlocks,

    #[error("failed to print wat: {0}")]
    #[diagnostic(code(wasm::print))]
    Print(String),
}
