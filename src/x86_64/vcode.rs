use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VReg(u32);

impl VReg {
    pub fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("vreg index overflows u32"))
    }
}

impl fmt::Display for VReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PReg {
    Rax,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
}

impl PReg {
    /// caller-saved registers only, so no prologue or epilogue is needed
    pub const ALLOCATABLE: [PReg; 9] = [
        PReg::Rax,
        PReg::Rcx,
        PReg::Rdx,
        PReg::Rsi,
        PReg::Rdi,
        PReg::R8,
        PReg::R9,
        PReg::R10,
        PReg::R11,
    ];

    /// system v integer argument registers
    pub const ARGS: [PReg; 6] = [
        PReg::Rdi,
        PReg::Rsi,
        PReg::Rdx,
        PReg::Rcx,
        PReg::R8,
        PReg::R9,
    ];

    pub const RET: PReg = PReg::Rax;

    pub fn name(self) -> &'static str {
        match self {
            PReg::Rax => "rax",
            PReg::Rcx => "rcx",
            PReg::Rdx => "rdx",
            PReg::Rsi => "rsi",
            PReg::Rdi => "rdi",
            PReg::R8 => "r8",
            PReg::R9 => "r9",
            PReg::R10 => "r10",
            PReg::R11 => "r11",
        }
    }
}

impl fmt::Display for PReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reg {
    V(VReg),
    P(PReg),
}

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reg::V(v) => v.fmt(f),
            Reg::P(p) => p.fmt(f),
        }
    }
}

/// index of a block in [`VCode::blocks`]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Label(pub usize);

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AluOp {
    Add,
    Sub,
    Imul,
    And,
    Or,
    Xor,
}

