use index_vec::IndexVec;
use litec_span::id::ItemLocalId;

use crate::hir::{self, Node};

#[derive(Debug, Clone)]
pub enum OwnerNode<'hir> {
    Item(&'hir hir::Item<'hir>),
    Crate(&'hir hir::Mod<'hir>),
}

#[derive(Debug, Clone)]
pub struct OwnerNodes<'hir> {
    pub nodes: IndexVec<ItemLocalId, Node<'hir>>, // 所有节点，按 local_id 索引
}
