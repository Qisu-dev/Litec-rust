use bumpalo::Bump;
use litec_ast::ast::{Ident, NodeId, Visibility};
use litec_error::{Diag, ErrorGuaranteed};
use litec_hir::{
    adt::EnumVariantInfo,
    def::{DefKind, Res},
    def_data::DefData,
    def_path::DefPathHash,
    def_table::{DefTable, StableCrateId},
};
use litec_session::Session;
use litec_span::{
    StringId,
    id::{DefId, LocalDefId},
};
use rustc_hash::FxHashMap;
use std::ops::Deref;

use crate::{
    resolve_output::{PartialPath, ResolveOutput},
    ty::Ty,
};

#[derive(Debug, Clone)]
pub struct GlobalCtxt<'tcx> {
    pub(crate) sess: &'tcx Session,
    /// bump 分配器
    pub(crate) bump: &'tcx Bump,
    pub(crate) def_table: DefTable,
    pub(crate) ctor_map: FxHashMap<DefId, DefId>,
    pub(crate) enum_variant_map: FxHashMap<DefId, &'tcx [EnumVariantInfo]>,
    pub(crate) resolve_output: Option<ResolveOutput>,
    pub(crate) function_signatures: FxHashMap<DefId, (Vec<Ty>, Ty)>,
}

impl<'tcx> GlobalCtxt<'tcx> {
    /// 创建一个新的全局上下文。
    pub fn new(sess: &'tcx Session, bump: &'tcx Bump) -> Self {
        GlobalCtxt {
            sess,
            bump,
            def_table: DefTable::new(),
            ctor_map: FxHashMap::default(),
            enum_variant_map: FxHashMap::default(),
            resolve_output: None,
            function_signatures: FxHashMap::default(),
        }
    }

    pub fn sess(&self) -> &'tcx Session {
        self.sess
    }

    pub fn report_err(&self, diag: Diag) -> ErrorGuaranteed {
        self.sess.report_err(diag)
    }

    pub fn ty_ctxt(&'tcx self) -> TyCtxt<'tcx> {
        TyCtxt { gcx: self }
    }

    pub fn alloc<T>(&self, val: T) -> &'tcx mut T {
        self.bump.alloc(val)
    }

    pub fn alloc_slice_copy<T>(&self, slice: &[T]) -> &'tcx mut [T]
    where
        T: Copy,
    {
        self.bump.alloc_slice_copy(slice)
    }

    pub fn alloc_slice_clone<T>(&self, slice: &[T]) -> &'tcx mut [T]
    where
        T: Clone,
    {
        self.bump.alloc_slice_clone(slice)
    }

    pub fn alloc_with<T>(&self, f: impl FnOnce() -> T) -> &'tcx T {
        self.bump.alloc_with(f)
    }

    pub fn def_data(&self, def_id: DefId) -> Option<&DefData> {
        self.def_table.get(def_id)
    }

    // 获取本地定义数据
    pub fn local_def_data(&self, local_id: LocalDefId) -> &DefData {
        self.def_table.get_local(local_id)
    }

    // 通过 NodeId 查找 LocalDefId
    pub fn def_id_of(&self, node_id: NodeId) -> Option<LocalDefId> {
        self.def_table.def_id_for_node(node_id)
    }

    // 通过稳定哈希查找跨 crate 定义
    pub fn find_def_by_hash(
        &self,
        stable_crate_id: StableCrateId,
        hash: DefPathHash,
    ) -> Option<DefId> {
        self.def_table.find_by_hash(stable_crate_id, hash)
    }

    // 获取定义的稳定路径哈希
    pub fn def_path_hash(&mut self, def_id: DefId) -> DefPathHash {
        self.def_table.build_def_path(def_id.index).stable_hash()
    }

    // 获取定义名称
    pub fn def_name(&self, def_id: DefId) -> String {
        let data = self.def_data(def_id).unwrap();
        data.name.to_string()
    }

    pub fn create_local(
        &mut self,
        kind: DefKind,
        name: StringId,
        parent: Option<DefId>,
        visibility: Visibility,
    ) -> LocalDefId {
        self.def_table.create_local(kind, name, parent, visibility)
    }

    pub fn map_node(&mut self, node_id: NodeId, local_id: LocalDefId) {
        self.def_table.map_node(node_id, local_id);
    }

    pub fn record_ctor(&mut self, parent: DefId, ctor: DefId) {
        self.ctor_map.insert(parent, ctor);
    }

    pub fn ctor_of(&self, def_id: DefId) -> Option<DefId> {
        self.ctor_map.get(&def_id).copied()
    }

    pub fn record_variant_infos(&mut self, def_id: DefId, variant_infos: &'tcx [EnumVariantInfo]) {
        self.enum_variant_map.insert(def_id, variant_infos);
    }

    pub fn variants_of(&self, def_id: DefId) -> Option<&'tcx [EnumVariantInfo]> {
        self.enum_variant_map.get(&def_id).copied()
    }

    pub fn variant_of(&self, enum_def_id: DefId, name: &Ident) -> Option<&EnumVariantInfo> {
        self.variants_of(enum_def_id)?
            .iter()
            .find(|info| info.name == *name)
    }

    pub fn def_kind(&self, def_id: DefId) -> Option<DefKind> {
        self.def_table.def_kind(def_id)
    }

    pub fn visibility_of(&self, def_id: DefId) -> Option<Visibility> {
        self.def_table.visibility_of(def_id)
    }

    pub fn set_resolve_output(&mut self, output: ResolveOutput) {
        assert!(
            self.resolve_output.is_none(),
            "resolve_output 已经被设置好了"
        );
        self.resolve_output = Some(output);
    }

    /// 获取解析结果（仅供内部或 TyCtxt 使用）
    pub(crate) fn resolve_output(&self) -> &ResolveOutput {
        self.resolve_output
            .as_ref()
            .expect("resolve_output 未被设置")
    }

    pub fn store_fn_sig(&mut self, def_id: DefId, sig: (Vec<Ty>, Ty)) {
        self.function_signatures.insert(def_id, sig);
    }

    pub fn fn_sig(&self, def_id: DefId) -> Option<&(Vec<Ty>, Ty)> {
        self.function_signatures.get(&def_id)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TyCtxt<'tcx> {
    gcx: &'tcx GlobalCtxt<'tcx>,
}

impl<'tcx> TyCtxt<'tcx> {
    pub fn resolution(&self, node_id: NodeId) -> Option<&Res<NodeId>> {
        self.gcx.resolve_output().resolutions.get(&node_id)
    }

    pub fn partial_resolution(&self, node_id: NodeId) -> Option<&PartialPath> {
        self.gcx.resolve_output().partial_resolutions.get(&node_id)
    }
}

impl<'tcx> Deref for TyCtxt<'tcx> {
    type Target = GlobalCtxt<'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.gcx
    }
}
