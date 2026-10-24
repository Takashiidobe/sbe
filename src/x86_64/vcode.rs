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

/// two-address instructions: `Add` reads and writes `dst`
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inst<R> {
    MovImm { dst: R, imm: i64 },
    Mov { dst: R, src: R },
    Add { dst: R, src: R },
    Ret { has_value: bool },
}

impl<R: Copy> Inst<R> {
    pub fn uses(&self) -> Vec<R> {
        match *self {
            Inst::MovImm { .. } => vec![],
            Inst::Mov { src, .. } => vec![src],
            Inst::Add { dst, src } => vec![dst, src],
            Inst::Ret { .. } => vec![],
        }
    }

    pub fn defs(&self) -> Vec<R> {
        match *self {
            Inst::MovImm { dst, .. } | Inst::Mov { dst, .. } | Inst::Add { dst, .. } => vec![dst],
            Inst::Ret { .. } => vec![],
        }
    }

    pub fn map_regs<S>(&self, mut f: impl FnMut(R) -> S) -> Inst<S> {
        match *self {
            Inst::MovImm { dst, imm } => Inst::MovImm { dst: f(dst), imm },
            Inst::Mov { dst, src } => Inst::Mov {
                dst: f(dst),
                src: f(src),
            },
            Inst::Add { dst, src } => Inst::Add {
                dst: f(dst),
                src: f(src),
            },
            Inst::Ret { has_value } => Inst::Ret { has_value },
        }
    }
}

impl<R: fmt::Display> fmt::Display for Inst<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inst::MovImm { dst, imm } => write!(f, "mov {dst}, {imm}"),
            Inst::Mov { dst, src } => write!(f, "mov {dst}, {src}"),
            Inst::Add { dst, src } => write!(f, "add {dst}, {src}"),
            Inst::Ret { .. } => f.write_str("ret"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VCode<R> {
    pub name: String,
    pub insts: Vec<Inst<R>>,
}

impl<R: fmt::Display> fmt::Display for VCode<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "vcode %{}:", self.name)?;
        self.insts
            .iter()
            .try_for_each(|inst| writeln!(f, "    {inst}"))
    }
}
