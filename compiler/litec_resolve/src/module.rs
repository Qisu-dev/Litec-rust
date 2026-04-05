use crate::rib::{Rib, RibKind};
use indexmap::IndexMap;
use litec_ast::ast::{Ident, NodeId, Visibility};
use litec_hir::def::Res;
use litec_span::id::DefId;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub struct ModuleData {
    pub value_rib: Rib<Binding>,
    pub type_rib: Rib<Binding>,
    /// 子模块
    pub submodules: FxHashMap<Ident, (DefId, Visibility, FromKind)>,
}

impl ModuleData {
    pub fn new() -> Self {
        Self {
            value_rib: Rib::new(RibKind::Module),
            type_rib: Rib::new(RibKind::Module),
            submodules: FxHashMap::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub res: Res<NodeId>,
    pub vis: Visibility,
    pub from: FromKind
}

impl Binding {
    pub fn new(res: Res<NodeId>, vis: Visibility, from: FromKind) -> Self {
        Self {
            res,
            vis,
            from
        }
    }

    pub fn new_local(node_id: NodeId) -> Self {
        Self {
            res: Res::Local(node_id),
            vis: Visibility::Inherited,
            from: FromKind::Normal
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FromKind {
    Normal, // 正常定义
    GlobImport, // 通过全局导入定义
}