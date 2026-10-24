use iced_x86::code_asm::{AsmRegister64, CodeAssembler, r8, r9, r10, r11, rax, rcx, rdi, rdx, rsi};
use iced_x86::{Formatter, IntelFormatter};

use super::error::CompileError;
use super::vcode::{Inst, PReg, VCode};

pub struct Compiled {
    pub code: Vec<u8>,
    pub asm: String,
}

fn asm_reg(reg: PReg) -> AsmRegister64 {
    match reg {
        PReg::Rax => rax,
        PReg::Rcx => rcx,
        PReg::Rdx => rdx,
        PReg::Rsi => rsi,
        PReg::Rdi => rdi,
        PReg::R8 => r8,
        PReg::R9 => r9,
        PReg::R10 => r10,
        PReg::R11 => r11,
    }
}

fn assemble_error(err: iced_x86::IcedError) -> CompileError {
    CompileError::Assemble(err.to_string())
}

pub fn emit(code: &VCode<PReg>) -> Result<Compiled, CompileError> {
    let mut a = CodeAssembler::new(64).map_err(assemble_error)?;
    for inst in &code.insts {
        match *inst {
            Inst::MovImm { dst, imm } => a.mov(asm_reg(dst), imm),
            Inst::Mov { dst, src } => a.mov(asm_reg(dst), asm_reg(src)),
            Inst::Add { dst, src } => a.add(asm_reg(dst), asm_reg(src)),
            Inst::Ret { .. } => a.ret(),
        }
        .map_err(assemble_error)?;
    }

    let bytes = a.assemble(0).map_err(assemble_error)?;

    let mut formatter = IntelFormatter::new();
    let options = formatter.options_mut();
    options.set_space_after_operand_separator(true);
    options.set_hex_prefix("0x");
    options.set_hex_suffix("");
    options.set_uppercase_hex(false);
    options.set_signed_immediate_operands(true);
    let mut asm = format!("{}:\n", code.name);
    for inst in a.instructions() {
        asm.push_str("    ");
        formatter.format(inst, &mut asm);
        asm.push('\n');
    }

    Ok(Compiled { code: bytes, asm })
}
