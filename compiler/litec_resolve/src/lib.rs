pub mod def_collector;
pub mod module;
pub mod rib;

use std::collections::hash_map;

use crate::module::{Binding, FromKind};
use crate::rib::RibKind;
use crate::{module::ModuleData, rib::Rib};
use indexmap::map::Entry;
use litec_ast::ast::*;
use litec_ast::visit::{walk_block, walk_crate, walk_fn};
use litec_ast::{
    ast::{self, NodeId},
    visit::Visitor,
};
use litec_error::{Diagnostic, error};
use litec_hir::def::{DefKind, Namespace, Res};
use litec_session::Session;
use litec_span::id::DefId;
use litec_span::intern_global;
use rustc_hash::FxHashMap;

// 路径出现的上下文
enum PathContext {
    Use,  // use 语句，必须完全解析
    Expr, // 表达式，允许部分解析
    Type, // 类型位置，通常要求完全解析（可根据需要调整）
}

/// 路径解析结果
pub enum PathResolution {
    /// 完全解析，直接得到 `Res`
    Full(Binding),
    /// 部分解析：已解析到某个非模块项（如结构体），剩余段需要类型检查处理
    Partial {
        base: Res<NodeId>,           // 已解析到的定义（通常是类型或模块）
        remaining: Vec<PathSegment>, // 剩余未解析的段（例如 `new`）
    },
}

pub struct Resolver<'a> {
    session: &'a Session,
    node_to_def: FxHashMap<NodeId, DefId>,
    modules: FxHashMap<DefId, ModuleData>,
    module_path: Vec<DefId>,
    value_ribs: Vec<Rib<Res<NodeId>>>,
    type_ribs: Vec<Rib<Res<NodeId>>>,
    results: FxHashMap<NodeId, Res<NodeId>>,
    struct_fields: FxHashMap<DefId, Vec<Field>>,
    /// 当前所处的module, visibility, use_tree
    unresolved_uses: Vec<(DefId, Visibility, UseTree)>,
    root_module_def_id: DefId,
}

