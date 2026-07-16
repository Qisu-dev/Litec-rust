use std::marker::PhantomData;

use crate::def::Res;
use litec_ast::ast::{AssignOp, BinOp, Ident, Lit, Mutability, RangeLimits, UnOp, Visibility};
use litec_span::Span;
use litec_span::id::{DefId, HirId, OwnerId};

#[derive(Debug, Clone)]
pub struct Crate<'hir> {
    pub items: &'hir [&'hir Item<'hir>],
}

#[derive(Debug, Clone)]
pub struct PathSegment<'hir> {
    pub ident: Ident,
    pub generic_args: Option<&'hir GenericArgs<'hir>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Path<'hir, R = Res> {
    pub res: R,
    pub segments: &'hir [&'hir PathSegment<'hir>],
    pub span: Span,
}

/// 表达式节点。
#[derive(Debug, Clone)]
pub struct Expr<'hir> {
    pub hir_id: HirId,
    pub span: Span,
    pub kind: &'hir ExprKind<'hir>,
}

/// 表达式种类。
#[derive(Debug, Clone)]
pub enum ExprKind<'hir> {
    Binary(&'hir Expr<'hir>, BinOp, &'hir Expr<'hir>),
    Unary(UnOp, &'hir Expr<'hir>),
    Literal(Lit),
    QPath(&'hir QPath<'hir>),
    Grouped(&'hir Expr<'hir>),
    Assignment(&'hir Expr<'hir>, &'hir Expr<'hir>),
    AssignmentWithOp(&'hir Expr<'hir>, AssignOp, &'hir Expr<'hir>),
    Call(&'hir Expr<'hir>, &'hir [&'hir Expr<'hir>]),
    Block(&'hir Block<'hir>),
    If(
        &'hir Expr<'hir>,
        &'hir Block<'hir>,
        Option<&'hir Expr<'hir>>,
    ),
    While(&'hir Expr<'hir>, &'hir Block<'hir>),
    For {
        variable: &'hir Pat<'hir>,
        iter: &'hir Expr<'hir>,
        body: &'hir Block<'hir>,
    },
    Index(&'hir Expr<'hir>, &'hir Expr<'hir>),
    Range(&'hir Expr<'hir>, &'hir Expr<'hir>, RangeLimits),
    Loop(&'hir Block<'hir>),
    Field(&'hir Expr<'hir>, Ident),
    Bool(bool),
    Tuple(&'hir [&'hir Expr<'hir>]),
    Unit,
    AddressOf(&'hir Expr<'hir>),
    StructExpr(&'hir StructExpr<'hir>),
    Cast(&'hir Expr<'hir>, &'hir Ty<'hir>),
    Match(&'hir Expr<'hir>, &'hir [&'hir Arm<'hir>]),
    Return(Option<&'hir Expr<'hir>>),
    Continue,
    Break(Option<&'hir Expr<'hir>>),
}

#[derive(Debug, Clone)]
pub enum QPath<'hir> {
    /// 完全解析的路径（如变量、函数、模块）
    Resolved(&'hir Path<'hir>),
    /// 类型限定路径，例如 `Type::name`，其中 Type 已解析为 Res，name 还需要查找
    TypeRelative(Res, &'hir [&'hir PathSegment<'hir>]),
}

/// 结构体初始化表达式。
#[derive(Debug, Clone)]
pub struct StructExpr<'hir> {
    pub path: &'hir Path<'hir>,
    pub fields: &'hir [&'hir StructExprField<'hir>],
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
    pub kind: &'hir StmtKind<'hir>,
}

/// 语句种类。
#[derive(Debug, Clone)]
pub enum StmtKind<'hir> {
    Expr(&'hir Expr<'hir>),
    Semi(&'hir Expr<'hir>),
    Let(
        &'hir Pat<'hir>,
        Option<&'hir Ty<'hir>>,
        Option<&'hir Expr<'hir>>,
    ),
    Defer(&'hir Expr<'hir>),
}

/// 块节点。
#[derive(Debug, Clone)]
pub struct Block<'hir> {
    pub hir_id: HirId,
    pub stmts: &'hir [&'hir Stmt<'hir>],
    pub span: Span,
}

/// 类型节点。
#[derive(Debug, Clone)]
pub struct Ty<'hir> {
    pub hir_id: HirId,
    pub span: Span,
    pub kind: &'hir TyKind<'hir>,
}

