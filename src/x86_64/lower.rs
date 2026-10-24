use std::collections::HashMap;

use super::error::CompileError;
use super::vcode::{AluOp, Cc, Inst, Label, PReg, Reg, ShiftOp, UnaryOp, VBlock, VCode, VReg};
use crate::sir::{
    BinOp, Block, ControlFlow, CvtOp, Function, InstData, IntCC, Terminator, Type, UnOp, Value,
    ValueDef,
};

fn vreg(value: Value) -> Reg {
    Reg::V(VReg::from_index(value.index()))
}

fn fresh(next_vreg: &mut usize) -> Reg {
    *next_vreg += 1;
    Reg::V(VReg::from_index(*next_vreg - 1))
}

/// narrow values keep undefined upper bits, so ops that can see them read an extended copy
fn extended(
    insts: &mut Vec<Inst<Reg>>,
    next_vreg: &mut usize,
    func: &Function,
    value: Value,
    signed: bool,
) -> Reg {
    let ty = func.value_type(value);
    if ty == Type::I64 {
        return vreg(value);
    }
    let dst = fresh(next_vreg);
    insts.push(Inst::Ext {
        signed,
        bits: ty.bits() as u8,
        dst,
        src: vreg(value),
    });
    dst
}

// an icmp result is exactly 0 or 1; any other i8 may carry garbage above bit 7
fn condition_reg(
    insts: &mut Vec<Inst<Reg>>,
    next_vreg: &mut usize,
    func: &Function,
    cond: Value,
) -> Reg {
    let from_icmp = match func.value_def(cond) {
        ValueDef::Result(inst) => matches!(func.inst(inst), InstData::Icmp { .. }),
        ValueDef::Param(..) => false,
    };
    if from_icmp {
        vreg(cond)
    } else {
        extended(insts, next_vreg, func, cond, false)
    }
}

struct Layout {
    block_index: HashMap<Block, usize>,
    /// blocks holding the param moves of an edge, keyed by (source, position in `targets`)
    edge_index: HashMap<(Block, usize), usize>,
}

impl Layout {
    fn new(func: &Function, order: &[Block]) -> Self {
        let mut layout = Layout {
            block_index: HashMap::new(),
            edge_index: HashMap::new(),
        };
        let mut next = 0;
        for &block in order {
            layout.block_index.insert(block, next);
            next += 1;
            if let Some(terminator) = func.block(block).terminator() {
                for (idx, (_, args)) in terminator.targets().iter().enumerate().rev() {
                    if !args.is_empty() && !matches!(terminator, Terminator::Jump { .. }) {
                        layout.edge_index.insert((block, idx), next);
                        next += 1;
                    }
                }
            }
        }
        layout
    }
}

/// emits `dsts[i] = srcs[i]` as if all moves happened at once, using fresh vregs to break cycles
fn parallel_moves(dsts: &[Value], srcs: &[Value], next_vreg: &mut usize) -> Vec<Inst<Reg>> {
    let mut pending: Vec<(Reg, Reg)> = dsts
        .iter()
        .zip(srcs)
        .map(|(&d, &s)| (vreg(d), vreg(s)))
        .filter(|(d, s)| d != s)
        .collect();
    let mut out = Vec::new();
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .position(|&(d, _)| pending.iter().all(|&(_, s)| s != d));
        if let Some(i) = ready {
            let (dst, src) = pending.remove(i);
            out.push(Inst::Mov { dst, src });
            continue;
        }
        let blocked = pending[0].0;
        let saved = Reg::V(VReg::from_index(*next_vreg));
        *next_vreg += 1;
        out.push(Inst::Mov {
            dst: saved,
            src: blocked,
        });
        for (_, src) in &mut pending {
            if *src == blocked {
                *src = saved;
            }
        }
    }
    out
}

