use std::fmt;

use miette::Diagnostic;
use thiserror::Error;

use super::{
    Block, ControlFlow, FuncRef, Function, Inst, InstData, Terminator, Type, Value, ValueDef,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Inst(Inst),
    Terminator(Block),
}

impl fmt::Display for Use {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Use::Inst(inst) => write!(f, "{inst}"),
            Use::Terminator(block) => write!(f, "the terminator of {block}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error, Diagnostic)]
pub enum VerifyError {
    #[error("{0} has no terminator")]
    #[diagnostic(code(sir::verify::missing_terminator))]
    MissingTerminator(Block),

    #[error("entry block params do not match the function signature")]
    #[diagnostic(code(sir::verify::entry_params))]
    EntryParams,

    #[error("function params cannot be i8")]
    #[diagnostic(code(sir::verify::i8_param))]
    I8Param,

    #[error("{user} uses {value}, which is not defined in this function")]
    #[diagnostic(code(sir::verify::unknown_value))]
    UnknownValue { user: Use, value: Value },

    #[error("{user} uses {value}, whose definition does not dominate the use")]
    #[diagnostic(code(sir::verify::not_dominated))]
    NotDominated { user: Use, value: Value },

    #[error("{user} uses {value} of type {found}, expected {expected}")]
    #[diagnostic(code(sir::verify::type_mismatch))]
    TypeMismatch {
        user: Use,
        value: Value,
        expected: Type,
        found: Type,
    },

    #[error("{user} converts {value} of type {from} to {to}, which is the wrong direction")]
    #[diagnostic(code(sir::verify::bad_conversion))]
    BadConversion {
        user: Use,
        value: Value,
        from: Type,
        to: Type,
    },

    #[error("{user} calls {callee}, which is not declared in this function")]
    #[diagnostic(code(sir::verify::unknown_callee))]
    UnknownCallee { user: Use, callee: FuncRef },

    #[error("{user} calls {callee} with the wrong number of args")]
    #[diagnostic(code(sir::verify::call_arity))]
    CallArity { user: Use, callee: FuncRef },

    #[error("{user} calls {callee}, which returns more than one value")]
    #[diagnostic(
        code(sir::verify::call_returns),
        help("calls to functions with several return values are not supported")
    )]
    CallReturns { user: Use, callee: FuncRef },

    #[error("{0} returns the wrong number of values for the signature")]
    #[diagnostic(code(sir::verify::return_arity))]
    ReturnArity(Block),

    #[error("{from} jumps to {dest} with the wrong number of args")]
    #[diagnostic(code(sir::verify::jump_arity))]
    JumpArity { from: Block, dest: Block },

    #[error("{from} jumps to {dest}, which does not exist")]
    #[diagnostic(code(sir::verify::unknown_block))]
    UnknownBlock { from: Block, dest: Block },

    #[error("{from} jumps to the entry block")]
    #[diagnostic(code(sir::verify::jump_to_entry))]
    JumpToEntry { from: Block },
}

#[derive(Clone, Debug, PartialEq, Eq, Error, Diagnostic)]
#[error("function %{name} failed verification")]
pub struct VerifyErrors {
    name: String,
    #[related]
    pub errors: Vec<VerifyError>,
}

pub fn verify(func: &Function) -> Result<(), VerifyErrors> {
    let mut verifier = Verifier {
        func,
        errors: Vec::new(),
    };
    verifier.check_entry_params();
    verifier.check_structure();
    verifier.check_dominance();

    if verifier.errors.is_empty() {
        Ok(())
    } else {
        Err(VerifyErrors {
            name: func.name().to_owned(),
            errors: verifier.errors,
        })
    }
}

struct Verifier<'a> {
    func: &'a Function,
    errors: Vec<VerifyError>,
}

