use std::fmt;

use miette::Diagnostic;
use thiserror::Error;

use super::{Block, Function, Inst, Terminator, Type, Value, ValueDef};

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
    }

    fn check_structure(&mut self) {
        let func = self.func;
        for block in func.blocks() {
            let data = func.block(block);
            for &inst in data.insts() {
                for &arg in func.inst(inst).args() {
                    self.check_arg(Use::Inst(inst), arg, Type::I64);
                }
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
                Terminator::Jump { dest, args } => {
                    if !func.contains_block(*dest) {
                        self.errors.push(VerifyError::UnknownBlock {
                            from: block,
                            dest: *dest,
                        });
                        continue;
                    }
                    if *dest == func.entry_block() {
                        self.errors.push(VerifyError::JumpToEntry { from: block });
                    }
                    let params = func.block(*dest).params();
                    if args.len() != params.len() {
                        self.errors.push(VerifyError::JumpArity {
                            from: block,
                            dest: *dest,
                        });
                    }
                    for (&arg, &param) in args.iter().zip(params) {
                        self.check_arg(Use::Terminator(block), arg, func.value_type(param));
                    }
                }
            }
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
        let doms = Dominators::compute(func);

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
                .map(|(i, &inst)| (Use::Inst(inst), i + 1, func.inst(inst).args()))
                .chain(
                    data.terminator()
                        .map(|t| (Use::Terminator(block), data.insts().len() + 1, t.args())),
                );

            for (user, pos, args) in uses {
                for &value in args.iter().filter(|&&v| func.contains_value(v)) {
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

struct Dominators {
    idom: Vec<Option<usize>>,
}

impl Dominators {
    // cooper-harvey-kennedy iterative dominators over blocks reachable from entry
    fn compute(func: &Function) -> Self {
        let n = func.blocks().count();
        let successors = |b: usize| match func.block(Block::from_index(b)).terminator() {
            Some(Terminator::Jump { dest, .. }) if func.contains_block(*dest) => Some(dest.index()),
            _ => None,
        };

        let mut postorder = Vec::new();
        let mut visited = vec![false; n];
        let mut stack = vec![(0, false)];
        while let Some((b, done)) = stack.pop() {
            if done {
                postorder.push(b);
                continue;
            }
            if std::mem::replace(&mut visited[b], true) {
                continue;
            }
            stack.push((b, true));
            stack.extend(successors(b).filter(|&s| !visited[s]).map(|s| (s, false)));
        }
        let rpo: Vec<usize> = postorder.into_iter().rev().collect();
        let mut rpo_num = vec![usize::MAX; n];
        for (i, &b) in rpo.iter().enumerate() {
            rpo_num[b] = i;
        }

        let mut preds = vec![Vec::new(); n];
        for &b in &rpo {
            if let Some(s) = successors(b) {
                preds[s].push(b);
            }
        }

        let mut idom: Vec<Option<usize>> = vec![None; n];
        idom[0] = Some(0);
        let mut changed = true;
        while changed {
            changed = false;
            for &b in rpo.iter().skip(1) {
                let mut candidates = preds[b].iter().copied().filter(|&p| idom[p].is_some());
                let Some(first) = candidates.next() else {
                    continue;
                };
                let new_idom = candidates.fold(first, |a, p| {
                    let (mut a, mut p) = (a, p);
                    while a != p {
                        while rpo_num[a] > rpo_num[p] {
                            a = idom[a].unwrap();
                        }
                        while rpo_num[p] > rpo_num[a] {
                            p = idom[p].unwrap();
                        }
                    }
                    a
                });
                if idom[b] != Some(new_idom) {
                    idom[b] = Some(new_idom);
                    changed = true;
                }
            }
        }
        Self { idom }
    }

    fn is_reachable(&self, block: Block) -> bool {
        self.idom[block.index()].is_some()
    }

    fn dominates(&self, a: Block, b: Block) -> bool {
        let (a, mut b) = (a.index(), b.index());
        loop {
            if a == b {
                return true;
            }
            match self.idom[b] {
                Some(parent) if parent != b => b = parent,
                _ => return false,
            }
        }
    }
}
