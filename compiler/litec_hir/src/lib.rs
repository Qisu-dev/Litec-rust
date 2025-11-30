use litec_span::{Span, StringId};
use litec_ast::token::TokenKind;
use litec_ast::ast;
use rustc_hash::FxHashMap;
use litec_ast::ast::AbiType as AstAbiType;

#[derive(Debug, Clone)]
pub struct Crate {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: StringId,
    pub kind: AttributeKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum AttributeKind {
    /// 无参数：#[attr]
    Simple,
    /// 只有位置参数：#[attr(expr1, expr2)]
    Positional(Vec<Expr>),
    /// 只有命名参数：#[attr(key = value)]
    Named(FxHashMap<StringId, Expr>),
    /// 混合参数：#[attr(pos, key = value)]
    Mixed {
        positional: Vec<Expr>,
        named: FxHashMap<StringId, Expr>,
    },
}

#[derive(Debug, Clone)]
pub enum Item {
    Function {
        attribute: Option<Attribute>,
        visibility: Visibility,
        name: StringId,
        params: Vec<Param>,
        return_type: Option<Type>,
        body: Block,
        span: Span,
    },
    Struct {
        attribute: Option<Attribute>,
        visibility: Visibility,
        name: StringId,
        fields: Vec<Field>,
        span: Span,
    },
    Use {
        visibility: Visibility,
        path: Vec<StringId>,
        items: Vec<UseItem>,
        rename: Option<StringId>,
        span: Span,
    },
    Extern {
        visibility: Visibility,
        abi: AbiType,
        items: Vec<ExternItem>,
        span: Span
    }
}

#[derive(Debug, Clone)]
pub enum ExternItem {
    Function {
        name: StringId,
        params: Vec<Param>,
        return_type: Option<Type>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum AbiType {
    Builtin,
    C
}

impl From<AstAbiType> for AbiType {
    fn from(value: AstAbiType) -> Self {
        match value {
            AstAbiType::Builtin => Self::Builtin,
            AstAbiType::C => Self::C
        }
    }
}

#[derive(Debug, Clone)]
pub struct UseItem {
    pub name: StringId,
    pub rename: Option<StringId>,
    pub items: Vec<UseItem>,
    pub span: Span
}

#[derive(Debug, Clone, Copy)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: StringId,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: StringId,
    pub ty: Type,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum Mutability {
    Mut,
    Const
}

impl From<ast::Mutability> for Mutability {
    fn from(value: ast::Mutability) -> Self {
        match value {
            ast::Mutability::Mut => Self::Mut,
            ast::Mutability::Const => Self::Const
        }
    }
}

#[derive(Debug, Clone)]
pub enum Type {
    Named {
        name: StringId,
        span: Span,
    },
    Generic {
        name: StringId,
        args: Vec<Type>,
        span: Span
    },
    Tuple {
        elements: Vec<Type>,
        span: Span
    },
    Reference {
        mutable: Mutability,
        target: Box<Type>,
        span: Span
    },
    Pointer {
        mutable: Mutability,
        target: Box<Type>,
        span: Span
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    // 基本表达式
    Literal {
        value: LiteralValue, // 解析后的值
        span: Span,
    },
    Ident {
        name: StringId,
        span: Span,
    },
    
    // 算术运算
    Binary {
        left: Box<Expr>,
        right: Box<Expr>,
        op: BinOp,
        span: Span
    },

    Unary {
        op: UnOp,
        operand: Box<Expr>,
        span: Span
    },

    Posifix {
        operand: Box<Expr>,
        op: PosOp,
        span: Span
    },
    
    // 赋值运算 - 统一使用基本的 Assign 节点
    Assign {
        target: Box<Expr>,
        op: AssignOp,
        value: Box<Expr>,
        span: Span,
    },
    
    // 复合表达式
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Block {
        block: Block
    },
    If {
        condition: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<Expr>>,
        span: Span,
    },
    Loop {
        body: Box<Block>,
        span: Span,
    },
    FieldAccess {
        base: Box<Expr>,
        field: StringId,
        span: Span,
    },
    PathAccess {
        segments: Vec<StringId>,
        span: Span,
    },
    
    // 分组表达式（保留括号语义）
    Grouped {
        expr: Box<Expr>,
        span: Span,
    },

    Index {
        indexed: Box<Expr>,
        index: Box<Expr>,
        span: Span
    },

    To {
        start: Box<Expr>,
        end: Box<Expr>,
        span: Span
    },
    ToEq {
        start: Box<Expr>,
        end: Box<Expr>,
        span: Span
    },
    Tuple {
        elements: Vec<Expr>,
        span: Span
    },
    Unit {
        span: Span
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(Box<Expr>),
    Let {
        mutable: Mutability,
        name: StringId,
        ty: Option<Type>,
        value: Option<Expr>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Break {
        value: Option<Expr>,
        span: Span,
    },
    Continue {
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,    // - (算术负)
    Not,    // ! (逻辑非)
    Deref,  // * (解引用)
    AddrOf, // & (取地址)
}

impl From<TokenKind> for UnOp {
    fn from(value: TokenKind) -> Self {
        match value {
            TokenKind::Minus => Self::Neg,
            TokenKind::Bang => Self::Not,
            TokenKind::Mul => Self::Deref,
            TokenKind::BitAnd => Self::AddrOf,
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AssignOp {
    Simple,     // =
    Add,        // +=
    Subtract,   // -=
    Multiply,   // *=
    Divide,     // /=
    Remainder,  // %=
    // 可以根据需要添加更多
}

impl From<TokenKind> for AssignOp {
    fn from(value: TokenKind) -> Self {
        match value {
            TokenKind::Assign => Self::Simple,
            TokenKind::PlusEq => Self::Add,
            TokenKind::MinusEq => Self::Subtract,
            TokenKind::MulEq => Self::Multiply,
            TokenKind::DivEq => Self::Divide,
            TokenKind::RemainderEq => Self::Remainder,
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // 算术运算
    Add,    // +
    Sub,    // -
    Mul,    // *
    Div,    // /
    Rem,    // %
    
    // 比较运算  
    Eq,     // ==
    Ne,     // !=
    Lt,     // <
    Le,     // <=
    Gt,     // >
    Ge,     // >=
    
    // 逻辑运算
    And,    // &&
    Or,     // ||
    
    // 位运算
    BitAnd, // &
    BitOr,  // |
    BitXor, // ^
    Shl,    // <<
    Shr,    // >>
}

impl From <TokenKind> for BinOp {
    fn from(value: TokenKind) -> Self {
        match value {
            TokenKind::Add => Self::Add,
            TokenKind::Minus => Self::Sub,
            TokenKind::Mul => Self::Mul,
            TokenKind::Div => Self::Div,
            TokenKind::Remainder => Self::Rem,
            TokenKind::EqEq => Self::Eq,
            TokenKind::NotEq => Self::Ne,
            TokenKind::Lt => Self::Lt,
            TokenKind::Le => Self::Le,
            TokenKind::Gt => Self::Gt,
            TokenKind::Ge => Self::Ge,
            TokenKind::And => Self::And,
            TokenKind::Or => Self::Or,
            TokenKind::BitAnd => Self::BitAnd,
            TokenKind::BitOr => Self::BitOr,
            TokenKind::BitXor => Self::BitXor,
            TokenKind::Shl => Self::Shl,
            TokenKind::Shr => Self::Shr,
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosOp {
    /// ++
    Plus,
    /// --
    Sub,
}

impl From<TokenKind> for PosOp {
    fn from(value: TokenKind) -> Self {
        match value {
            TokenKind::PlusPlus => Self::Plus,
            TokenKind::MinusMinus => Self::Sub,
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Clone)]
pub enum LitIntValue {
    I8(i8), I16(i16), I32(i32), I64(i64), I128(i128), Isize(isize),
    U8(u8), U16(u16), U32(u32), U64(u64), U128(u128), Usize(usize), // 添加无符号类型
    Unknown(i16), 
}

#[derive(Debug, Clone)]
pub enum LitFloatValue {
    F32(f32),
    F64(f64),
    Unknown(f32), // 用于未指定类型的浮点字面量
}

#[derive(Debug, Clone)]
pub enum LiteralValue {
    Int {
        value: LitIntValue
    },
    Float {
        value: LitFloatValue
    },
    Bool(bool),             // 布尔值
    Str(StringId),       // 字符串字面量（使用 StringId）
    Char(char),             // 字符字面量
    Unit
}

// 为枚举类型自动生成 span 方法
// 使用方法: impl_span_for_enum!(EnumName, [Variant1, Variant2, ...], (SpecialArm));
// 例如: (Expr::Block { block } => block.span)
macro_rules! impl_span_for_enum {
    (
        $enum_name:ident, 
        [ $($common_variant:ident),* $(,)? ],
        ($($special_arm:tt)*)
    ) => {
        impl $enum_name {
            pub fn span(&self) -> Span {
                match self {
                    // 处理所有通用变体
                    $( $enum_name::$common_variant { span, .. } => *span, )*
                    // 插入特殊分支
                    $($special_arm)*
                }
            }
        }
    };
}

// 为 Expr 自动实现 span (Block 变体)
impl_span_for_enum!(
    Expr,
    [
        Literal, Ident, Binary,
        Assign, Unary,
        Call, If, Loop, FieldAccess, PathAccess, Grouped, Posifix,
        Index, To, ToEq, Tuple, Unit
    ],
    (Expr::Block { block } => block.span)
);

// 为 Stmt 自动实现 span (Expr 变体)
impl_span_for_enum!(
    Stmt,
    [Let, Return, Break, Continue],
    (Stmt::Expr(expr) => expr.span())
);