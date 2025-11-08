use crate::def_id::DefId;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Char,
    Str,
    Adt(DefId), // 指向结构体/枚举等
    Ptr(Box<Ty>),
    Ref {
        mutable: bool,
        to: Arc<Ty>,
    },
    Array {
        elem: Arc<Ty>,
        len: Option<u64>,
    },
    Fn {
        params: Vec<Ty>,
        return_ty: Box<Ty>,
    },
    Tuple(Vec<Ty>),
    Unknown,
    SelfType,
    Never, // !
    Unit,
    Error
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IntKind {
    Unknown,   // 如果以后要 HM / 泛型
    I8, I16, I32, I64, I128, Isize,
    U8, U16, U32, U64, U128, Usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FloatKind {
    Unknow,
    F32, 
    F64,
}

impl Ty {
    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int(_) | Ty::Float(_))
    }
}