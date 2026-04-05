// litec_typed_hir/src/interner.rs

use std::cell::RefCell;
use std::collections::HashSet;

use bumpalo::Bump;

use crate::ty::*;

#[derive(Debug)]
pub struct TyInterner<'tcx> {
    pub types: RefCell<HashSet<Ty<'tcx>>>,
}

impl<'tcx> TyInterner<'tcx> {
    pub fn new(arena: &'tcx Bump) -> Self {
        let mut types: HashSet<Ty<'tcx>> = HashSet::new();

        // 只创建最基础的类型
        let bool = arena.alloc(TyKind::Bool);
        let char = arena.alloc(TyKind::Char);
        let unit = arena.alloc(TyKind::Unit);
        let never = arena.alloc(TyKind::Never);
        let str = arena.alloc(TyKind::Str);
        let unknown = arena.alloc(TyKind::Unknown);

        // 插入
        types.insert(bool);
        types.insert(char);
        types.insert(unit);
        types.insert(never);
        types.insert(str);
        types.insert(unknown);

        Self {
            types: RefCell::new(types),
        }
    }

    /// 获取或创建类型
    pub fn mk_int(&self, arena: &'tcx Bump, int_ty: IntTy) -> Ty<'tcx> {
        let kind = TyKind::Int(int_ty);

        // 检查是否存在
        if let Some(&existing) = self.types.borrow().get(&kind) {
            return existing;
        }

        // 创建并插入
        let ty = arena.alloc(kind);
        self.types.borrow_mut().insert(ty);
        ty
    }

    pub fn mk_uint(&self, arena: &'tcx Bump, uint_ty: UintTy) -> Ty<'tcx> {
        let kind = TyKind::Uint(uint_ty);
        if let Some(&existing) = self.types.borrow().get(&kind) {
            return existing;
        }
        let ty = arena.alloc(kind);
        self.types.borrow_mut().insert(ty);
        ty
    }

    pub fn mk_float(&self, arena: &'tcx Bump, float_ty: FloatTy) -> Ty<'tcx> {
        let kind = TyKind::Float(float_ty);
        if let Some(&existing) = self.types.borrow().get(&kind) {
            return existing;
        }
        let ty = arena.alloc(kind);
        self.types.borrow_mut().insert(ty);
        ty
    }

    // 常用类型的便捷方法
    pub fn bool(&self) -> Ty<'tcx> {
        *self.types.borrow().get(&TyKind::Bool).unwrap()
    }

    pub fn char(&self) -> Ty<'tcx> {
        *self.types.borrow().get(&TyKind::Char).unwrap()
    }

    pub fn unit(&self) -> Ty<'tcx> {
        *self.types.borrow().get(&TyKind::Tuple(&[])).unwrap()
    }

    pub fn never(&self) -> Ty<'tcx> {
        *self.types.borrow().get(&TyKind::Never).unwrap()
    }

    pub fn str(&self) -> Ty<'tcx> {
        *self.types.borrow().get(&TyKind::Str).unwrap()
    }

    pub fn unknown(&self) -> Ty<'tcx> {
        *self.types.borrow().get(&TyKind::Unknown).unwrap()
    }
}
