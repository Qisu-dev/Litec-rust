use litec_ast::ast::{NodeId, PathSegment};
use litec_hir::def::Res;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Default)]
pub struct ResolveOutput {
    /// 完全解析的路径结果：NodeId → Res
    pub resolutions: FxHashMap<NodeId, Res<NodeId>>,
    /// 部分解析的路径结果（例如 `Vec::new` 中的 `Vec` 部分）
    pub partial_resolutions: FxHashMap<NodeId, PartialPath>,
}

impl ResolveOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn resolution(&self, node_id: NodeId) -> Option<&Res<NodeId>> {
        self.resolutions.get(&node_id)
    }

    pub fn partial(&self, node_id: NodeId) -> Option<&PartialPath> {
        self.partial_resolutions.get(&node_id)
    }
}

#[derive(Debug, Clone)]
pub struct PartialPath {
    pub base: Res<NodeId>,
    pub remaining: Vec<PathSegment>,
}