impl AluOp {
    pub fn mnemonic(self) -> &'static str {
        match self {
            AluOp::Add => "add",
            AluOp::Sub => "sub",
            AluOp::Imul => "imul",
            AluOp::And => "and",
            AluOp::Or => "or",
            AluOp::Xor => "xor",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShiftOp {
    Shl,
    Shr,
    Sar,
}

impl ShiftOp {
    pub fn mnemonic(self) -> &'static str {
        match self {
            ShiftOp::Shl => "shl",
            ShiftOp::Shr => "shr",
            ShiftOp::Sar => "sar",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

impl UnaryOp {
    pub fn mnemonic(self) -> &'static str {
        match self {
            UnaryOp::Neg => "neg",
            UnaryOp::Not => "not",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cc {
    E,
    Ne,
    L,
    Le,
    G,
    Ge,
    B,
    Be,
    A,
    Ae,
}

impl Cc {
    pub fn suffix(self) -> &'static str {
        match self {
            Cc::E => "e",
            Cc::Ne => "ne",
            Cc::L => "l",
            Cc::Le => "le",
            Cc::G => "g",
            Cc::Ge => "ge",
            Cc::B => "b",
            Cc::Be => "be",
            Cc::A => "a",
            Cc::Ae => "ae",
        }
    }
}

/// two-address instructions: `Alu` reads and writes `dst`. `Shift` takes its count in `cl`, so
/// `count` is `rcx` by the time of emission. `Cqo` and `Div` work on the `hi:lo` pair, which is
/// `rdx:rax`; the quotient lands in `lo` and the remainder in `hi`. `Setcc` writes only the low
/// byte of `dst`, so it is always followed by `Movzx8`. branches end a block: a conditional
/// `Jnz` is always followed by a `Jmp`, so blocks never fall through implicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inst<R> {
    MovImm {
        dst: R,
        imm: i64,
    },
    Mov {
        dst: R,
        src: R,
    },
    Alu {
        op: AluOp,
        dst: R,
        src: R,
    },
    Shift {
        op: ShiftOp,
        dst: R,
        count: R,
    },
    Unary {
        op: UnaryOp,
        dst: R,
    },
    Cqo {
        hi: R,
        lo: R,
    },
    Cmp {
        a: R,
        b: R,
    },
    Setcc {
        cc: Cc,
        dst: R,
    },
    Movzx8 {
        dst: R,
    },
    /// extends the low `bits` of `src` into the full width of `dst`
    Ext {
        signed: bool,
        bits: u8,
        dst: R,
        src: R,
    },
    Cmov {
        cc: Cc,
        dst: R,
        src: R,
    },
    Div {
        signed: bool,
        divisor: R,
        lo: R,
        hi: R,
    },
    Test {
        reg: R,
    },
    Jnz {
        target: Label,
    },
    Jmp {
        target: Label,
    },
    Ret {
        has_value: bool,
    },
}

impl<R: Copy> Inst<R> {
    pub fn uses(&self) -> Vec<R> {
        match *self {
            Inst::MovImm { .. } | Inst::Jnz { .. } | Inst::Jmp { .. } | Inst::Ret { .. } => vec![],
            Inst::Mov { src, .. } => vec![src],
            Inst::Alu { dst, src, .. } => vec![dst, src],
            Inst::Shift { dst, count, .. } => vec![dst, count],
            Inst::Unary { dst, .. } => vec![dst],
            Inst::Cqo { lo, .. } => vec![lo],
            Inst::Cmp { a, b } => vec![a, b],
            Inst::Setcc { .. } => vec![],
            Inst::Movzx8 { dst } => vec![dst],
            Inst::Ext { src, .. } => vec![src],
            Inst::Cmov { dst, src, .. } => vec![dst, src],
            Inst::Div {
                divisor, lo, hi, ..
            } => vec![divisor, lo, hi],
            Inst::Test { reg } => vec![reg],
        }
    }

    pub fn defs(&self) -> Vec<R> {
        match *self {
            Inst::MovImm { dst, .. }
            | Inst::Mov { dst, .. }
            | Inst::Alu { dst, .. }
            | Inst::Shift { dst, .. }
            | Inst::Unary { dst, .. }
            | Inst::Setcc { dst, .. }
            | Inst::Movzx8 { dst }
            | Inst::Ext { dst, .. }
            | Inst::Cmov { dst, .. } => vec![dst],
            Inst::Cqo { hi, .. } => vec![hi],
            Inst::Div { lo, hi, .. } => vec![lo, hi],
            Inst::Cmp { .. }
            | Inst::Test { .. }
            | Inst::Jnz { .. }
            | Inst::Jmp { .. }
            | Inst::Ret { .. } => vec![],
        }
    }

    pub fn target(&self) -> Option<Label> {
        match *self {
            Inst::Jnz { target } | Inst::Jmp { target } => Some(target),
            _ => None,
        }
    }

    pub fn map_regs<S>(&self, mut f: impl FnMut(R) -> S) -> Inst<S> {
        match *self {
            Inst::MovImm { dst, imm } => Inst::MovImm { dst: f(dst), imm },
            Inst::Mov { dst, src } => Inst::Mov {
                dst: f(dst),
                src: f(src),
            },
            Inst::Alu { op, dst, src } => Inst::Alu {
                op,
                dst: f(dst),
                src: f(src),
            },
            Inst::Shift { op, dst, count } => Inst::Shift {
                op,
                dst: f(dst),
                count: f(count),
            },
            Inst::Unary { op, dst } => Inst::Unary { op, dst: f(dst) },
            Inst::Cmp { a, b } => Inst::Cmp { a: f(a), b: f(b) },
            Inst::Setcc { cc, dst } => Inst::Setcc { cc, dst: f(dst) },
            Inst::Movzx8 { dst } => Inst::Movzx8 { dst: f(dst) },
            Inst::Ext {
                signed,
                bits,
                dst,
                src,
            } => Inst::Ext {
                signed,
                bits,
                dst: f(dst),
                src: f(src),
            },
            Inst::Cmov { cc, dst, src } => Inst::Cmov {
                cc,
                dst: f(dst),
                src: f(src),
            },
            Inst::Cqo { hi, lo } => Inst::Cqo {
                hi: f(hi),
                lo: f(lo),
            },
            Inst::Div {
                signed,
                divisor,
                lo,
                hi,
            } => Inst::Div {
                signed,
                divisor: f(divisor),
                lo: f(lo),
                hi: f(hi),
            },
            Inst::Test { reg } => Inst::Test { reg: f(reg) },
            Inst::Jnz { target } => Inst::Jnz { target },
            Inst::Jmp { target } => Inst::Jmp { target },
            Inst::Ret { has_value } => Inst::Ret { has_value },
        }
    }
}

impl<R: fmt::Display> fmt::Display for Inst<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inst::MovImm { dst, imm } => write!(f, "mov {dst}, {imm}"),
            Inst::Mov { dst, src } => write!(f, "mov {dst}, {src}"),
            Inst::Alu { op, dst, src } => write!(f, "{} {dst}, {src}", op.mnemonic()),
            Inst::Shift { op, dst, .. } => write!(f, "{} {dst}, cl", op.mnemonic()),
            Inst::Unary { op, dst } => write!(f, "{} {dst}", op.mnemonic()),
            Inst::Cqo { .. } => f.write_str("cqo"),
            Inst::Cmp { a, b } => write!(f, "cmp {a}, {b}"),
            Inst::Setcc { cc, dst } => write!(f, "set{} {dst}", cc.suffix()),
            Inst::Movzx8 { dst } => write!(f, "movzx {dst}, {dst}"),
            Inst::Ext {
                signed,
                bits,
                dst,
                src,
            } => write!(
                f,
                "{}{bits} {dst}, {src}",
                if *signed { "movsx" } else { "movzx" }
            ),
            Inst::Cmov { cc, dst, src } => write!(f, "cmov{} {dst}, {src}", cc.suffix()),
            Inst::Div {
                signed, divisor, ..
            } => write!(f, "{} {divisor}", if *signed { "idiv" } else { "div" }),
            Inst::Test { reg } => write!(f, "test {reg}, {reg}"),
            Inst::Jnz { target } => write!(f, "jnz {target}"),
            Inst::Jmp { target } => write!(f, "jmp {target}"),
            Inst::Ret { .. } => f.write_str("ret"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VBlock<R> {
    pub insts: Vec<Inst<R>>,
}

/// blocks in layout order; the first is the entry
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VCode<R> {
    pub name: String,
    pub blocks: Vec<VBlock<R>>,
}

impl<R: fmt::Display> fmt::Display for VCode<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "vcode %{}:", self.name)?;
        for (i, block) in self.blocks.iter().enumerate() {
            writeln!(f, "{}:", Label(i))?;
            block
                .insts
                .iter()
                .try_for_each(|inst| writeln!(f, "    {inst}"))?;
        }
        Ok(())
    }
}
