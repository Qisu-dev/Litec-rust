use litec_hir::def::Res;
use litec_middle::ty::Ty;
use litec_span::Span;

/// Trait 引用，包含 trait 的 DefId 和泛型参数替换
#[derive(Debug, Clone)]
pub struct TraitRef {
    pub trait_res: Res,
    pub substs: Vec<Ty>,
}

/// 类型约束
#[derive(Debug, Clone)]
pub enum Constraint {
    /// 类型相等约束
    Eq(Ty, Ty, Span),
    /// 类型 ty 必须实现 trait_ref 指定的 trait
    Trait(TraitRef, Ty, Span),
}
