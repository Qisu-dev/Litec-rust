use litec_ast::ast::{self, *};
use litec_ast::visit::{walk_block, walk_generic_param, walk_item};
use litec_ast::{ast::NodeId, visit::Visitor};
use litec_hir::def::Namespace;
use litec_span::id::{CrateNum, DefId, DefIndex};
use rustc_hash::FxHashMap;

pub struct DefCollector {
    /// 当前 crate 的编号（通常为 LOCAL_CRATE = 0）
    crate_num: CrateNum,
    /// 下一个可用的 DefIndex（从 1 开始，0 预留给根模块）
    next_index: DefIndex,
    /// NodeId 到 DefId 的映射
    node_to_def: FxHashMap<NodeId, DefId>,
    /// DefId 到父 DefId 的映射（用于构建 DefPath）
    def_to_parent: FxHashMap<DefId, DefId>,
    /// 当前正在处理的父 DefId 栈
    parent_stack: Vec<DefId>,
}

impl DefCollector {
    pub fn new(crate_num: CrateNum) -> Self {
        Self {
            crate_num,
            next_index: 1,
            node_to_def: FxHashMap::default(),
            def_to_parent: FxHashMap::default(),
            parent_stack: Vec::new(),
        }
    }

    pub fn collect(&mut self, ast: &ast::Crate) {
        self.visit_crate(ast);
    }

    /// 分配一个新的 DefId，并将当前父栈的栈顶作为其父级
    fn alloc_def_id(&mut self) -> DefId {
        let id = DefId {
            krate: self.crate_num,
            index: self.next_index,
        };
        self.next_index += 1;
        id
    }

    /// 进入一个定义节点：分配 DefId，记录映射，推入父栈
    fn enter_def(&mut self, def_id: DefId) -> DefId {
        if let Some(parent) = self.current_parent() {
            self.def_to_parent.insert(def_id, parent);
        }
        self.parent_stack.push(def_id);
        def_id
    }

    fn current_parent(&self) -> Option<DefId> {
        self.parent_stack.last().map(|parent| *parent)
    }

    /// 退出当前定义节点（弹出父栈）
    fn exit_def(&mut self) {
        self.parent_stack.pop().expect("parent stack underflow");
    }

    fn insert(&mut self, node_id: NodeId, def_id: DefId) {
        self.node_to_def.insert(node_id, def_id);
    }

    /// 设置根模块（整个 crate）的 DefId，并初始化父栈
    pub fn set_root(&mut self, root_node_id: NodeId) {
        let root_def = DefId {
            krate: self.crate_num,
            index: 0,
        };
        self.node_to_def.insert(root_node_id, root_def);
        self.parent_stack.push(root_def);
    }

    /// 返回收集到的所有数据
    pub fn finish(self) -> (FxHashMap<NodeId, DefId>, FxHashMap<DefId, DefId>) {
        (self.node_to_def, self.def_to_parent)
    }
}

impl Visitor for DefCollector {
    fn visit_item(&mut self, item: &Item) {
        let def_id = self.alloc_def_id();
        self.node_to_def.insert(item.node_id, def_id);

        if let Some(parent) = self.current_parent() {
            self.def_to_parent.insert(def_id, parent);
        }

        self.enter_def(def_id);

        walk_item(self, item);

        self.exit_def();
    }

    fn visit_generic_param(&mut self, param: &GenericParam) {
        let def_id = self.alloc_def_id();
        self.node_to_def.insert(param.node_id, def_id);
        if let Some(parent) = self.current_parent() {
            self.def_to_parent.insert(def_id, parent);
        }
        self.enter_def(def_id);
        walk_generic_param(self, param);
        self.exit_def();
    }

    fn visit_extern_item(&mut self, item: &ExternItem) {
        match &item.kind {
            ExternItemKind::Fn(_) => {
                let def_id = self.alloc_def_id();
                self.node_to_def.insert(item.node_id, def_id);
                if let Some(parent) = self.current_parent() {
                    self.def_to_parent.insert(def_id, parent);
                }
            }
        }
    }
}
