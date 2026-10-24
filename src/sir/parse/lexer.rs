use miette::SourceSpan;

use super::error::{Failure, PResult, ParseErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TokenKind {
    Ident,
    FuncName,
    Int,
    LParen,
    RParen,
    Comma,
    Colon,
    LBrace,
    RBrace,
    Equals,
    Arrow,
    Eof,
}

impl TokenKind {
    pub fn describe(self) -> &'static str {
        match self {
            TokenKind::Ident => "an identifier",
            TokenKind::FuncName => "a function name",
            TokenKind::Int => "an integer",
            TokenKind::LParen => "`(`",
            TokenKind::RParen => "`)`",
            TokenKind::Comma => "`,`",
            TokenKind::Colon => "`:`",
            TokenKind::LBrace => "`{`",
            TokenKind::RBrace => "`}`",
            TokenKind::Equals => "`=`",
            TokenKind::Arrow => "`->`",
            TokenKind::Eof => "end of input",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub len: usize,
    pub newline_before: bool,
}

impl Token {
    pub fn span(&self) -> SourceSpan {
        SourceSpan::new(self.start.into(), self.len)
    }

    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.start + self.len]
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

pub(super) fn lex(src: &str) -> PResult<Vec<Token>> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut newline = false;

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\n' => {
                newline = true;
                i += 1;
                continue;
            }
            b' ' | b'\t' | b'\r' => {
                i += 1;
                continue;
            }
            b';' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            _ => {}
        }

        let start = i;
        let kind = match b {
            b'(' => single(&mut i, TokenKind::LParen),
            b')' => single(&mut i, TokenKind::RParen),
            b',' => single(&mut i, TokenKind::Comma),
            b':' => single(&mut i, TokenKind::Colon),
            b'{' => single(&mut i, TokenKind::LBrace),
            b'}' => single(&mut i, TokenKind::RBrace),
            b'=' => single(&mut i, TokenKind::Equals),
            b'-' if bytes.get(i + 1) == Some(&b'>') => {
                i += 2;
                TokenKind::Arrow
            }
            b'-' | b'0'..=b'9' => {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                if i - start == 1 && b == b'-' {
                    return Err(unexpected(src, start));
                }
                TokenKind::Int
            }
            b'%' => {
                i += 1;
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
                if i - start == 1 {
                    return Err(unexpected(src, start));
                }
                TokenKind::FuncName
            }
            _ if b.is_ascii_alphabetic() || b == b'_' => {
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
                TokenKind::Ident
            }
            _ => return Err(unexpected(src, start)),
        };

        tokens.push(Token {
            kind,
            start,
            len: i - start,
            newline_before: newline,
        });
        newline = false;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        start: src.len(),
        len: 0,
        newline_before: true,
    });
    Ok(tokens)
}

fn single(i: &mut usize, kind: TokenKind) -> TokenKind {
    *i += 1;
    kind
}

fn unexpected(src: &str, start: usize) -> Failure {
    let ch = src[start..].chars().next().expect("start is in bounds");
    Failure {
        kind: ParseErrorKind::UnexpectedChar(ch),
        span: SourceSpan::new(start.into(), ch.len_utf8()),
    }
}