/// expects a verified function
pub fn lower(func: &Function) -> Result<VCode<Reg>, CompileError> {
    let signature = func.signature();
    if signature.params.len() > PReg::ARGS.len() {
        return Err(CompileError::TooManyParams {
            found: signature.params.len(),
            max: PReg::ARGS.len(),
        });
    }
    if signature.returns.len() > 1 {
        return Err(CompileError::TooManyReturns(signature.returns.len()));
    }

    let cfg = ControlFlow::compute(func);
    let order = cfg.reverse_postorder();
    let layout = Layout::new(func, order);
    let mut next_vreg = func.num_values();
    let mut blocks = Vec::new();
    let mut slot_offsets = Vec::new();
    let mut stack_size = 0u32;
    for &size in func.stack_slots() {
        slot_offsets.push(stack_size);
        stack_size += size.next_multiple_of(8);
    }

    for &block in order {
        let data = func.block(block);
        let mut insts = Vec::new();

        if block == func.entry_block() {
            for (&param, &arg) in data.params().iter().zip(&PReg::ARGS) {
                insts.push(Inst::Mov {
                    dst: vreg(param),
                    src: Reg::P(arg),
                });
            }
        }

        for &inst in data.insts() {
            if let InstData::Call { callee, args } = func.inst(inst) {
                let symbol = func.callee(*callee).name.clone();
                lower_call(&mut insts, symbol, args, func.inst_result(inst))?;
                continue;
            }
            if let InstData::Store {
                args: [value, addr],
            } = func.inst(inst)
            {
                insts.push(Inst::Store {
                    bits: func.value_type(*value).bits() as u8,
                    addr: vreg(*addr),
                    src: vreg(*value),
                });
                continue;
            }
            let result = vreg(
                func.inst_result(inst)
                    .expect("only calls and stores are void"),
            );
            match func.inst(inst) {
                InstData::Iconst { value, .. } => insts.push(Inst::MovImm {
                    dst: result,
                    imm: *value,
                }),
                InstData::Icmp { cc, args: [a, b] } => {
                    let signed = matches!(cc, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge);
                    let a = extended(&mut insts, &mut next_vreg, func, *a, signed);
                    let b = extended(&mut insts, &mut next_vreg, func, *b, signed);
                    insts.push(Inst::Cmp { a, b });
                    insts.push(Inst::Setcc {
                        cc: condition(*cc),
                        dst: result,
                    });
                    insts.push(Inst::Movzx8 { dst: result });
                }
                InstData::Select {
                    args: [cond, then_value, else_value],
                } => {
                    insts.push(Inst::Mov {
                        dst: result,
                        src: vreg(*else_value),
                    });
                    let cond = condition_reg(&mut insts, &mut next_vreg, func, *cond);
                    insts.push(Inst::Test { reg: cond });
                    insts.push(Inst::Cmov {
                        cc: Cc::Ne,
                        dst: result,
                        src: vreg(*then_value),
                    });
                }
                InstData::Unary { op, arg } => {
                    insts.push(Inst::Mov {
                        dst: result,
                        src: vreg(*arg),
                    });
                    insts.push(Inst::Unary {
                        op: match op {
                            UnOp::Ineg => UnaryOp::Neg,
                            UnOp::Bnot => UnaryOp::Not,
                        },
                        dst: result,
                    });
                }
                InstData::Convert { op, arg, .. } => match op {
                    CvtOp::Sextend | CvtOp::Uextend => insts.push(Inst::Ext {
                        signed: *op == CvtOp::Sextend,
                        bits: func.value_type(*arg).bits() as u8,
                        dst: result,
                        src: vreg(*arg),
                    }),
                    CvtOp::Ireduce => insts.push(Inst::Mov {
                        dst: result,
                        src: vreg(*arg),
                    }),
                },
                InstData::Load { ty, addr } => insts.push(Inst::Load {
                    bits: ty.bits() as u8,
                    dst: result,
                    addr: vreg(*addr),
                }),
                InstData::StackAddr { slot } => insts.push(Inst::StackAddr {
                    dst: result,
                    offset: slot_offsets[slot.index()],
                }),
                InstData::Call { .. } | InstData::Store { .. } => unreachable!("lowered above"),
                InstData::Binary { op, args: [a, b] } => {
                    let ty = func.value_type(*a);
                    let signed = matches!(op, BinOp::Sshr | BinOp::Sdiv | BinOp::Srem);
                    let exts = |op: &BinOp| match op {
                        BinOp::Ushr | BinOp::Sshr => (true, false),
                        BinOp::Sdiv | BinOp::Udiv | BinOp::Srem | BinOp::Urem => (true, true),
                        _ => (false, false),
                    };
                    let (ext_a, ext_b) = exts(op);
                    let ra = if ext_a {
                        extended(&mut insts, &mut next_vreg, func, *a, signed)
                    } else {
                        vreg(*a)
                    };
                    let rb = if ext_b {
                        extended(&mut insts, &mut next_vreg, func, *b, signed)
                    } else {
                        vreg(*b)
                    };
                    lower_binary(&mut insts, &mut next_vreg, *op, ty, result, ra, rb);
                }
            }
        }

        let jump_to = |dest: Block| Inst::Jmp {
            target: Label(layout.block_index[&dest]),
        };
        let mut edges = Vec::new();
        match data.terminator().expect("verified function") {
            Terminator::Return { values } => {
                if let [value] = values.as_slice() {
                    insts.push(Inst::Mov {
                        dst: Reg::P(PReg::RET),
                        src: vreg(*value),
                    });
                }
                insts.push(Inst::Ret {
                    has_value: !values.is_empty(),
                });
            }
            Terminator::Jump { dest, args } => {
                insts.extend(parallel_moves(
                    func.block(*dest).params(),
                    args,
                    &mut next_vreg,
                ));
                insts.push(jump_to(*dest));
            }
            Terminator::Trap | Terminator::Unreachable => insts.push(Inst::Ud2),
            terminator @ (Terminator::Brif { .. } | Terminator::Switch { .. }) => {
                let targets = terminator.targets();
                let mut labels = vec![Label(0); targets.len()];
                for (idx, &(dest, args)) in targets.iter().enumerate().rev() {
                    match layout.edge_index.get(&(block, idx)) {
                        Some(&edge) => {
                            labels[idx] = Label(edge);
                            let mut moves =
                                parallel_moves(func.block(dest).params(), args, &mut next_vreg);
                            moves.push(jump_to(dest));
                            edges.push(VBlock { insts: moves });
                        }
                        None => labels[idx] = Label(layout.block_index[&dest]),
                    }
                }
                match terminator {
                    Terminator::Brif { cond, .. } => {
                        let cond = condition_reg(&mut insts, &mut next_vreg, func, *cond);
                        insts.push(Inst::Test { reg: cond });
                        insts.push(Inst::Jnz { target: labels[0] });
                        insts.push(Inst::Jmp { target: labels[1] });
                    }
                    Terminator::Switch { value, cases, .. } => {
                        let ty = func.value_type(*value);
                        let value = extended(&mut insts, &mut next_vreg, func, *value, false);
                        for (case, &target) in cases.iter().zip(&labels) {
                            let key = fresh(&mut next_vreg);
                            insts.push(Inst::MovImm {
                                dst: key,
                                imm: ty.truncate(case.key) as i64,
                            });
                            insts.push(Inst::Cmp { a: value, b: key });
                            insts.push(Inst::Jz { target });
                        }
                        insts.push(Inst::Jmp {
                            target: labels[cases.len()],
                        });
                    }
                    _ => unreachable!("matched above"),
                }
            }
        }
        blocks.push(VBlock { insts });
        blocks.extend(edges);
    }

    Ok(VCode {
        name: func.name().to_owned(),
        blocks,
        stack_size,
    })
}

