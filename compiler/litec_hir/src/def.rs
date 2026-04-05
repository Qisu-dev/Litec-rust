use std::hash::Hash;

use crate::hir::PrimTy;
use litec_span::id::{DefId, HirId};

/// 定义的具体种类。
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
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
    /// use 导入 `use foo::bar;`
    Use,
    /// 模块 `mod foo;` 或 `mod foo { ... }`
    Module,
    /// extern 块 `extern "C" { ... }`
    ForeignMod,

    /// 类型参数 `T` in `struct Foo<T>`
    TyParam,

    /// 结构体或枚举变体的构造函数
    Ctor,
    /// 结构体、枚举的字段
    Field,
    /// 错误占位符，用于无法解析的定义
    Err,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Res<Id = HirId> {
    /// 用户定义的内容(函数、结构体、枚举等)
    Def(DefKind, DefId),

    /// 局部变量或函数参数
    Local(Id),

    /// 基本类型（如 i32, str）
    PrimTy(PrimTy),

    /// 解析失败时的占位符
    Err,
}

#[derive(Debug, Clone, Default)]
pub struct PerNS<T> {
    pub value_ns: T,
    pub type_ns: T,
    pub macro_ns: T,
}

impl<T> PerNS<T> {
    pub fn set(&mut self, ns: Namespace, value: T) {
        match ns {
            Namespace::Value => {
                self.value_ns = value;
            }
            Namespace::Type => {
                self.type_ns = value;
            }
        }
    }

    pub fn get(&self, ns: Namespace) -> &T {
        match ns {
            Namespace::Value => &self.value_ns,
            Namespace::Type => &self.type_ns,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    Value,
    Type,
}
