use super::ast::{BlockAst, FunctionAst, InstAst, Name, Op, Param, TargetAst, TermAst};
use super::error::{Failure, PResult, ParseErrorKind};
use super::lexer::{Token, TokenKind};
use crate::sir::{BinOp, Type, UnOp};

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
        let name_tok = self.expect(TokenKind::FuncName)?;
        let name = &name_tok.text(self.src)[1..];

        self.expect(TokenKind::LParen)?;
        let params = self.types(true)?;
        self.expect(TokenKind::RParen)?;
        let returns = if self.eat(TokenKind::Arrow) {
            self.types(false)?
        } else {
            Vec::new()
        };

        self.expect(TokenKind::LBrace)?;
        let mut blocks = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            blocks.push(self.block()?);
        }
        let end = self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Eof)?;

        Ok(FunctionAst {
            name,
            params,
            returns,
            blocks,
            end_span: end.span(),
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
        match tok.text(self.src) {
            "i64" => Ok(Type::I64),
            other => Err(Failure {
                kind: ParseErrorKind::UnknownType(other.to_owned()),
                span: tok.span(),
            }),
        }
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

    fn inst(&mut self) -> PResult<InstAst<'a>> {
        let result = self.name()?;
        self.expect(TokenKind::Equals)?;
        let op_tok = self.expect(TokenKind::Ident)?;
        let op = match op_tok.text(self.src) {
            "iconst" => {
                let tok = self.expect(TokenKind::Int)?;
                let value = tok.text(self.src).parse().map_err(|_| Failure {
                    kind: ParseErrorKind::InvalidInteger(tok.text(self.src).to_owned()),
                    span: tok.span(),
                })?;
                Op::Iconst(value)
            }
            other => {
                if let Some(op) = UnOp::from_mnemonic(other) {
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
