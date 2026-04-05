pub mod builtins;

use litec_hir::{AbiType, AssignOp, BinOp, LiteralValue, Mutability, PosOp, UnOp, Visibility};
use litec_span::{Span, StringId};
use litec_ty::{def_id::DefId, ty::Ty};

use crate::builtins::Builtins;

#[derive(Debug)]
pub struct Definition {
    pub def_id: DefId,
    pub name: StringId,
    pub span: Span,
}

#[derive(Debug)]
pub struct TypedCrate<'hir> {
    pub items: Vec<TypedItem<'hir>>,
    pub builtin: Builtins<'hir>,
    pub definitions: Vec<Definition>,
}

#[derive(Debug, Clone)]
pub enum TypedExpr<'hir> {
    /* ---------- 原子 ---------- */
    Literal {
        value: LiteralValue,
        ty: Ty<'hir>,
        span: Span,
    },
    Local {
        def_id: DefId,
        ty: Ty<'hir>,
        span: Span,
    },
    Global {
        def_id: DefId, // 直接存储DefId而不是ResResult
        ty: Ty<'hir>,
        span: Span,
    },

    /* ---------- 运算 ---------- */
    Binary {
        left: Box<TypedExpr<'hir>>,
        op: BinOp,
        right: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },
    Unary {
        op: UnOp,
        operand: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },
    Postfix {
        operand: Box<TypedExpr<'hir>>,
        op: PosOp,
        ty: Ty<'hir>,
        span: Span,
    },
    Assign {
        target: Box<TypedExpr<'hir>>,
        op: AssignOp,
        value: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },
    AddressOf {
        expr: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },
    Dereference {
        expr: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },

    /* ---------- 复合 ---------- */
    Call {
        callee: DefId,
        args: Vec<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },
    Block {
        block: TypedBlock<'hir>,
        ty: Ty<'hir>, // 添加类型信息
        span: Span,
    },
    If {
        condition: Box<TypedExpr<'hir>>,
        then_branch: TypedBlock<'hir>,
        else_branch: Option<Box<TypedExpr<'hir>>>,
        ty: Ty<'hir>,
        span: Span,
    },
    Loop {
        body: TypedBlock<'hir>,
        ty: Ty<'hir>,
        span: Span,
    },
    FieldAccess {
        base: Box<TypedExpr<'hir>>,
        field: TypedField<'hir>,
        def_id: DefId, // 字段的DefId
        ty: Ty<'hir>,
        span: Span,
    },
    Index {
        indexed: Box<TypedExpr<'hir>>,
        index: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },
    Tuple {
        elements: Vec<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },
    Unit {
        ty: Ty<'hir>,
        span: Span,
    },

    /* ---------- 范围 ---------- */
    To {
        start: Box<TypedExpr<'hir>>,
        end: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },
    ToEq {
        start: Box<TypedExpr<'hir>>,
        end: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },

    /* ---------- 分组 ---------- */
    Grouped {
        expr: Box<TypedExpr<'hir>>,
        ty: Ty<'hir>,
        span: Span,
    },

    StructInit {
        def_id: DefId,
        fields: Vec<(StringId, TypedExpr<'hir>)>,
        ty: Ty<'hir>,
        span: Span,
    },

    Cast {
        expr: Box<TypedExpr<'hir>>,
        kind: CastKind,
        ty: Ty<'hir>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum CastKind {
    // 无需转换
    Identity, // 相同类型

    // 整数
    SignExtend, // i8 -> i32
    ZeroExtend, // u8 -> u32
    Truncate,   // i32 -> i8

    // 浮点
    IntToFloat,   // i32 -> f64
    UintToFloat,  // u32 -> f64
    FloatToInt,   // f64 -> i32 (截断)
    FloatToUint,  // f64 -> u32 (截断)
    FloatPromote, // f32 -> f64
    FloatDemote,  // f64 -> f32

    // 指针
    PtrToPtr, // *T -> *U
    PtrToInt, // *T -> usize
    IntToPtr, // usize -> *T

    Bitcast, // 直接转换
}

// ========================
// 语句
// ========================

#[derive(Debug, Clone)]
pub enum TypedStmt<'hir> {
    Expr(Box<TypedExpr<'hir>>),
    Let {
        mutable: Mutability,
        name: StringId,
        def_id: DefId, // 变量的 DefId
        ty: Ty<'hir>,  // 声明的类型
        init: Option<Box<TypedExpr<'hir>>>,
        span: Span,
    },
    Return {
        value: Option<Box<TypedExpr<'hir>>>,
        span: Span,
    },
    Break {
        value: Option<Box<TypedExpr<'hir>>>,
        ty: Ty<'hir>,
        span: Span,
    },
    Continue {
        ty: Ty<'hir>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct TypedBlock<'hir> {
    pub stmts: Vec<TypedStmt<'hir>>,
    pub tail: Option<Box<TypedExpr<'hir>>>,
    pub ty: Ty<'hir>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypedParam<'hir> {
    pub name: StringId,
    pub def_id: DefId,
    pub ty: Ty<'hir>,
    pub span: Span,
}

#[derive(Debug)]
pub enum TypedItem<'hir> {
    Function {
        def_id: DefId,
        visibility: Visibility,
        name: StringId,
        params: Vec<TypedParam<'hir>>,
        return_ty: Ty<'hir>,
        body: TypedBlock<'hir>,
        span: Span,
    },
    Struct {
        def_id: DefId,
        visibility: Visibility,
        name: StringId,
        fields: Vec<TypedField<'hir>>,
        span: Span,
    },
    Use {
        visibility: Visibility,
        alias: StringId,
        target: DefId,
        span: Span,
    },
    Module {
        def_id: DefId,
        visibility: Visibility,
        name: StringId,
        items: Vec<TypedItem<'hir>>,
        span: Span,
    },
    Extern {
        visibility: Visibility,
        abi: AbiType,
        items: Vec<TypedExternItem<'hir>>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum TypedExternItem<'hir> {
    Function {
        def_id: DefId,
        name: StringId,
        params: Vec<TypedParam<'hir>>,
        is_variadic: bool,
        return_ty: Ty<'hir>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct TypedField<'hir> {
    pub name: StringId,
    pub def_id: DefId,
    pub ty: Ty<'hir>,
    pub visibility: Visibility,
    pub index: u32,
    pub span: Span,
}