/// 类型种类。
#[derive(Debug, Clone)]
pub enum TyKind<'hir> {
    QPath(&'hir QPath<'hir>),
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
        elems: &'hir [&'hir Ty<'hir>],
    },
    FnPtr {
        inputs: &'hir [&'hir Ty<'hir>],
        output: &'hir Ty<'hir>,
    },
    SelfTy,
    Infer,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PrimTy {
    Int(IntTy),
    Uint(UintTy),
    Float(FloatTy),
    Bool,
    Char,
    Str,
    Unit,
    Never,
}

impl PrimTy {
    pub fn is_numeric(&self) -> bool {
        matches!(self, PrimTy::Int(_) | PrimTy::Uint(_) | PrimTy::Float(_))
    }
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
    F32,
    F64,
}

/// 项节点。
#[derive(Debug, Clone)]
pub struct Item<'hir, K = ItemKind<'hir>> {
    pub hir_id: HirId,
    pub def_id: DefId,
    pub visibility: Visibility,
    pub span: Span,
    pub kind: K,
    _marker: PhantomData<&'hir ()>,
}

impl<'hir, K> Item<'hir, K> {
    pub fn new(hir_id: HirId, def_id: DefId, visibility: Visibility, span: Span, kind: K) -> Self {
        Self {
            hir_id,
            def_id,
            visibility,
            span,
            kind,
            _marker: PhantomData,
        }
    }
}

/// 项种类。
#[derive(Debug, Clone)]
pub enum ItemKind<'hir> {
    Fn(&'hir Fn<'hir>),
    Struct(Ident, &'hir Generics<'hir>, &'hir StructKind<'hir>),
    Use(&'hir UsePath<'hir>, UseKind),
    ForeignMod(&'hir ForeignMod<'hir>),
    Module(Ident, &'hir Mod<'hir>),
    Impl(&'hir Impl<'hir>),
    Trait(Ident, &'hir Generics<'hir>, &'hir [&'hir TraitItem<'hir>]),
    TypeAlias(&'hir TypeAlias<'hir>),
    Enum(Ident, &'hir Generics<'hir>, &'hir [&'hir Variant<'hir>]),
}

#[derive(Debug, Clone)]
pub struct Variant<'hir> {
    pub hir_id: HirId,
    pub def_id: DefId,
    pub name: Ident,
    pub data: VariantData<'hir>,
    pub ctor_def_id: Option<DefId>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum VariantData<'hir> {
    Unit,
    Tuple(&'hir [&'hir Ty<'hir>]),
    Struct(&'hir [&'hir Field<'hir>]),
}

#[derive(Debug, Clone)]
pub struct TypeAlias<'hir> {
    pub name: Ident,
    pub generics: &'hir Generics<'hir>,
    pub ty: &'hir Ty<'hir>,
}

#[derive(Debug, Clone)]
pub enum TraitItemKind<'hir> {
    Fn(&'hir FnSig<'hir>),
}

pub type TraitItem<'hir> = Item<'hir, TraitItemKind<'hir>>;

#[derive(Debug, Clone)]
pub struct Impl<'hir> {
    pub generics: &'hir Generics<'hir>,
    pub of_trait: Option<&'hir Path<'hir>>,
    pub self_ty: &'hir Ty<'hir>,
    pub items: &'hir [(DefId, &'hir ImplItemKind<'hir>)],
}

#[derive(Debug, Clone)]
pub enum ImplItemKind<'hir> {
    Fn(&'hir Fn<'hir>),
    TypeAlias(Ident, &'hir Generics<'hir>, &'hir Ty<'hir>),
}

#[derive(Debug, Clone)]
pub enum StructKind<'hir> {
    Unit,
    Tuple(&'hir [&'hir Ty<'hir>]),     // 元组字段类型列表
    Struct(&'hir [&'hir Field<'hir>]), // 命名字段列表
}

/// 函数项。
#[derive(Debug, Clone)]
pub struct Fn<'hir> {
    pub sig: &'hir FnSig<'hir>,
    pub body: &'hir Block<'hir>,
}

/// 函数签名。
#[derive(Debug, Clone)]
pub struct FnSig<'hir> {
    pub name: Ident,
    pub generics: &'hir Generics<'hir>,
    pub params: &'hir [&'hir Param<'hir>],
    pub return_type: &'hir Ty<'hir>,
    pub is_variadic: bool,
}

