use std::collections::HashMap;

use super::error::CompileError;
use super::vcode::{Inst, Label, PReg, Reg, VBlock, VCode, VReg};
use crate::sir::{Block, ControlFlow, Function, InstData, Terminator, Value};

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
                InstData::Iadd { args: [a, b] } => {
                    insts.push(Inst::Mov {
                        dst: result,
                        src: vreg(*a),
                    });
                    insts.push(Inst::Add {
                        dst: result,
                        src: vreg(*b),
                    });
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
