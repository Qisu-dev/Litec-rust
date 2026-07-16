pub mod def_collector;
pub mod module;
pub mod rib;

use std::collections::hash_map;

use crate::module::{Binding, FromKind};
use crate::rib::RibKind;
use crate::{module::ModuleData, rib::Rib};
use indexmap::map::Entry;
use litec_ast::ast::*;
use litec_ast::visit::{walk_block, walk_extern, walk_fn};
use litec_ast::{
    ast::{self, NodeId},
    visit::Visitor,
};
use litec_error::{PResult, error};
use litec_hir::def::{BuiltinTrait, CtorKind, CtorOf, DefKind, Namespace, PerNS, Res};
use litec_hir::hir::{FloatTy, IntTy, PrimTy, UintTy};
use litec_middle::context::TyCtxt;
use litec_middle::resolve_output::{PartialPath, ResolveOutput};
use litec_span::StringId;
use litec_span::id::{DefId, LOCAL_CRATE};
use rustc_hash::FxHashMap;

// 路径出现的上下文
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathContext {
    Use,  // use 语句，必须完全解析
    Expr, // 表达式，允许部分解析
    Type, // 类型位置，通常要求完全解析（可根据需要调整）
}

/// 路径解析结果
#[derive(Debug, Clone)]
pub enum PathResolution {
    /// 完全解析，直接得到 `Res`
    Full(Binding),
    /// 部分解析：已解析到某个非模块项（如结构体），剩余段需要类型检查处理
    Partial(PartialPath),
}

#[derive(Debug)]
pub struct Resolver<'a> {
    tcx: TyCtxt<'a>,
    prelude_symbols: FxHashMap<StringId, Res<NodeId>>,

    modules: FxHashMap<DefId, ModuleData>,
    module_path: Vec<DefId>,
    value_ribs: Vec<Rib<Res<NodeId>>>,
    type_ribs: Vec<Rib<Res<NodeId>>>,
    results: FxHashMap<NodeId, Res<NodeId>>,
    partial_resolutions: FxHashMap<NodeId, PartialPath>,
    /// 当前所处的module, visibility, use_tree
    unresolved_uses: Vec<(DefId, Visibility, UseTree)>,
    root_module_def_id: DefId,
    current_impl: Option<DefId>,
    current_trait: Option<DefId>,
}

impl<'a> Resolver<'a> {
    pub fn new(tcx: TyCtxt<'a>) -> Self {
        Self {
            tcx,
            prelude_symbols: FxHashMap::default(),
            modules: FxHashMap::default(),
            module_path: Vec::new(),
            value_ribs: Vec::new(),
            type_ribs: Vec::new(),
            results: FxHashMap::default(),
            partial_resolutions: FxHashMap::default(),
            unresolved_uses: Vec::new(),
            root_module_def_id: DefId::default(),
            current_impl: None,
            current_trait: None,
        }
    }

    fn enter_module(&mut self, module_id: DefId) {
        if let Some(id) = self.module_path.last()
            && *id == module_id
        {
            return;
        }
        self.module_path.push(module_id);
    }

    fn exit_module(&mut self) {
        self.module_path.pop();
    }

    fn push_value_rib(&mut self, rib_kind: RibKind) {
        self.value_ribs.push(Rib::new(rib_kind));
    }
    fn pop_value_rib(&mut self) {
        self.value_ribs.pop().expect("value rib stack underflow");
    }

    fn push_type_rib(&mut self, rib_kind: RibKind) {
        self.type_ribs.push(Rib::new(rib_kind));
    }
    fn pop_type_rib(&mut self) {
        self.type_ribs.pop().expect("type rib stack underflow");
    }

    fn insert_value_binding(&mut self, ident: Ident, res: Res<NodeId>) {
        self.value_ribs.last_mut().unwrap().insert(ident, res);
    }

    fn insert_type_binding(&mut self, ident: Ident, res: Res<NodeId>) {
        self.type_ribs.last_mut().unwrap().insert(ident, res);
    }

    fn insert_module_value_binding(&mut self, ident: Ident, binding: Binding) {
        if let Some(module_id) = self.module_path.last() {
            if let Some(module) = self.modules.get_mut(&module_id) {
                module.value_rib.insert(ident, binding);
            }
        }
    }

    fn insert_module_type_binding(&mut self, ident: Ident, binding: Binding) {
        if let Some(module_id) = self.module_path.last() {
            if let Some(module) = self.modules.get_mut(&module_id) {
                module.type_rib.insert(ident, binding);
            }
        }
    }

    fn insert_module_binding(&mut self, ident: Ident, ns: Namespace, binding: Binding) {
        match ns {
            Namespace::Value => self.insert_module_value_binding(ident, binding),
            Namespace::Type => self.insert_module_type_binding(ident, binding),
        }
    }

    fn lookup_value(&self, ident: Ident) -> Option<Res<NodeId>> {
        for rib in self.value_ribs.iter().rev() {
            if let Some(res) = rib.get(&ident) {
                return Some(res.clone());
            }
        }
        if let Some(module_id) = self.module_path.last() {
            if let Some(module) = self.modules.get(&module_id) {
                if let Some(binding) = module.value_rib.get(&ident) {
                    return Some(binding.res);
                }
            }
        }
        None
    }

    fn lookup_type(&self, ident: Ident) -> Option<Res<NodeId>> {
        for rib in self.type_ribs.iter().rev() {
            if let Some(res) = rib.get(&ident) {
                return Some(res.clone());
            }
        }
        if let Some(module_id) = self.module_path.last() {
            if let Some(module) = self.modules.get(&module_id) {
                if let Some(binding) = module.type_rib.get(&ident) {
                    return Some(binding.res);
                }
            }
        }
        if let Some(prim_ty) = self.prelude_symbols.get(&ident.text) {
            return Some(*prim_ty);
        }
        None
    }

    fn lookup_value_in_module(&self, module_id: DefId, ident: Ident) -> Option<Binding> {
        self.modules
            .get(&module_id)
            .and_then(|module| module.value_rib.get(&ident).cloned())
    }

    fn lookup_type_in_module(&self, module_id: DefId, ident: Ident) -> Option<Binding> {
        self.modules
            .get(&module_id)
            .and_then(|module| module.type_rib.get(&ident).cloned())
    }

