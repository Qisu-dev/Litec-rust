use crate::{
    def::DefKind,
    def_data::DefData,
    def_path::{DefPath, DefPathHash, DefPathKind},
    metadata::{CrateKind, CrateMetadata, SerDefData},
};
use index_vec::IndexVec;
use litec_ast::ast::{NodeId, Visibility};
use litec_span::{
    StringId,
    id::{CrateNum, DefId, DefIndex, INVALID_CRATE, LOCAL_CRATE, LocalDefId},
    intern_global,
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StableCrateId(pub u64);

#[derive(Debug, Clone)]
pub struct LocalDefTable {
    crate_num: CrateNum,
    data: IndexVec<DefIndex, DefData>,
    node_map: FxHashMap<NodeId, LocalDefId>,
    hash_to_index: FxHashMap<(StableCrateId, DefPathHash), DefIndex>,
}

impl LocalDefTable {
    pub fn new(crate_num: CrateNum) -> Self {
        Self {
            crate_num,
            data: IndexVec::new(),
            node_map: FxHashMap::default(),
            hash_to_index: FxHashMap::default(),
        }
    }

    pub fn create(
        &mut self,
        kind: DefKind,
        name: StringId,
        parent: Option<DefId>,
        visibility: Visibility,
    ) -> DefId {
        let index = DefIndex::new(self.data.len());
        let path_data = match kind {
            DefKind::Crate => DefPathKind::CrateRoot,
            DefKind::Module => DefPathKind::Module(name),
            DefKind::Struct => DefPathKind::Struct(name),
            DefKind::Enum => DefPathKind::Enum(name),
            DefKind::ImplFn => DefPathKind::ImplFn(name),
            DefKind::ImplTy => DefPathKind::ImplTy(name),
            DefKind::Variant => DefPathKind::Variant(name),
            DefKind::ExternCrate => DefPathKind::ExternCrate(name),
            DefKind::Impl | DefKind::TraitImpl => DefPathKind::Impl,
            DefKind::Fn => DefPathKind::Fn(name),
            DefKind::Trait => DefPathKind::Trait(name),
            DefKind::TyAlias => DefPathKind::TyAlias(name),
            DefKind::Const => DefPathKind::Const(name),
            DefKind::Static => DefPathKind::Static(name),
            DefKind::ExternFn => DefPathKind::Fn(name),
            DefKind::TraitFn => DefPathKind::Fn(name),
            DefKind::Ctor(..) => DefPathKind::Ctor,
            DefKind::TyParam => DefPathKind::TyParam(name),
        };
        let def_id = DefId {
            krate: self.crate_num,
            index,
        };
        self.data.push(DefData {
            kind,
            name,
            parent,
            path_data,
            visibility,
        });
        def_id
    }

    /// 记录 NodeId → DefId 映射
    pub fn record_node(&mut self, node_id: NodeId, local_def_id: LocalDefId) {
        self.node_map.insert(node_id, local_def_id);
    }

    /// 通过 NodeId 查找 DefId
    pub fn def_id_for_node(&self, node_id: NodeId) -> Option<LocalDefId> {
        self.node_map.get(&node_id).copied()
    }

    /// 获取定义数据
    pub fn get(&self, index: DefIndex) -> Option<&DefData> {
        self.data.get(index)
    }

    /// 构建完整 DefPath
    pub fn build_def_path(&self, index: DefIndex) -> DefPath {
        let mut segments = Vec::new();
        let mut cur = index;
        while let Some(data) = self.get(cur) {
            segments.push(data.path_data.clone());
            if let Some(parent) = data.parent {
                cur = parent.index;
            } else {
                break;
            }
        }
        segments.reverse();
        DefPath { segments }
    }

    /// 计算所有定义的稳定哈希并建立反向索引
    pub fn compute_hashes(&mut self, stable_crate_id: StableCrateId) {
        for (idx, _) in self.data.iter().enumerate() {
            let index = DefIndex::new(idx);
            let def_path = self.build_def_path(index);
            let hash = def_path.stable_hash();
            self.hash_to_index.insert((stable_crate_id, hash), index);
        }
    }

    /// 通过稳定哈希查找 DefIndex
    pub fn find_index_by_hash(
        &self,
        stable_crate_id: StableCrateId,
        hash: DefPathHash,
    ) -> Option<DefIndex> {
        self.hash_to_index.get(&(stable_crate_id, hash)).copied()
    }

    pub fn to_metadata(
        &self,
        stable_crate_id: StableCrateId,
        crate_name: &str,
        crate_kind: CrateKind,
    ) -> CrateMetadata {
        let mut defs = Vec::new();

        for (_, data) in self.data.iter().enumerate() {
            let name_str = data.name.to_string();
            // 获取父定义的 DefPathHash（如果存在）
            let parent_hash = data.parent.and_then(|parent_def_id| {
                Some(self.build_def_path(parent_def_id.index).stable_hash())
            });
            let path_data = data.path_data.into();
            defs.push(SerDefData {
                kind: data.kind,
                name: name_str,
                parent: parent_hash,
                path_data,
                visibility: data.visibility,
            });
        }

        let def_path_hash_to_index = self
            .hash_to_index
            .iter()
            .map(|((_, hash), &idx)| (*hash, idx.raw()))
            .collect();

        CrateMetadata {
            stable_crate_id,
            crate_name: crate_name.to_string(),
            defs,
            def_path_hash_to_index,
            deps: vec![], // 可填充依赖的 stable id
            crate_kind,
        }
    }

    pub fn from_metadata(
        meta: CrateMetadata,
        def_table: &DefTable, // 用于解析 parent 哈希
    ) -> Self {
        let mut table = LocalDefTable::new(INVALID_CRATE);

        for def_meta in meta.defs {
            let name = intern_global(&def_meta.name);
            let path_data = def_meta.path_data.into();
            let parent = def_meta
                .parent
                .and_then(|hash| def_table.find_by_hash(meta.stable_crate_id, hash));
            let data = DefData {
                kind: def_meta.kind,
                name,
                parent,
                path_data,
                visibility: def_meta.visibility,
            };
            table.data.push(data);
        }
        // 重建 hash_to_index
        for (hash, raw_idx) in meta.def_path_hash_to_index {
            let index = DefIndex::new(raw_idx as usize);
            table
                .hash_to_index
                .insert((meta.stable_crate_id, hash), index);
        }
        table
    }

    pub fn visibility_of(&self, def_index: DefIndex) -> Option<Visibility> {
        self.get(def_index).map(|data| data.visibility)
    }
}

/// 全局定义表，管理当前 crate 和所有依赖 crate 的定义。
#[derive(Debug, Clone)]
pub struct DefTable {
    /// 每个 crate 的本地定义表
    local_tables: Vec<LocalDefTable>,
    /// 稳定 crate ID 到 CrateNum 的映射。
    stable_id_to_crate: FxHashMap<StableCrateId, CrateNum>,
    /// 当前正在编译的 crate 的 CrateNum
    local_crate_num: CrateNum,
}

impl DefTable {
    pub fn new() -> Self {
        let mut tables = Vec::new();
        tables.push(LocalDefTable::new(LOCAL_CRATE)); // 当前 crate，CrateNum(0)
        DefTable {
            local_tables: tables,
            stable_id_to_crate: FxHashMap::default(),
            local_crate_num: LOCAL_CRATE,
        }
    }

    /// 获取当前 crate 的本地定义表（可变）
    pub fn local_mut(&mut self) -> &mut LocalDefTable {
        &mut self.local_tables[self.local_crate_num.0 as usize]
    }

    /// 获取当前 crate 的本地定义表（只读）
    pub fn local(&self) -> &LocalDefTable {
        &self.local_tables[self.local_crate_num.0 as usize]
    }

    /// 为当前 crate 创建一个新定义
    pub fn create_local(
        &mut self,
        kind: DefKind,
        name: StringId,
        parent: Option<DefId>,
        visibility: Visibility,
    ) -> LocalDefId {
        let def_id = self.local_mut().create(kind, name, parent, visibility);
        LocalDefId::from_def_id(def_id)
    }

    /// 记录 NodeId → LocalDefId 映射。
    pub fn map_node(&mut self, node_id: NodeId, local_id: LocalDefId) {
        self.local_mut().record_node(node_id, local_id);
    }

    /// 通过 NodeId 查找 LocalDefId。
    pub fn def_id_for_node(&self, node_id: NodeId) -> Option<LocalDefId> {
        self.local().def_id_for_node(node_id)
    }

    /// 获取任意 crate 的定义数据。
    pub fn get(&self, def_id: DefId) -> Option<&DefData> {
        self.local().get(def_id.index)
    }

    /// 获取本地定义数据。
    pub fn get_local(&self, local_id: LocalDefId) -> &DefData {
        self.get(local_id.to_def_id()).unwrap()
    }

    /// 注册一个外部 crate 的定义表。
    /// 返回分配的 CrateNum
    pub fn register_crate(&mut self, stable_id: StableCrateId, table: LocalDefTable) -> CrateNum {
        let crate_num = CrateNum(self.local_tables.len() as u32);
        self.local_tables.push(table);
        self.stable_id_to_crate.insert(stable_id, crate_num);
        crate_num
    }

    /// 通过稳定 crate ID 查找 CrateNum。
    pub fn crate_num_by_stable_id(&self, stable_id: StableCrateId) -> Option<CrateNum> {
        self.stable_id_to_crate.get(&stable_id).copied()
    }

    pub fn find_by_hash(
        &self,
        stable_crate_id: StableCrateId,
        path_hash: DefPathHash,
    ) -> Option<DefId> {
        let crate_num = self.crate_num_by_stable_id(stable_crate_id)?;
        let table = &self.local_tables[crate_num.0 as usize];
        let index = table.find_index_by_hash(stable_crate_id, path_hash)?;
        Some(DefId {
            krate: crate_num,
            index,
        })
    }

    pub fn finalize_local(&mut self, stable_crate_id: StableCrateId) {
        self.local_mut().compute_hashes(stable_crate_id);
        self.stable_id_to_crate
            .insert(stable_crate_id, self.local_crate_num);
    }

    pub fn build_def_path(&mut self, index: DefIndex) -> DefPath {
        self.local_mut().build_def_path(index)
    }

    pub fn def_kind(&self, def_id: DefId) -> Option<DefKind> {
        self.local().get(def_id.index).map(|data| data.kind)
    }

    pub fn visibility_of(&self, def_id: DefId) -> Option<Visibility> {
        self.local().visibility_of(def_id.index)
    }
}
