mod ast;
mod error;
mod lexer;
mod lower;
mod syntax;

pub use error::{ParseError, ParseErrorKind};

use super::Function;

/// Parses one function. The result is not verified; call `verify` on it.
#[allow(clippy::result_large_err)]
pub fn parse_function(source_name: &str, source: &str) -> Result<Function, ParseError> {
    let wrap = |f: error::Failure| ParseError::new(f.kind, f.span, source_name, source);
    let tokens = lexer::lex(source).map_err(wrap)?;
    let ast = syntax::Parser::new(source, tokens)
        .function()
        .map_err(wrap)?;
    lower::lower(ast).map_err(wrap)
}
