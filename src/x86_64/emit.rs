use std::collections::BTreeSet;

use iced_x86::BlockEncoderOptions;
use iced_x86::code_asm::{
    AsmRegister8, AsmRegister16, AsmRegister32, AsmRegister64, CodeAssembler, al, ax, bl, bx, cl,
    cx, di, dil, dl, dx, eax, ebx, ecx, edi, edx, esi, r8, r8b, r8d, r8w, r9, r9b, r9d, r9w, r10,
    r10b, r10d, r10w, r11, r11b, r11d, r11w, r12, r12b, r12d, r12w, r13, r13b, r13d, r13w, r14,
    r14b, r14d, r14w, r15, r15b, r15d, r15w, rax, rbx, rcx, rdi, rdx, rsi, rsp, si, sil,
};
use iced_x86::{Formatter, IntelFormatter};

use super::error::CompileError;
use super::vcode::{AluOp, Cc, Inst, PReg, ShiftOp, UnaryOp, VCode};

/// a 32-bit pc-relative field at `offset` that the linker points at `symbol`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reloc {
    pub offset: usize,
    pub symbol: String,
}

pub struct Compiled {
    pub name: String,
    pub code: Vec<u8>,
    pub asm: String,
    pub relocs: Vec<Reloc>,
}

/// stands in for the callee; the rel32 it produces is zeroed and left to the linker
const CALL_PLACEHOLDER: u64 = 0x1000_0000;

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
        PReg::Rbx => rbx,
        PReg::R12 => r12,
        PReg::R13 => r13,
        PReg::R14 => r14,
        PReg::R15 => r15,
    }
}

fn asm_reg8(reg: PReg) -> AsmRegister8 {
    match reg {
        PReg::Rax => al,
        PReg::Rcx => cl,
        PReg::Rdx => dl,
        PReg::Rsi => sil,
        PReg::Rdi => dil,
        PReg::R8 => r8b,
        PReg::R9 => r9b,
        PReg::R10 => r10b,
        PReg::R11 => r11b,
        PReg::Rbx => bl,
        PReg::R12 => r12b,
        PReg::R13 => r13b,
        PReg::R14 => r14b,
        PReg::R15 => r15b,
    }
}

fn asm_reg16(reg: PReg) -> AsmRegister16 {
    match reg {
        PReg::Rax => ax,
        PReg::Rcx => cx,
        PReg::Rdx => dx,
        PReg::Rsi => si,
        PReg::Rdi => di,
        PReg::R8 => r8w,
        PReg::R9 => r9w,
        PReg::R10 => r10w,
        PReg::R11 => r11w,
        PReg::Rbx => bx,
        PReg::R12 => r12w,
        PReg::R13 => r13w,
        PReg::R14 => r14w,
        PReg::R15 => r15w,
    }
}

