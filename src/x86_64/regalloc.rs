use std::collections::BTreeMap;

use super::error::CompileError;
use super::vcode::{Inst, PReg, Reg, VCode, VReg};

type Interval = (usize, usize);

fn overlaps(a: Interval, b: Interval) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// straight-line linear scan without spilling. value `v` occupies `[first def, last use]`; a value
/// whose last use is instruction `i` may share a register with one defined at `i`.
pub fn allocate(code: VCode<Reg>) -> Result<VCode<PReg>, CompileError> {
    let mut vregs: BTreeMap<VReg, Interval> = BTreeMap::new();
    let mut open: BTreeMap<PReg, Interval> = BTreeMap::new();
    let mut busy: BTreeMap<PReg, Vec<Interval>> = BTreeMap::new();

    for (i, inst) in code.insts.iter().enumerate() {
        for reg in inst.uses() {
            match reg {
                Reg::V(v) => vregs.entry(v).or_insert((i, i)).1 = i,
                Reg::P(p) => open.entry(p).or_insert((0, i)).1 = i,
            }
        }
        for reg in inst.defs() {
            match reg {
                Reg::V(v) => vregs.entry(v).or_insert((i, i)).1 = i,
                Reg::P(p) => {
                    if let Some(prev) = open.insert(p, (i, i)) {
                        busy.entry(p).or_default().push(prev);
                    }
                }
            }
        }
    }
    for (p, interval) in open {
        busy.entry(p).or_default().push(interval);
    }

    let mut order: Vec<_> = vregs.into_iter().collect();
    order.sort_by_key(|&(v, (start, _))| (start, v));

    let mut assigned: Vec<(Interval, PReg)> = Vec::new();
    let mut assignment: BTreeMap<VReg, PReg> = BTreeMap::new();
    for (v, interval) in order {
        let free = PReg::ALLOCATABLE.into_iter().find(|p| {
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
    let insts = code
        .insts
        .iter()
        .map(|inst| inst.map_regs(to_preg))
        .filter(|inst| !matches!(inst, Inst::Mov { dst, src } if dst == src))
        .collect();

    Ok(VCode {
        name: code.name,
        insts,
    })
}
