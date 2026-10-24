use std::collections::{BTreeMap, BTreeSet};

use super::error::CompileError;
use super::vcode::{Inst, PReg, Reg, VBlock, VCode, VReg};

type Interval = (usize, usize);

fn overlaps(a: Interval, b: Interval) -> bool {
    a.0 < b.1 && b.0 < a.1
}

fn widen(intervals: &mut BTreeMap<VReg, Interval>, v: VReg, pos: usize) {
    let interval = intervals.entry(v).or_insert((pos, pos));
    interval.0 = interval.0.min(pos);
    interval.1 = interval.1.max(pos);
}

/// live-in and live-out vreg sets per block, by backward dataflow
fn liveness(blocks: &[VBlock<Reg>]) -> (Vec<BTreeSet<VReg>>, Vec<BTreeSet<VReg>>) {
    let vregs = |regs: Vec<Reg>| -> Vec<VReg> {
        regs.into_iter()
            .filter_map(|r| match r {
                Reg::V(v) => Some(v),
                Reg::P(_) => None,
            })
            .collect()
    };

    let (mut read_first, mut written) = (Vec::new(), Vec::new());
    for block in blocks {
        let (mut reads, mut writes) = (BTreeSet::new(), BTreeSet::new());
        for inst in &block.insts {
            reads.extend(
                vregs(inst.uses())
                    .into_iter()
                    .filter(|v| !writes.contains(v)),
            );
            writes.extend(vregs(inst.defs()));
        }
        read_first.push(reads);
        written.push(writes);
    }

    let successors: Vec<Vec<usize>> = blocks
        .iter()
        .map(|b| {
            b.insts
                .iter()
                .filter_map(|i| i.target())
                .map(|l| l.0)
                .collect()
        })
        .collect();

    let mut live_in = vec![BTreeSet::new(); blocks.len()];
    let mut live_out = vec![BTreeSet::new(); blocks.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for b in (0..blocks.len()).rev() {
            let out: BTreeSet<VReg> = successors[b]
                .iter()
                .flat_map(|&s| live_in[s].iter().copied())
                .collect();
            let mut live = read_first[b].clone();
            live.extend(out.difference(&written[b]).copied());
            if out != live_out[b] || live != live_in[b] {
                live_out[b] = out;
                live_in[b] = live;
                changed = true;
            }
        }
    }
    (live_in, live_out)
}

/// linear scan without spilling over the blocks in layout order. a vreg occupies one interval
/// covering every position where it is live; a value whose last use is instruction `i` may share
/// a register with one defined at `i`.
pub fn allocate(code: VCode<Reg>) -> Result<VCode<PReg>, CompileError> {
    let (live_in, live_out) = liveness(&code.blocks);

    let mut vregs: BTreeMap<VReg, Interval> = BTreeMap::new();
    let mut open: BTreeMap<PReg, Interval> = BTreeMap::new();
    let mut busy: BTreeMap<PReg, Vec<Interval>> = BTreeMap::new();

    let mut i = 0;
    for (b, block) in code.blocks.iter().enumerate() {
        for &v in &live_in[b] {
            widen(&mut vregs, v, i);
        }
        for &v in &live_out[b] {
            widen(&mut vregs, v, i + block.insts.len() - 1);
        }
        for inst in &block.insts {
            if matches!(inst, Inst::Call { .. }) {
                for p in PReg::ALLOCATABLE {
                    busy.entry(p).or_default().push((i, i + 1));
                }
            }
            for reg in inst.uses() {
                match reg {
                    Reg::V(v) => widen(&mut vregs, v, i),
                    Reg::P(p) => open.entry(p).or_insert((0, i)).1 = i,
                }
            }
            for reg in inst.defs() {
                match reg {
                    Reg::V(v) => widen(&mut vregs, v, i),
                    Reg::P(p) => {
                        if let Some(prev) = open.insert(p, (i, i)) {
                            busy.entry(p).or_default().push(prev);
                        }
                    }
                }
            }
            i += 1;
        }
    }
    for (p, interval) in open {
        busy.entry(p).or_default().push(interval);
    }

    let has_call = code
        .blocks
        .iter()
        .any(|b| b.insts.iter().any(|i| matches!(i, Inst::Call { .. })));
    let candidates: Vec<PReg> = PReg::ALLOCATABLE
        .into_iter()
        .chain(PReg::CALLEE_SAVED.into_iter().filter(|_| has_call))
        .collect();

    let mut order: Vec<_> = vregs.into_iter().collect();
    order.sort_by_key(|&(v, (start, _))| (start, v));

    let mut assigned: Vec<(Interval, PReg)> = Vec::new();
    let mut assignment: BTreeMap<VReg, PReg> = BTreeMap::new();
    for (v, interval) in order {
        let free = candidates.iter().copied().find(|p| {
            let fixed = busy.get(p).into_iter().flatten();
            let taken = assigned.iter().filter(|(_, q)| q == p).map(|(iv, _)| iv);
            !fixed.chain(taken).any(|&other| overlaps(interval, other))
        });
        let preg = free.ok_or(CompileError::OutOfRegisters)?;
        assigned.push((interval, preg));
        assignment.insert(v, preg);
    }

    let to_preg = |reg: Reg| match reg {
        Reg::V(v) => assignment[&v],
        Reg::P(p) => p,
    };
    let mut blocks: Vec<VBlock<PReg>> = code
        .blocks
        .iter()
        .map(|block| VBlock {
            insts: block
                .insts
                .iter()
                .map(|inst| inst.map_regs(to_preg))
                .filter(|inst| !matches!(inst, Inst::Mov { dst, src } if dst == src))
                .collect(),
        })
        .collect();

    if has_call || code.stack_size > 0 {
        add_frame(&mut blocks, &assignment, code.stack_size);
    }

    Ok(VCode {
        name: code.name,
        blocks,
        stack_size: code.stack_size,
    })
}

/// saves the callee-saved registers the function uses and reserves the stack slots. the return
/// address leaves `rsp` 8 off a 16-byte boundary, so the frame is padded to keep calls aligned.
fn add_frame(blocks: &mut [VBlock<PReg>], assignment: &BTreeMap<VReg, PReg>, stack_size: u32) {
    let saved: Vec<PReg> = PReg::CALLEE_SAVED
        .into_iter()
        .filter(|p| assignment.values().any(|q| q == p))
        .collect();
    let mut padding = stack_size as i32;
    if (saved.len() as i32 * 8 + padding) % 16 == 0 {
        padding += 8;
    }

    let mut prologue: Vec<Inst<PReg>> = saved.iter().map(|&reg| Inst::Push { reg }).collect();
    let mut epilogue: Vec<Inst<PReg>> = saved.iter().rev().map(|&reg| Inst::Pop { reg }).collect();
    if padding != 0 {
        prologue.push(Inst::AdjustSp { bytes: padding });
        epilogue.insert(0, Inst::AdjustSp { bytes: -padding });
    }

    for block in blocks.iter_mut() {
        let mut insts = Vec::new();
        for inst in block.insts.drain(..) {
            if matches!(inst, Inst::Ret { .. }) {
                insts.extend(epilogue.iter().cloned());
            }
            insts.push(inst);
        }
        block.insts = insts;
    }
    blocks[0].insts.splice(0..0, prologue);
}
