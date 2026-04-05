use std::ops::Deref;

use bumpalo::Bump;
use litec_span::SourceMap;

/// 全局编译上下文，生命周期 `'hir` 统一所有借用数据。
#[derive(Debug, Clone)]
pub struct GlobalCtxt<'hir> {
    /// bump 分配器
    pub bump: &'hir Bump,
    /// 全局SourceMap
    pub source_map: SourceMap,
}

impl<'hir> GlobalCtxt<'hir> {
    /// 创建一个新的全局上下文。
    pub fn new(bump: &'hir Bump, source_map: SourceMap) -> Self {
        GlobalCtxt {
            bump,
            source_map: source_map,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TyCtxt<'tcx> {
    gcx: &'tcx GlobalCtxt<'tcx>,
}

impl<'tcx> Deref for TyCtxt<'tcx> {
    type Target = GlobalCtxt<'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.gcx
    }
}