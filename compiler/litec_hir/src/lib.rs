use litec_span::{Span, StringId};
use litec_ast::token::TokenKind;
use rustc_hash::FxHashMap;

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
    /// has no argument attribute like #[test]
    Simple,
    /// an attribute has arguments like #[derive(Debug, Clone)]
    WithArguments(Vec<AttributeValue>),
    /// 键值对参数：#[cfg(target_os = "linux")]
    KeyValue(FxHashMap<StringId, AttributeValue>),
}

#[derive(Debug, Clone)]
pub enum AttributeValue {
    Ident(StringId),    // 标识符：Clone, Debug
    String(StringId),   // 字符串："linux"
    Number(f64),        // 数字：42, 3.14
    Bool(bool),         // 布尔值：true, false
}

#[derive(Debug, Clone)]
pub enum Item {
    Function {
        visibility: Visibility,
        name: StringId,
        params: Vec<Param>,
        return_type: Option<Type>,
        body: Block,
        span: Span,
    },
    Struct {
        visibility: Visibility,
        name: StringId,
        fields: Vec<Field>,
        span: Span,
    },
    Use {
        visibility: Visibility,
        path: Vec<StringId>,
        items: Option<Vec<UseItem>>,
        rename: Option<StringId>,
        span: Span,
    }
}

#[derive(Debug, Clone)]
pub struct UseItem {
    pub name: StringId,
    pub rename: Option<StringId>,
    pub items: Option<Vec<UseItem>>,
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

#[derive(Debug, Clone)]
pub enum Type {
    Named {
        name: StringId,
        span: Span,
    },
    // 可以添加更多类型变体
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
    Addition {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Subtract {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Multiply {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Divide {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Remainder {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    
    // 比较运算
    Equal {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    NotEqual {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    LessThan {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    LessThanOrEqual {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    GreaterThan {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    GreaterThanOrEqual {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    
    // 逻辑运算
    LogicalAnd {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    LogicalOr {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    LogicalNot {
        operand: Box<Expr>,
        span: Span,
    },
    
    // 赋值运算 - 统一使用基本的 Assign 节点
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
        original_op: Option<TokenKind>, // 保留原始操作符信息
    },
    
    // 一元运算
    Negate {
        operand: Box<Expr>,
        span: Span,
    },
    AddressOf {
        base: Box<Expr>,
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
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(Box<Expr>),
    Let {
        name: StringId,
        ty: Option<Type>,
        value: Option<Box<Expr>>,
        span: Span,
    },
    Return {
        value: Option<Box<Expr>>,
        span: Span,
    },
    Break {
        value: Option<Box<Expr>>,
        span: Span,
    },
    Continue {
        span: Span,
    },
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
        Literal, Ident, Addition, Subtract, Multiply, Divide, Remainder,
        Equal, NotEqual, LessThan, LessThanOrEqual, GreaterThan, GreaterThanOrEqual,
        LogicalAnd, LogicalOr, LogicalNot,
        Assign, Negate, AddressOf,
        Call, If, Loop, FieldAccess, PathAccess, Grouped
    ],
    (Expr::Block { block } => block.span)
);

// 为 Stmt 自动实现 span (Expr 变体)
impl_span_for_enum!(
    Stmt,
    [Let, Return, Break, Continue],
    (Stmt::Expr(expr) => expr.span())
);