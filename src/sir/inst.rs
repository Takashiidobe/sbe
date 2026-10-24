use super::{Block, Type, Value};

/// shift amounts are taken modulo 64. `sdiv`, `udiv`, `srem` and `urem` trap on a zero divisor,
/// and the signed forms also trap on `i64::MIN` by `-1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Iadd,
    Isub,
    Imul,
    Band,
    Bor,
    Bxor,
    Ishl,
    Ushr,
    Sshr,
    Sdiv,
    Udiv,
    Srem,
    Urem,
}

impl BinOp {
    const ALL: [BinOp; 13] = [
        BinOp::Iadd,
        BinOp::Isub,
        BinOp::Imul,
        BinOp::Band,
        BinOp::Bor,
        BinOp::Bxor,
        BinOp::Ishl,
        BinOp::Ushr,
        BinOp::Sshr,
        BinOp::Sdiv,
        BinOp::Udiv,
        BinOp::Srem,
        BinOp::Urem,
    ];

    pub fn mnemonic(self) -> &'static str {
        match self {
            BinOp::Iadd => "iadd",
            BinOp::Isub => "isub",
            BinOp::Imul => "imul",
            BinOp::Band => "band",
            BinOp::Bor => "bor",
            BinOp::Bxor => "bxor",
            BinOp::Ishl => "ishl",
            BinOp::Ushr => "ushr",
            BinOp::Sshr => "sshr",
            BinOp::Sdiv => "sdiv",
            BinOp::Udiv => "udiv",
            BinOp::Srem => "srem",
            BinOp::Urem => "urem",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.mnemonic() == text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Ineg,
    Bnot,
}

impl UnOp {
    const ALL: [UnOp; 2] = [UnOp::Ineg, UnOp::Bnot];

    pub fn mnemonic(self) -> &'static str {
        match self {
            UnOp::Ineg => "ineg",
            UnOp::Bnot => "bnot",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.mnemonic() == text)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstData {
    Iconst { value: i64 },
    Unary { op: UnOp, arg: Value },
    Binary { op: BinOp, args: [Value; 2] },
}

impl InstData {
    pub fn result_type(&self) -> Type {
        match self {
            InstData::Iconst { .. } | InstData::Unary { .. } | InstData::Binary { .. } => Type::I64,
        }
    }

    pub fn args(&self) -> &[Value] {
        match self {
            InstData::Iconst { .. } => &[],
            InstData::Unary { arg, .. } => std::slice::from_ref(arg),
            InstData::Binary { args, .. } => args,
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