#[derive(Debug, Clone)]
pub struct Param<'hir> {
    pub hir_id: HirId,
    pub pat: &'hir Pat<'hir>,
    pub ty: &'hir Ty<'hir>,
    pub span: Span,
    pub is_self: bool,
    pub self_kind: Option<SelfKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    Value,   // self
    Pointer, // *self
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

#[derive(Debug, Clone)]
pub struct ForeignItem<'hir> {
    pub hir_id: HirId,
    pub def_id: DefId,
    pub name: Ident,
    pub vis: Visibility,
    pub span: Span,
    pub kind: ForeignItemKind<'hir>,
}

#[derive(Debug, Clone)]
pub enum ForeignItemKind<'hir> {
    /// 外部函数声明
    Fn(&'hir FnSig<'hir>),
}

/// extern 块
#[derive(Debug, Clone)]
pub struct ForeignMod<'hir> {
    pub abi: Option<Ident>,
    pub items: &'hir [&'hir ForeignItem<'hir>],
}

#[derive(Debug, Clone)]
pub struct Mod<'hir> {
    pub items: &'hir [&'hir Item<'hir>],
}

#[derive(Debug, Clone)]
pub enum Node<'hir> {
    Expr(&'hir Expr<'hir>),
    Stmt(&'hir Stmt<'hir>),
    Item(&'hir Item<'hir>),
    Ty(&'hir Ty<'hir>),
    Block(&'hir Block<'hir>),
    Param(&'hir Param<'hir>),
    Field(&'hir Field<'hir>),
    GenericParam(&'hir GenericParam<'hir>),
    Pat(&'hir Pat<'hir>),
    Arm(&'hir Arm<'hir>),
    Variant(&'hir Variant<'hir>),
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
            Node::GenericParam(p) => p.hir_id,
            Node::Pat(p) => p.hir_id,
            Node::Arm(a) => a.hir_id,
            Node::Variant(v) => v.hir_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Generics<'hir> {
    pub params: &'hir [&'hir GenericParam<'hir>],
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct GenericParam<'hir> {
    pub hir_id: HirId,
    pub def_id: DefId,
    pub name: Ident,
    pub kind: GenericParamKind,
    pub bounds: Option<&'hir Bounds<'hir>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Bounds<'hir> {
    pub bounds: &'hir [&'hir Path<'hir>],
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum GenericParamKind {
    Ty,
}

#[derive(Debug, Clone)]
pub struct GenericArgs<'hir> {
    pub args: &'hir [&'hir GenericArg<'hir>],
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum GenericArg<'hir> {
    Type(&'hir Ty<'hir>),
}

/// 模式节点（用于 let、match 等）
#[derive(Debug, Clone)]
pub struct Pat<'hir> {
    pub hir_id: HirId,
    pub span: Span,
    pub kind: &'hir PatKind<'hir>,
}

/// 模式种类
#[derive(Debug, Clone)]
pub enum PatKind<'hir> {
    /// 通配符 `_`
    Wild,
    /// 绑定变量 `x` 或 `mut x`
    Ident(Mutability, Ident),
    /// 元组模式 `(a, b)`
    Tuple(&'hir [&'hir Pat<'hir>]),
    /// 结构体模式 `S { field: pat, .. }`
    Struct(
        &'hir Path<'hir>,
        &'hir [&'hir StructFieldPat<'hir>],
        bool, /* has_rest */
    ),
    /// 枚举变体模式 `E::Variant(pat)` 或 `E::Variant { field: pat }`
    Enum(&'hir Path<'hir>, Option<&'hir Pat<'hir>>), // 简化：仅支持单元/元组变体，结构体变体可扩展
    /// 字面量模式 `42` 或 `"hello"`
    Lit(Lit),
    /// 范围模式 `a..b` `a..=b`
    Range(&'hir Expr<'hir>, &'hir Expr<'hir>, RangeLimits),
    /// 或者模式 `p1 | p2`
    Or(&'hir [&'hir Pat<'hir>]),
}

/// 结构体字段模式 `field: pat` 或 `field` (简写)
#[derive(Debug, Clone)]
pub struct StructFieldPat<'hir> {
    pub name: Ident,
    pub pat: &'hir Pat<'hir>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Arm<'hir> {
    pub hir_id: HirId,
    pub pat: &'hir Pat<'hir>,            // 模式
    pub guard: Option<&'hir Expr<'hir>>, // 守卫条件 (如 `if x > 0`)
    pub body: &'hir Expr<'hir>,          // 匹配体
}

#[derive(Debug, Clone, Copy)]
pub struct ItemId {
    pub owner_id: OwnerId,
}
