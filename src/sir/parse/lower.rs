use std::collections::HashMap;

use super::ast::{FunctionAst, Name, Op, TargetAst, TermAst};
use super::error::{Failure, PResult, ParseErrorKind};
use crate::sir::{
    Block, FuncRef, Function, InstData, Signature, StackSlot, SwitchCase, Terminator, Value,
};

struct Names<'a> {
    values: HashMap<&'a str, Value>,
    blocks: HashMap<&'a str, Block>,
    callees: HashMap<&'a str, FuncRef>,
    stack_slots: HashMap<&'a str, StackSlot>,
}

impl<'a> Names<'a> {
    fn value(&self, name: Name<'a>) -> PResult<Value> {
        self.values.get(name.text).copied().ok_or(Failure {
            kind: ParseErrorKind::UndefinedValue(name.text.to_owned()),
            span: name.span,
        })
    }

    fn callee(&self, name: Name<'a>) -> PResult<FuncRef> {
        self.callees.get(name.text).copied().ok_or(Failure {
            kind: ParseErrorKind::UndefinedCallee(name.text.to_owned()),
            span: name.span,
        })
    }

    fn stack_slot(&self, name: Name<'a>) -> PResult<StackSlot> {
        self.stack_slots.get(name.text).copied().ok_or(Failure {
            kind: ParseErrorKind::UndefinedStackSlot(name.text.to_owned()),
            span: name.span,
        })
    }

