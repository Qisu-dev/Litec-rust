use crate::hir::PrimTy;
use litec_span::id::{DefId, HirId};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// 定义的具体种类。
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum DefKind {
    /// 函数定义 `fn foo() {}`
    Fn,
    /// 结构体定义 `struct Foo { ... }`
    Struct,
    /// trait 定义 `trait Foo { ... }`
    Trait,
    /// 类型别名 `type Foo = Bar;`
    TyAlias,
    /// 常量项 `const X: usize = 42;`
    Const,
    /// 静态项 `static X: i32 = 42;`
    Static,
    /// 外部 crate 引入 `extern crate foo;`
    ExternCrate,
    /// 模块 `mod foo;` 或 `mod foo { ... }`
    Module,
    /// 枚举
    Enum,
    /// 枚举变体
    Variant,
    /// impl内的函数
    ImplFn,
    /// impl内的类型
    ImplTy,
    /// 外部函数
    ExternFn,
    /// impl块
    Impl,
    /// impl trait库
    TraitImpl,
    /// trait内函数
    TraitFn,

    TyParam,

    Ctor(CtorOf, CtorKind),

    /// 一个crate
    Crate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CtorOf {
    Struct,  // 结构体构造函数比如 `struct Foo(i32, i64);`
    Variant, // 一个枚举的构造函数
}

/// 构造函数的行为类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CtorKind {
    Fn,    // 像函数一样调用，例如 `Foo(42)`
    Const, // 像常量一样取值，例如 `Foo`
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Res<Id = HirId> {
    /// 用户定义的内容(函数、结构体、枚举等)
    Def(DefKind, DefId),

    /// 局部变量或函数参数
    Local(Id),

    /// 基本类型（如 i32, str）
    PrimTy(PrimTy),

    /// trait的def id
    SelfTyParam { trait_: DefId },

    SelfTyAlias {
        /// 这里的alias_to指向的是impl
        /// 因为在resolver中无法获取准确的类型def id
        /// 只能通过impl的def id来实现效果
        alias_to: DefId,
    },

    /// def id指向的是impl的def id
    SelfCtor(DefId),

    /// 内置的 trait
    BuiltinTrait(BuiltinTrait),

    /// 解析失败时的占位符
    Err,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinTrait {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Not,
    Deref,
    Clone,
    Copy,
    Default,
}

#[derive(Debug, Clone, Default)]
pub struct PerNS<T> {
    pub value_ns: Option<T>,
    pub type_ns: Option<T>,
}

impl<T> PerNS<T> {
    pub fn set(&mut self, ns: Namespace, value: T) {
        match ns {
            Namespace::Value => {
                self.value_ns = Some(value);
            }
            Namespace::Type => {
                self.type_ns = Some(value);
            }
        }
    }

    pub fn get(&self, ns: Namespace) -> Option<&T> {
        match ns {
            Namespace::Value => self.value_ns.as_ref(),
            Namespace::Type => self.type_ns.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    Value,
    Type,
}
