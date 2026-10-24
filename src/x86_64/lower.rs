use super::error::CompileError;
use super::vcode::{Inst, PReg, Reg, VCode, VReg};
use crate::sir::{Function, InstData, Terminator, Value};

fn vreg(value: Value) -> Reg {
    Reg::V(VReg::from_index(value.index()))
}

/// expects a verified function
pub fn lower(func: &Function) -> Result<VCode<Reg>, CompileError> {
    if func.blocks().count() != 1 {
        return Err(CompileError::MultipleBlocks);
    }
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

    let block = func.block(func.entry_block());
    let mut insts = Vec::new();

    for (&param, &arg) in block.params().iter().zip(&PReg::ARGS) {
        insts.push(Inst::Mov {
            dst: vreg(param),
            src: Reg::P(arg),
        });
    }

    for &inst in block.insts() {
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

    match block.terminator() {
        Some(Terminator::Return { values }) => {
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
        Some(Terminator::Jump { .. }) | None => return Err(CompileError::MultipleBlocks),
    }

    Ok(VCode {
        name: func.name().to_owned(),
        insts,
    })
}
