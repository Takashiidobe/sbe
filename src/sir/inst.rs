use super::{Block, FuncRef, StackSlot, Type, Value};

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
    /// signed int to float
    FcvtFromSint,
    /// float to signed int; a NaN or out-of-range input gives an unspecified value, never a trap
    FcvtToSint,
    Fpromote,
    Fdemote,
    /// reinterprets the bits as a type of the same width
    Bitcast,
}

impl CvtOp {
    const ALL: [CvtOp; 8] = [
        CvtOp::Sextend,
        CvtOp::Uextend,
        CvtOp::Ireduce,
        CvtOp::FcvtFromSint,
        CvtOp::FcvtToSint,
        CvtOp::Fpromote,
        CvtOp::Fdemote,
        CvtOp::Bitcast,
    ];

    pub fn mnemonic(self) -> &'static str {
        match self {
            CvtOp::Sextend => "sextend",
            CvtOp::Uextend => "uextend",
            CvtOp::Ireduce => "ireduce",
            CvtOp::FcvtFromSint => "fcvt_from_sint",
            CvtOp::FcvtToSint => "fcvt_to_sint",
            CvtOp::Fpromote => "fpromote",
            CvtOp::Fdemote => "fdemote",
            CvtOp::Bitcast => "bitcast",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.mnemonic() == text)
    }

    /// extends must widen and reduces must narrow, among ints; float conversions must cross
    /// between the kinds they name
    pub fn accepts(self, from: Type, to: Type) -> bool {
        match self {
            CvtOp::Sextend | CvtOp::Uextend => {
                from.is_int() && to.is_int() && to.bits() > from.bits()
            }
            CvtOp::Ireduce => from.is_int() && to.is_int() && to.bits() < from.bits(),
            CvtOp::FcvtFromSint => from.is_int() && to.is_float(),
            CvtOp::FcvtToSint => from.is_float() && to.is_int(),
            CvtOp::Fpromote => from == Type::F32 && to == Type::F64,
            CvtOp::Fdemote => from == Type::F64 && to == Type::F32,
            CvtOp::Bitcast => from.bits() == to.bits() && from.is_float() != to.is_float(),
        }
    }
}

/// binary float ops take two operands of one float type and yield that type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatOp {
    Fadd,
    Fsub,
    Fmul,
    Fdiv,
}

impl FloatOp {
    const ALL: [FloatOp; 4] = [FloatOp::Fadd, FloatOp::Fsub, FloatOp::Fmul, FloatOp::Fdiv];

    pub fn mnemonic(self) -> &'static str {
        match self {
            FloatOp::Fadd => "fadd",
            FloatOp::Fsub => "fsub",
            FloatOp::Fmul => "fmul",
            FloatOp::Fdiv => "fdiv",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.mnemonic() == text)
    }
}

/// every condition but `Ne` is false when an operand is NaN; `Ne` is true
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatCC {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl FloatCC {
    const ALL: [FloatCC; 6] = [
        FloatCC::Eq,
        FloatCC::Ne,
        FloatCC::Lt,
        FloatCC::Le,
        FloatCC::Gt,
        FloatCC::Ge,
    ];

    pub fn mnemonic(self) -> &'static str {
        match self {
            FloatCC::Eq => "eq",
            FloatCC::Ne => "ne",
            FloatCC::Lt => "lt",
            FloatCC::Le => "le",
            FloatCC::Gt => "gt",
            FloatCC::Ge => "ge",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|cc| cc.mnemonic() == text)
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
    /// `bits` holds the IEEE pattern; an `f32` uses the low 32 bits
    Fconst {
        ty: Type,
        bits: u64,
    },
    Fbinary {
        op: FloatOp,
        args: [Value; 2],
    },
    /// compares two floats of one type and yields an i8 that is exactly 0 or 1
    Fcmp {
        cc: FloatCC,
        args: [Value; 2],
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
    /// reads a `ty` from the `i64` address `addr`; narrow results have undefined upper bits like
    /// any other narrow value
    Load {
        ty: Type,
        addr: Value,
    },
    /// writes `args[0]` to the `i64` address `args[1]`; yields nothing
    Store {
        args: [Value; 2],
    },
    /// the `i64` address of a stack slot, valid until the function returns
    StackAddr {
        slot: StackSlot,
    },
}

impl InstData {
    /// `None` for a store and for a call to a callee that returns nothing
    pub fn result_type(
        &self,
        type_of: impl Fn(Value) -> Type,
        callee_return: impl Fn(FuncRef) -> Option<Type>,
    ) -> Option<Type> {
        Some(match self {
            InstData::Iconst { ty, .. } | InstData::Convert { ty, .. } => *ty,
            InstData::Unary { arg, .. } => type_of(*arg),
            InstData::Binary { args: [a, _], .. } => type_of(*a),
            InstData::Icmp { .. } | InstData::Fcmp { .. } => Type::I8,
            InstData::Fbinary { args: [a, _], .. } => type_of(*a),
            InstData::Fconst { ty, .. } => *ty,
            InstData::Select {
                args: [_, then_value, _],
            } => type_of(*then_value),
            InstData::Call { callee, .. } => return callee_return(*callee),
            InstData::Load { ty, .. } => *ty,
            InstData::Store { .. } => return None,
            InstData::StackAddr { .. } => Type::I64,
        })
    }

    pub fn args(&self) -> &[Value] {
        match self {
            InstData::Iconst { .. } | InstData::Fconst { .. } | InstData::StackAddr { .. } => &[],
            InstData::Load { addr, .. } => std::slice::from_ref(addr),
            InstData::Store { args } => args,
            InstData::Unary { arg, .. } | InstData::Convert { arg, .. } => {
                std::slice::from_ref(arg)
            }
            InstData::Binary { args, .. }
            | InstData::Icmp { args, .. }
            | InstData::Fbinary { args, .. }
            | InstData::Fcmp { args, .. } => args,
            InstData::Select { args } => args,
            InstData::Call { args, .. } => args,
        }
    }
}

/// takes `dest` when the switch value equals `key` once both are truncated to the value's type
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchCase {
    pub key: i64,
    pub dest: Block,
    pub args: Vec<Value>,
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
    /// compares an integer `value` against each case in order and falls back to `default`
    Switch {
        value: Value,
        cases: Vec<SwitchCase>,
        default: Block,
        default_args: Vec<Value>,
    },
    /// traps when executed
    Trap,
    /// marks a point control never reaches; reaching it is undefined, and both backends trap
    Unreachable,
}

impl Terminator {
    pub fn args(&self) -> Vec<Value> {
        match self {
            Terminator::Return { values } => values.clone(),
            Terminator::Trap | Terminator::Unreachable => vec![],
            Terminator::Switch {
                value,
                cases,
                default_args,
                ..
            } => std::iter::once(*value)
                .chain(cases.iter().flat_map(|c| c.args.iter().copied()))
                .chain(default_args.iter().copied())
                .collect(),
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
            Terminator::Return { .. } | Terminator::Trap | Terminator::Unreachable => vec![],
            Terminator::Switch {
                cases,
                default,
                default_args,
                ..
            } => cases
                .iter()
                .map(|c| (c.dest, c.args.as_slice()))
                .chain(std::iter::once((*default, default_args.as_slice())))
                .collect(),
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
