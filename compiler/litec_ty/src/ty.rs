//! 类型系统核心定义

use std::fmt;
use std::hash::{Hash, Hasher};
use std::ptr;

use litec_span::StringId;

use crate::context::TyCtxt;
use crate::def_id::DefId;

/// 类型引用 - 指向 arena 中的类型
pub type Ty<'tcx> = &'tcx TyKind<'tcx>;

/// 类型种类（核心枚举）
#[derive(Debug, Clone, Copy)]
pub enum TyKind<'tcx> {
    // 基础类型
    Int(IntTy),
    Uint(UintTy),
    Float(FloatTy),
    Bool,
    Char,
    Str,
    Unit,

    // 复合类型
    Tuple(&'tcx [Ty<'tcx>]),
    Array(Ty<'tcx>, u64),
    Slice(Ty<'tcx>),

    // 指针类型
    Ref(Ty<'tcx>, Mutability),
    RawPtr,
    Ptr(Ty<'tcx>, Mutability),

    // 函数类型
    FnSig(FnSig<'tcx>),
    FnPtr(FnSig<'tcx>),
    ExternFn(FnSig<'tcx>),

    // 用户定义类型
    Adt(DefId, &'tcx [Ty<'tcx>]),

    // 泛型/特殊
    Param(ParamTy),
    Infer(InferVar),
    SelfType,
    Never,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mutability {
    Mut,
    Const
}

/// 整数类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
}

/// 无符号整数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UintTy {
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

/// 浮点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatTy {
    F32,
    F64,
}

/// 类型参数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamTy {
    pub index: u32,
    pub name: StringId,
}

/// 推断变量
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InferVar(pub u32);

/// 函数签名
#[derive(Debug, Clone, Copy)]
pub struct FnSig<'tcx> {
    pub inputs: &'tcx [Ty<'tcx>],
    pub output: Ty<'tcx>,
    pub variadic: bool,
}

// ========== 方法实现 ==========

impl<'tcx> TyKind<'tcx> {
    pub fn is_unit(self) -> bool {
        matches!(self, TyKind::Tuple(&[]))
    }

    pub fn is_never(self) -> bool {
        matches!(self, TyKind::Never)
    }

    pub fn is_primitive(self) -> bool {
        matches!(
            self,
            TyKind::Bool | TyKind::Char | TyKind::Int(_) | TyKind::Uint(_) | TyKind::Float(_)
        )
    }

    pub fn is_integral(self) -> bool {
        matches!(self, TyKind::Int(_) | TyKind::Uint(_))
    }

    pub fn is_floating_point(self) -> bool {
        matches!(self, TyKind::Float(_))
    }

    pub fn is_numeric(self) -> bool {
        self.is_integral() || self.is_floating_point()
    }

    pub fn is_fn(self) -> bool {
        matches!(
            self,
            TyKind::FnSig(..) | TyKind::FnPtr(_) | TyKind::ExternFn(_)
        )
    }

    pub fn is_pointer(self) -> bool {
        matches!(self, TyKind::Ptr(..) | TyKind::RawPtr)
    }

    pub fn is_sized(&self, _tcx: TyCtxt<'tcx>) -> bool {
        match self {
            TyKind::Str | TyKind::Slice(_) => false,
            TyKind::Tuple(tys) => tys.iter().all(|ty| ty.is_sized(_tcx)),
            _ => true,
        }
    }
}

// ========== Display 实现 ==========

impl<'tcx> fmt::Display for Ty<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TyKind::Int(i) => write!(f, "{}", i),
            TyKind::Uint(u) => write!(f, "{}", u),
            TyKind::Float(fl) => write!(f, "{}", fl),
            TyKind::Bool => write!(f, "bool"),
            TyKind::Char => write!(f, "char"),
            TyKind::Str => write!(f, "str"),
            TyKind::Tuple(tys) if tys.is_empty() => write!(f, "()"),
            TyKind::Tuple(tys) => {
                write!(f, "(")?;
                for (i, ty) in tys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", ty)?;
                }
                write!(f, ")")
            }
            TyKind::Array(ty, len) => write!(f, "[{}; {}]", ty, len),
            TyKind::Slice(ty) => write!(f, "[{}]", ty),
            TyKind::Ref(ty, Mutability::Mut) => write!(f, "&mut {}", ty),
            TyKind::Ref(ty, Mutability::Const) => write!(f, "&{}", ty),
            TyKind::RawPtr => write!(f, "RawPtr"),
            TyKind::Ptr(ty, Mutability::Const) => write!(f, "*const {}", ty),
            TyKind::Ptr(ty, Mutability::Mut) => write!(f, "*mut {}", ty),
            TyKind::FnSig(sig) => {
                write!(f, "fn(")?;
                for (i, ty) in sig.inputs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", ty)?;
                }
                if sig.variadic {
                    write!(f, "...")?;
                }
                write!(f, ") -> {}", sig.output)
            }
            TyKind::FnPtr(sig) | TyKind::ExternFn(sig) => {
                write!(f, "fn(")?;
                for (i, ty) in sig.inputs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", ty)?;
                }
                if sig.variadic {
                    write!(f, "...")?;
                }
                write!(f, ") -> {}", sig.output)
            }
            TyKind::Adt(def_id, _) => write!(f, "{{adt#{}}}", def_id.index),
            TyKind::Param(p) => write!(f, "_{}", p.index),
            TyKind::Infer(v) => write!(f, "?{}", v.0),
            TyKind::SelfType => write!(f, "Self"),
            TyKind::Never => write!(f, "!"),
            TyKind::Unknown => write!(f, "{{unknown}}"),
            TyKind::Unit => write!(f, "Unit"),
        }
    }
}

