//! 内建类型和函数

use litec_span::{Span, StringId};
use litec_ty::{def_id::DefId, ty::Ty};

/// 内建定义
#[derive(Debug)]
pub struct Builtins<'tcx> {
    /// 类型 DefId
    pub def_ids: BuiltinDefIds,
    /// 内建函数
    pub functions: Vec<BuiltinFunction<'tcx>>,
}

#[derive(Debug, Clone, Copy)]
pub struct BuiltinDefIds {
    pub i8: DefId,
    pub i16: DefId,
    pub i32: DefId,
    pub i64: DefId,
    pub i128: DefId,
    pub isize: DefId,
    pub u8: DefId,
    pub u16: DefId,
    pub u32: DefId,
    pub u64: DefId,
    pub u128: DefId,
    pub usize: DefId,
    pub f32: DefId,
    pub f64: DefId,
    pub bool: DefId,
    pub char: DefId,
    pub str: DefId,
    pub unit: DefId,
    pub never: DefId,
    pub raw_ptr: DefId
}

#[derive(Debug)]
pub struct BuiltinFunction<'tcx> {
    pub name: StringId,
    pub def_id: DefId,
    pub params: Vec<BuiltinParam<'tcx>>,
    pub is_variadic: bool,
    pub ret: Ty<'tcx>,
    pub span: Span,
}

#[derive(Debug)]
pub struct BuiltinParam<'tcx> {
    pub name: StringId,
    pub ty: Ty<'tcx>,
}