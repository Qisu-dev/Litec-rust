use litec_typed_hir::{def_id::DefId, ty::Ty};
use rustc_hash::FxHashMap;

#[derive(Debug)]
pub struct TypeCtxt {
    type_map: FxHashMap<DefId, Ty>,
    // 添加变量状态跟踪
    var_states: FxHashMap<DefId, VarState>,

    child_map: FxHashMap<DefId, DefId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarState {
    Initialized,
    Moved,
    UnInitialized,
}

impl TypeCtxt {
    pub fn new() -> Self {
        Self {
            type_map: FxHashMap::default(),
            var_states: FxHashMap::default(),
            child_map: FxHashMap::default(),
        }
    }

    /// 插入DefId到类型的映射
    pub fn insert(&mut self, def_id: DefId, ty: Ty) {
        self.type_map.insert(def_id, ty);
        self.var_states.insert(def_id, VarState::Initialized);
    }

    /// 获取DefId对应的类型
    pub fn get(&self, def_id: DefId) -> Option<&Ty> {
        self.type_map.get(&def_id)
    }

    pub fn get_var_state(&self, def_id: DefId) -> Option<&VarState> {
        self.var_states.get(&def_id)
    }

    pub fn get_parent(&self, child: DefId) -> Option<DefId> {
        self.child_map.get(&child).copied()
    }

    pub fn get_parent_state(&self, def_id: DefId) -> Option<VarState> {
        // 首先检查变量本身的状态
        if let Some(state) = self.get_var_state(def_id) {
            return Some(*state);
        }

        // 然后递归检查父节点的状态
        if let Some(parent_id) = self.get_parent(def_id) {
            self.get_parent_state(parent_id)
        } else {
            None
        }
    }

    pub fn set_var_state(&mut self, def_id: DefId, state: VarState) {
        self.var_states.insert(def_id, state);
    }
}
