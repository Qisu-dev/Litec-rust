use litec_span::{Span, StringId};
use litec_typed_hir::def_id::DefId;

// 重新导出常用小类型，避免重复定义
pub use litec_hir::{AbiType, AssignOp, BinOp, LiteralValue, Mutability, PosOp, UnOp, Visibility};

#[derive(Debug, Clone)]
pub struct RCrate {
    pub items: Vec<RItem>,
}

#[derive(Debug, Clone)]
pub enum RItem {
    Function {
        def_id: DefId, // 函数自身
        visibility: Visibility,
        name: StringId, // 保留原名字符串（用于报错/文档）
        params: Vec<RParam>,
        return_type: Option<RType>,
        body: RBlock,
        span: Span,
    },
    Struct {
        def_id: DefId,
        visibility: Visibility,
        name: StringId,
        fields: Vec<RField>,
        span: Span,
    },
    Use {
        visibility: Visibility,
        alias: StringId, // use foo as bar 中的 bar
        target: DefId,   // 已解析路径
        span: Span,
    },
    Extern {
        visibility: Visibility,
        abi: AbiType,
        items: Vec<RExternItem>,
        span: Span,
    },
    Module {
        def_id: DefId,
        visibility: Visibility,
        name: StringId,
        items: Vec<RItem>,
        span: Span,
    },
}
#[derive(Debug, Clone)]
pub enum RExternItem {
    Function {
        def_id: DefId,
        name: StringId,
        is_variadic: bool,
        params: Vec<RParam>,
        return_type: Option<RType>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum RType {
    Unknown,
    Named {
        id: DefId,
        span: Span,
    },
    Generic {
        id: DefId,
        args: Vec<RType>,
        span: Span,
    },
    Tuple {
        elements: Vec<RType>,
        span: Span,
    },
    Reference {
        mutable: Mutability,
        target: Box<RType>,
        span: Span,
    },
    Pointer {
        mutable: Mutability,
        target: Box<RType>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct RParam {
    pub def_id: DefId,
    pub name: StringId,
    pub ty: RType,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RField {
    pub def_id: DefId,
    pub name: StringId,
    pub ty: RType,
    pub visibility: Visibility,
    pub index: u32,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RBlock {
    pub stmts: Vec<RStmt>,
    pub tail: Option<Box<RExpr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum RStmt {
    Expr(Box<RExpr>),
    Let {
        mutable: Mutability,
        name: StringId,
        def_id: DefId, // 局部变量也占一个 DefId，方便后续捕捉/生命周期
        ty: Option<RType>,
        value: Option<Box<RExpr>>,
        span: Span,
    },
    Return {
        value: Option<Box<RExpr>>,
        span: Span,
    },
    Break {
        value: Option<Box<RExpr>>,
        span: Span,
    },
    Continue {
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum RExpr {
    /* ---------- 原子 ---------- */
    Literal {
        value: LiteralValue,
        span: Span,
    },
    Local {
        def_id: DefId,
        span: Span,
    }, // 局部变量
    Global {
        def_id: DefId,
        span: Span,
    }, // 全局路径（已解析）

    /* ---------- 运算 ---------- */
    Binary {
        left: Box<RExpr>,
        op: BinOp,
        right: Box<RExpr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        operand: Box<RExpr>,
        span: Span,
    },
    Postfix {
        operand: Box<RExpr>,
        op: PosOp,
        span: Span,
    },
    Assign {
        target: Box<RExpr>,
        op: AssignOp,
        value: Box<RExpr>,
        span: Span,
    },
    AddressOf {
        expr: Box<RExpr>,
        span: Span,
    },
    Dereference {
        expr: Box<RExpr>,
        span: Span,
    },

    /* ---------- 复合 ---------- */
    Call {
        callee: DefId,
        args: Vec<RExpr>,
        span: Span,
    },
    Block {
        block: RBlock,
    },
    If {
        condition: Box<RExpr>,
        then_branch: RBlock,
        else_branch: Option<Box<RExpr>>,
        span: Span,
    },
    Loop {
        body: Box<RBlock>,
        span: Span,
    },
    FieldAccess {
        base: Box<RExpr>,
        field: RField, // 保留字符串，用于私有字段检查
        def_id: DefId,
        span: Span,
    },
    Index {
        indexed: Box<RExpr>,
        index: Box<RExpr>,
        span: Span,
    },
    Tuple {
        elements: Vec<RExpr>,
        span: Span,
    },
    Unit {
        span: Span,
    },

    /* ---------- 范围 ---------- */
    To {
        start: Box<RExpr>,
        end: Box<RExpr>,
        span: Span,
    },
    ToEq {
        start: Box<RExpr>,
        end: Box<RExpr>,
        span: Span,
    },

    /* ---------- 分组 ---------- */
    Grouped {
        expr: Box<RExpr>,
        span: Span,
    },

    StructInit {
        def_id: DefId,
        fields: Vec<(StringId, RExpr)>,
        span: Span,
    },
    Cast {
        expr: Box<RExpr>,
        ty: RType,
        span: Span,
    },
}

// 更通用的枚举span实现宏，支持多种特殊处理变体
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
    RExpr,
    [
        Literal,
        Local,
        Global,
        Binary,
        Unary,
        Postfix,
        Assign,
        Call,
        If,
        Loop,
        FieldAccess,
        Index,
        Tuple,
        Unit,
        To,
        ToEq,
        Grouped,
        AddressOf,
        Dereference,
        StructInit,
        Cast
    ],
     (
        // 特殊处理：Block 直接返回 span
        RExpr::Block { block: RBlock { span, .. } } => *span,
    )
);

impl_span_for_enum!(
    RStmt,
    [
        Let,
        Return,
        Break,
        Continue,
    ],
    (
        RStmt::Expr(expr) => expr.span(),
    )
);

impl_span_for_enum!(RItem, [Function, Struct, Extern, Use, Module], ());