    fn values(&self, names: &[Name<'a>]) -> PResult<Vec<Value>> {
        names.iter().map(|&n| self.value(n)).collect()
    }

    fn block(&self, name: Name<'a>) -> PResult<Block> {
        self.blocks.get(name.text).copied().ok_or(Failure {
            kind: ParseErrorKind::UndefinedBlock(name.text.to_owned()),
            span: name.span,
        })
    }
}

impl<'a> Names<'a> {
    fn target(&self, target: &TargetAst<'a>) -> PResult<(Block, Vec<Value>)> {
        Ok((self.block(target.dest)?, self.values(&target.args)?))
    }
}

pub(super) fn lower(ast: FunctionAst<'_>) -> PResult<Function> {
    let Some(entry_ast) = ast.blocks.first() else {
        return Err(Failure {
            kind: ParseErrorKind::NoBlocks,
            span: ast.end_span,
        });
    };

    let mut func = Function::new(
        ast.name,
        Signature {
            params: ast.params.clone(),
            returns: ast.returns.clone(),
        },
    );
    let entry_params = func.block(func.entry_block()).params().to_vec();
    let entry_matches = entry_ast.params.len() == ast.params.len()
        && entry_ast
            .params
            .iter()
            .zip(&ast.params)
            .all(|(p, &ty)| p.ty == ty);
    if !entry_matches {
        return Err(Failure {
            kind: ParseErrorKind::EntryParams,
            span: entry_ast.name.span,
        });
    }

    // names are resolved up front so values may be used in blocks listed before their definition;
    // the numbering below mirrors the creation order of the second pass
    let mut names = Names {
        values: HashMap::new(),
        blocks: HashMap::new(),
        callees: HashMap::new(),
        stack_slots: HashMap::new(),
    };
    for slot in &ast.stack_slots {
        let id = func.create_stack_slot(slot.size);
        if names.stack_slots.insert(slot.alias.text, id).is_some() {
            return Err(Failure {
                kind: ParseErrorKind::DuplicateStackSlot(slot.alias.text.to_owned()),
                span: slot.alias.span,
            });
        }
    }
    for callee in &ast.callees {
        let id = func.declare_callee(
            callee.name,
            Signature {
                params: callee.params.clone(),
                returns: callee.returns.clone(),
            },
        );
        if names.callees.insert(callee.alias.text, id).is_some() {
            return Err(Failure {
                kind: ParseErrorKind::DuplicateCallee(callee.alias.text.to_owned()),
                span: callee.alias.span,
            });
        }
    }
    let mut next_value = func.num_values();

    for (i, block) in ast.blocks.iter().enumerate() {
        let id = if i == 0 {
            func.entry_block()
        } else {
            func.create_block()
        };
        if names.blocks.insert(block.name.text, id).is_some() {
            return Err(Failure {
                kind: ParseErrorKind::DuplicateBlock(block.name.text.to_owned()),
                span: block.name.span,
            });
        }

        for (j, param) in block.params.iter().enumerate() {
            let value = if i == 0 {
                entry_params[j]
            } else {
                next_value += 1;
                Value::from_index(next_value - 1)
            };
            insert_value(&mut names, param.name, value)?;
        }
        for inst in &block.insts {
            if let Some(result) = inst.result {
                next_value += 1;
                insert_value(&mut names, result, Value::from_index(next_value - 1))?;
            }
        }
    }

    for (i, block) in ast.blocks.iter().enumerate() {
        let id = Block::from_index(i);
        if i > 0 {
            for param in &block.params {
                let value = func.append_block_param(id, param.ty);
                debug_assert_eq!(names.values[param.name.text], value);
            }
        }
        for inst in &block.insts {
            let data = match &inst.op {
                Op::Iconst(ty, value) => InstData::Iconst {
                    ty: *ty,
                    value: *value,
                },
                Op::Fconst(ty, bits) => InstData::Fconst {
                    ty: *ty,
                    bits: *bits,
                },
                Op::Fbinary(op, a, b) => InstData::Fbinary {
                    op: *op,
                    args: [names.value(*a)?, names.value(*b)?],
                },
                Op::Fcmp(cc, a, b) => InstData::Fcmp {
                    cc: *cc,
                    args: [names.value(*a)?, names.value(*b)?],
                },
                Op::Convert(op, ty, arg) => InstData::Convert {
                    op: *op,
                    ty: *ty,
                    arg: names.value(*arg)?,
                },
                Op::Icmp(cc, a, b) => InstData::Icmp {
                    cc: *cc,
                    args: [names.value(*a)?, names.value(*b)?],
                },
                Op::Select(cond, then_value, else_value) => InstData::Select {
                    args: [
                        names.value(*cond)?,
                        names.value(*then_value)?,
                        names.value(*else_value)?,
                    ],
                },
                Op::Unary(op, arg) => InstData::Unary {
                    op: *op,
                    arg: names.value(*arg)?,
                },
                Op::Binary(op, a, b) => InstData::Binary {
                    op: *op,
                    args: [names.value(*a)?, names.value(*b)?],
                },
                Op::Load(ty, addr) => InstData::Load {
                    ty: *ty,
                    addr: names.value(*addr)?,
                },
                Op::StackAddr(slot) => InstData::StackAddr {
                    slot: names.stack_slot(*slot)?,
                },
                Op::Store(value, addr) => {
                    func.push_store(id, names.value(*value)?, names.value(*addr)?);
                    continue;
                }
                Op::Call(callee_name, args) => {
                    let callee = names.callee(*callee_name)?;
                    let returns = !func.callee(callee).signature.returns.is_empty();
                    let problem = match (returns, inst.result) {
                        (true, None) => Some("returns a value that must be named"),
                        (false, Some(_)) => Some("returns nothing"),
                        _ => None,
                    };
                    if let Some(problem) = problem {
                        return Err(Failure {
                            kind: ParseErrorKind::CallResult(callee_name.text.to_owned(), problem),
                            span: callee_name.span,
                        });
                    }
                    let args = names.values(args)?;
                    func.push_call(id, callee, args);
                    continue;
                }
            };
            let value = func.push_inst(id, data);
            debug_assert_eq!(names.values[inst.result.expect("not void").text], value);
        }
        let terminator = match &block.terminator {
            Some(TermAst::Return(values)) => Terminator::Return {
                values: names.values(values)?,
            },
            Some(TermAst::Jump(target)) => {
                let (dest, args) = names.target(target)?;
                Terminator::Jump { dest, args }
            }
            Some(TermAst::Brif(cond, then_target, else_target)) => {
                let (then_dest, then_args) = names.target(then_target)?;
                let (else_dest, else_args) = names.target(else_target)?;
                Terminator::Brif {
                    cond: names.value(*cond)?,
                    then_dest,
                    then_args,
                    else_dest,
                    else_args,
                }
            }
            Some(TermAst::Switch {
                value,
                cases,
                default,
            }) => {
                let cases = cases
                    .iter()
                    .map(|(key, target)| {
                        let (dest, args) = names.target(target)?;
                        Ok(SwitchCase {
                            key: *key,
                            dest,
                            args,
                        })
                    })
                    .collect::<PResult<Vec<_>>>()?;
                let (default, default_args) = names.target(default)?;
                Terminator::Switch {
                    value: names.value(*value)?,
                    cases,
                    default,
                    default_args,
                }
            }
            Some(TermAst::Trap) => Terminator::Trap,
            Some(TermAst::Unreachable) => Terminator::Unreachable,
            None => continue,
        };
        func.set_terminator(id, terminator);
    }

    Ok(func)
}

fn insert_value<'a>(names: &mut Names<'a>, name: Name<'a>, value: Value) -> PResult<()> {
    match names.values.insert(name.text, value) {
        None => Ok(()),
        Some(_) => Err(Failure {
            kind: ParseErrorKind::DuplicateValue(name.text.to_owned()),
            span: name.span,
        }),
    }
}
