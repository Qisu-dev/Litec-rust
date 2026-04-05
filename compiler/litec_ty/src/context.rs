//! 类型上下文 - 类型的唯一构造入口

use std::cell::RefCell;

use bumpalo::Bump;
use litec_span::StringId;

use crate::def_id::DefId;
use crate::interner::TyInterner;
use crate::ty::*;

/// 类型上下文
/// 'tcx 是 arena 的生命周期
#[derive(Debug, Clone, Copy)]
pub struct TyCtxt<'tcx> {
    arena: &'tcx Bump,
    interner: &'tcx RefCell<TyInterner<'tcx>>,
}

impl<'tcx> TyCtxt<'tcx> {
    pub fn new(arena: &'tcx Bump) -> Self {
        let interner: &'tcx RefCell<TyInterner<'tcx>> =
            arena.alloc(RefCell::new(TyInterner::new(arena)));

        Self { arena, interner }
    }

    // 常用类型（预创建的）
    pub fn mk_bool(&self) -> Ty<'tcx> {
        self.interner.borrow().bool()
    }
    pub fn mk_char(&self) -> Ty<'tcx> {
        self.interner.borrow().char()
    }
    pub fn mk_unit(&self) -> Ty<'tcx> {
        self.interner.borrow().unit()
    }
    pub fn mk_never(&self) -> Ty<'tcx> {
        self.interner.borrow().never()
    }
    pub fn mk_str(&self) -> Ty<'tcx> {
        self.interner.borrow().str()
    }
    pub fn mk_unknown(&self) -> Ty<'tcx> {
        self.interner.borrow().unknown()
    }

    pub fn mk_i8(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_int(self.arena, IntTy::I8)
    }
    pub fn mk_i16(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_int(self.arena, IntTy::I16)
    }
    pub fn mk_i32(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_int(self.arena, IntTy::I32)
    }
    pub fn mk_i64(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_int(self.arena, IntTy::I64)
    }
    pub fn mk_i128(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_int(self.arena, IntTy::I128)
    }
    pub fn mk_isize(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_int(self.arena, IntTy::Isize)
    }

    pub fn mk_u8(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_uint(self.arena, UintTy::U8)
    }
    pub fn mk_u16(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_uint(self.arena, UintTy::U16)
    }
    pub fn mk_u32(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_uint(self.arena, UintTy::U32)
    }
    pub fn mk_u64(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_uint(self.arena, UintTy::U64)
    }
    pub fn mk_u128(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_uint(self.arena, UintTy::U128)
    }
    pub fn mk_usize(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_uint(self.arena, UintTy::Usize)
    }

    pub fn mk_f32(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_float(self.arena, FloatTy::F32)
    }
    pub fn mk_f64(&self) -> Ty<'tcx> {
        self.interner.borrow().mk_float(self.arena, FloatTy::F64)
    }

    pub fn mk_tuple(&self, tys: &[Ty<'tcx>]) -> Ty<'tcx> {
        if tys.is_empty() {
            return self.mk_unit();
        }
        let slice = self.arena.alloc_slice_copy(tys);
        self.intern(TyKind::Tuple(slice))
    }

    pub fn mk_array(&self, elem: Ty<'tcx>, len: u64) -> Ty<'tcx> {
        self.intern(TyKind::Array(elem, len))
    }

    pub fn mk_slice(&self, elem: Ty<'tcx>) -> Ty<'tcx> {
        self.intern(TyKind::Slice(elem))
    }

    pub fn mk_ref(&self, ty: Ty<'tcx>, mutbl: Mutability) -> Ty<'tcx> {
        self.intern(TyKind::Ref(ty, mutbl))
    }

    pub fn mk_raw_ptr(&self) -> Ty<'tcx> {
        self.intern(TyKind::RawPtr)
    }

    pub fn mk_ptr(&self, ty: Ty<'tcx>, mutbl: Mutability) -> Ty<'tcx> {
        self.intern(TyKind::Ptr(ty, mutbl))
    }

    pub fn mk_fn_ptr(&self, sig: FnSig<'tcx>) -> Ty<'tcx> {
        self.intern(TyKind::FnPtr(sig))
    }

    pub fn mk_extern_fn(&self, sig: FnSig<'tcx>) -> Ty<'tcx> {
        self.intern(TyKind::ExternFn(sig))
    }

    pub fn mk_fn_def(&self, sig: FnSig<'tcx>) -> Ty<'tcx> {
        self.intern(TyKind::FnSig(sig))
    }

    pub fn mk_adt(&self, def_id: DefId, substs: &'tcx [Ty<'tcx>]) -> Ty<'tcx> {
        self.intern(TyKind::Adt(def_id, substs))
    }

    pub fn mk_param(&self, index: u32, name: StringId) -> Ty<'tcx> {
        self.intern(TyKind::Param(ParamTy { index, name }))
    }

    pub fn mk_infer(&self, var: InferVar) -> Ty<'tcx> {
        self.intern(TyKind::Infer(var))
    }

    pub fn mk_self_type(&self) -> Ty<'tcx> {
        self.intern(TyKind::SelfType)
    }

    pub fn mk_fn_sig(&self, inputs: &[Ty<'tcx>], output: Ty<'tcx>, variadic: bool) -> FnSig<'tcx> {
        FnSig {
            inputs: self.arena.alloc_slice_copy(inputs),
            output,
            variadic,
        }
    }

    pub fn alloc_slice<T: Copy>(&self, slice: &[T]) -> &'tcx [T] {
        self.arena.alloc_slice_copy(slice)
    }

    /// 内部化核心逻辑
    fn intern(&self, kind: TyKind<'tcx>) -> Ty<'tcx> {
        // 基础类型直接返回
        match kind {
            TyKind::Bool => return self.mk_bool(),
            TyKind::Char => return self.mk_char(),
            TyKind::Tuple(&[]) => return self.mk_unit(),
            TyKind::Never => return self.mk_never(),
            TyKind::Str => return self.mk_str(),
            TyKind::Int(IntTy::I8) => return self.mk_i8(),
            TyKind::Int(IntTy::I16) => return self.mk_i16(),
            TyKind::Int(IntTy::I32) => return self.mk_i32(),
            TyKind::Int(IntTy::I64) => return self.mk_i64(),
            TyKind::Int(IntTy::I128) => return self.mk_i128(),
            TyKind::Int(IntTy::Isize) => return self.mk_isize(),
            TyKind::Uint(UintTy::U8) => return self.mk_u8(),
            TyKind::Uint(UintTy::U16) => return self.mk_u16(),
            TyKind::Uint(UintTy::U32) => return self.mk_u32(),
            TyKind::Uint(UintTy::U64) => return self.mk_u64(),
            TyKind::Uint(UintTy::U128) => return self.mk_u128(),
            TyKind::Uint(UintTy::Usize) => return self.mk_usize(),
            TyKind::Float(FloatTy::F32) => return self.mk_f32(),
            TyKind::Float(FloatTy::F64) => return self.mk_f64(),
            TyKind::Unknown => return self.mk_unknown(),
            _ => {}
        }

        // 检查是否已存在
        {
            let interner = self.interner.borrow();
            if let Some(&existing) = interner.types.borrow().get(&kind) {
                return existing;
            }
        }

        // 分配新类型
        let ty: Ty<'tcx> = self.arena.alloc(kind);
        let interner = self.interner.borrow();
        interner.types.borrow_mut().insert(ty);
        ty
    }
}
