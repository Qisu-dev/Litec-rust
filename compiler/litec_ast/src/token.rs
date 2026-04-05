use litec_span::{Span, StringId};

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Token {
    pub span: Span,
    pub kind: TokenKind,
    pub text: StringId,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span, text: StringId) -> Self {
        Token {
            span: span,
            kind: kind,
            text: text,
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
    Assign,
    /// `==`
    EqEq,
    /// `!=`
    NotEq,
    /// `!`
    Bang,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `-`
    Minus,
    /// `--`
    MinusMinus,
    /// `-=`
    MinusEq,
    /// `&`
    BitAnd,
    /// `&=`
    BitAndEq,
    /// `&&`
    And,
    /// `|`
    BitOr,
    /// `|=`
    BitOrEq,
    /// `||`
    Or,
    /// `+`
    Add,
    /// `++`
    PlusPlus,
    /// `+=`
    PlusEq,
    /// `*`
    Mul,
    /// `*=`
    MulEq,
    /// `/`
    Div,
    /// `/=`
    DivEq,
    /// `^`
    BitXor,
    /// `^=`
    BitXorEq,
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
    /// `..=`
    ToEq,
    /// `...`
    Ellipsis,
    /// `<<`
    Shl,
    /// `<<=`
    ShlEq,
    /// `>>`
    Shr,
    /// `>>=`
    ShrEq,

    // Keyword
    Fn,
    Let,
    If,
    Else,
    While,
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
    Const,
    Mod,
    Super,
    Crate,
    /// self
    SelfLower,
    /// Self
    SelfUpper,
    Trait,
    Type,
    Impl,
    For,

    Error,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiteralKind {
    Integer,
    Float,
    Char,
    Str,
}