    fn lookup_in_module(&self, module_id: DefId, ident: Ident, ns: Namespace) -> Option<Binding> {
        match ns {
            Namespace::Type => self.lookup_type_in_module(module_id, ident),
            Namespace::Value => self.lookup_value_in_module(module_id, ident),
        }
    }

    fn resolve_path(&self, path: &Path, context: PathContext) -> PResult<PerNS<PathResolution>> {
        let segments = &path.segments;

        if segments.len() == 1 {
            let ident = segments[0].name;
            if context == PathContext::Expr {
                if let Some(res) = self.lookup_value(ident) {
                    let binding = Binding::new(res, Visibility::Public, FromKind::Normal);
                    return Ok(PerNS {
                        type_ns: None,
                        value_ns: Some(PathResolution::Full(binding)),
                    });
                }
            }
            if context == PathContext::Type {
                if let Some(res) = self.lookup_type(ident) {
                    let binding = Binding::new(res, Visibility::Public, FromKind::Normal);
                    return Ok(PerNS {
                        type_ns: Some(PathResolution::Full(binding)),
                        value_ns: None,
                    });
                }
            }
        }

        let first = &segments[0];

        let (start_module, remaining) = match first.name.to_string().as_str() {
            "super" => match self.module_path.as_slice() {
                &[.., super_module, _] => (super_module, &segments[1..]),
                _ => {
                    return Err(self
                        .tcx
                        .sess()
                        .report_err(error("没有父模块").with_span(first.span)));
                }
            },
            "crate" => (self.root_module_def_id, &segments[1..]),
            "self" => (*self.module_path.last().unwrap(), &segments[1..]),
            _ => (*self.module_path.last().unwrap(), segments.as_slice()),
        };

        let ((module_id, vis, from), remaining_after_modules) =
            self.resolve_module_prefix(start_module, remaining)?;

        if remaining_after_modules.is_empty() {
            let module_binding = Binding::new(Res::Def(DefKind::Module, module_id), vis, from);
            return Ok(PerNS {
                type_ns: Some(PathResolution::Full(module_binding.clone())),
                value_ns: Some(PathResolution::Full(module_binding)),
            });
        }

        let first_rem = &remaining_after_modules[0];
        let rest_rem = &remaining_after_modules[1..];

        let mut type_res = self.resolve_one_ns(module_id, first_rem, rest_rem, Namespace::Type)?;
        let mut value_res =
            self.resolve_one_ns(module_id, first_rem, rest_rem, Namespace::Value)?;

        if let Some(PathResolution::Partial(partial)) = &type_res {
            if let Res::Def(DefKind::Enum, enum_def_id) = partial.base {
                if let Some(variant_seg) = partial.remaining.first() {
                    let variant_name = variant_seg.name;
                    if let Some(variant_info) = self.tcx.variant_of(enum_def_id, &variant_name) {
                        let variant_def_id = variant_info.variant_def_id;
                        let ctor_res = match self.tcx.ctor_of(variant_def_id) {
                            Some(ctor_id) => {
                                let ctor_kind = match self.tcx.def_kind(ctor_id) {
                                    Some(DefKind::Ctor(_, kind)) => kind,
                                    _ => unreachable!(),
                                };
                                Res::Def(DefKind::Ctor(CtorOf::Variant, ctor_kind), ctor_id)
                            }
                            None => Res::Def(DefKind::Variant, variant_def_id),
                        };
                        let vis = self.tcx.visibility_of(enum_def_id).unwrap();
                        let binding = Binding::new(ctor_res, vis, FromKind::Normal);

                        value_res = Some(PathResolution::Full(binding));
                        type_res = None;
                    }
                }
            }
        }

        if context != PathContext::Expr {
            if let Some(PathResolution::Partial { .. }) = type_res {
                return Err(self
                    .tcx
                    .report_err(error("类型路径不能部分解析").with_span(path.span)));
            }
            if let Some(PathResolution::Partial { .. }) = value_res {
                return Err(self
                    .tcx
                    .report_err(error("值路径不能部分解析").with_span(path.span)));
            }
        }

        Ok(PerNS {
            type_ns: type_res,
            value_ns: value_res,
        })
    }

    fn resolve_type_path(&self, path: &Path) -> PResult<Option<Res<NodeId>>> {
        let per_ns = self.resolve_path(path, PathContext::Type)?;
        if let Some(resolution) = per_ns.type_ns {
            match resolution {
                PathResolution::Full(binding) => Ok(Some(binding.res)),
                PathResolution::Partial { .. } => Err(self
                    .tcx
                    .sess()
                    .report_err(error("应当完全解析").with_span(path.span))),
            }
        } else {
            if path.segments.len() == 1
                && let Some(res) = self.prelude_symbols.get(&path.segments[0].name.text)
            {
                return Ok(Some(res.clone()));
            }
            Ok(None)
        }
    }

    fn resolve_one_ns(
        &self,
        module_id: DefId,
        first_rem: &PathSegment,
        rest_rem: &[PathSegment],
        ns: Namespace,
    ) -> PResult<Option<PathResolution>> {
        if first_rem.name.text.to_string() == "Self" {
            match ns {
                Namespace::Type => {
                    if let Some(impl_def_id) = self.current_impl {
                        let binding = Binding::new(
                            Res::SelfTyAlias {
                                alias_to: impl_def_id,
                            },
                            Visibility::Public,
                            FromKind::Normal,
                        );
                        return Ok(Some(if rest_rem.is_empty() {
                            PathResolution::Full(binding)
                        } else {
                            PathResolution::Partial(PartialPath {
                                base: binding.res,
                                remaining: rest_rem.to_vec(),
                            })
                        }));
                    } else if let Some(trait_def_id) = self.current_trait {
                        let binding = Binding::new(
                            Res::SelfTyParam {
                                trait_: trait_def_id,
                            },
                            Visibility::Public,
                            FromKind::Normal,
                        );
                        return Ok(Some(if rest_rem.is_empty() {
                            PathResolution::Full(binding)
                        } else {
                            PathResolution::Partial(PartialPath {
                                base: binding.res,
                                remaining: rest_rem.to_vec(),
                            })
                        }));
                    } else {
                        return Err(self.tcx.report_err(
                            error("`Self` only allowed in traits and impls")
                                .with_span(first_rem.span),
                        ));
                    }
                }
                Namespace::Value => {
                    if let Some(impl_def_id) = self.current_impl {
                        let binding = Binding::new(
                            Res::SelfCtor(impl_def_id),
                            Visibility::Public,
                            FromKind::Normal,
                        );
                        return Ok(Some(if rest_rem.is_empty() {
                            PathResolution::Full(binding)
                        } else {
                            PathResolution::Partial(PartialPath {
                                base: binding.res,
                                remaining: rest_rem.to_vec(),
                            })
                        }));
                    } else {
                        return Err(self.tcx.report_err(
                            error("`Self` 函数调用只能在 impl 中").with_span(first_rem.span),
                        ));
                    }
                }
            }
        }

        let binding = match self.lookup_in_module(module_id, first_rem.name, ns) {
            Some(b) => b,
            None => return Ok(None),
        };

        if module_id != *self.module_path.last().unwrap() && binding.vis != Visibility::Public {
            return Err(self.tcx.sess().report_err(
                error(format!("项 `{}` 是 private", first_rem.name.to_string()))
                    .with_span(first_rem.span),
            ));
        }

        match rest_rem.is_empty() {
            true => Ok(Some(PathResolution::Full(binding))),
            false => Ok(Some(PathResolution::Partial(PartialPath {
                base: binding.res,
                remaining: rest_rem.to_vec(),
            }))),
        }
    }