impl<'a> Resolver<'a> {
    pub fn new(session: &'a Session, node_to_def: FxHashMap<NodeId, DefId>) -> Self {
        Self {
            session,
            node_to_def,
            modules: FxHashMap::default(),
            module_path: Vec::new(),
            value_ribs: Vec::new(),
            type_ribs: Vec::new(),
            results: FxHashMap::default(),
            struct_fields: FxHashMap::default(),
            unresolved_uses: Vec::new(),
            root_module_def_id: DefId::default(),
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

    fn lookup(&self, ident: Ident, ns: Namespace) -> Option<Res<NodeId>> {
        match ns {
            Namespace::Type => self.lookup_type(ident),
            Namespace::Value => self.lookup_value(ident),
        }
    }

    fn lookup_in_module(&self, module_id: DefId, ident: Ident, ns: Namespace) -> Option<Binding> {
        match ns {
            Namespace::Type => self.lookup_type_in_module(module_id, ident),
            Namespace::Value => self.lookup_value_in_module(module_id, ident),
        }
    }

    fn resolve_path(
        &self,
        path: &Path,
        context: PathContext,
        ns: Namespace, // 初始命名空间，仅用于第一段（如果非关键字）
    ) -> Option<PathResolution> {
        let segments = &path.segments;
        if segments.is_empty() {
            return None;
        }

        // 1. 处理第一段的关键字，确定起始模块和剩余段
        let first = &segments[0];
        let (start_module, remaining) = match first.name.text.to_string().as_ref() {
            "super" => match self.module_path.as_slice() {
                &[.., super_module, _] => (super_module, &segments[1..]),
                _ => {
                    self.session.report(
                        error("模块路径小于2, 没有父模块")
                            .with_span(first.span)
                            .build(),
                    );
                    return None;
                }
            },
            "crate" => (self.root_module_def_id, &segments[1..]),
            "self" => (*self.module_path.last()?, &segments[1..]),
            _ => (*self.module_path.last()?, segments.as_slice()), // 不消耗第一段
        };

        let ((module_id, vis, from), remaining_after_modules) =
            self.resolve_module_prefix(start_module, remaining)?;

        if remaining_after_modules.is_empty() {
            // 整个路径都是模块
            return Some(PathResolution::Full(Binding::new(
                Res::Def(DefKind::Module, module_id),
                vis,
                from,
            )));
        }

        // 至少还有一个段
        let first_rem = &remaining_after_modules[0];
        let rest_rem = &remaining_after_modules[1..];

        // 在当前模块中按指定命名空间查找第一个剩余段
        let binding = self.lookup_in_module(module_id, first_rem.name, ns)?;
        // 可见性检查
        if module_id != *self.module_path.last()? && binding.vis != Visibility::Public {
            self.session.report(
                error(format!("项 `{}` 是 private", first_rem.name.to_string()))
                    .with_span(first_rem.span)
                    .build(),
            );
            return None;
        }

        // 根据上下文和剩余段数量决定最终结果
        match (context, rest_rem.is_empty()) {
            // use 语句：必须完全解析（没有剩余段）
            (PathContext::Use, true) => Some(PathResolution::Full(binding)),
            (PathContext::Use, false) => {
                self.session.report(
                    error("use不可以导入 struct 的类型或函数")
                        .with_span(path.span)
                        .build(),
                );
                None
            }
            // 表达式上下文：允许部分解析
            (PathContext::Expr, true) => Some(PathResolution::Full(binding)),
            (PathContext::Expr, false) => Some(PathResolution::Partial {
                base: binding.res,
                remaining: rest_rem.to_vec(),
            }),
            // 类型上下文：通常要求完全解析（可根据需要放宽）
            (PathContext::Type, true) => Some(PathResolution::Full(binding)),
            (PathContext::Type, false) => {
                self.session.report(
                    error("expected a type, found associated item")
                        .with_span(path.span)
                        .build(),
                );
                None
            }
        }
    }

    /// 从 start_module 开始，尽可能多地解析路径中的模块段
    /// 返回 ((最终模块, 模块可访问性), 剩余段切片)
    fn resolve_module_prefix<'b>(
        &self,
        start_module: DefId,
        segments: &'b [PathSegment],
    ) -> Option<((DefId, Visibility, FromKind), &'b [PathSegment])> {
        let mut current = start_module;
        let mut vis = Visibility::Inherited;
        let mut from = FromKind::Normal;
        let mut idx = 0;
        for (i, seg) in segments.iter().enumerate() {
            let module_data = self.modules.get(&current)?;
            // 模块在 value 命名空间中（submodules）
            if let Some((sub_id, vis_, from_)) = module_data.submodules.get(&seg.name) {
                // 跨模块可见性检查
                if current != *self.module_path.last()? && *vis_ != Visibility::Public {
                    self.session
                        .report(error("模块为private, 不可访问").with_span(seg.span).build());
                    return None;
                }
                current = *sub_id;
                vis = *vis_;
                from = *from_;
                idx = i + 1;
            } else {
                break;
            }
        }
        Some(((current, vis, from), &segments[idx..]))
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

    fn register_item(&mut self, item: &Item) {
        let def_id = self.node_to_def[&item.node_id];
        match &item.kind {
            ItemKind::Fn(fn_) => {
                self.insert_module_binding(
                    fn_.sig.name,
                    Namespace::Value,
                    Binding::new(
                        Res::Def(DefKind::Fn, def_id),
                        item.visibility,
                        FromKind::Normal,
                    ),
                );
            }
            ItemKind::Struct(ident, _, fields) => {
                self.insert_module_binding(
                    *ident,
                    Namespace::Type,
                    Binding::new(
                        Res::Def(DefKind::Struct, def_id),
                        item.visibility,
                        FromKind::Normal,
                    ),
                );
                self.struct_fields.insert(def_id, fields.clone());
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
                                    Res::Def(DefKind::Fn, def_id),
                                    item.visibility,
                                    FromKind::Normal,
                                ),
                            );
                        }
                    }
                }
            }
            ItemKind::Module(ident, inline) => {
                let def_id = self.node_to_def[&item.node_id];
                if self.modules[&self.module_path.last().unwrap()]
                    .submodules
                    .contains_key(ident)
                {
                    self.session
                        .report(error("重复定义模块").with_span(ident.span).build());
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
            ItemKind::Impl(_) => todo!(),
            ItemKind::TypeAlias(type_alias) => todo!(),
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
                    self.session
                        .report(error("无法解析导入").with_span(use_tree.span).build());
                }
            }
        }
    }

    fn resolve_use(&mut self, vis: Visibility, use_tree: &UseTree, changed: &mut bool) -> bool {
        match &use_tree.kind {
            UseTreeKind::Glob => {
                // 1. 解析前缀为模块
                let module_def_id = match self.resolve_module_path(&use_tree.prefix) {
                    Some(id) => id,
                    None => {
                        // 前缀尚未解析（可能依赖其他模块），保留等待
                        return true;
                    }
                };
                let module = self.modules.get(&module_def_id).unwrap();

                // 2. 收集公开的子模块、类型、值
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
                    if let hash_map::Entry::Vacant(e) = current.submodules.entry(ident) {
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
                false // 已解析完成，不再需要保留
            }

            UseTreeKind::Simple(rename) => {
                let target_name =
                    rename.unwrap_or_else(|| use_tree.prefix.segments.last().unwrap().name);

                // 分别解析类型和值命名空间
                let type_resolution =
                    self.resolve_path(&use_tree.prefix, PathContext::Use, Namespace::Type);
                let value_resolution =
                    self.resolve_path(&use_tree.prefix, PathContext::Use, Namespace::Value);

                // 如果两者都未解析（返回 None），说明依赖未满足，需要等待
                if type_resolution.is_none() && value_resolution.is_none() {
                    return true;
                }

                let current = self
                    .modules
                    .get_mut(&self.module_path.last().unwrap())
                    .unwrap();
                let mut any_inserted = false;

                // 处理类型命名空间
                if let Some(resolution) = type_resolution {
                    match resolution {
                        PathResolution::Full(binding) => {
                            if binding.vis != Visibility::Public {
                                self.session.report(
                                    error(format!("类型 `{}` 为 private", target_name.to_string()))
                                        .with_span(use_tree.span)
                                        .build(),
                                );
                            } else {
                                if let Entry::Vacant(e) =
                                    current.type_rib.bindings.entry(target_name)
                                {
                                    let mut new_binding = binding;
                                    new_binding.vis = vis;
                                    new_binding.from = FromKind::Normal;
                                    e.insert(new_binding);
                                    any_inserted = true;
                                }
                            }
                        }
                        PathResolution::Partial { .. } => {
                            // use 语句不允许部分解析
                            self.session.report(
                                error("cannot import associated item or method")
                                    .with_span(use_tree.span)
                                    .build(),
                            );
                        }
                    }
                }

                // 处理值命名空间
                if let Some(resolution) = value_resolution {
                    match resolution {
                        PathResolution::Full(binding) => {
                            if binding.vis != Visibility::Public {
                                self.session.report(
                                    error(format!(
                                        "value `{}` is private",
                                        target_name.to_string()
                                    ))
                                    .with_span(use_tree.span)
                                    .build(),
                                );
                            } else {
                                if let Entry::Vacant(e) =
                                    current.value_rib.bindings.entry(target_name)
                                {
                                    let mut new_binding = binding;
                                    new_binding.vis = vis;
                                    new_binding.from = FromKind::Normal;
                                    e.insert(new_binding);
                                    any_inserted = true;
                                }
                            }
                        }
                        PathResolution::Partial { .. } => {
                            self.session.report(
                                error("cannot import associated item or method")
                                    .with_span(use_tree.span)
                                    .build(),
                            );
                        }
                    }
                }

                if any_inserted {
                    *changed = true;
                }
                false // 已解析（或报错），不再保留
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
        self.early_resolve(ast);
        self.late_resolve(ast);
    }

    fn early_resolve(&mut self, ast: &ast::Crate) {
        let def_id = self.node_to_def[&ast.node_id];
        self.root_module_def_id = def_id;
        self.modules.insert(def_id, ModuleData::new());
        self.enter_module(def_id);
        for item in &ast.items {
            self.register_item(item);
        }
        self.resolve_uses();
        walk_crate(self, ast);

        self.exit_module();
    }

    fn late_resolve(&mut self, ast: &ast::Crate) {
        self.visit_crate(ast);
    }
}

impl<'a> Visitor for Resolver<'a> {
    fn visit_fn(&mut self, fn_: &Fn) {
        self.push_value_rib(RibKind::Function);

        walk_fn(self, fn_);

        self.pop_value_rib();
    }

    fn visit_param(&mut self, param: &Param) {
        self.insert_value_binding(param.name, Res::Local(param.node_id));
    }

    fn visit_block(&mut self, block: &Block) {
        self.push_value_rib(RibKind::Normal);

        walk_block(self, block);

        self.pop_value_rib();
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Expr(expr) | StmtKind::Semi(expr) => self.visit_expr(expr),
            StmtKind::Let(_, ident, ty, expr) => {
                self.insert_value_binding(*ident, Res::Local(stmt.node_id));

                if let Some(ty) = ty {
                    self.visit_ty(ty);
                }
                if let Some(expr) = expr {
                    self.visit_expr(expr);
                }
            }
            StmtKind::Return(expr) | StmtKind::Break(expr) => {
                if let Some(expr) = expr {
                    self.visit_expr(expr);
                }
            }
            _ => {}
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
                mutability,
                variable,
                iter,
                body,
            } => {
                self.insert_value_binding(*variable, Res::Local(expr.node_id));
                self.visit_expr(iter);
                self.visit_block(body);
            }
            ExprKind::Index(expr, expr1) => todo!(),
            ExprKind::Range(expr, expr1, range_limits) => todo!(),
            ExprKind::Loop(block) => todo!(),
            ExprKind::Field(expr, ident) => todo!(),
            ExprKind::Path(path) => todo!(),
            ExprKind::Bool(_) => todo!(),
            ExprKind::Tuple(exprs) => todo!(),
            ExprKind::Unit => todo!(),
            ExprKind::AddressOf(expr) => todo!(),
            ExprKind::StructExpr(struct_expr) => todo!(),
            ExprKind::Cast(expr, ty) => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use litec_parse::{node_collector::NodeCollector, parser::parse};
    use litec_span::{SourceMap, Span, id::LOCAL_CRATE, intern_global};

    use crate::def_collector::DefCollector;

    use super::*;

    fn run_test_on_source(src: &str, test: impl FnOnce(Resolver, Crate)) {
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "test.lt".to_string(),
            src.to_string(),
            &PathBuf::from("test.lt"),
        );
        let sess = Session::new(source_map);
        let mut ast = parse(&sess, file_id); // 假设有 parse_source
        let mut node_collector = NodeCollector::new();
        node_collector.collect(&mut ast);
        let mut def_collector = DefCollector::new(LOCAL_CRATE);
        def_collector.set_root(NodeId::from_raw(0));
        def_collector.collect(&ast);
        let (node_to_def, _) = def_collector.finish();
        let mut resolver = Resolver::new(&sess, node_to_def);
        resolver.resolve_crate(&ast);
        sess.print_diagnotics();
        test(resolver, ast);
    }

    #[test]
    fn test_use_simple() {
        let src = "
            mod foo { pub fn bar() {} }
            use foo::bar;
        ";
        run_test_on_source(src, |resolver, _| {
            let binding = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: intern_global("bar"),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(binding.is_some());
        });
    }

    #[test]
    fn test_use_glob_nested() {
        let src = "
            mod foo {
                pub use bar::*;
                mod bar {
                    pub fn baz() {}
                }
            }
            use foo::{baz};
        ";
        run_test_on_source(src, |resolver, _| {
            let binding = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: intern_global("baz"),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            dbg!(&resolver.modules[&resolver.root_module_def_id]);
            assert!(binding.is_some());
        });
    }

    #[test]
    fn test_forward_use() {
        let src = "
            use foo::bar::baz;
            pub mod foo {
                pub use bar::*;
                mod bar {
                    pub fn baz() {

                    }
                }
            }
        ";
        run_test_on_source(src, |resolver, ast| {
            let binding = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: intern_global("baz"),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(binding.is_none());
        });
    }

    #[test]
    fn test_trait_item() {
        let src = "
            trait A {
            }
        ";
        run_test_on_source(src, |resolver, ast| {
            let binding = resolver.lookup_in_module(
                resolver.root_module_def_id,
                Ident {
                    text: intern_global("baz"),
                    span: Span::default(),
                },
                Namespace::Value,
            );
            assert!(binding.is_none());
        });
    }
}
