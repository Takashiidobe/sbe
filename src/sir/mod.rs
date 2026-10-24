mod builder;
mod cfg;
mod entities;
mod function;
mod inst;
mod parse;
mod printer;
mod types;
mod verify;

pub use builder::{BuilderError, FunctionBuilder};
pub use cfg::ControlFlow;
pub use entities::{Block, Inst, Value};
pub use function::{BlockData, Function, Signature, ValueDef};
pub use inst::{BinOp, InstData, Terminator};
pub use parse::{ParseError, ParseErrorKind, parse_function};
pub use types::Type;
pub use verify::{Use, VerifyError, VerifyErrors, verify};
