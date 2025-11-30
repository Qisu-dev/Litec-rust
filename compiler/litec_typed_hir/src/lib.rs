pub mod def_id;
pub mod ty;

use litec_hir::LiteralValue;
use litec_span::{Span, StringId};

use crate::{def_id::DefId, ty::Ty};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefKind {
    Function,
    Struct,
    Variable,
    Parameter,
    Field,
    Module,
    Constant,
    ExternFunction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId {
    pub id: u32,
}

impl ModuleId {
    pub fn new(id: u32) -> Self {
        ModuleId { id }
    }
}

#[derive(Debug)]
pub struct TypedCrate {
    pub items: Vec<TypedItem>,
}

#[derive(Debug, Clone)]
pub enum TypedExpr {
    Literal {
        value: LiteralValue,
        ty: Ty,
        span: Span,
    },
    Ident {
        name: StringId, // 用于错误提示
        def_id: DefId,  // ✅ 指向定义
        ty: Ty,
        span: Span,
    },
    Addition {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },
    Subtract {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },
    Multiply {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },
    Divide {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },
    Remainder {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },

    Equal {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty, // bool
        span: Span,
    },
    NotEqual {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty, // bool
        span: Span,
    },
    LessThan {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty, // bool
        span: Span,
    },
    LessThanOrEqual {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty, // bool
        span: Span,
    },
    GreaterThan {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty, // bool
        span: Span,
    },
    GreaterThanOrEqual {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty, // bool
        span: Span,
    },

    LogicalAnd {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },
    LogicalOr {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },
    LogicalNot {
        operand: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },

    Assign {
        target: Box<TypedExpr>,
        value: Box<TypedExpr>,
        ty: Ty, // () 或其他
        span: Span,
    },

    Negate {
        operand: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },
    Dereference {
        expr: Box<TypedExpr>,
        ty: Ty,
        span: Span,
    },
    AddressOf {
        base: Box<TypedExpr>,
        mutable: bool,
        ty: Ty,
        span: Span,
    },

    Call {
        callee: DefId,
        args: Vec<TypedExpr>,
        ty: Ty, // 返回类型
        span: Span,
    },

    Block {
        block: TypedBlock,
    },

    If {
        condition: Box<TypedExpr>,
        then_branch: TypedBlock,
        else_branch: Option<Box<TypedExpr>>,
        ty: Ty,
        span: Span,
    },

    Loop {
        body: TypedBlock,
        ty: Ty, // ! (never type)
        span: Span,
    },

    FieldAccess {
        base: Box<TypedExpr>,
        field: StringId,
        def_id: DefId,
        ty: Ty,
        span: Span,
    },

    PathAccess {
        def_id: DefId, // 全局唯一
        ty: Ty,
        span: Span,
    },
}

// ========================
// 语句
// ========================

#[derive(Debug, Clone)]
pub enum TypedStmt {
    Expr(Box<TypedExpr>),
    Let {
        name: StringId,
        def_id: DefId, // 变量的 DefId
        ty: Ty,        // 声明的类型
        init: Option<Box<TypedExpr>>,
        span: Span,
    },
    Return {
        value: Option<Box<TypedExpr>>,
        span: Span,
    },
    Break {
        value: Option<Box<TypedExpr>>,
        span: Span,
    },
    Continue {
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct TypedBlock {
    pub stmts: Vec<TypedStmt>,
    pub tail: Option<Box<TypedExpr>>,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypedParam {
    pub name: StringId,
    pub def_id: DefId,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Debug)]
pub enum TypedItem {
    Function {
        def_id: DefId,
        visibility: Visibility,
        name: StringId,
        params: Vec<TypedParam>,
        return_ty: Ty,
        body: TypedBlock,
        span: Span,
    },
    Struct {
        def_id: DefId,
        visibility: Visibility,
        name: StringId,
        fields: Vec<TypedField>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct TypedField {
    pub name: StringId,
    pub def_id: DefId,
    pub ty: Ty,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Visibility {
    Public,
    Private,
}

macro_rules! impl_ty {
    (
        $enum_name:ident,
        [ $($common_variant:ident),* $(,)? ],
        ($($special_arm:tt)*)
    ) => {
        impl $enum_name {
            pub fn ty(&self) -> Ty {
                match self {
                    // 处理所有通用变体
                    $( $enum_name::$common_variant { ty, .. } => ty.clone(), )*
                    // 插入特殊分支
                    $($special_arm)*
                }
            }
        }
    };
}

impl_ty!(
    TypedExpr,
    [
        Literal,Ident,Subtract,Divide,Addition,Multiply,Remainder,Equal,
        NotEqual,LessThan,LessThanOrEqual,GreaterThan,GreaterThanOrEqual,LogicalAnd,LogicalNot,
        LogicalOr,Loop,Assign,Negate,Dereference,AddressOf,Call,If,FieldAccess, PathAccess
    ],
    (TypedExpr::Block { block } => block.ty.clone())
);

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

impl_span_for_enum!(
    TypedExpr,
    [
        Literal, Ident, Addition, Subtract, Multiply, Divide, Remainder,
        Equal, NotEqual, LessThan, LessThanOrEqual, GreaterThan, GreaterThanOrEqual,
        LogicalAnd, LogicalOr, LogicalNot, Assign, Negate, Dereference, AddressOf,
        Call, If, Loop, FieldAccess, PathAccess
    ],
    (TypedExpr::Block { block } => block.span)
);

impl_span_for_enum!(
    TypedStmt,
    [
        Let, Return, Break, Continue
    ],
    (TypedStmt::Expr(expr) => expr.span())
);
