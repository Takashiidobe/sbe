use miette::Diagnostic;
use thiserror::Error;

use crate::sir::VerifyErrors;

#[derive(Debug, Error, Diagnostic)]
pub enum CompileError {
    #[error("function is invalid")]
    #[diagnostic(code(x86_64::invalid_function))]
    Invalid(#[from] VerifyErrors),

    #[error("x86_64 supports at most {max} register params, found {found}")]
    #[diagnostic(code(x86_64::too_many_params))]
    TooManyParams { found: usize, max: usize },

    #[error("x86_64 supports at most one return value, found {0}")]
    #[diagnostic(code(x86_64::too_many_returns))]
    TooManyReturns(usize),

    #[error("calls are not supported yet")]
    #[diagnostic(code(x86_64::call))]
    Call,

    #[error("ran out of registers; spilling is not implemented")]
    #[diagnostic(code(x86_64::out_of_registers))]
    OutOfRegisters,

    #[error("failed to assemble: {0}")]
    #[diagnostic(code(x86_64::assemble))]
    Assemble(String),

    #[error("failed to write object file: {0}")]
    #[diagnostic(code(x86_64::object))]
    Object(String),
}
