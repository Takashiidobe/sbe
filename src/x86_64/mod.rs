mod emit;
mod error;
mod lower;
mod object;
mod regalloc;
mod vcode;

pub use emit::{Compiled, Reloc, emit};
pub use error::CompileError;
pub use lower::lower;
pub use object::module_object;
pub use regalloc::allocate;
pub use vcode::{Inst, Label, PReg, Reg, VBlock, VCode, VReg};

use crate::sir::{Function, Module, verify, verify_module};

pub fn compile(func: &Function) -> Result<Compiled, CompileError> {
    verify(func)?;
    emit(&allocate(lower(func)?)?)
}

/// compiles every defined function; calls become relocations against the callee's name
pub fn compile_module(module: &Module) -> Result<Vec<Compiled>, CompileError> {
    verify_module(module)?;
    module.functions().iter().map(compile).collect()
}
