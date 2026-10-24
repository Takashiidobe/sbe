use super::{Block, Type, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstData {
    Iconst { value: i64 },
    Iadd { args: [Value; 2] },
}

impl InstData {
    pub fn result_type(&self) -> Type {
        match self {
            InstData::Iconst { .. } | InstData::Iadd { .. } => Type::I64,
        }
    }

    pub fn args(&self) -> &[Value] {
        match self {
            InstData::Iconst { .. } => &[],
            InstData::Iadd { args } => args,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Return {
        values: Vec<Value>,
    },
    Jump {
        dest: Block,
        args: Vec<Value>,
    },
    /// takes `then_dest` when `cond` is nonzero, `else_dest` otherwise
    Brif {
        cond: Value,
        then_dest: Block,
        then_args: Vec<Value>,
        else_dest: Block,
        else_args: Vec<Value>,
    },
}

impl Terminator {
    pub fn args(&self) -> Vec<Value> {
        match self {
            Terminator::Return { values } => values.clone(),
            Terminator::Jump { args, .. } => args.clone(),
            Terminator::Brif {
                cond,
                then_args,
                else_args,
                ..
            } => std::iter::once(*cond)
                .chain(then_args.iter().chain(else_args).copied())
                .collect(),
        }
    }

    /// every outgoing edge with the values it passes to the destination's params
    pub fn targets(&self) -> Vec<(Block, &[Value])> {
        match self {
            Terminator::Return { .. } => vec![],
            Terminator::Jump { dest, args } => vec![(*dest, args)],
            Terminator::Brif {
                then_dest,
                then_args,
                else_dest,
                else_args,
                ..
            } => vec![(*then_dest, then_args), (*else_dest, else_args)],
        }
    }
}
