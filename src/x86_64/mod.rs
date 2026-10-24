mod emit;
mod error;
mod lower;
mod regalloc;
mod vcode;

pub use emit::{Compiled, emit};
pub use error::CompileError;
pub use lower::lower;
pub use regalloc::allocate;
pub use vcode::{Inst, PReg, Reg, VCode, VReg};

use crate::sir::{Function, verify};

pub fn compile(func: &Function) -> Result<Compiled, CompileError> {
    verify(func)?;
    emit(&allocate(lower(func)?)?)
}
