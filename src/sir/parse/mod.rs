mod ast;
mod error;
mod lexer;
mod lower;
mod syntax;

pub use error::{ParseError, ParseErrorKind};

use super::{Function, Module, Signature};

/// Parses one function. The result is not verified; call `verify` on it.
#[allow(clippy::result_large_err)]
pub fn parse_function(source_name: &str, source: &str) -> Result<Function, ParseError> {
    let wrap = |f: error::Failure| ParseError::new(f.kind, f.span, source_name, source);
    let tokens = lexer::lex(source).map_err(wrap)?;
    let ast = syntax::Parser::new(source, tokens)
        .single_function()
        .map_err(wrap)?;
    lower::lower(ast).map_err(wrap)
}

/// Parses `import` declarations and functions. The result is not verified; call `verify_module`.
#[allow(clippy::result_large_err)]
pub fn parse_module(source_name: &str, source: &str) -> Result<Module, ParseError> {
    let wrap = |f: error::Failure| ParseError::new(f.kind, f.span, source_name, source);
    let tokens = lexer::lex(source).map_err(wrap)?;
    let items = syntax::Parser::new(source, tokens).module().map_err(wrap)?;

    let mut module = Module::new();
    for item in items {
        match item {
            ast::Item::Import(import) => module.declare_import(
                import.name,
                Signature {
                    params: import.params,
                    returns: import.returns,
                },
            ),
            ast::Item::Function(func) => module.add_function(lower::lower(func).map_err(wrap)?),
        }
    }
    Ok(module)
}
