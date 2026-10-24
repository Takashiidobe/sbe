use std::collections::HashMap;

use super::error::CompileError;
use super::vcode::{AluOp, Inst, Label, PReg, Reg, ShiftOp, UnaryOp, VBlock, VCode, VReg};
use crate::sir::{BinOp, Block, ControlFlow, Function, InstData, Terminator, UnOp, Value};

fn vreg(value: Value) -> Reg {
    Reg::V(VReg::from_index(value.index()))
}

struct Layout {
    block_index: HashMap<Block, usize>,
    /// blocks holding the param moves of a `brif` edge, keyed by (source, is_then)
    edge_index: HashMap<(Block, bool), usize>,
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
            if let Some(Terminator::Brif {
                then_args,
                else_args,
                ..
            }) = func.block(block).terminator()
            {
                for (is_then, args) in [(false, else_args), (true, then_args)] {
                    if !args.is_empty() {
                        layout.edge_index.insert((block, is_then), next);
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
            let result = vreg(func.inst_result(inst));
            match func.inst(inst) {
                InstData::Iconst { value } => insts.push(Inst::MovImm {
                    dst: result,
                    imm: *value,
                }),
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
                InstData::Binary { op, args: [a, b] } => {
                    lower_binary(&mut insts, *op, result, vreg(*a), vreg(*b));
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
            Terminator::Brif {
                cond,
                then_dest,
                then_args,
                else_dest,
                else_args,
            } => {
                let mut targets = [Label(0); 2];
                for (slot, (is_then, dest, args)) in [
                    (false, *else_dest, else_args),
                    (true, *then_dest, then_args),
                ]
                .into_iter()
                .enumerate()
                {
                    match layout.edge_index.get(&(block, is_then)) {
                        Some(&edge) => {
                            targets[slot] = Label(edge);
                            let mut moves =
                                parallel_moves(func.block(dest).params(), args, &mut next_vreg);
                            moves.push(jump_to(dest));
                            edges.push(VBlock { insts: moves });
                        }
                        None => targets[slot] = Label(layout.block_index[&dest]),
                    }
                }
                let [else_target, then_target] = targets;
                insts.push(Inst::Test { reg: vreg(*cond) });
                insts.push(Inst::Jnz {
                    target: then_target,
                });
                insts.push(Inst::Jmp {
                    target: else_target,
                });
            }
        }
        blocks.push(VBlock { insts });
        blocks.extend(edges);
    }

    Ok(VCode {
        name: func.name().to_owned(),
        blocks,
    })
}

fn lower_binary(insts: &mut Vec<Inst<Reg>>, op: BinOp, result: Reg, a: Reg, b: Reg) {
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
    let shift = |insts: &mut Vec<Inst<Reg>>, op| {
        let count = Reg::P(PReg::Rcx);
        insts.push(Inst::Mov {
            dst: result,
            src: a,
        });
        insts.push(Inst::Mov { dst: count, src: b });
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
