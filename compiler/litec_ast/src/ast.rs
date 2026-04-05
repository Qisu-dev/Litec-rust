use crate::{
    token::LiteralKind,
    util::{accos_op::Fixity, precedence::Precedence},
};
use litec_span::{Span, Spanned, StringId};

index_vec::define_index_type! {
    pub struct NodeId = u32;
    DEBUG_FORMAT = "Node({})";
}

pub const DUMMY_NODE_ID: NodeId = NodeId::from_raw_unchecked(u32::MAX);

#[derive(Debug, Clone)]
pub struct Attr {
    pub path: Path,          // 如 `lang`
    pub arg: Option<StrLit>, // 参数，如 `"add"`
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Crate {
    pub node_id: NodeId,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Inherited,
}

#[derive(Debug, Clone)]
pub struct Item<K = ItemKind> {
    pub node_id: NodeId,
    pub attr: Option<Attr>,
    pub visibility: Visibility,
    pub span: Span,
    pub kind: K,
}

#[derive(Debug, Clone)]
pub enum ItemKind {
    /// 一个函数声明
    /// 例如 `fn foo<T>() -> T`
    Fn(Fn),
    /// 一个结构体声明
    /// 例如 `struct Foo<A> { x: A }`
    Struct(Ident, GenericParams, Vec<Field>),
    /// 一个使用声明
    /// e.g. `use foo;` `use foo::bar;` `use foo::bar as FooBar;`
    Use(UseTree),
    /// 一个模块声明
    /// 例如 `extern "C" { ... }` `extern { ... }`
    Extern(Extern),
    /// 一个模块声明
    /// 例如 `mod foo;` `mod foo { ... }`
    Module(Ident, Inline),
    /// 一个实现
    /// 例如 `impl Foo { ... }` `impl<T> Foo<T> { ... }`
    Impl(Impl),
    /// 一个特征
    /// 例如 `trait Foo { ... }`
    Trait(Ident, Vec<TraitItem>) ,
    /// 一个类型别名
    /// 例如 `type foo = i32;`
    TypeAlias(TypeAlias),
}

pub type TraitItem = Item<TraitItemKind>;

#[derive(Debug, Clone)]
pub enum TraitItemKind {
    Fn(Fn),
}

#[derive(Debug, Clone)]
pub struct Impl {
    pub node_id: NodeId,
    pub generics: GenericParams,
    pub of_trait: Option<Path>,
    pub self_ty: Box<Ty>,
    pub items: Vec<ImplItem>,
}

pub type ImplItem = Item<ImplItemKind>;

#[derive(Debug, Clone)]
pub enum ImplItemKind {
    Fn(Fn),          // 方法定义
    Type(TypeAlias), // 关联类型（trait 实现中）
}

#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub node_id: NodeId,
    pub ident: Ident,
    pub generics: GenericParams,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub enum Inline {
    Inline(Vec<Item>),
    External(Vec<Item>),
}

#[derive(Debug, Clone)]
pub struct Fn {
    pub node_id: NodeId,
    pub sig: FnSig,
    pub body: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct FnSig {
    pub name: Ident,
    pub generics: GenericParams,
    pub params: Vec<Param>,
    pub return_type: FnRetTy,
    pub is_variadic: bool,
}

#[derive(Debug, Clone)]
pub struct Extern {
    pub node_id: NodeId,
    pub abi: Option<Ident>,
    pub items: Vec<ExternItem>,
}

pub type ExternItem = Item<ExternItemKind>;

#[derive(Debug, Clone)]
pub enum ExternItemKind {
    /// 一个外部函数声明
    Fn(Fn),
}

#[derive(Debug, Clone)]
pub struct UseTree {
    pub node_id: NodeId,
    pub prefix: Path,
    pub kind: UseTreeKind,
    /// 指向整个UseTree
    /// 例如 `use foo::{bar, baz};`
    ///   span -> ^^^^^^^^^^^^^^^
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum UseTreeKind {
    /// 例如 `use foo;` `use foo as rename;`
    Simple(Option<Ident>),
    /// 例如
    /// ```text
    /// use foo::{bar, baz};`
    ///  span -> ^^^^^^^^^^
    /// ```
    Nested(Vec<UseTree>, Span),
    /// 例如 `use foo::*;`
    Glob,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub node_id: NodeId,
    pub name: Ident,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub node_id: NodeId,
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub node_id: NodeId,
    pub name: Ident,
    pub ty: Ty,
    pub visibility: Visibility,
    pub index: u32,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub node_id: NodeId,
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    /// 二元运算符
    /// 例如 `1 + 2`
    Binary(Box<Expr>, BinOp, Box<Expr>),
    /// 一元运算符
    /// 例如 `!true`
    Unary(UnOp, Box<Expr>),
    Literal(Lit),
    /// 用括号包裹的表达式
    /// 例如 `(1 + 2)`
    Grouped(Box<Expr>),
    /// 普通赋值
    /// 比如 `x = 0;`
    Assignment(Box<Expr>, Box<Expr>),
    /// 带运算符的赋值
    /// 比如 `x += 0;`
    AssignmentWithOp(Box<Expr>, AssignOp, Box<Expr>),
    /// 函数调用
    /// 例如 `foo(1, 2)`
    Call(Box<Expr>, Vec<Expr>),
    /// 块表达式
    /// 例如 `{ foo }`
    Block(Box<Block>),
    /// 条件表达式
    /// 例如 `if true { 1 } else { 2 }`
    If(Box<Expr>, Block, Option<Box<Expr>>),
    /// while循环
    /// 例如 `while true { 1 }`
    While(Box<Expr>, Box<Block>),
    /// for循环
    /// 例如 `for i in 0..10 { 1 }`
    For {
        mutability: Mutability,
        variable: Ident,
        iter: Box<Expr>,
        body: Box<Block>,
    },
    /// 索引
    /// 例如 `foo[1]`
    Index(Box<Expr>, Box<Expr>),
    /// 范围
    /// 例如 `1..2` `1..=2`
    Range(Box<Expr>, Box<Expr>, RangeLimits),
    /// 无限循环
    /// 例如 `loop { 1 }`
    Loop(Box<Block>),
    /// 成员访问
    /// 例如 `foo.bar`
    Field(Box<Expr>, Ident),
    /// 路径访问
    /// 例如 `foo::bar`
    Path(Path),
    /// bool 表达式
    /// 例如 `true` `false`
    Bool(bool),
    /// 元组表达式
    /// 例如 `(1, 2)`
    Tuple(Vec<Expr>),
    /// 空值
    /// 表现为 ()
    Unit,
    /// 取地址
    /// 例如 `&foo`
    AddressOf(Box<Expr>),
    StructExpr(StructExpr),
    Cast(Box<Expr>, Box<Ty>),
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub node_id: NodeId,
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Expr(Box<Expr>),
    Semi(Box<Expr>),
    Let(Mutability, Ident, Option<Box<Ty>>, Option<Box<Expr>>),
    Return(Option<Box<Expr>>),
    Continue,
    Break(Option<Box<Expr>>),
}

#[derive(Debug, Clone)]
pub struct Ty {
    pub node_id: NodeId,
    pub kind: TyKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TyKind {
    /// `std::vec::Vec<T>` `Foo`
    Path { path: Path },

    /// Never 类型 `!`
    Never,
    /// 单元类型 `()`
    Unit,

    /// 引用类型：`&T` 或 `&mut T`
    /// 仅允许函数参数中使用 `&mut T`
    Ref {
        mutability: Mutability, // 不可变/可变
        ty: Box<Ty>,
    },
    /// 原始指针：`*const T` / `*mut T`（unsafe 块内使用）
    Ptr { mutability: Mutability, ty: Box<Ty> },

    /// 数组：`[T; 5]`
    Array {
        elem: Box<Ty>,
        len: Box<Expr>, // 编译时常量表达式
    },
    /// 切片：`[T]`
    Slice { elem: Box<Ty> },
    /// 元组：`(T, U, V)`
    Tuple { elems: Vec<Ty> },

    /// `fn(i32) -> String`
    FnPtr {
        inputs: Vec<Ty>, // 参数类型列表
        output: Box<Ty>, // 返回类型
    },

    /// `_` 用于类型推导
    Infer,
}

#[derive(Debug, Clone)]
pub struct Path {
    pub node_id: NodeId,
    pub segments: Vec<PathSegment>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PathSegment {
    pub node_id: NodeId,
    pub name: Ident,
    pub span: Span,
    pub generic_args: Option<GenericArgs>,
}

#[derive(Debug, Clone)]
pub struct GenericArgs {
    pub args: Vec<GenericArg>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum GenericArg {
    Type(Ty),
    // 未来会有 Const
}

#[derive(Debug, Clone)]
pub struct GenericParams {
    pub node_id: NodeId,
    pub params: Vec<GenericParam>,
    pub span: Span,
}

impl GenericParams {
    pub fn empty() -> Self {
        Self {
            node_id: DUMMY_NODE_ID,
            params: Vec::new(),
            span: Span::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub node_id: NodeId,
    pub name: Ident, // "T", "U"
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct Ident {
    pub text: StringId,
    pub span: Span,
}

impl Ident {
    pub fn to_string(&self) -> String {
        self.text.to_string()
    }

    pub fn to_path(&self) -> Path {
        Path {
            node_id: DUMMY_NODE_ID,
            segments: vec![PathSegment {
                node_id: DUMMY_NODE_ID,
                name: *self,
                span: self.span,
                generic_args: None,
            }],
            span: self.span,
        }
    }
}

impl std::hash::Hash for Ident {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl PartialEq for Ident {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for Ident {}

#[derive(Debug, Clone)]
pub enum FnRetTy {
    // span指向了类型插入的地方
    Default(Span),
    Ty(Ty),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    Mutable,
    Immutable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOpKind {
    /// +
    Add,
    /// -
    Sub,
    /// *
    Mul,
    /// /
    Div,
    /// %
    Rem,
    /// &&
    And,
    /// ||
    Or,
    /// ^
    BitXor,
    /// &
    BitAnd,
    /// |
    BitOr,
    /// <<
    Shl,
    /// >>
    Shr,
    /// ==
    Eq,
    /// <
    Lt,
    /// <=
    Le,
    /// !=
    Ne,
    /// >=
    Ge,
    /// >
    Gt,
}

impl BinOpKind {
    pub fn precedence(&self) -> Precedence {
        match self {
            BinOpKind::Add => Precedence::Sum,
            BinOpKind::Sub => Precedence::Sum,
            BinOpKind::Mul => Precedence::Product,
            BinOpKind::Div => Precedence::Product,
            BinOpKind::Rem => Precedence::Product,
            BinOpKind::And => Precedence::LAnd,
            BinOpKind::Or => Precedence::LOr,
            BinOpKind::BitXor => Precedence::BitXor,
            BinOpKind::BitAnd => Precedence::BitAnd,
            BinOpKind::BitOr => Precedence::BitOr,
            BinOpKind::Shl => Precedence::Shift,
            BinOpKind::Shr => Precedence::Shift,
            BinOpKind::Eq => Precedence::Compare,
            BinOpKind::Lt => Precedence::Compare,
            BinOpKind::Le => Precedence::Compare,
            BinOpKind::Ne => Precedence::Compare,
            BinOpKind::Ge => Precedence::Compare,
            BinOpKind::Gt => Precedence::Compare,
        }
    }

    pub fn fixity(&self) -> Fixity {
        use BinOpKind::*;
        match self {
            Eq | Ne | Lt | Le | Gt | Ge => Fixity::None,
            Add | Sub | Mul | Div | Rem | And | Or | BitXor | BitAnd | BitOr | Shl | Shr => {
                Fixity::Left
            }
        }
    }
}

pub type BinOp = Spanned<BinOpKind>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOpKind {
    /// +=
    AddAssign,
    /// -=
    SubAssign,
    /// *=
    MulAssign,
    /// /=
    DivAssign,
    /// %=
    RemAssign,
    /// ^=
    BitXorAssign,
    /// &=
    BitAndAssign,
    /// |=
    BitOrAssign,
    /// <<=
    ShlAssign,
    /// >>=
    ShrAssign,
}

pub type AssignOp = Spanned<AssignOpKind>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// *
    Deref,
    /// !
    Not,
    /// -
    Neg,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum RangeLimits {
    /// 半开合区间 `..`
    HalfOpen,
    /// 全闭区间 `..=`
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lit {
    pub kind: LiteralKind,
    pub value: StringId,
    pub suffix: Option<StringId>,
}

#[derive(Debug, Clone)]
pub struct StructExpr {
    pub node_id: NodeId,
    pub path: Path,
    pub fields: Vec<StructExprField>,
}

#[derive(Debug, Clone)]
pub struct StructExprField {
    pub name: Ident,
    pub value: Expr,
    pub is_shorthand: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StrLit {
    pub text: StringId,
    pub span: Span,
}