impl fmt::Display for IntTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntTy::I8 => write!(f, "i8"),
            IntTy::I16 => write!(f, "i16"),
            IntTy::I32 => write!(f, "i32"),
            IntTy::I64 => write!(f, "i64"),
            IntTy::I128 => write!(f, "i128"),
            IntTy::Isize => write!(f, "isize"),
        }
    }
}

impl fmt::Display for UintTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UintTy::U8 => write!(f, "u8"),
            UintTy::U16 => write!(f, "u16"),
            UintTy::U32 => write!(f, "u32"),
            UintTy::U64 => write!(f, "u64"),
            UintTy::U128 => write!(f, "u128"),
            UintTy::Usize => write!(f, "usize"),
        }
    }
}

impl fmt::Display for FloatTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FloatTy::F32 => write!(f, "f32"),
            FloatTy::F64 => write!(f, "f64"),
        }
    }
}

// TyKind 的 PartialEq 和 Hash（用于内部化查找）
impl<'tcx> PartialEq for TyKind<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        use TyKind::*;
        match (self, other) {
            (Int(a), Int(b)) => a == b,
            (Uint(a), Uint(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Bool, Bool) => true,
            (Char, Char) => true,
            (Str, Str) => true,
            (Tuple(a), Tuple(b)) => ptr::eq(a.as_ptr(), b.as_ptr()) || a == b,
            (Array(t1, n1), Array(t2, n2)) => t1 == t2 && n1 == n2,
            (Slice(a), Slice(b)) => a == b,
            (Ref(t1, m1), Ref(t2, m2)) => t1 == t2 && m1 == m2,
            (RawPtr, RawPtr) => true,
            (Ptr(pointee, m1), Ptr(pointee2, m2)) => pointee == pointee2 && m1 == m2,
            (FnSig(s1), FnSig(s2)) => s1 == s2,
            (FnPtr(s1), FnPtr(s2)) => s1 == s2,
            (ExternFn(s1), ExternFn(s2)) => s1 == s2,
            (Adt(d1, s1), Adt(d2, s2)) => d1 == d2 && ptr::eq(s1.as_ptr(), s2.as_ptr()),
            (Param(a), Param(b)) => a == b,
            (Infer(a), Infer(b)) => a == b,
            (SelfType, SelfType) => true,
            (Never, Never) => true,
            (Unknown, Unknown) => true,
            _ => false,
        }
    }
}

impl<'tcx> Eq for TyKind<'tcx> {}

impl<'tcx> Hash for TyKind<'tcx> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use TyKind::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Int(i) => i.hash(state),
            Uint(u) => u.hash(state),
            Float(fl) => fl.hash(state),
            Tuple(ts) => {
                ts.len().hash(state);
                ts.as_ptr().hash(state);
            }
            Array(t, n) => {
                t.hash(state);
                n.hash(state);
            }
            Slice(t) => t.hash(state),
            Ref(t, m) => {
                t.hash(state);
                m.hash(state);
            }
            RawPtr => {
                0.hash(state);
            }
            FnSig(sig) => {
                sig.hash(state);
            }
            ExternFn(s) => s.hash(state),
            Adt(d, s) => {
                d.hash(state);
                s.as_ptr().hash(state);
            }
            Param(p) => p.hash(state),
            Infer(v) => v.hash(state),
            _ => {}
        }
    }
}

impl<'tcx> Hash for FnSig<'tcx> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inputs.as_ptr().hash(state);
        self.inputs.len().hash(state);
        self.output.hash(state);
        self.variadic.hash(state);
    }
}

impl<'tcx> PartialEq for FnSig<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        self.variadic == other.variadic
            && self.output == other.output
            && ptr::eq(self.inputs.as_ptr(), other.inputs.as_ptr())
    }
}

impl<'tcx> Eq for FnSig<'tcx> {}
