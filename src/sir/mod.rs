mod entities;
mod function;
mod inst;
mod types;

pub use entities::{Block, Inst, Value};
pub use function::{BlockData, Function, Signature, ValueDef};
pub use inst::{InstData, Terminator};
pub use types::Type;