impl Verifier<'_> {
    fn check_entry_params(&mut self) {
        let func = self.func;
        let params = func.block(func.entry_block()).params();
        let matches = params.len() == func.signature().params.len()
            && params
                .iter()
                .zip(&func.signature().params)
                .all(|(&v, &ty)| func.value_type(v) == ty);
        if !matches {
            self.errors.push(VerifyError::EntryParams);
        }
        if func.signature().params.contains(&Type::I8) {
            self.errors.push(VerifyError::I8Param);
        }
    }

    fn check_structure(&mut self) {
        let func = self.func;
        for block in func.blocks() {
            let data = func.block(block);
            for &inst in data.insts() {
                self.check_inst(Use::Inst(inst), func.inst(inst));
            }

            let Some(terminator) = data.terminator() else {
                self.errors.push(VerifyError::MissingTerminator(block));
                continue;
            };
            match terminator {
                Terminator::Return { values } => {
                    let returns = &func.signature().returns;
                    if values.len() != returns.len() {
                        self.errors.push(VerifyError::ReturnArity(block));
                    }
                    for (&value, &ty) in values.iter().zip(returns) {
                        self.check_arg(Use::Terminator(block), value, ty);
                    }
                }
                Terminator::Jump { .. } | Terminator::Brif { .. } => {
                    if let Terminator::Brif { cond, .. } = terminator {
                        self.check_arg(Use::Terminator(block), *cond, Type::I8);
                    }
                    for (dest, args) in terminator.targets() {
                        self.check_edge(block, dest, args);
                    }
                }
            }
        }
    }

    fn check_inst(&mut self, user: Use, data: &InstData) {
        match data {
            InstData::Iconst { .. } => {}
            InstData::Unary { arg, .. } => self.check_same_type(user, *arg, *arg),
            InstData::Convert { op, ty, arg } => {
                if self.func.contains_value(*arg) {
                    let from = self.func.value_type(*arg);
                    if !op.accepts(from, *ty) {
                        self.errors.push(VerifyError::BadConversion {
                            user,
                            value: *arg,
                            from,
                            to: *ty,
                        });
                    }
                } else {
                    self.errors
                        .push(VerifyError::UnknownValue { user, value: *arg });
                }
            }
            InstData::Binary { args: [a, b], .. } | InstData::Icmp { args: [a, b], .. } => {
                self.check_same_type(user, *a, *b);
            }
            InstData::Select {
                args: [cond, then_value, else_value],
            } => {
                self.check_arg(user, *cond, Type::I8);
                self.check_same_type(user, *then_value, *else_value);
            }
            InstData::Call { callee, args } => self.check_call(user, *callee, args),
        }
    }

    fn check_call(&mut self, user: Use, callee: FuncRef, args: &[Value]) {
        if !self.func.contains_callee(callee) {
            self.errors
                .push(VerifyError::UnknownCallee { user, callee });
            return;
        }
        let signature = &self.func.callee(callee).signature;
        if signature.returns.len() > 1 {
            self.errors.push(VerifyError::CallReturns { user, callee });
        }
        if args.len() != signature.params.len() {
            self.errors.push(VerifyError::CallArity { user, callee });
        }
        for (&arg, &ty) in args.iter().zip(&signature.params) {
            self.check_arg(user, arg, ty);
        }
    }

    fn check_same_type(&mut self, user: Use, first: Value, second: Value) {
        if self.func.contains_value(first) {
            let ty = self.func.value_type(first);
            self.check_arg(user, second, ty);
        } else {
            self.errors
                .push(VerifyError::UnknownValue { user, value: first });
        }
    }

    fn check_edge(&mut self, from: Block, dest: Block, args: &[Value]) {
        let func = self.func;
        if !func.contains_block(dest) {
            self.errors.push(VerifyError::UnknownBlock { from, dest });
            return;
        }
        if dest == func.entry_block() {
            self.errors.push(VerifyError::JumpToEntry { from });
        }
        let params = func.block(dest).params();
        if args.len() != params.len() {
            self.errors.push(VerifyError::JumpArity { from, dest });
        }
        for (&arg, &param) in args.iter().zip(params) {
            self.check_arg(Use::Terminator(from), arg, func.value_type(param));
        }
    }

    fn check_arg(&mut self, user: Use, value: Value, expected: Type) {
        if !self.func.contains_value(value) {
            self.errors.push(VerifyError::UnknownValue { user, value });
            return;
        }
        let found = self.func.value_type(value);
        if found != expected {
            self.errors.push(VerifyError::TypeMismatch {
                user,
                value,
                expected,
                found,
            });
        }
    }

    fn check_dominance(&mut self) {
        let func = self.func;
        let doms = ControlFlow::compute(func);

        let mut inst_pos = vec![None; func.num_insts()];
        for block in func.blocks() {
            for (i, &inst) in func.block(block).insts().iter().enumerate() {
                inst_pos[inst.index()] = Some((block, i + 1));
            }
        }
        let def_pos = |value: Value| match func.value_def(value) {
            ValueDef::Param(block, _) => Some((block, 0)),
            ValueDef::Result(inst) => inst_pos[inst.index()],
        };

        for block in func.blocks().filter(|&b| doms.is_reachable(b)) {
            let data = func.block(block);
            let uses = data
                .insts()
                .iter()
                .enumerate()
                .map(|(i, &inst)| (Use::Inst(inst), i + 1, func.inst(inst).args().to_vec()))
                .chain(
                    data.terminator()
                        .map(|t| (Use::Terminator(block), data.insts().len() + 1, t.args())),
                );

            for (user, pos, args) in uses {
                for value in args.into_iter().filter(|&v| func.contains_value(v)) {
                    let Some((def_block, def_idx)) = def_pos(value) else {
                        self.errors.push(VerifyError::NotDominated { user, value });
                        continue;
                    };
                    let dominated = if def_block == block {
                        def_idx < pos
                    } else {
                        doms.dominates(def_block, block)
                    };
                    if !dominated {
                        self.errors.push(VerifyError::NotDominated { user, value });
                    }
                }
            }
        }
    }
}
