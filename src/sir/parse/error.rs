use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ParseErrorKind {
    #[error("unexpected character `{0}`")]
    UnexpectedChar(char),

    #[error("invalid integer `{0}`")]
    InvalidInteger(String),

    #[error("expected {expected}, found {found}")]
    Expected { expected: String, found: String },

    #[error("unknown type `{0}`")]
    UnknownType(String),

    #[error("unknown instruction `{0}`")]
    UnknownOp(String),

    #[error("unknown condition code `{0}`")]
    UnknownCondition(String),

    #[error("block `{0}` is defined more than once")]
    DuplicateBlock(String),

    #[error("value `{0}` is defined more than once")]
    DuplicateValue(String),

    #[error("block `{0}` is not defined")]
    UndefinedBlock(String),

    #[error("value `{0}` is not defined")]
    UndefinedValue(String),

    #[error("function `{0}` is declared more than once")]
    DuplicateCallee(String),

    #[error("function `{0}` is not declared")]
    UndefinedCallee(String),

    #[error("stack slot `{0}` is declared more than once")]
    DuplicateStackSlot(String),

    #[error("stack slot `{0}` is not declared")]
    UndefinedStackSlot(String),

    #[error("call to `{0}` {1}")]
    CallResult(String, &'static str),

    #[error("entry block params do not match the function signature")]
    EntryParams,

    #[error("function has no blocks")]
    NoBlocks,
}

#[derive(Clone, Debug, Error, Diagnostic)]
#[error("{kind}")]
#[diagnostic(code(sir::parse))]
pub struct ParseError {
    pub kind: ParseErrorKind,
    #[source_code]
    source_code: NamedSource<String>,
    #[label("here")]
    span: SourceSpan,
}

impl ParseError {
    pub(super) fn new(
        kind: ParseErrorKind,
        span: SourceSpan,
        source_name: &str,
        source: &str,
    ) -> Self {
        Self {
            kind,
            source_code: NamedSource::new(source_name, source.to_owned()),
            span,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Failure {
    pub kind: ParseErrorKind,
    pub span: SourceSpan,
}

pub(super) type PResult<T> = Result<T, Failure>;
