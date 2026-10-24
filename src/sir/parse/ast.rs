use miette::SourceSpan;

use crate::sir::{BinOp, CvtOp, IntCC, Type, UnOp};

#[derive(Clone, Copy, Debug)]
pub(super) struct Name<'a> {
    pub text: &'a str,
    pub span: SourceSpan,
}

pub(super) struct Param<'a> {
    pub name: Name<'a>,
    pub ty: Type,
}

pub(super) enum Op<'a> {
    Iconst(Type, i64),
    Convert(CvtOp, Type, Name<'a>),
    Unary(UnOp, Name<'a>),
    Icmp(IntCC, Name<'a>, Name<'a>),
    Select(Name<'a>, Name<'a>, Name<'a>),
    Binary(BinOp, Name<'a>, Name<'a>),
    Call(Name<'a>, Vec<Name<'a>>),
    Load(Type, Name<'a>),
    Store(Name<'a>, Name<'a>),
    StackAddr(Name<'a>),
}

pub(super) struct InstAst<'a> {
    pub result: Option<Name<'a>>,
    pub op: Op<'a>,
}

pub(super) struct TargetAst<'a> {
    pub dest: Name<'a>,
    pub args: Vec<Name<'a>>,
}

pub(super) enum TermAst<'a> {
    Return(Vec<Name<'a>>),
    Jump(TargetAst<'a>),
    Brif(Name<'a>, TargetAst<'a>, TargetAst<'a>),
    Switch {
        value: Name<'a>,
        cases: Vec<(i64, TargetAst<'a>)>,
        default: TargetAst<'a>,
    },
    Trap,
    Unreachable,
}

pub(super) struct BlockAst<'a> {
    pub name: Name<'a>,
    pub params: Vec<Param<'a>>,
    pub insts: Vec<InstAst<'a>>,
    pub terminator: Option<TermAst<'a>>,
}

pub(super) struct CalleeAst<'a> {
    pub alias: Name<'a>,
    pub name: &'a str,
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
}

pub(super) struct StackSlotAst<'a> {
    pub alias: Name<'a>,
    pub size: u32,
}

pub(super) struct FunctionAst<'a> {
    pub name: &'a str,
    pub stack_slots: Vec<StackSlotAst<'a>>,
    pub callees: Vec<CalleeAst<'a>>,
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
    pub blocks: Vec<BlockAst<'a>>,
    pub end_span: SourceSpan,
}

pub(super) struct ImportAst<'a> {
    pub name: &'a str,
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
}

pub(super) enum Item<'a> {
    Import(ImportAst<'a>),
    Function(FunctionAst<'a>),
}
