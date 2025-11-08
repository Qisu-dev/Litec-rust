use litec_span::{Span, StringId};

#[derive(Debug,PartialEq,Clone)]
pub struct Token<'src> {
    pub span: Span,
    pub kind: TokenKind,
    pub text: &'src str
}

impl<'src> Token<'src> {
    pub fn new(kind: TokenKind, span: Span, text: &'src str) -> Self{
        Token {
            span: span,
            kind: kind,
            text: text
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Ident,

    Literal {
        kind: LiteralKind,
        suffix: Option<StringId>,
    },

    /// `;`
    Semi,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `::`
    PathAccess,
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
    /// `{`
    OpenBrace,
    /// `}`
    CloseBrace,
    /// `[`
    OpenBracket,
    /// `]`
    CloseBracket,
    /// `@`
    At,
    /// `#`
    Hash,
    /// `~`
    Tilde,
    /// `?`
    Question,
    /// `:`
    Colon,
    /// `$`
    Dollar,
    /// `=`
    Eq,
    /// `==`
    EqEq,
    /// `!=`
    NotEq,
    /// `!`
    Bang,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,
    /// `-`
    Minus,
    /// `--`
    MinusMinus,
    /// `-=`
    MinusEq,
    /// `&`
    BitAnd,
    /// `&&`
    And,
    /// `|`
    BitOr,
    /// `||`
    Or,
    /// `+`
    Plus,
    /// `++`
    PlusPlus,
    /// `+=`
    PlusEq,
    /// `*`
    Star,
    /// `*=`
    StarEq,
    /// `/`
    Slash,
    /// `/=`
    SlashEq,
    /// `^`
    Caret,
    /// `%`
    Remainder,
    /// `%=`
    RemainderEq,
    /// `->`
    Arrow,
    /// `=>`
    FatArrow,
    /// `..`
    To,

    // Keyword
    Fn,
    Let,
    If,
    Else,
    While,
    For,
    Return,
    True,
    False,
    In,
    Struct,
    Loop,
    Break,
    Continue,
    Pub,
    Priv,
    Use,
    As,
    Extern,
    Mut,

    Error,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiteralKind {
    Int {
        base: Base
    },

    Float {
        base: Base
    },

    Char {
        terminated: bool
    },

    Str {
        terminated: bool
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Base {
    Binary = 2,
    
    Octal = 8,
    
    Decimal = 10,
    
    Hexadecimal = 16
}