    fn lookup_module(
        &self,
        module_def_id: DefId,
        name: Ident,
    ) -> Option<(DefId, Visibility, FromKind)> {
        if module_def_id.krate == LOCAL_CRATE {
            self.modules
                .get(&module_def_id)?
                .submodules
                .get(&name)
                .copied()
        } else {
            None
        }
    }

    fn resolve_module_prefix<'b>(
        &self,
        start_module: DefId,
        segments: &'b [PathSegment],
    ) -> PResult<((DefId, Visibility, FromKind), &'b [PathSegment])> {
        let mut current = start_module;
        let mut vis = Visibility::Inherited;
        let mut from = FromKind::Normal;
        let mut idx = 0;
        for (i, seg) in segments.iter().enumerate() {
            if let Some((sub_id, vis_, from_)) = self.lookup_module(current, seg.name) {
                if seg.generic_args.is_some() {
                    return Err(self
                        .tcx
                        .sess()
                        .report_err(error("模块路径不应有泛型参数").with_span(seg.span)));
                }
                // 跨模块可见性检查
                if current != *self.module_path.last().unwrap() && vis_ != Visibility::Public {
                    return Err(self.tcx.sess().report_err(
                        error(format!("模块 `{}` 不可访问", seg.name.to_string()))
                            .with_span(seg.span),
                    ));
                }
                current = sub_id;
                vis = vis_;
                from = from_;
                idx = i + 1;
            } else {
                break;
            }
        }
        Ok(((current, vis, from), &segments[idx..]))
    }

    fn resolve_module_path(&self, path: &Path) -> Option<DefId> {
        let mut current_module_id = self.module_path.last()?;
        for seg in &path.segments {
            if let Some((module_def_id, _, _)) =
                self.modules[&current_module_id].submodules.get(&seg.name)
            {
                current_module_id = module_def_id;
            } else {
                return None;
            }
        }
        Some(*current_module_id)
    }

    fn def_id(&self, node_id: NodeId) -> DefId {
        self.tcx
            .def_id_of(node_id)
            .unwrap_or_else(|| panic!("DefId not found for node {:?}", node_id))
            .to_def_id()
    }

    fn register_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Fn(fn_) => {
                self.insert_module_binding(
                    fn_.sig.name,
                    Namespace::Value,
                    Binding::new(
                        Res::Def(DefKind::Fn, self.def_id(item.node_id)),
                        item.visibility,
                        FromKind::Normal,
                    ),
                );
            }
            ItemKind::Struct(ident, _, kind) => {
                let struct_def_id = self.def_id(item.node_id);
                let ty_binding = Binding::new(
                    Res::Def(DefKind::Struct, struct_def_id),
                    item.visibility,
                    FromKind::Normal,
                );
                self.insert_module_type_binding(*ident, ty_binding);

                if let Some(ctor_def_id) = self.tcx.ctor_of(struct_def_id) {
                    let ctor_kind = match kind {
                        StructKind::Unit => CtorKind::Const,
                        StructKind::Tuple(_) => CtorKind::Fn,
                        _ => unreachable!(),
                    };
                    let ctor_res = Res::Def(DefKind::Ctor(CtorOf::Struct, ctor_kind), ctor_def_id);
                    let ctor_binding = Binding::new(ctor_res, item.visibility, FromKind::Normal);
                    self.insert_module_value_binding(*ident, ctor_binding);
                }
            }
            ItemKind::Use(use_tree) => {
                self.unresolved_uses.push((
                    self.module_path[self.module_path.len() - 1],
                    item.visibility,
                    use_tree.clone(),
                ));
            }
            ItemKind::Extern(extern_) => {
                for extern_item in &extern_.items {
                    match &extern_item.kind {
                        ExternItemKind::Fn(fn_) => {
                            self.insert_module_binding(
                                fn_.sig.name,
                                Namespace::Value,
                                Binding::new(
                                    Res::Def(DefKind::Fn, self.def_id(item.node_id)),
                                    item.visibility,
                                    FromKind::Normal,
                                ),
                            );
                        }
                    }
                }
            }
            ItemKind::Module(ident, inline) => {
                let def_id = self.def_id(item.node_id);
                if self.modules[&self.module_path.last().unwrap()]
                    .submodules
                    .contains_key(ident)
                {
                    self.tcx
                        .sess()
                        .report(error("重复定义模块").with_span(ident.span));
                    return;
                }
                self.modules.insert(def_id, ModuleData::new());
                self.modules
                    .get_mut(&self.module_path.last().unwrap())
                    .unwrap()
                    .submodules
                    .insert(*ident, (def_id, item.visibility, FromKind::Normal));
                self.enter_module(def_id);
                match inline {
                    Inline::External(items) | Inline::Inline(items) => {
                        for item in items {
                            self.register_item(item);
                        }
                    }
                }
                self.exit_module();
            }
            ItemKind::Impl(_impl) => {}
            ItemKind::TypeAlias(type_alias) => {
                self.insert_module_type_binding(
                    type_alias.name,
                    Binding::new(
                        Res::Def(DefKind::TyAlias, self.def_id(item.node_id)),
                        item.visibility,
                        FromKind::Normal,
                    ),
                );
            }
            ItemKind::Trait(ident, _, _items) => {
                self.insert_module_type_binding(
                    *ident,
                    Binding::new(
                        Res::Def(DefKind::Trait, self.def_id(item.node_id)),
                        item.visibility,
                        FromKind::Normal,
                    ),
                );
            }
            ItemKind::Enum(ident, _generics, _variants) => {
                let enum_def_id = self.def_id(item.node_id);

                let ty_binding = Binding::new(
                    Res::Def(DefKind::Enum, enum_def_id),
                    item.visibility,
                    FromKind::Normal,
                );
                self.insert_module_type_binding(*ident, ty_binding);
            }
        }
    }

    fn resolve_uses(&mut self) {
        let mut unresolved = std::mem::take(&mut self.unresolved_uses);
        let mut changed = true;
        const MAX_ITER: usize = 1000;

        for _ in 0..MAX_ITER {
            if !changed {
                break;
            }
            changed = false;
            let mut still_unresolved = Vec::new();

            for (def_id, vis, use_tree) in unresolved {
                self.enter_module(def_id);
                // resolve_use 返回 true 表示该 use 需要保留到下一轮
                if self.resolve_use(vis, &use_tree, &mut changed) {
                    still_unresolved.push((def_id, vis, use_tree));
                }
                self.exit_module();
            }

            unresolved = still_unresolved;
            if unresolved.is_empty() {
                break;
            }
        }

        for (_, _, use_tree) in unresolved {
            match use_tree.kind {
                UseTreeKind::Glob => {}
                _ => {
                    self.tcx
                        .sess()
                        .report(error("无法解析导入").with_span(use_tree.span));
                }
            }
        }
    }

    fn resolve_use(&mut self, vis: Visibility, use_tree: &UseTree, changed: &mut bool) -> bool {
        match &use_tree.kind {
            UseTreeKind::Glob => {
                let module_def_id = match self.resolve_module_path(&use_tree.prefix) {
                    Some(id) => id,
                    None => {
                        return true;
                    }
                };
                let module = self.modules.get(&module_def_id).unwrap();

                let submodules_to_insert: Vec<_> = module
                    .submodules
                    .iter()
                    .filter(|(_, (_, sub_vis, _))| *sub_vis == Visibility::Public)
                    .map(|(k, (def_id, _, _))| (*k, *def_id))
                    .collect();
                let type_bindings_to_insert: Vec<_> = module
                    .type_rib
                    .bindings
                    .iter()
                    .filter(|(_, b)| b.vis == Visibility::Public)
                    .map(|(k, b)| (*k, b.clone()))
                    .collect();
                let value_bindings_to_insert: Vec<_> = module
                    .value_rib
                    .bindings
                    .iter()
                    .filter(|(_, b)| b.vis == Visibility::Public)
                    .map(|(k, b)| (*k, b.clone()))
                    .collect();

                let current = self
                    .modules
                    .get_mut(&self.module_path.last().unwrap())
                    .unwrap();
                let mut any_inserted = false;

                // 插入子模块
                for (ident, def_id) in submodules_to_insert {
                    if let std::collections::hash_map::Entry::Vacant(e) =
                        current.submodules.entry(ident)
                    {
                        e.insert((def_id, vis, FromKind::GlobImport));
                        any_inserted = true;
                    }
                }
                // 插入类型
                for (ident, mut binding) in type_bindings_to_insert {
                    if let Entry::Vacant(e) = current.type_rib.bindings.entry(ident) {
                        binding.vis = vis; // 使用 use 的可见性覆盖原可见性？实际上导入后，导入项的可见性由 use 的 vis 决定
                        binding.from = FromKind::GlobImport;
                        e.insert(binding);
                        any_inserted = true;
                    }
                }
                // 插入值
                for (ident, mut binding) in value_bindings_to_insert {
                    if let Entry::Vacant(e) = current.value_rib.bindings.entry(ident) {
                        binding.vis = vis;
                        binding.from = FromKind::GlobImport;
                        e.insert(binding);
                        any_inserted = true;
                    }
                }

                if any_inserted {
                    *changed = true;
                }

                true
            }

            UseTreeKind::Simple(rename) => {
                let target_name =
                    rename.unwrap_or_else(|| use_tree.prefix.segments.last().unwrap().name);
                let per_ns = match self.resolve_path(&use_tree.prefix, PathContext::Use) {
                    Ok(p) => p,
                    Err(_) => return true,
                };

                let current = self
                    .modules
                    .get_mut(&self.module_path.last().unwrap())
                    .unwrap();
                let mut any_inserted = false;

                // 辅助：插入模块到 submodules
                let insert_submodule = |current: &mut ModuleData,
                                        name: Ident,
                                        def_id: DefId,
                                        vis: Visibility,
                                        from: FromKind|
                 -> bool {
                    if let hash_map::Entry::Vacant(e) = current.submodules.entry(name) {
                        e.insert((def_id, vis, from));
                        true
                    } else {
                        false
                    }
                };

                match per_ns.type_ns {
                    Some(PathResolution::Full(binding)) => {
                        if let Res::Def(DefKind::Module, def_id) = binding.res {
                            if insert_submodule(current, target_name, def_id, vis, binding.from) {
                                any_inserted = true;
                            }
                        }
                        if let Entry::Vacant(e) = current.type_rib.bindings.entry(target_name) {
                            let mut new_binding = binding;
                            new_binding.vis = vis;
                            new_binding.from = FromKind::Normal;
                            e.insert(new_binding);
                            any_inserted = true;
                        }
                    }
                    Some(PathResolution::Partial { .. }) => {
                        self.tcx.sess().report_err(
                            error("cannot use associated item in `use`").with_span(use_tree.span),
                        );
                    }
                    None => {}
                }

                match per_ns.value_ns {
                    Some(PathResolution::Full(binding)) => {
                        if let Res::Def(DefKind::Module, def_id) = binding.res {
                            if !current.submodules.contains_key(&target_name) {
                                if insert_submodule(current, target_name, def_id, vis, binding.from)
                                {
                                    any_inserted = true;
                                }
                            }
                        }
                        if let Entry::Vacant(e) = current.value_rib.bindings.entry(target_name) {
                            let mut new_binding = binding;
                            new_binding.vis = vis;
                            new_binding.from = FromKind::Normal;
                            e.insert(new_binding);
                            any_inserted = true;
                        }
                    }
                    Some(PathResolution::Partial { .. }) => {
                        self.tcx.sess().report_err(
                            error("cannot use associated item in `use`").with_span(use_tree.span),
                        );
                    }
                    None => {}
                }

                if any_inserted {
                    *changed = true;
                }

                false
            }

            UseTreeKind::Nested(use_trees, _) => {
                let mut any_unresolved = false;
                // 对于嵌套导入，每个子项独立解析，前缀是当前前缀 + 子前缀
                for child_tree in use_trees {
                    let new_prefix = Path {
                        node_id: DUMMY_NODE_ID,
                        segments: [
                            use_tree.prefix.segments.clone(),
                            child_tree.prefix.segments.clone(),
                        ]
                        .concat(),
                        span: use_tree.span,
                    };
                    let child_use_tree = UseTree {
                        node_id: DUMMY_NODE_ID,
                        prefix: new_prefix,
                        kind: child_tree.kind.clone(),
                        span: child_tree.span,
                    };
                    if self.resolve_use(vis, &child_use_tree, changed) {
                        any_unresolved = true;
                    }
                }
                any_unresolved // 如果任何子项未解析，整体保留
            }
        }
    }

    pub fn resolve_crate(&mut self, ast: &ast::Crate) {
        self.init_prelude();
        self.early_resolve(ast);
        self.late_resolve(ast);
    }

    pub fn take_output(&mut self) -> ResolveOutput {
        ResolveOutput {
            resolutions: std::mem::take(&mut self.results),
            partial_resolutions: std::mem::take(&mut self.partial_resolutions),
        }
    }

    fn init_prelude(&mut self) {
        use BuiltinTrait::*;
        use PrimTy::*;

        let symbols = [
            ("i8", Int(IntTy::I8)),
            ("i16", Int(IntTy::I16)),
            ("i32", Int(IntTy::I32)),
            ("i64", Int(IntTy::I64)),
            ("i128", Int(IntTy::I128)),
            ("isize", Int(IntTy::Isize)),
            ("u8", Uint(UintTy::U8)),
            ("u16", Uint(UintTy::U16)),
            ("u32", Uint(UintTy::U32)),
            ("u64", Uint(UintTy::U64)),
            ("u128", Uint(UintTy::U128)),
            ("usize", Uint(UintTy::Usize)),
            ("f32", Float(FloatTy::F32)),
            ("f64", Float(FloatTy::F64)),
            ("bool", Bool),
            ("char", Char),
            ("str", Str)
        ];

        for (name, prim) in symbols {
            let id = StringId::from(name);
            self.prelude_symbols.insert(id, Res::PrimTy(prim));
        }

        let traits = [
            ("Add", Add),
            ("Sub", Sub),
            ("Mul", Mul),
            ("Div", Div),
            ("Rem", Rem),
            ("Neg", Neg),
            ("Not", Not),
            ("Clone", Clone),
            ("Copy", Copy),
            ("Default", Default),
        ];

        for (name, builtin_trait) in traits {
            let id = StringId::from(name);
            self.prelude_symbols
                .insert(id, Res::BuiltinTrait(builtin_trait));
        }
    }

    fn early_resolve(&mut self, ast: &ast::Crate) {
        let def_id = self.tcx.def_id_of(ast.node_id).unwrap().to_def_id();
        self.root_module_def_id = def_id;
        self.modules.insert(def_id, ModuleData::new());
        self.enter_module(def_id);
        for item in &ast.items {
            self.register_item(item);
        }
        self.resolve_uses();

        self.exit_module();
    }

    fn late_resolve(&mut self, ast: &ast::Crate) {
        self.enter_module(self.root_module_def_id);
        self.visit_crate(ast);
        self.exit_module();
    }
}

