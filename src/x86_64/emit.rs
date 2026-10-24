use std::collections::BTreeSet;

use iced_x86::code_asm::{AsmRegister64, CodeAssembler, r8, r9, r10, r11, rax, rcx, rdi, rdx, rsi};
use iced_x86::{Formatter, IntelFormatter};

use super::error::CompileError;
use super::vcode::{Inst, PReg, VCode};

pub struct Compiled {
    pub name: String,
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

    // a block left empty by dropping its fallthrough jump shares the next block's label
    let emitted = |index: usize, inst: &Inst<PReg>| !matches!(inst, Inst::Jmp { target } if target.0 == index + 1);
    let mut resolved: Vec<usize> = (0..code.blocks.len()).collect();
    for index in (0..code.blocks.len() - 1).rev() {
        if !code.blocks[index].insts.iter().any(|i| emitted(index, i)) {
            resolved[index] = resolved[index + 1];
        }
    }
    let mut labels: Vec<_> = (0..code.blocks.len()).map(|_| a.create_label()).collect();
    let targeted: BTreeSet<usize> = code
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter().filter_map(|i| i.target()))
        .map(|t| resolved[t.0])
        .collect();

    let mut lines = Vec::new();
    for (index, block) in code.blocks.iter().enumerate() {
        if targeted.contains(&index) && resolved[index] == index {
            a.set_label(&mut labels[index]).map_err(assemble_error)?;
            lines.push(Line::Label(index));
        }
        for inst in block.insts.iter().filter(|i| emitted(index, i)) {
            match *inst {
                Inst::MovImm { dst, imm } => a.mov(asm_reg(dst), imm),
                Inst::Mov { dst, src } => a.mov(asm_reg(dst), asm_reg(src)),
                Inst::Add { dst, src } => a.add(asm_reg(dst), asm_reg(src)),
                Inst::Test { reg } => a.test(asm_reg(reg), asm_reg(reg)),
                Inst::Jnz { target } => a.jne(labels[resolved[target.0]]),
                Inst::Jmp { target } => a.jmp(labels[resolved[target.0]]),
                Inst::Ret { .. } => a.ret(),
            }
            .map_err(assemble_error)?;
            lines.push(match *inst {
                Inst::Jnz { target } => Line::Branch("jne", resolved[target.0]),
                Inst::Jmp { target } => Line::Branch("jmp", resolved[target.0]),
                _ => Line::Plain,
            });
        }
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
    let mut instructions = a.instructions().iter();
    for line in lines {
        match line {
            Line::Label(index) => asm.push_str(&format!(".Lbb{index}:\n")),
            Line::Branch(mnemonic, target) => {
                instructions.next();
                asm.push_str(&format!("    {mnemonic} .Lbb{target}\n"));
            }
            Line::Plain => {
                asm.push_str("    ");
                formatter.format(instructions.next().expect("one per line"), &mut asm);
                asm.push('\n');
            }
        }
    }

    Ok(Compiled {
        name: code.name.clone(),
        code: bytes,
        asm,
    })
}

enum Line {
    Label(usize),
    Branch(&'static str, usize),
    Plain,
}