fn lower_call(
    insts: &mut Vec<Inst<Reg>>,
    symbol: String,
    args: &[Value],
    result: Option<Value>,
) -> Result<(), CompileError> {
    if args.len() > PReg::ARGS.len() {
        return Err(CompileError::TooManyParams {
            found: args.len(),
            max: PReg::ARGS.len(),
        });
    }
    let arg_regs: Vec<Reg> = PReg::ARGS.iter().map(|&p| Reg::P(p)).collect();
    for (&arg, &dst) in args.iter().zip(&arg_regs) {
        insts.push(Inst::Mov {
            dst,
            src: vreg(arg),
        });
    }
    insts.push(Inst::Call {
        symbol,
        args: arg_regs[..args.len()].to_vec(),
        ret: result.map(|_| Reg::P(PReg::RET)),
    });
    if let Some(result) = result {
        insts.push(Inst::Mov {
            dst: vreg(result),
            src: Reg::P(PReg::RET),
        });
    }
    Ok(())
}

fn lower_binary(
    insts: &mut Vec<Inst<Reg>>,
    next_vreg: &mut usize,
    op: BinOp,
    ty: Type,
    result: Reg,
    a: Reg,
    b: Reg,
) {
    let alu = |insts: &mut Vec<Inst<Reg>>, op| {
        insts.push(Inst::Mov {
            dst: result,
            src: a,
        });
        insts.push(Inst::Alu {
            op,
            dst: result,
            src: b,
        });
    };
    let mut shift = |insts: &mut Vec<Inst<Reg>>, op| {
        let count = Reg::P(PReg::Rcx);
        insts.push(Inst::Mov {
            dst: result,
            src: a,
        });
        insts.push(Inst::Mov { dst: count, src: b });
        if ty != Type::I64 {
            let mask = fresh(next_vreg);
            insts.push(Inst::MovImm {
                dst: mask,
                imm: i64::from(ty.bits()) - 1,
            });
            insts.push(Inst::Alu {
                op: AluOp::And,
                dst: count,
                src: mask,
            });
        }
        insts.push(Inst::Shift {
            op,
            dst: result,
            count,
        });
    };
    let divide = |insts: &mut Vec<Inst<Reg>>, signed: bool, quotient: bool| {
        let (lo, hi) = (Reg::P(PReg::Rax), Reg::P(PReg::Rdx));
        insts.push(Inst::Mov { dst: lo, src: a });
        if signed {
            insts.push(Inst::Cqo { hi, lo });
        } else {
            insts.push(Inst::MovImm { dst: hi, imm: 0 });
        }
        insts.push(Inst::Div {
            signed,
            divisor: b,
            lo,
            hi,
        });
        insts.push(Inst::Mov {
            dst: result,
            src: if quotient { lo } else { hi },
        });
    };
    match op {
        BinOp::Iadd => alu(insts, AluOp::Add),
        BinOp::Isub => alu(insts, AluOp::Sub),
        BinOp::Imul => alu(insts, AluOp::Imul),
        BinOp::Band => alu(insts, AluOp::And),
        BinOp::Bor => alu(insts, AluOp::Or),
        BinOp::Bxor => alu(insts, AluOp::Xor),
        BinOp::Ishl => shift(insts, ShiftOp::Shl),
        BinOp::Ushr => shift(insts, ShiftOp::Shr),
        BinOp::Sshr => shift(insts, ShiftOp::Sar),
        BinOp::Sdiv => divide(insts, true, true),
        BinOp::Udiv => divide(insts, false, true),
        BinOp::Srem => divide(insts, true, false),
        BinOp::Urem => divide(insts, false, false),
    }
}

fn condition(cc: IntCC) -> Cc {
    match cc {
        IntCC::Eq => Cc::E,
        IntCC::Ne => Cc::Ne,
        IntCC::Slt => Cc::L,
        IntCC::Sle => Cc::Le,
        IntCC::Sgt => Cc::G,
        IntCC::Sge => Cc::Ge,
        IntCC::Ult => Cc::B,
        IntCC::Ule => Cc::Be,
        IntCC::Ugt => Cc::A,
        IntCC::Uge => Cc::Ae,
    }
}
