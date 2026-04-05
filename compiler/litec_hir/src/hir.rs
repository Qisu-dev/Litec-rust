use index_vec::IndexVec;
use litec_ast::ast::{
    AssignOpKind, BinOpKind, Ident, Lit, Mutability, RangeLimits, UnOp, Visibility,
};
use litec_span::Span;
use litec_span::id::{DefId, HirId, ItemLocalId, LocalDefId, OwnerId};

use crate::def::Res;

#[derive(Debug, Clone)]
pub struct Crate<'hir> {
    pub owners: IndexVec<LocalDefId, IndexVec<ItemLocalId, Node<'hir>>>,
}

impl<'hir> Crate<'hir> {
    pub fn new() -> Self {
        Self {
            owners: IndexVec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PathSegment<'hir> {
    pub ident: Ident,
    pub generic_args: Option<&'hir GenericArgs<'hir>>,
    pub span: Span,
}

/// 解析后的完整路径。
#[derive(Debug, Clone)]
pub struct Path<'hir, R = Res> {
    pub res: R,
    pub segments: &'hir [PathSegment<'hir>],
    pub span: Span,
}

/// 表达式节点。
#[derive(Debug, Clone)]
pub struct Expr<'hir> {
    pub hir_id: HirId,
    pub span: Span,
    pub kind: ExprKind<'hir>,
}

/// 表达式种类。
#[derive(Debug, Clone)]
pub enum ExprKind<'hir> {
    Binary(&'hir Expr<'hir>, BinOpKind, &'hir Expr<'hir>),
    Unary(UnOp, &'hir Expr<'hir>),
    Literal(Lit),
    Path(&'hir Path<'hir>),
    Grouped(&'hir Expr<'hir>),
    Assignment(&'hir Expr<'hir>, &'hir Expr<'hir>),
    AssignmentWithOp(&'hir Expr<'hir>, AssignOpKind, &'hir Expr<'hir>),
    Call(&'hir Expr<'hir>, Vec<&'hir Expr<'hir>>),
    Block(&'hir Block<'hir>),
    If(
        &'hir Expr<'hir>,
        &'hir Block<'hir>,
        Option<&'hir Expr<'hir>>,
    ),
    While(&'hir Expr<'hir>, &'hir Block<'hir>),
    For {
        variable: Ident,
        iter: &'hir Expr<'hir>,
        body: &'hir Block<'hir>,
    },
    Index(&'hir Expr<'hir>, &'hir Expr<'hir>),
    Range(
        Option<&'hir Expr<'hir>>,
        Option<&'hir Expr<'hir>>,
        RangeLimits,
    ),
    Loop(&'hir Block<'hir>),
    Field(&'hir Expr<'hir>, Ident),
    Bool(bool),
    Tuple(Vec<&'hir Expr<'hir>>),
    Unit,
    AddressOf(&'hir Expr<'hir>),
    StructExpr(StructExpr<'hir>),
    Cast(&'hir Expr<'hir>, &'hir Ty<'hir>),
}

/// 结构体初始化表达式。
#[derive(Debug, Clone)]
pub struct StructExpr<'hir> {
    pub path: &'hir Path<'hir>,
    pub fields: Vec<StructExprField<'hir>>,
}

/// 结构体初始化字段。
#[derive(Debug, Clone)]
pub struct StructExprField<'hir> {
    pub name: Ident,
    pub value: &'hir Expr<'hir>,
    pub is_shorthand: bool,
    pub span: Span,
}

/// 语句节点。
#[derive(Debug, Clone)]
pub struct Stmt<'hir> {
    pub hir_id: HirId,
    pub span: Span,
    pub kind: StmtKind<'hir>,
}

/// 语句种类。
#[derive(Debug, Clone)]
pub enum StmtKind<'hir> {
    Expr(&'hir Expr<'hir>),
    Semi(&'hir Expr<'hir>),
    Let(
        Mutability,
        Ident,
        Option<&'hir Ty<'hir>>,
        Option<&'hir Expr<'hir>>,
    ),
    Return(Option<&'hir Expr<'hir>>),
    Continue,
    Break(Option<&'hir Expr<'hir>>),
}

/// 块节点。
#[derive(Debug, Clone)]
pub struct Block<'hir> {
    pub hir_id: HirId,
    pub stmts: Vec<&'hir Stmt<'hir>>,
    pub tail: Option<&'hir Expr<'hir>>,
    pub span: Span,
}

/// 类型节点。
#[derive(Debug, Clone)]
pub struct Ty<'hir> {
    pub hir_id: HirId,
    pub span: Span,
    pub kind: TyKind<'hir>,
}

