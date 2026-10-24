use super::{Block, FuncRef, Type, Value};

/// binary ops take two operands of one integer type and yield that type. shift amounts are taken
/// modulo the type's width. `sdiv`, `udiv`, `srem` and `urem` trap on a zero divisor. the signed
/// forms trap on `i64::MIN` by `-1`; for narrower types that overflow wraps instead.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CvtOp {
    Sextend,
    Uextend,
    Ireduce,
}

impl CvtOp {
    const ALL: [CvtOp; 3] = [CvtOp::Sextend, CvtOp::Uextend, CvtOp::Ireduce];

    pub fn mnemonic(self) -> &'static str {
        match self {
            CvtOp::Sextend => "sextend",
            CvtOp::Uextend => "uextend",
            CvtOp::Ireduce => "ireduce",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.mnemonic() == text)
    }

    /// extends must widen and reduces must narrow
    pub fn accepts(self, from: Type, to: Type) -> bool {
        match self {
            CvtOp::Sextend | CvtOp::Uextend => to.bits() > from.bits(),
            CvtOp::Ireduce => to.bits() < from.bits(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntCC {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
    Ult,
    Ule,
    Ugt,
    Uge,
}

impl IntCC {
    const ALL: [IntCC; 10] = [
        IntCC::Eq,
        IntCC::Ne,
        IntCC::Slt,
        IntCC::Sle,
        IntCC::Sgt,
        IntCC::Sge,
        IntCC::Ult,
        IntCC::Ule,
        IntCC::Ugt,
        IntCC::Uge,
    ];

    pub fn mnemonic(self) -> &'static str {
        match self {
            IntCC::Eq => "eq",
            IntCC::Ne => "ne",
            IntCC::Slt => "slt",
            IntCC::Sle => "sle",
            IntCC::Sgt => "sgt",
            IntCC::Sge => "sge",
            IntCC::Ult => "ult",
            IntCC::Ule => "ule",
            IntCC::Ugt => "ugt",
            IntCC::Uge => "uge",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|cc| cc.mnemonic() == text)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstData {
    /// `value` is truncated to `ty`
    Iconst {
        ty: Type,
        value: i64,
    },
    Unary {
        op: UnOp,
        arg: Value,
    },
    Binary {
        op: BinOp,
        args: [Value; 2],
    },
    /// `ty` is the result type; `op` must widen or narrow the argument accordingly
    Convert {
        op: CvtOp,
        ty: Type,
        arg: Value,
    },
    /// compares two ints of one type and yields an i8 that is exactly 0 or 1
    Icmp {
        cc: IntCC,
        args: [Value; 2],
    },
    /// `args` is `[cond, then_value, else_value]`; `cond` is an i8 and the arms share a type
    Select {
        args: [Value; 3],
    },
    /// calls a function declared on this function; it yields a value only when the callee returns
    /// one, and callees return at most one value
    Call {
        callee: FuncRef,
        args: Vec<Value>,
    },
}

impl InstData {
    /// `None` for a call to a callee that returns nothing
    pub fn result_type(
        &self,
        type_of: impl Fn(Value) -> Type,
        callee_return: impl Fn(FuncRef) -> Option<Type>,
    ) -> Option<Type> {
        Some(match self {
            InstData::Iconst { ty, .. } | InstData::Convert { ty, .. } => *ty,
            InstData::Unary { arg, .. } => type_of(*arg),
            InstData::Binary { args: [a, _], .. } => type_of(*a),
            InstData::Icmp { .. } => Type::I8,
            InstData::Select {
                args: [_, then_value, _],
            } => type_of(*then_value),
            InstData::Call { callee, .. } => return callee_return(*callee),
        })
    }

    pub fn args(&self) -> &[Value] {
        match self {
            InstData::Iconst { .. } => &[],
            InstData::Unary { arg, .. } | InstData::Convert { arg, .. } => {
                std::slice::from_ref(arg)
            }
            InstData::Binary { args, .. } | InstData::Icmp { args, .. } => args,
            InstData::Select { args } => args,
            InstData::Call { args, .. } => args,
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
