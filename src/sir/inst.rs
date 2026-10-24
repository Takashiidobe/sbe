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
    Return { values: Vec<Value> },
    Jump { dest: Block, args: Vec<Value> },
}

impl Terminator {
    pub fn args(&self) -> &[Value] {
        match self {
            Terminator::Return { values } => values,
            Terminator::Jump { args, .. } => args,
        }
    }
}