/// 类型种类。
#[derive(Debug, Clone)]
pub enum TyKind<'hir> {
    Path(&'hir Path<'hir>),
    Never,
    Unit,
    Ref {
        mutability: Mutability,
        ty: &'hir Ty<'hir>,
    },
    Ptr {
        mutability: Mutability,
        ty: &'hir Ty<'hir>,
    },
    Array {
        elem: &'hir Ty<'hir>,
        len: &'hir Expr<'hir>,
    }, // len 是常量表达式
    Slice {
        elem: &'hir Ty<'hir>,
    },
    Tuple {
        elems: Vec<&'hir Ty<'hir>>,
    },
    FnPtr {
        inputs: Vec<&'hir Ty<'hir>>,
        output: &'hir Ty<'hir>,
    },
    Infer,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PrimTy {
    Int(IntTy),
    Uint(UintTy),
    Float(FloatTy),
    Str,
    Bool,
    Char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntTy {
    Isize,
    I8,
    I16,
    I32,
    I64,
    I128,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum UintTy {
    Usize,
    U8,
    U16,
    U32,
    U64,
    U128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FloatTy {
    F16,
    F32,
    F64,
    F128,
}

/// 项节点。
#[derive(Debug, Clone)]
pub struct Item<'hir> {
    pub hir_id: HirId,
    pub def_id: DefId,
    pub visibility: Visibility,
    pub span: Span,
    pub kind: ItemKind<'hir>,
}

/// 项种类。
#[derive(Debug, Clone)]
pub enum ItemKind<'hir> {
    Fn(Fn<'hir>),
    Struct(Ident, &'hir GenericParams<'hir>, Vec<Field<'hir>>),
    Use(&'hir UsePath<'hir>, UseKind),
    Extern(Extern<'hir>),
    Module(Ident, &'hir Mod<'hir>),
}

/// 函数项。
#[derive(Debug, Clone)]
pub struct Fn<'hir> {
    pub sig: FnSig<'hir>,
    pub body: &'hir Block<'hir>,
}

/// 函数签名。
#[derive(Debug, Clone)]
pub struct FnSig<'hir> {
    pub name: Ident,
    pub generics: &'hir GenericParams<'hir>,
    pub params: Vec<&'hir Param<'hir>>,
    pub return_type: &'hir Ty<'hir>,
    pub is_variadic: bool,
}

/// 函数参数。
#[derive(Debug, Clone)]
pub struct Param<'hir> {
    pub hir_id: HirId,
    pub name: Ident,
    pub ty: &'hir Ty<'hir>,
    pub span: Span,
}

/// 结构体字段。
#[derive(Debug, Clone)]
pub struct Field<'hir> {
    pub hir_id: HirId,
    pub name: Ident,
    pub ty: &'hir Ty<'hir>,
    pub visibility: Visibility,
    pub index: u32,
    pub span: Span,
}

pub type UsePath<'hir> = Path<'hir>;

/// use 树种类。
#[derive(Debug, Clone)]
pub enum UseKind {
    /// 单个路径
    /// 多个路径会被展开为多个单个路径,
    /// 例如 `use a::{b, c}` 会被展开为 `use a::b; use a::c;`
    /// ident指名称,如果有别名是别名否则是本名
    /// 如 `use foo::bar as baz` ident是baz, `use foo::bar` ident是bar
    Single(Ident),
    /// 通配符
    /// 如 `use foo::*`
    Glob,
}

/// extern 块。
#[derive(Debug, Clone)]
pub struct Extern<'hir> {
    pub abi: Option<Ident>,
    pub items: Vec<Item<'hir>>,
}

#[derive(Debug, Clone)]
pub struct Mod<'hir> {
    pub items: &'hir [ItemId],
}

/// 所有 HIR 节点的统一枚举，用于在 `GlobalCtxt` 中存储。
#[derive(Debug, Clone)]
pub enum Node<'hir> {
    Expr(&'hir Expr<'hir>),
    Stmt(&'hir Stmt<'hir>),
    Item(&'hir Item<'hir>),
    Ty(&'hir Ty<'hir>),
    Block(&'hir Block<'hir>),
    Param(&'hir Param<'hir>),
    Field(&'hir Field<'hir>),
}

impl<'hir> Node<'hir> {
    /// 获取节点对应的 `HirId`。
    pub fn hir_id(&self) -> HirId {
        match self {
            Node::Expr(e) => e.hir_id,
            Node::Stmt(s) => s.hir_id,
            Node::Item(i) => i.hir_id,
            Node::Ty(t) => t.hir_id,
            Node::Block(b) => b.hir_id,
            Node::Param(p) => p.hir_id,
            Node::Field(f) => f.hir_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenericParams<'hir> {
    pub params: &'hir [GenericParam],
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub hir_id: HirId,
    pub def_id: DefId,
    pub name: Ident,
    pub kind: GenericParamKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum GenericParamKind {
    Ty,
}

#[derive(Debug, Clone)]
pub struct GenericArgs<'hir> {
    pub args: &'hir [GenericArg<'hir>],
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum GenericArg<'hir> {
    Type(&'hir Ty<'hir>),
}

#[derive(Debug, Clone, Copy)]
pub struct ItemId {
    pub owner_id: OwnerId,
}
