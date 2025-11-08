use litec_span::{Span, StringId};
use crate::token::{LiteralKind, TokenKind};

#[derive(Debug)]
pub struct Crate {
    pub items: Vec<Item>
}

impl Crate {
    pub fn new(statements: Vec<Item>) -> Self {
        Crate { items: statements }
    }
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: StringId,
    pub args: Option<Vec<StringId>>,
    pub span: Span
}

#[derive(Debug, Clone, Copy)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug)]
pub enum Item {
    Function {
        attribute: Option<Attribute>,
        visibility: Visibility,
        name: StringId,
        return_type: Option<TypeAnnotation>,
        params: Vec<Param>,
        body: Block,
        span: Span
    },
    Struct {
        attribute: Option<Attribute>,
        visibility: Visibility,
        name: StringId,
        fields: Vec<Field>,
        span: Span
    },
    Use {
        visibility: Visibility,
        path: Vec<StringId>,
        items: Option<Vec<UseItem>>,
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

#[derive(Debug)]
pub enum ExternItem {
    Function {
        name: StringId,
        params: Vec<Param>,
        return_type: Option<TypeAnnotation>,
        span: Span,
    },
}

#[derive(Debug)]
pub enum AbiType {
    Builtin,
    C
}


#[derive(Debug)]
pub struct UseItem {
    pub name: StringId, // 例如 "stdin"
    pub rename: Option<StringId>, // 可选的重命名，例如 "stdin" -> "my_stdin"
    pub items: Option<Vec<UseItem>>, // 递归定义，例如 d::e
    pub span: Span
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: StringId,
    pub ty: TypeAnnotation,
    pub span: Span
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
    pub ty: TypeAnnotation,
    pub visibility: Visibility,
    pub span: Span
}

#[derive(Debug, Clone)]
pub enum TypeAnnotation {
    Ident {
        name: StringId,
        span: Span
    }
}

impl TypeAnnotation {
    pub fn span(&self) -> Span {
        match self {
            Self::Ident { span, .. } => *span
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Binary {
        left: Box<Expr>,
        op: TokenKind,
        right: Box<Expr>,
        span: Span
    },
    Unary {
        op: TokenKind,
        operand: Box<Expr>,
        span: Span,
    },
    Posifix {
        op: TokenKind,
        expr: Box<Expr>,
        span: Span
    },
    Literal {
        kind: LiteralKind,
        value: StringId,
        suffix: Option<StringId>,
        span: Span,
    },
    Ident {
        name: StringId,
        span: Span,
    },
    Grouped {
        expr: Box<Expr>,
        span: Span,
    },
    Assignment {
        target: Box<Expr>,
        op: AssignOp,
        value: Box<Expr>,
        span: Span,
    },
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
    While {
        condition: Box<Expr>,
        body: Block,
        span: Span,
    },
    For {
        variable: Box<Expr>,
        generator: Box<Expr>,
        body: Block,
        span: Span
    },
    Index {
        indexed: Box<Expr>,
        index: Box<Expr>,
        span: Span
    },
    To {
        strat: Box<Expr>,
        end: Box<Expr>,
        span: Span
    },
    Loop {
        body: Block,
        span: Span
    },
    FieldAccess {
        base: Box<Expr>,
        name: StringId,
        span: Span
    },
    PathAccess {
        segments: Vec<StringId>,
        span: Span
    },
    Bool {
        value: bool,
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
    Expr {
        expr: Box<Expr>
    },
    Let {
        mutable: bool,
        name: StringId,
        ty: Option<TypeAnnotation>,
        value: Option<Expr>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Continue {
        span: Span,
    },
    Break {
        value: Option<Expr>,
        span: Span
    }
}

// 赋值操作符 - 用于赋值，不产生值（或者说产生单位值）
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

// 为包含 `span: Span` 字段的结构体实现 `span()` 方法
macro_rules! impl_span_for_struct {
    ($struct_name:ident) => {
        impl $struct_name {
            pub fn span(&self) -> Span {
                self.span
            }
        }
    };
}

// 为只有一个变体的枚举实现 `span()`（不太实用）
// 或者为所有变体都包含 `span` 的枚举，但需要手动列出
macro_rules! impl_span_for_enum_with_common_span {
    ($enum_name:ident, $($variant:ident),*) => {
        impl $enum_name {
            pub fn span(&self) -> Span {
                match self {
                    $( $enum_name::$variant { span, .. } => *span, )*
                }
            }
        }
    };
}

// 为包含 Block 的枚举实现 `span()`（特殊处理）
macro_rules! impl_span_for_enum_with_block {
    ($enum_name:ident, $($common_variant:ident),*; $block_variant:ident) => {
        impl $enum_name {
            pub fn span(&self) -> Span {
                match self {
                    $( $enum_name::$common_variant { span, .. } => *span, )*
                    $enum_name::$block_variant { block } => block.span,
                }
            }
        }
    };
}

// 为所有简单结构体实现 span
impl_span_for_struct!(Param);
impl_span_for_struct!(Field);
impl_span_for_struct!(Block); // 注意：Block 的 span 可能需要更复杂的逻辑

// 为所有变体都包含 `span` 字段的枚举实现
impl_span_for_enum_with_common_span!(Item, Function, Struct, Use, Extern);

// 为 Expr 实现（通用变体 + Block 特殊变体）
impl_span_for_enum_with_block!(
    Expr,
    Binary, PathAccess, Unary, Literal, Ident, Grouped, Assignment, Call, If, While, Loop, For, Posifix, FieldAccess, Bool, To, Index, Tuple, Unit;
    Block
);

// 为 Stmt 实现（Expr 特殊，其他通用）
// 我们需要一个更通用的宏
macro_rules! impl_span_for_stmt {
    ($($common_variant:ident),*; $expr_variant:ident) => {
        impl Stmt {
            pub fn span(&self) -> Span {
                match self {
                    $( Stmt::$common_variant { span, .. } => *span, )*
                    Stmt::$expr_variant { expr } => expr.span(),
                }
            }
        }
    };
}

impl_span_for_stmt!(Let, Return, Continue, Break; Expr);