fn asm_reg32(reg: PReg) -> AsmRegister32 {
    match reg {
        PReg::Rax => eax,
        PReg::Rcx => ecx,
        PReg::Rdx => edx,
        PReg::Rsi => esi,
        PReg::Rdi => edi,
        PReg::R8 => r8d,
        PReg::R9 => r9d,
        PReg::R10 => r10d,
        PReg::R11 => r11d,
        PReg::Rbx => ebx,
        PReg::R12 => r12d,
        PReg::R13 => r13d,
        PReg::R14 => r14d,
        PReg::R15 => r15d,
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
    let mut calls = Vec::new();
    for (index, block) in code.blocks.iter().enumerate() {
        if targeted.contains(&index) && resolved[index] == index {
            a.set_label(&mut labels[index]).map_err(assemble_error)?;
            lines.push(Line::Label(index));
        }
        for inst in block.insts.iter().filter(|i| emitted(index, i)) {
            match *inst {
                Inst::MovImm { dst, imm } => a.mov(asm_reg(dst), imm),
                Inst::Mov { dst, src } => a.mov(asm_reg(dst), asm_reg(src)),
                Inst::Alu { op, dst, src } => {
                    let (dst, src) = (asm_reg(dst), asm_reg(src));
                    match op {
                        AluOp::Add => a.add(dst, src),
                        AluOp::Sub => a.sub(dst, src),
                        AluOp::Imul => a.imul_2(dst, src),
                        AluOp::And => a.and(dst, src),
                        AluOp::Or => a.or(dst, src),
                        AluOp::Xor => a.xor(dst, src),
                    }
                }
                Inst::Shift { op, dst, count } => {
                    debug_assert_eq!(count, PReg::Rcx);
                    let dst = asm_reg(dst);
                    match op {
                        ShiftOp::Shl => a.shl(dst, cl),
                        ShiftOp::Shr => a.shr(dst, cl),
                        ShiftOp::Sar => a.sar(dst, cl),
                    }
                }
                Inst::Unary { op, dst } => match op {
                    UnaryOp::Neg => a.neg(asm_reg(dst)),
                    UnaryOp::Not => a.not(asm_reg(dst)),
                },
                Inst::Cqo { .. } => a.cqo(),
                Inst::Cmp { a: x, b } => a.cmp(asm_reg(x), asm_reg(b)),
                Inst::Setcc { cc, dst } => {
                    let dst = asm_reg8(dst);
                    match cc {
                        Cc::E => a.sete(dst),
                        Cc::Ne => a.setne(dst),
                        Cc::L => a.setl(dst),
                        Cc::Le => a.setle(dst),
                        Cc::G => a.setg(dst),
                        Cc::Ge => a.setge(dst),
                        Cc::B => a.setb(dst),
                        Cc::Be => a.setbe(dst),
                        Cc::A => a.seta(dst),
                        Cc::Ae => a.setae(dst),
                    }
                }
                Inst::Movzx8 { dst } => a.movzx(asm_reg(dst), asm_reg8(dst)),
                Inst::Ext {
                    signed,
                    bits,
                    dst,
                    src,
                } => match (signed, bits) {
                    (true, 8) => a.movsx(asm_reg(dst), asm_reg8(src)),
                    (true, 16) => a.movsx(asm_reg(dst), asm_reg16(src)),
                    (true, 32) => a.movsxd(asm_reg(dst), asm_reg32(src)),
                    (false, 8) => a.movzx(asm_reg(dst), asm_reg8(src)),
                    (false, 16) => a.movzx(asm_reg(dst), asm_reg16(src)),
                    (false, 32) => a.mov(asm_reg32(dst), asm_reg32(src)),
                    _ => unreachable!("extension from {bits} bits"),
                },
                Inst::Cmov { cc, dst, src } => {
                    let (dst, src) = (asm_reg(dst), asm_reg(src));
                    match cc {
                        Cc::E => a.cmove(dst, src),
                        Cc::Ne => a.cmovne(dst, src),
                        Cc::L => a.cmovl(dst, src),
                        Cc::Le => a.cmovle(dst, src),
                        Cc::G => a.cmovg(dst, src),
                        Cc::Ge => a.cmovge(dst, src),
                        Cc::B => a.cmovb(dst, src),
                        Cc::Be => a.cmovbe(dst, src),
                        Cc::A => a.cmova(dst, src),
                        Cc::Ae => a.cmovae(dst, src),
                    }
                }
                Inst::Div {
                    signed, divisor, ..
                } => {
                    if signed {
                        a.idiv(asm_reg(divisor))
                    } else {
                        a.div(asm_reg(divisor))
                    }
                }
                Inst::Test { reg } => a.test(asm_reg(reg), asm_reg(reg)),
                Inst::Jnz { target } => a.jne(labels[resolved[target.0]]),
                Inst::Jmp { target } => a.jmp(labels[resolved[target.0]]),
                Inst::Ret { .. } => a.ret(),
                Inst::Call { ref symbol, .. } => {
                    let mut site = a.create_label();
                    a.set_label(&mut site).map_err(assemble_error)?;
                    calls.push((site, symbol.clone()));
                    a.call(CALL_PLACEHOLDER)
                }
                Inst::Push { reg } => a.push(asm_reg(reg)),
                Inst::Pop { reg } => a.pop(asm_reg(reg)),
                Inst::AdjustSp { bytes } if bytes >= 0 => a.sub(rsp, bytes),
                Inst::AdjustSp { bytes } => a.add(rsp, -bytes),
            }
            .map_err(assemble_error)?;
            lines.push(match *inst {
                Inst::Jnz { target } => Line::Branch("jne", resolved[target.0]),
                Inst::Jmp { target } => Line::Branch("jmp", resolved[target.0]),
                Inst::Call { ref symbol, .. } => Line::Call(symbol.clone()),
                _ => Line::Plain,
            });
        }
    }

    let assembled = a
        .assemble_options(0, BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS)
        .map_err(assemble_error)?;
    let mut bytes = assembled.inner.code_buffer.clone();
    let mut relocs = Vec::new();
    for (site, symbol) in calls {
        let start = assembled.label_ip(&site).map_err(assemble_error)? as usize;
        bytes[start + 1..start + 5].fill(0);
        relocs.push(Reloc {
            offset: start + 1,
            symbol,
        });
    }

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
            Line::Call(symbol) => {
                instructions.next();
                asm.push_str(&format!("    call {symbol}\n"));
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
        relocs,
    })
}

enum Line {
    Label(usize),
    Call(String),
    Branch(&'static str, usize),
    Plain,
}
