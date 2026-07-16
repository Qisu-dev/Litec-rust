use litec_hir::hir::PrimTy;
use litec_span::id::DefId;
use std::fmt;

/// 类型变量 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub usize);

/// 语义类型（用于类型推断和类型检查）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    /// 类型变量（未确定）
    Var(TypeVarId),
    /// 基本类型（i32, bool, char, ...）
    Prim(PrimTy),
    /// ADT（结构体、枚举）
    Adt(DefId, Vec<Ty>),       // 泛型参数实例化
    /// 函数类型
    Fn(Vec<Ty>, Box<Ty>),
    /// 指针类型
    Ptr(Box<Ty>),
    /// 元组
    Tuple(Vec<Ty>),

    Never,
    /// 错误占位符
    Error,
}

impl fmt::Display for Ty {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Var(id) => write!(fmt, "?{}", id.0),
            Ty::Prim(p) => write!(fmt, "{:?}", p),
            Ty::Adt(def_id, args) => {
                write!(fmt, "Adt({:?}", def_id)?;
                if !args.is_empty() {
                    write!(fmt, "<")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 { write!(fmt, ", ")?; }
                        write!(fmt, "{}", arg)?;
                    }
                    write!(fmt, ">")?;
                }
                write!(fmt, ")")
            }
            Ty::Fn(params, ret) => {
                write!(fmt, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 { write!(fmt, ", ")?; }
                    write!(fmt, "{}", p)?;
                }
                write!(fmt, ") -> {}", ret)
            }
            Ty::Ptr(inner) => write!(fmt, "*{}", inner),
            Ty::Tuple(elems) => {
                write!(fmt, "(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 { write!(fmt, ", ")?; }
                    write!(fmt, "{}", e)?;
                }
                write!(fmt, ")")
            }
            Ty::Never => write!(fmt, "!"),
            Ty::Error => write!(fmt, "{{error}}"),
        }
    }
}