impl<'a> Visitor for Resolver<'a> {
    fn visit_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Fn(fn_) => {
                self.push_type_rib(RibKind::Function);
                self.push_value_rib(RibKind::Function);
                walk_fn(self, fn_);
                self.pop_value_rib();
                self.pop_type_rib();
            }
            ItemKind::Struct(_ident, generics, struct_kind) => {
                self.push_type_rib(RibKind::Struct);

                self.visit_generics(generics);

                self.visit_struct_kind(struct_kind);
                self.pop_type_rib();
            }
            ItemKind::Use(_) => {}
            ItemKind::Extern(ext) => {
                walk_extern(self, ext);
            }
            ItemKind::Module(_ident, inline) => {
                let mod_def_id = self.def_id(item.node_id);
                self.enter_module(mod_def_id);
                match inline {
                    Inline::External(items) | Inline::Inline(items) => {
                        for item in items {
                            self.visit_item(item);
                        }
                    }
                }
                self.exit_module();
            }
            ItemKind::Impl(impl_) => {
                let impl_def_id = self.def_id(item.node_id);
                self.current_impl = Some(impl_def_id);
                self.push_type_rib(RibKind::Normal);
                self.visit_generics(&impl_.generics);
                self.visit_ty(&impl_.self_ty);
                for impl_item in &impl_.items {
                    self.visit_impl_item(impl_item);
                }
                self.pop_type_rib();
                self.current_impl = None;
            }
            ItemKind::Trait(_ident, generics, items) => {
                self.push_type_rib(RibKind::Trait);

                let trait_def_id = self.def_id(item.node_id);
                self.current_trait = Some(trait_def_id);

                self.visit_generics(generics);
                for trait_item in items {
                    self.visit_trait_item(trait_item);
                }

                self.current_trait = None;
                self.pop_type_rib();
            }
            ItemKind::TypeAlias(type_alias) => {
                self.push_type_rib(RibKind::TyAlias);
                self.visit_generics(&type_alias.generics);

                self.visit_ty(&type_alias.ty);
                self.pop_type_rib();
            }
            ItemKind::Enum(_ident, generics, variants) => {
                self.push_type_rib(RibKind::Enum);
                self.visit_generics(generics);
                for variant in variants {
                    self.visit_variant(variant);
                }
                self.pop_type_rib();
            }
        }
    }

    fn visit_generics(&mut self, generic_params: &Generics) {
        for param in &generic_params.params {
            let def_id = self.tcx.def_id_of(param.node_id).unwrap().to_def_id();
            self.insert_type_binding(param.name, Res::Def(DefKind::TyParam, def_id));

            if let Some(bounds) = &param.bounds {
                self.visit_bounds(bounds);
            }
        }
    }

    fn visit_fn(&mut self, fn_: &Fn) {
        self.push_value_rib(RibKind::Function);

        walk_fn(self, fn_);

        self.pop_value_rib();
    }

    fn visit_param(&mut self, param: &Param) {
        match &param.kind {
            ParamKind::Normal(pat, ty) => {
                self.visit_pat(pat);
                self.visit_ty(ty);
            }
            ParamKind::SelfPtr(_mutability) | ParamKind::SelfValue(_mutability) => {
                self.insert_value_binding(
                    Ident {
                        text: "self".into(),
                        span: param.span,
                    },
                    Res::Local(param.node_id),
                );
            }
        }
    }

    fn visit_pat(&mut self, pat: &Pat) {
        match &pat.kind {
            PatKind::Wild => {}
            PatKind::Ident(_mutability, ident) => {
                if let Some(res) = self.lookup_value(*ident) {
                    self.results.insert(pat.node_id, res);
                } else {
                    let res = Res::Local(pat.node_id);
                    self.insert_value_binding(*ident, res);
                    self.results.insert(pat.node_id, res);
                }
            }
            PatKind::Tuple(pats) => {
                for p in pats {
                    self.visit_pat(p);
                }
            }
            PatKind::Struct(path, field_pats, _) => {
                let per_ns = match self.resolve_path(path, PathContext::Type) {
                    Ok(p) => p,
                    Err(_) => return,
                };
                // 模式中路径必须是完全解析的类型（结构体）
                match per_ns.type_ns {
                    Some(PathResolution::Full(binding)) => {
                        self.results.insert(pat.node_id, binding.res);
                    }
                    _ => {
                        self.tcx
                            .sess()
                            .report_err(error("expected struct type").with_span(path.span));
                        return;
                    }
                }
                for field_pat in field_pats {
                    self.visit_pat(&field_pat.pat);
                }
            }
            PatKind::Enum(path, inner_pat) => {
                let per_ns = match self.resolve_path(path, PathContext::Type) {
                    Ok(p) => p,
                    Err(_) => return,
                };
                match per_ns.type_ns {
                    Some(PathResolution::Full(binding)) => {
                        self.results.insert(pat.node_id, binding.res);
                    }
                    _ => {
                        self.tcx
                            .sess()
                            .report_err(error("expected enum variant").with_span(path.span));
                        return;
                    }
                }
                if let Some(inner) = inner_pat {
                    self.visit_pat(inner);
                }
            }
            PatKind::Lit(_) => {}
            PatKind::Range(start, end, _) => {
                self.visit_expr(start);
                self.visit_expr(end);
            }
            PatKind::Or(pats) => {
                for subpat in pats {
                    self.visit_pat(subpat);
                }
            }
        }
    }

    fn visit_block(&mut self, block: &Block) {
        self.push_value_rib(RibKind::Normal);

        walk_block(self, block);

        self.pop_value_rib();
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Expr(expr) | StmtKind::Semi(expr) => self.visit_expr(expr),
            StmtKind::Let(pat, ty, expr) => {
                self.visit_pat(pat);
                if let Some(ty) = ty {
                    self.visit_ty(ty);
                }
                if let Some(expr) = expr {
                    self.visit_expr(expr);
                }
            }
            StmtKind::Defer(expr) => {
                self.visit_expr(expr);
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Binary(left, _, right) => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::Unary(_, expr) => {
                self.visit_expr(expr);
            }
            ExprKind::Literal(_) => {}
            ExprKind::Grouped(expr) => {
                self.visit_expr(expr);
            }
            ExprKind::Assignment(target, from) => {
                self.visit_expr(from);
                self.visit_expr(target);
            }
            ExprKind::AssignmentWithOp(target, _, from) => {
                self.visit_expr(from);
                self.visit_expr(target);
            }
            ExprKind::Call(fn_, args) => {
                self.visit_expr(fn_);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::Block(block) => {
                self.visit_block(block);
            }
            ExprKind::If(conditioin, then, else_) => {
                self.visit_expr(conditioin);
                self.visit_block(then);
                if let Some(else_) = else_ {
                    self.visit_expr(else_);
                }
            }
            ExprKind::While(condtion, body) => {
                self.visit_expr(condtion);
                self.visit_block(body);
            }
            ExprKind::For {
                variable,
                iter,
                body,
            } => {
                self.visit_pat(variable);
                self.visit_expr(iter);
                self.visit_block(body);
            }
            ExprKind::Index(indexed, index) => {
                self.visit_expr(indexed);
                self.visit_expr(index);
            }
            ExprKind::Range(start, end, _range_limits) => {
                self.visit_expr(start);
                self.visit_expr(end);
            }
            ExprKind::Loop(block) => {
                self.visit_block(block);
            }
            ExprKind::Field(expr, _ident) => {
                self.visit_expr(expr);
            }
            ExprKind::Path(path) => {
                self.visit_path(path);

                if let Ok(result) = self.resolve_path(path, PathContext::Expr) {
                    if let Some(resolution) = result.value_ns {
                        match resolution {
                            PathResolution::Full(binding) => {
                                self.results.insert(path.node_id, binding.res);
                            }
                            PathResolution::Partial(parial_path) => {
                                self.partial_resolutions.insert(path.node_id, parial_path);
                            }
                        }
                    } else {
                        self.tcx.sess().report_err(
                            error(format!("未知的值 `{}`", path.to_string())).with_span(path.span),
                        );
                    }
                } else {
                    self.tcx.sess().report_err(
                        error(format!("未知的值 `{}`", path.to_string())).with_span(path.span),
                    );
                }
            }
            ExprKind::Bool(_) => {}
            ExprKind::Tuple(exprs) => {
                for expr in exprs {
                    self.visit_expr(expr);
                }
            }
            ExprKind::Unit => {}
            ExprKind::AddressOf(expr) => {
                self.visit_expr(expr);
            }
            ExprKind::StructExpr(struct_expr) => {
                self.visit_struct_expr(struct_expr);
            }
            ExprKind::Cast(expr, ty) => {
                self.visit_expr(expr);
                self.visit_ty(ty);
            }
            ExprKind::Match(expr, arms) => {
                self.visit_expr(expr);
                for arm in arms {
                    self.visit_arm(arm);
                }
            }
            ExprKind::Return(expr) => {
                if let Some(expr) = expr {
                    self.visit_expr(expr);
                }
            }
            ExprKind::Continue => {}
            ExprKind::Break(expr) => {
                if let Some(expr) = expr {
                    self.visit_expr(expr);
                }
            }
        }
    }

    fn visit_ty(&mut self, ty: &Ty) {
        match &ty.kind {
            TyKind::Path { path } => {
                self.visit_path(path);

                let Ok(result) = self.resolve_type_path(path) else {
                    return;
                };

                match result {
                    Some(res) => {
                        self.results.insert(ty.node_id, res);
                        self.results.insert(path.node_id, res);
                    }
                    None => {
                        self.tcx.sess().report_err(
                            error(format!("未知的类型 `{}`", path.to_string()))
                                .with_span(path.span),
                        );
                        return;
                    }
                }
            }
            TyKind::Never => {}
            TyKind::Unit => {}
            TyKind::Ptr { mutability: _, ty } => {
                self.visit_ty(ty);
            }
            TyKind::Array { elem, len } => {
                self.visit_ty(elem);
                self.visit_expr(len);
            }
            TyKind::Slice { elem } => {
                self.visit_ty(elem);
            }
            TyKind::Tuple { elems } => {
                for elem in elems {
                    self.visit_ty(elem);
                }
            }
            TyKind::FnPtr { inputs, output } => {
                for input in inputs {
                    self.visit_ty(input);
                }
                self.visit_ty(output);
            }
            TyKind::SelfTy => {
                if let Some(impl_def_id) = self.current_impl {
                    self.results.insert(
                        ty.node_id,
                        Res::SelfTyAlias {
                            alias_to: impl_def_id,
                        },
                    );
                } else if let Some(trait_def_id) = self.current_trait {
                    self.results.insert(
                        ty.node_id,
                        Res::SelfTyParam {
                            trait_: trait_def_id,
                        },
                    );
                } else {
                    self.tcx
                        .sess()
                        .report_err(error("`Self`只能出现在 trait 和 impl 中").with_span(ty.span));
                }
            }
        }
    }

    fn visit_arm(&mut self, arm: &Arm) {
        self.visit_pat(&arm.pat);
        self.visit_expr(&arm.body);
    }

    fn visit_bounds(&mut self, bounds: &Bounds) {
        for bound in &bounds.bounds {
            if let Ok(Some(res)) = self.resolve_type_path(bound) {
                self.results.insert(bound.node_id, res);
            } else {
                self.tcx.sess().report_err(
                    error(format!("无法解析 trait `{}`", bound.to_string())).with_span(bound.span),
                );
            }
        }
    }

    fn visit_path_segment(&mut self, segment: &PathSegment) {
        if let Some(generic_args) = &segment.generic_args {
            for arg in &generic_args.args {
                match arg {
                    GenericArg::Type(ty) => self.visit_ty(ty),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Resolver;
    use crate::def_collector::DefCollector;
    use bumpalo::Bump;
    use litec_ast::ast::*;
    use litec_hir::def::{DefKind, Namespace, Res};
    use litec_middle::context::GlobalCtxt;
    use litec_parse::{node_collector::NodeCollector, parser::Parser};
    use litec_session::Session;
    use litec_span::{SourceMap, Span};
    use std::path::Path;

    fn with_resolver<F>(src: &str, f: F)
    where
        F: FnOnce(&Resolver, &Crate),
    {
        let source_map = SourceMap::new();
        let session = Session::new(source_map);
        let file = session.mut_source_map().add_file(
            "test".into(),
            src.to_string(),
            &Path::new("test.lt"),
        );
        let parser = Parser::new(&session, file);
        let mut krate = parser.parse();
        let mut node_collector = NodeCollector::new();
        node_collector.collect(&mut krate);

        let bump = Bump::new();
        let mut gcx = GlobalCtxt::new(&session, &bump);

        let def_collector = DefCollector::new(&mut gcx);
        def_collector.collect(&krate);

        let mut resolver = Resolver::new(gcx.ty_ctxt());
        resolver.resolve_crate(&krate);
        session.diag_ctxt().flush();

        f(&resolver, &krate);
    }

    #[test]
    fn test_glob_import() {
        let src = r#"
        mod foo {
            pub fn bar() {}
            pub struct Baz;
        }
        use foo::*;
        fn main() {
            bar();
            let _ = Baz;
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let bar = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "bar".into(),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(bar.is_some());
            let baz = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Baz".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            assert!(baz.is_some());
        });
    }

    #[test]
    fn test_partial_path_storage() {
        let src = r#"
        struct Vec<T>;
        impl<T> Vec<T> {
            fn new() -> Self { Vec }
        }
        fn main() {
            let v = Vec::new();
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            assert!(!resolver.partial_resolutions.is_empty());
        });
    }

    #[test]
    fn test_type_path_resolution() {
        let src = r#"
        struct Foo;
        fn main() {
            let x: Foo = Foo;
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            // 表达式位置 Foo 应解析为构造函数（值）
            let expr_foo = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Foo".into(),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(expr_foo.is_some());

            // 类型位置 Foo 应解析为结构体
            let type_foo = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Foo".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            assert!(type_foo.is_some());

            match type_foo.unwrap().res {
                Res::Def(DefKind::Struct, _) => {}
                _ => panic!("Expected struct definition"),
            }
        });
    }

    #[test]
    fn test_private_access_error() {
        let src = r#"
        mod bar {
            fn baz() {}
        }
        fn main() {
            bar::baz();
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            assert!(resolver.tcx.sess().diag_ctxt().diags_count() != 0);
        });
    }

    #[test]
    fn test_unresolved_path_error() {
        let src = r#"
        fn main() {
            let x = unknown_variable;
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            assert!(resolver.tcx.sess().diag_ctxt().diags_count() != 0);
        });
    }

    #[test]
    fn test_nested_module_visibility() {
        let src = r#"
        mod outer {
            pub mod inner {
                pub fn func() {}
            }
        }
        use outer::inner::func;
        fn main() {
            outer::inner::func();
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let func = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "func".into(),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(func.is_some());
            match func.unwrap().res {
                Res::Def(DefKind::Fn, _) => {}
                _ => panic!("Expected function definition"),
            }
        });
    }

    #[test]
    fn test_struct_generics() {
        let src = r#"
        struct Gen<T> {
            field: T,
        }
        fn main() {
            let x: Gen<i32>;
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let gen_ty = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Gen".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            assert!(gen_ty.is_some());
            match gen_ty.unwrap().res {
                Res::Def(DefKind::Struct, _) => {}
                _ => panic!("Expected struct definition"),
            }
        });
    }

    #[test]
    fn test_enum_generics() {
        let src = r#"
        enum Option<T> {
            Some(T),
            None,
        }
        fn main() {
            let x: Option<i32>;
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let option = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Option".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            assert!(option.is_some());
            match option.unwrap().res {
                Res::Def(DefKind::Enum, _) => {}
                _ => panic!("Expected enum definition"),
            }
        });
    }

    #[test]
    fn test_type_alias() {
        let src = r#"
        type Alias = i32;
        fn main() {
            let x: Alias = 0;
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let alias = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Alias".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            assert!(alias.is_some());
            match alias.unwrap().res {
                Res::Def(DefKind::TyAlias, _) => {}
                _ => panic!("Expected type alias"),
            }
        });
    }

    #[test]
    fn test_use_as_rename() {
        let src = r#"
        mod foo {
            pub fn bar() {}
        }
        use foo::bar as baz;
        fn main() {
            baz();
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let baz = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "baz".into(),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(baz.is_some());
            match baz.unwrap().res {
                Res::Def(DefKind::Fn, _) => {}
                _ => panic!("Expected function"),
            }
        });
    }

    #[test]
    fn test_nested_use_glob() {
        let src = r#"
        mod a {
            pub mod b {
                pub fn f() {}
            }
        }
        use a::*;
        fn main() {
            b::f();
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let b = resolver.modules[&resolver.root_module_def_id]
                .submodules
                .get(&Ident {
                    text: "b".into(),
                    span: Span::default(),
                });
            assert!(b.is_some());

            let f = resolver.lookup_in_module(
                b.unwrap().0,
                Ident {
                    text: "f".into(),
                    span: Span::default(),
                },
                Namespace::Value,
            );

            assert!(f.is_some())
        });
    }

    #[test]
    fn test_self_type_in_impl() {
        let src = r#"
        struct Foo;
        impl Foo {
            fn make() -> Self {
                Self
            }
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let self_res = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Self".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            // Self 不应出现在模块作用域，只应在 impl 内部解析
            assert!(self_res.is_none());
            // 没有错误表示解析成功
            assert_eq!(resolver.tcx.sess().diag_ctxt().diags_count(), 0);
        });
    }

    #[test]
    fn test_shadowing() {
        let src = r#"
        fn main() {
            let x = 1;
            {
                let x = 2;
                let y = x;
            }
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            assert_eq!(resolver.tcx.sess().diag_ctxt().diags_count(), 0);
        });
    }

    #[test]
    fn test_enum_variant_use() {
        let src = r#"
        enum Option<T> {
            Some(T),
            None,
        }
        use Option::Some;
        fn main() {
            let x = Some(42);
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let some = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Some".into(),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(some.is_some());
            match some.unwrap().res {
                Res::Def(DefKind::Ctor(_, _), _) => {}
                _ => panic!("Expected constructor"),
            }
        });
    }

    #[test]
    fn test_associated_function_via_type() {
        let src = r#"
        struct Foo;
        impl Foo {
            fn new() -> Self { Foo }
        }
        fn main() {
            let f = Foo::new();
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            assert!(!resolver.partial_resolutions.is_empty());
            assert_eq!(resolver.tcx.sess().diag_ctxt().diags_count(), 0);
        });
    }

    #[test]
    fn test_trait_self_type() {
        let src = r#"
        trait Shape {
            fn area(self) -> Self;
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let self_ty = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Self".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            assert!(self_ty.is_none());
            assert_eq!(resolver.tcx.sess().diag_ctxt().diags_count(), 0);
        });
    }

    #[test]
    fn test_generic_bounds() {
        let src = r#"
        trait Clone {}
        struct Foo<T: Clone> {
            field: T,
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let clone = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "Clone".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            assert!(clone.is_some());
            match clone.unwrap().res {
                Res::Def(DefKind::Trait, _) => {}
                _ => panic!("Expected trait"),
            }
        });
    }

    #[test]
    fn test_path_with_generic_args() {
        let src = r#"
        struct Vec<T>;
        type IntVec = Vec<i32>;
        "#;
        with_resolver(src, |resolver, _krate| {
            let int_vec = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "IntVec".into(),
                    span: Span::default(),
                },
                Namespace::Type,
            );
            assert!(int_vec.is_some());
            assert_eq!(resolver.tcx.sess().diag_ctxt().diags_count(), 0);
        });
    }

    #[test]
    fn test_multi_segment_use() {
        let src = r#"
        mod a {
            pub mod b {
                pub fn c() {}
            }
        }
        use a::b::c;
        fn main() {
            c();
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            let c = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: "c".into(),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(c.is_some());
            match c.unwrap().res {
                Res::Def(DefKind::Fn, _) => {}
                _ => panic!("Expected function"),
            }
        });
    }

    #[test]
    fn test_self_ctor_in_impl() {
        let src = r#"
        struct Foo(i32);
        impl Foo {
            fn new(x: i32) -> Self {
                Self(x)
            }
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            assert_eq!(resolver.tcx.sess().diag_ctxt().diags_count(), 0);
        });
    }

    #[test]
    fn test_trait() {
        let src = r#"
        trait Foo {
            fn foo() -> ();
        }

        fn bar<T: Foo>(a: T) {
            a.foo();
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            assert_eq!(resolver.tcx.sess().diag_ctxt().diags_count(), 0);
        });
    }

    #[test]
    fn test_builtin_trait() {
        let src = r#"
        struct Foo;
        
        impl Add for Foo {
            fn add(*self, other: *Foo) -> Foo {
                Foo
            }
        }
        "#;
        with_resolver(src, |resolver, _krate| {
            assert_eq!(resolver.tcx.sess().diag_ctxt().diags_count(), 0);
        });
    }
}
