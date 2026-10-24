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

pub(super) struct FunctionAst<'a> {
    pub name: &'a str,
    pub callees: Vec<CalleeAst<'a>>,
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
    pub blocks: Vec<BlockAst<'a>>,
    pub end_span: SourceSpan,
}
