use crate::{
    token::LiteralKind,
    util::{accos_op::Fixity, precedence::Precedence},
};
use litec_span::{Span, Spanned, StringId};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    Struct(Ident, Generics, StructKind),
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
    /// 例如 `trait Foo<T> { ... }`
    Trait(Ident, Generics, Vec<TraitItem>),
    /// 一个类型别名
    /// 例如 `type foo = i32;`
    TypeAlias(TypeAlias),
    /// 一个枚举定义
    /// 例如 `enum Result<T, E> { OK(T), Err(E) }`
    Enum(Ident, Generics, Vec<Variant>),
}

#[derive(Debug, Clone)]
pub enum StructKind {
    Unit,               // struct Foo;
    Tuple(Vec<Ty>),     // struct Foo(i32, bool);
    Struct(Vec<Field>), // struct Foo { x: i32, y: bool }
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub ident: Ident,
    pub data: VariantData,
    pub span: Span,
    pub node_id: NodeId,
}

#[derive(Debug, Clone)]
pub enum VariantData {
    Struct(Vec<VariantField>),
    Tuple(Vec<Ty>),
    Unit,
}

#[derive(Debug, Clone)]
pub struct VariantField {
    pub name: Ident,
    pub ty: Ty,
    pub span: Span,
    pub node_id: NodeId,
}

pub type TraitItem = Item<TraitItemKind>;

#[derive(Debug, Clone)]
pub enum TraitItemKind {
    Fn(FnSig),
}

#[derive(Debug, Clone)]
pub struct Impl {
    pub node_id: NodeId,
    pub generics: Generics,
    /// 当impl trait时, of_trait为Some, 内容是trait的path
    pub of_trait: Option<Path>,
    pub self_ty: Box<Ty>,
    pub items: Vec<ImplItem>,
}

pub type ImplItem = Item<ImplItemKind>;

#[derive(Debug, Clone)]
pub enum ImplItemKind {
    Fn(Fn),          // 方法定义
}

#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub node_id: NodeId,
    pub name: Ident,
    pub generics: Generics,
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
    pub generics: Generics,
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
    pub kind: ParamKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ParamKind {
    Normal(Pat, Ty),
    /// 比如 `*self` `*mut self`
    SelfPtr(Mutability),
    /// 比如 `self` `mut self`
    SelfValue(Mutability),
}

#[derive(Debug, Clone)]
pub struct Block {
    pub node_id: NodeId,
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub node_id: NodeId,
    pub name: Ident,
    pub ty: Ty,
    pub visibility: Visibility,
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
        variable: Pat,
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
    /// 结构体初始化
    /// 例如 `Foo { foo }`
    StructExpr(StructExpr),
    /// 类型转换
    /// 例如 `foo as usize`
    Cast(Box<Expr>, Box<Ty>),
    /// 模式匹配
    /// 例如 `match foo { arm1 => { ... } , arm2 => ... }`
    Match(Box<Expr>, Vec<Arm>),
    /// 返回
    /// 例如 `return 1;` `return ;`
    Return(Option<Box<Expr>>),
    /// 重新循环
    /// 例如 `continue;`
    Continue,
    /// 跳出循环
    /// 例如 `break;` `break 1;`
    Break(Option<Box<Expr>>),
}

impl ExprKind {
    pub fn expr_requires_semi_to_be_stmt(&self) -> bool {
        !matches!(
            self,
            ExprKind::If(..)
                | ExprKind::Loop(..)
                | ExprKind::While(..)
                | ExprKind::For { .. }
                | ExprKind::Block(..)
                | ExprKind::Match(..)
                | ExprKind::Return(..)
                | ExprKind::Continue
                | ExprKind::Break(..)
        )
    }
}

#[derive(Debug, Clone)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Box<Expr>>, // if condition
    pub body: Box<Expr>,
    pub span: Span,
    pub node_id: NodeId,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub node_id: NodeId,
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// 不带分号的表达式
    /// 比如 `if cond { .. } else { .. }`
    Expr(Box<Expr>),
    /// 带分号的表达式
    /// 比如 `1;` `loop { .. };`
    Semi(Box<Expr>),
    Let(Pat, Option<Box<Ty>>, Option<Box<Expr>>),
    Defer(Box<Expr>),
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

    /// 原始指针：`*const T` / `*mut T`
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

    /// 自身类型 `Self`
    SelfTy,
}

#[derive(Debug, Clone)]
pub struct Path {
    pub node_id: NodeId,
    pub segments: Vec<PathSegment>,
    pub span: Span,
}

impl ToString for Path {
    fn to_string(&self) -> String {
        self.segments
            .iter()
            .map(|seg| seg.name.text.to_string()) // 假设 StringId 实现了 ToString
            .collect::<Vec<_>>()
            .join("::")
    }
}

#[derive(Debug, Clone)]
pub struct PathSegment {
    pub node_id: NodeId,
    pub name: Ident,
    pub span: Span,
    pub generic_args: Option<GenericArgs>,
}

impl PathSegment {
    pub fn from_ident(ident: Ident) -> Self {
        Self {
            node_id: DUMMY_NODE_ID,
            name: ident,
            span: ident.span,
            generic_args: None,
        }
    }
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
pub struct Generics {
    pub node_id: NodeId,
    pub params: Vec<Generic>,
    pub span: Span,
}

impl Generics {
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
pub struct Generic {
    pub node_id: NodeId,
    pub name: Ident, // "T", "U"
    pub bounds: Option<Bounds>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Bounds {
    pub node_id: NodeId,
    pub bounds: Vec<Path>, // Trait + Trait ...
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct Ident {
    pub text: StringId,
    pub span: Span,
}

impl Ident {
    pub fn new(text: StringId, span: Span) -> Self {
        Self { text, span }
    }

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

impl From<Ident> for StringId {
    #[inline]
    fn from(value: Ident) -> Self {
        value.text
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

#[derive(Debug, Clone)]
pub struct Pat {
    pub node_id: NodeId,
    pub kind: PatKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum PatKind {
    /// 通配符 `_`
    Wild,
    /// 标识符绑定，如 `x`, `mut x`
    Ident(Mutability, Ident),
    /// 元组模式 `(a, b, c)`
    Tuple(Vec<Pat>),
    /// 结构体模式 `Point { x, y }` 或 `Point { x: 1, y }`
    Struct(Path, Vec<StructFieldPat>, bool), // bool 表示是否有 `..`
    /// 枚举模式 `Some(x)` 或 `None`
    Enum(Path, Option<Box<Pat>>),
    /// 字面量模式 `0`, `"hello"`
    Lit(Lit),
    /// 范围模式 `1..=5`
    Range(Box<Expr>, Box<Expr>, RangeLimits),
    /// 多重模式 `1 | 2`
    Or(Vec<Pat>),
}

#[derive(Debug, Clone)]
pub struct StructFieldPat {
    pub name: Ident,
    pub pat: Pat,
    pub span: Span,
    pub node_id: NodeId,
}
