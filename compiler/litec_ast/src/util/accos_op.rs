use crate::{
    ast::{AssignOpKind, BinOpKind, RangeLimits},
    token::{Token, TokenKind}, util::precedence::Precedence,
};

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AssocOp {
    /// 二元运算符号
    Binary(BinOpKind),
    /// 带符号等于的符号, 例如 `+=`
    AssignOp(AssignOpKind),
    /// `=`
    Assign,
    /// `as`
    Cast,
    /// `..` 或 `..=`
    Range(RangeLimits),
}

impl AssocOp {
    pub fn from_token(token: &Token) -> Option<Self> {
        use AssocOp::*;
        match token.kind {
            TokenKind::Assign => Some(Assign),
            TokenKind::EqEq => Some(Binary(BinOpKind::Eq)),
            TokenKind::NotEq => Some(Binary(BinOpKind::Ne)),
            TokenKind::Lt => Some(Binary(BinOpKind::Lt)),
            TokenKind::Le => Some(Binary(BinOpKind::Le)),
            TokenKind::Gt => Some(Binary(BinOpKind::Gt)),
            TokenKind::Ge => Some(Binary(BinOpKind::Ge)),
            TokenKind::Minus => Some(Binary(BinOpKind::Sub)),
            TokenKind::MinusEq => Some(AssignOp(AssignOpKind::SubAssign)),
            TokenKind::BitAnd => Some(Binary(BinOpKind::BitAnd)),
            TokenKind::BitAndEq => Some(AssignOp(AssignOpKind::BitAndAssign)),
            TokenKind::And => Some(Binary(BinOpKind::And)),
            TokenKind::BitOr => Some(Binary(BinOpKind::BitOr)),
            TokenKind::BitOrEq => Some(AssignOp(AssignOpKind::BitOrAssign)),
            TokenKind::Or => Some(Binary(BinOpKind::Or)),
            TokenKind::Add => Some(Binary(BinOpKind::Add)),
            TokenKind::PlusEq => Some(AssignOp(AssignOpKind::AddAssign)),
            TokenKind::Mul => Some(Binary(BinOpKind::Mul)),
            TokenKind::MulEq => Some(AssignOp(AssignOpKind::MulAssign)),
            TokenKind::Div => Some(Binary(BinOpKind::Div)),
            TokenKind::DivEq => Some(AssignOp(AssignOpKind::DivAssign)),
            TokenKind::BitXor => Some(Binary(BinOpKind::BitXor)),
            TokenKind::BitXorEq => Some(AssignOp(AssignOpKind::BitXorAssign)),
            TokenKind::Remainder => Some(Binary(BinOpKind::Rem)),
            TokenKind::RemainderEq => Some(AssignOp(AssignOpKind::RemAssign)),
            TokenKind::To => Some(Range(RangeLimits::HalfOpen)),
            TokenKind::ToEq => Some(Range(RangeLimits::Closed)),
            TokenKind::Shl => Some(Binary(BinOpKind::Shl)),
            TokenKind::ShlEq => Some(AssignOp(AssignOpKind::ShlAssign)),
            TokenKind::Shr => Some(Binary(BinOpKind::Shr)),
            TokenKind::ShrEq => Some(AssignOp(AssignOpKind::ShrAssign)),
            TokenKind::As => Some(Cast),
            _ => None,
        }
    }

    pub fn precedence(&self) -> Precedence {
        match self {
            AssocOp::Binary(bin_op) => bin_op.precedence(),
            AssocOp::Assign | AssocOp::AssignOp(_) => Precedence::Assign,
            AssocOp::Cast => Precedence::Cast,
            AssocOp::Range(_) => Precedence::Range,
        }
    }

    pub fn fixity(&self) -> Fixity {
        use AssocOp::*;

        match self {
            Assign | AssignOp(_) => Fixity::Right,
            Binary(bin_op) => bin_op.fixity(),
            Cast => Fixity::Left,
            Range(_) => Fixity::None,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Fixity {
    Left,   // 左结合：a + b + c 解析为 (a + b) + c
    Right,  // 右结合：a = b = c 解析为 a = (b = c)
    None,   // 不可结合：a < b < c 非法
}