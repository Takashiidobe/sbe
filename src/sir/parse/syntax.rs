use super::ast::{
    BlockAst, CalleeAst, FunctionAst, ImportAst, InstAst, Item, Name, Op, Param, StackSlotAst,
    TargetAst, TermAst,
};
use super::error::{Failure, PResult, ParseErrorKind};
use super::lexer::{Token, TokenKind};
use crate::sir::{BinOp, CvtOp, IntCC, Type, UnOp};

pub(super) struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            src,
            tokens,
            pos: 0,
        }
    }

    pub fn function(&mut self) -> PResult<FunctionAst<'a>> {
        let kw = self.expect(TokenKind::Ident)?;
        if kw.text(self.src) != "function" {
            return Err(self.expected_at("`function`", kw));
        }
        let (name, params, returns) = self.signature()?;

        self.expect(TokenKind::LBrace)?;
        let mut callees = Vec::new();
        let mut stack_slots = Vec::new();
        while self.peek().kind == TokenKind::Ident && self.peek_at(1).kind == TokenKind::Equals {
            if self.peek_at(2).kind == TokenKind::FuncName {
                callees.push(self.callee()?);
            } else if self.peek_at(2).kind == TokenKind::Ident
                && self.peek_at(2).text(self.src) == "stack_slot"
            {
                stack_slots.push(self.stack_slot()?);
            } else {
                break;
            }
        }
        let mut blocks = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            blocks.push(self.block()?);
        }
        let end = self.expect(TokenKind::RBrace)?;

        Ok(FunctionAst {
            name,
            stack_slots,
            callees,
            params,
            returns,
            blocks,
            end_span: end.span(),
        })
    }

    pub fn single_function(&mut self) -> PResult<FunctionAst<'a>> {
        let ast = self.function()?;
        self.expect(TokenKind::Eof)?;
        Ok(ast)
    }

    pub fn module(&mut self) -> PResult<Vec<Item<'a>>> {
        let mut items = Vec::new();
        while self.peek().kind != TokenKind::Eof {
            let kw = self.peek();
            items.push(
                if kw.kind == TokenKind::Ident && kw.text(self.src) == "import" {
                    self.advance();
                    Item::Import(self.import()?)
                } else {
                    Item::Function(self.function()?)
                },
            );
        }
        Ok(items)
    }

    fn import(&mut self) -> PResult<ImportAst<'a>> {
        let (name, params, returns) = self.signature()?;
        Ok(ImportAst {
            name,
            params,
            returns,
        })
    }

    fn signature(&mut self) -> PResult<(&'a str, Vec<Type>, Vec<Type>)> {
        let name_tok = self.expect(TokenKind::FuncName)?;
        self.expect(TokenKind::LParen)?;
        let params = self.types(true)?;
        self.expect(TokenKind::RParen)?;
        let returns = if self.eat(TokenKind::Arrow) {
            self.types(false)?
        } else {
            Vec::new()
        };
        Ok((&name_tok.text(self.src)[1..], params, returns))
    }

    fn stack_slot(&mut self) -> PResult<StackSlotAst<'a>> {
        let alias = self.name()?;
        self.expect(TokenKind::Equals)?;
        self.advance();
        let tok = self.expect(TokenKind::Int)?;
        let size = tok.text(self.src).parse().map_err(|_| Failure {
            kind: ParseErrorKind::InvalidInteger(tok.text(self.src).to_owned()),
            span: tok.span(),
        })?;
        Ok(StackSlotAst { alias, size })
    }

    fn callee(&mut self) -> PResult<CalleeAst<'a>> {
        let alias = self.name()?;
        self.expect(TokenKind::Equals)?;
        let (name, params, returns) = self.signature()?;
        Ok(CalleeAst {
            alias,
            name,
            params,
            returns,
        })
    }

    fn types(&mut self, allow_empty: bool) -> PResult<Vec<Type>> {
        let mut types = Vec::new();
        if allow_empty && self.peek().kind == TokenKind::RParen {
            return Ok(types);
        }
        loop {
            types.push(self.ty()?);
            if !self.eat(TokenKind::Comma) {
                return Ok(types);
            }
        }
    }

    fn ty(&mut self) -> PResult<Type> {
        let tok = self.expect(TokenKind::Ident)?;
        Type::from_name(tok.text(self.src)).ok_or_else(|| Failure {
            kind: ParseErrorKind::UnknownType(tok.text(self.src).to_owned()),
            span: tok.span(),
        })
    }

    fn name(&mut self) -> PResult<Name<'a>> {
        let tok = self.expect(TokenKind::Ident)?;
        Ok(Name {
            text: tok.text(self.src),
            span: tok.span(),
        })
    }

    fn block(&mut self) -> PResult<BlockAst<'a>> {
        let name = self.name()?;
        let mut params = Vec::new();
        if self.eat(TokenKind::LParen) {
            if self.peek().kind != TokenKind::RParen {
                loop {
                    let name = self.name()?;
                    self.expect(TokenKind::Colon)?;
                    let ty = self.ty()?;
                    params.push(Param { name, ty });
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;
        }
        self.expect(TokenKind::Colon)?;

        let mut insts = Vec::new();
        let mut terminator = None;
        while self.peek().kind == TokenKind::Ident {
            match self.peek().text(self.src) {
                "return" => {
                    self.advance();
                    terminator = Some(TermAst::Return(self.return_values()?));
                    break;
                }
                "jump" => {
                    self.advance();
                    terminator = Some(TermAst::Jump(self.target()?));
                    break;
                }
                "brif" => {
                    self.advance();
                    let cond = self.name()?;
                    self.expect(TokenKind::Comma)?;
                    let then_target = self.target()?;
                    self.expect(TokenKind::Comma)?;
                    terminator = Some(TermAst::Brif(cond, then_target, self.target()?));
                    break;
                }
                "call" if self.peek_at(1).kind != TokenKind::Equals => {
                    self.advance();
                    let (callee, args) = self.call_target()?;
                    insts.push(InstAst {
                        result: None,
                        op: Op::Call(callee, args),
                    });
                }
                "store" if self.peek_at(1).kind != TokenKind::Equals => {
                    self.advance();
                    let value = self.name()?;
                    self.expect(TokenKind::Comma)?;
                    insts.push(InstAst {
                        result: None,
                        op: Op::Store(value, self.name()?),
                    });
                }
                _ if self.peek_at(1).kind == TokenKind::Equals => insts.push(self.inst()?),
                _ => break,
            }
        }

        Ok(BlockAst {
            name,
            params,
            insts,
            terminator,
        })
    }

    fn call_target(&mut self) -> PResult<(Name<'a>, Vec<Name<'a>>)> {
        let callee = self.name()?;
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                args.push(self.name()?);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok((callee, args))
    }

    fn inst(&mut self) -> PResult<InstAst<'a>> {
        let result = Some(self.name()?);
        self.expect(TokenKind::Equals)?;
        let op_tok = self.expect(TokenKind::Ident)?;
        let op = match op_tok.text(self.src) {
            "iconst" => {
                let ty = self.ty()?;
                let tok = self.expect(TokenKind::Int)?;
                let value = tok.text(self.src).parse().map_err(|_| Failure {
                    kind: ParseErrorKind::InvalidInteger(tok.text(self.src).to_owned()),
                    span: tok.span(),
                })?;
                Op::Iconst(ty, value)
            }
            "icmp" => {
                let cc_tok = self.expect(TokenKind::Ident)?;
                let Some(cc) = IntCC::from_mnemonic(cc_tok.text(self.src)) else {
                    return Err(Failure {
                        kind: ParseErrorKind::UnknownCondition(cc_tok.text(self.src).to_owned()),
                        span: cc_tok.span(),
                    });
                };
                let a = self.name()?;
                self.expect(TokenKind::Comma)?;
                Op::Icmp(cc, a, self.name()?)
            }
            "load" => {
                let ty = self.ty()?;
                Op::Load(ty, self.name()?)
            }
            "stack_addr" => Op::StackAddr(self.name()?),
            "call" => {
                let (callee, args) = self.call_target()?;
                Op::Call(callee, args)
            }
            "select" => {
                let cond = self.name()?;
                self.expect(TokenKind::Comma)?;
                let then_value = self.name()?;
                self.expect(TokenKind::Comma)?;
                Op::Select(cond, then_value, self.name()?)
            }
            other => {
                if let Some(op) = CvtOp::from_mnemonic(other) {
                    let ty = self.ty()?;
                    Op::Convert(op, ty, self.name()?)
                } else if let Some(op) = UnOp::from_mnemonic(other) {
                    Op::Unary(op, self.name()?)
                } else if let Some(op) = BinOp::from_mnemonic(other) {
                    let a = self.name()?;
                    self.expect(TokenKind::Comma)?;
                    let b = self.name()?;
                    Op::Binary(op, a, b)
                } else {
                    return Err(Failure {
                        kind: ParseErrorKind::UnknownOp(other.to_owned()),
                        span: op_tok.span(),
                    });
                }
            }
        };
        Ok(InstAst { result, op })
    }

    // values must share a line with `return`, or the next line's `v = ...` would be taken as one
    fn return_values(&mut self) -> PResult<Vec<Name<'a>>> {
        let mut values = Vec::new();
        let next = self.peek();
        if next.kind != TokenKind::Ident || next.newline_before {
            return Ok(values);
        }
        loop {
            values.push(self.name()?);
            if !self.eat(TokenKind::Comma) {
                return Ok(values);
            }
        }
    }

    fn target(&mut self) -> PResult<TargetAst<'a>> {
        let dest = self.name()?;
        let mut args = Vec::new();
        let next = self.peek();
        if next.kind == TokenKind::LParen && !next.newline_before {
            self.advance();
            if self.peek().kind != TokenKind::RParen {
                loop {
                    args.push(self.name()?);
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;
        }
        Ok(TargetAst { dest, args })
    }

    fn peek(&self) -> Token {
        self.tokens[self.pos]
    }

    fn peek_at(&self, offset: usize) -> Token {
        self.tokens[(self.pos + offset).min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos];
        if tok.kind != TokenKind::Eof {
            self.pos += 1;
        }
        tok
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        let found = self.peek().kind == kind;
        if found {
            self.advance();
        }
        found
    }

    fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        let tok = self.peek();
        if tok.kind == kind {
            Ok(self.advance())
        } else {
            Err(self.expected_at(kind.describe(), tok))
        }
    }

    fn expected_at(&self, expected: &str, found: Token) -> Failure {
        let found_text = match found.kind {
            TokenKind::Eof => "end of input".to_owned(),
            _ => format!("`{}`", found.text(self.src)),
        };
        Failure {
            kind: ParseErrorKind::Expected {
                expected: expected.to_owned(),
                found: found_text,
            },
            span: found.span(),
        }
    }
}
