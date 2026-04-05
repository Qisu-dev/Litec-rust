pub mod per_ns;
pub mod rhir;
use crate::per_ns::PerNs;
use crate::rhir::{RBlock, RCrate, RExpr, RExternItem, RField, RItem, RParam, RStmt, RType};
use cfg_if::cfg_if;
use litec_error::{Diagnostic, error};
use litec_hir::{
    AbiType, Block as RawBlock, Crate as RawCrate, Expr as RawExpr, ExternItem as RawExternItem,
    Item as RawItem, Stmt as RawStmt, Type as RawType, Visibility,
};
use litec_lower::lower;
use litec_parse::parser::parse;
use litec_span::{FileId, SourceMap, Span, StringId, get_global_string, intern_global};
use litec_typed_hir::builtins::{Builtin, BuiltinDefIds, BuiltinTypes, Builtins};
use litec_typed_hir::{DefKind, def_id::DefId};
use rustc_hash::FxHashMap;

use std::{
    collections::VecDeque,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

macro_rules! define_builtin_types {
    ($resolver:expr, $root_module:expr, [$($name:ident),* $(,)?]) => {
        $(let $name = $resolver.create_def(stringify!($name).into(), Visibility::Public, DefKind::BuiltinType, Span::default(), $root_module);)*
    };
}

#[derive(Debug)]
pub struct Definition {
    pub def_id: DefId,
    pub visibility: Visibility,
    pub name: StringId,
    pub kind: DefKind,
    pub module_id: DefId,
    pub span: Span,
}

#[derive(Debug)]
pub struct ResolveOutput {
    pub definitions: Vec<Definition>,
    pub value_ns: PerNs<DefId>,
    pub type_ns: PerNs<DefId>,
    pub module_children: FxHashMap<DefId, Vec<DefId>>,
    pub import_map: FxHashMap<(DefId, StringId), DefId>,
    pub diagnostics: Vec<Diagnostic>,
    pub rhir: RCrate,
    pub struct_fields: FxHashMap<DefId, Vec<RField>>,
    pub builtin: Builtin,
}

#[derive(Debug, Clone)]
struct ModuleScope {
    pub def_id: DefId,
    pub items: FxHashMap<StringId, DefId>,
    pub import_map: FxHashMap<StringId, DefId>,
}

#[derive(Debug, Clone)]
struct PendingUse {
    module: DefId,
    alias: StringId,
    path: Vec<StringId>,
    span: Span,
}

#[derive(Debug)]
pub struct Resolver<'a> {
    index: AtomicU32,
    definitions: Vec<Definition>,
    value_ns: PerNs<DefId>,
    type_ns: PerNs<DefId>,
    module_scopes: FxHashMap<DefId, ModuleScope>,
    current_path: Vec<DefId>,
    main_file_path: PathBuf,
    source_map: &'a mut SourceMap,
    diagnostics: Vec<Diagnostic>,
    pending_uses: Vec<PendingUse>,
    module_children: FxHashMap<DefId, Vec<DefId>>,
    scopes: Vec<Scope>,
    builtin: Option<Builtin>,
    struct_fields: FxHashMap<DefId, Vec<RField>>,
    raw_module_items: FxHashMap<DefId, RawCrate>,
}

#[derive(Debug)]
struct Scope {
    bindings: FxHashMap<StringId, DefId>,
    kind: ScopeKind,
}

impl Scope {
    fn new(kind: ScopeKind) -> Self {
        Self {
            bindings: FxHashMap::default(),
            kind,
        }
    }

    fn get(&self, name: StringId) -> Option<DefId> {
        self.bindings.get(&name).copied()
    }

    fn insert(&mut self, name: StringId, def_id: DefId) {
        self.bindings.insert(name, def_id);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Function,
    Block,
    Loop,
    Module,
}

impl<'a> Resolver<'a> {
    pub fn new(source_map: &'a mut SourceMap, field_id: FileId) -> Self {
        let mut current_path = Vec::new();
        // 根模块哨兵：index = 0，kind = Module
        current_path.push(DefId::new(0, DefKind::Module));
        let path = source_map
            .file(field_id)
            .unwrap()
            .path
            .clone()
            .to_path_buf();
        let mut resolver = Self {
            index: AtomicU32::new(0),
            definitions: Vec::new(),
            value_ns: PerNs::new(),
            type_ns: PerNs::new(),
            module_scopes: FxHashMap::default(),
            current_path: current_path,
            main_file_path: path,
            source_map,
            diagnostics: Vec::new(),
            pending_uses: Vec::new(),
            module_children: FxHashMap::default(),
            scopes: Vec::new(),
            struct_fields: FxHashMap::default(),
            builtin: None,
            raw_module_items: FxHashMap::default(),
        };

        // 定义内置类型和函数
        resolver.define_builtin_types();

        resolver
    }

    pub fn define_builtin_types(&mut self) {
        let root_module = DefId::new(0, DefKind::Module);

        // 定义整数类型
        define_builtin_types!(
            self,
            root_module,
            [
                i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
            ]
        );

        // 定义浮点类型
        define_builtin_types!(self, root_module, [f32, f64]);

        // 定义其他基本类型
        define_builtin_types!(self, root_module, [bool, char, str, unit, never]);

        let raw_ptr = self.create_def(
            "RawPtr".into(),
            Visibility::Public,
            DefKind::BuiltinType,
            Span::default(),
            root_module,
        );
        // 返回所有内置类型的定义
        self.builtin = Some(Builtins {
            def_ids: BuiltinDefIds {
                i8,
                i16,
                i32,
                i64,
                i128,
                isize,
                u8,
                u16,
                u32,
                u64,
                u128,
                usize,
                f32,
                f64,
                bool,
                char,
                str,
                unit,
                never,
                raw_ptr,
            },
            types: BuiltinTypes { i8: (), i16: (), i32: (), i64: (), i128: (), isize: (), u8: (), u16: (), u32: (), u64: (), u128: (), usize: (), f32: (), f64: (), bool: (), char: (), str: (), unit: (), never: () }
            functions: Vec::new(),
        });
    }

    pub fn resolve(mut self, raw_crate: &RawCrate) -> ResolveOutput {
        let root = self.create_def(
            intern_global("crate"),
            Visibility::Public,
            DefKind::Module,
            Span::default(),
            DefId::new(0, DefKind::Module),
        );
        self.add_module_scope(root);

        self.collect_top_level(&raw_crate.items);
        self.resolve_uses();
        self.expand_pub_use();
        let import_map = self.flatten_import_map();
        self.current_path.clear();
        self.current_path = vec![root];
        let rhir = self.lower_to_rhir(raw_crate);

        ResolveOutput {
            definitions: self.definitions,
            value_ns: self.value_ns,
            type_ns: self.type_ns,
            module_children: self.module_children,
            import_map,
            diagnostics: self.diagnostics,
            rhir,
            struct_fields: self.struct_fields,
            builtin: self.builtin.unwrap(),
        }
    }

    // ==================== 顶层收集 ====================
    fn collect_top_level(&mut self, items: &[RawItem]) {
        for item in items {
            match item {
                RawItem::Module {
                    visibility,
                    name,
                    items,
                    span,
                    ..
                } => {
                    let vis = Visibility::from(*visibility);
                    let mod_def = self.create_top_def(
                        *name,
                        vis,
                        DefKind::Module,
                        *span,
                        self.current_module(),
                    );
                    self.push_module_child(self.current_module(), mod_def);
                    let scope = &mut self.module_scopes.get_mut(&self.current_module()).unwrap();
                    scope.items.insert(*name, mod_def);
                    self.module_scopes.insert(
                        mod_def,
                        ModuleScope {
                            def_id: mod_def,
                            items: FxHashMap::default(),
                            import_map: FxHashMap::default(),
                        },
                    );
                    self.current_path.push(mod_def);

                    if let Some(inline_items) = items {
                        self.collect_top_level(inline_items);
                        self.raw_module_items.insert(
                            mod_def,
                            RawCrate {
                                items: inline_items.clone(),
                            },
                        );
                    } else if let Some(krate) = self.load_module(*name, *span) {
                        self.collect_top_level(&krate.items);
                        self.raw_module_items.insert(mod_def, krate);
                    }
                    self.current_path.pop();
                }

                RawItem::Use {
                    visibility,
                    path,
                    rename,
                    span,
                    ..
                } => {
                    let alias = rename.unwrap_or(*path.last().unwrap());
                    self.record_use(
                        self.current_module(),
                        alias,
                        path.clone(),
                        *span,
                        Visibility::from(*visibility),
                    );
                }
                RawItem::Function {
                    visibility,
                    name,
                    span,
                    ..
                } => {
                    let vis = Visibility::from(*visibility);
                    let fn_def = self.create_top_def(
                        *name,
                        vis,
                        DefKind::Function,
                        *span,
                        self.current_module(),
                    );
                    self.push_module_child(self.current_module(), fn_def);
                    let module_scope = self.module_scopes.get_mut(&self.current_module()).unwrap();
                    module_scope.items.insert(*name, fn_def);
                    self.insert_ns(self.current_module(), *name, fn_def);
                }
                RawItem::Struct {
                    attribute: _,
                    visibility,
                    name,
                    fields,
                    span,
                } => {
                    let struct_def = self.create_top_def(
                        *name,
                        Visibility::from(*visibility),
                        DefKind::Struct,
                        *span,
                        self.current_module(),
                    );
                    self.push_module_child(self.current_module(), struct_def);
                    let scope = self.module_scopes.get_mut(&self.current_module()).unwrap();
                    scope.items.insert(*name, struct_def);
                    let mut rfields = Vec::new();
                    for field in fields {
                        let field_id = self.create_def(
                            field.name,
                            Visibility::from(field.visibility),
                            DefKind::Field,
                            field.span,
                            struct_def, // 字段属于结构体
                        );
                        rfields.push(RField {
                            def_id: field_id,
                            name: field.name,
                            ty: self.lower_type(&field.ty),
                            visibility: field.visibility,
                            index: field.index,
                            span: field.span,
                        });
                    }
                    self.struct_fields.insert(struct_def, rfields);
                }

                RawItem::Extern {
                    attribute: _,
                    visibility,
                    abi,
                    items,
                    span,
                } => {
                    // 检查是否为 "Litec" ABI
                    if *abi == AbiType::Lite {
                        // 处理 Litec 内置函数
                        for item in items {
                            match item {
                                RawExternItem::Function {
                                    visibility,
                                    name,
                                    span,
                                    ..
                                } => {
                                    // 创建内置函数定义
                                    let fn_def = self.create_def(
                                        *name,
                                        *visibility,
                                        DefKind::Intrinsic, // 使用 Intrinsic 而不是 ExternFunction
                                        *span,
                                        self.current_module(),
                                    );

                                    let module_scope =
                                        self.module_scopes.get_mut(&self.current_module()).unwrap();
                                    module_scope.items.insert(*name, fn_def);
                                    self.push_module_child(self.current_module(), fn_def);
                                    self.insert_ns(self.current_module(), *name, fn_def);
                                }
                            }
                        }
                    } else {
                        // 处理普通外部函数
                        let extern_def = self.create_top_def(
                            intern_global("extern"),
                            Visibility::from(*visibility),
                            DefKind::Extern,
                            *span,
                            self.current_module(),
                        );
                        self.push_module_child(self.current_module(), extern_def);
                        let scope = self.module_scopes.get_mut(&self.current_module()).unwrap();
                        scope.items.insert(intern_global("extern"), extern_def);

                        for item in items {
                            match item {
                                RawExternItem::Function {
                                    visibility,
                                    name,
                                    span,
                                    ..
                                } => {
                                    let fn_def = self.create_def(
                                        *name,
                                        *visibility,
                                        DefKind::ExternFunction,
                                        *span,
                                        self.current_module(),
                                    );
                                    let module_scope =
                                        self.module_scopes.get_mut(&self.current_module()).unwrap();
                                    module_scope.items.insert(*name, fn_def);
                                    self.push_module_child(self.current_module(), fn_def);
                                    self.insert_ns(self.current_module(), *name, fn_def);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ==================== 路径解析 ====================
    /// 入口：解析一条路径 `segments`，返回第一个**可见**的 DefId
    fn resolve_path_internal(&mut self, segments: &[StringId], error_span: Span) -> Option<DefId> {
        if segments.is_empty() {
            return None;
        }

        let first = segments[0];
        let rest = &segments[1..];
        let use_mod = *self
            .current_path
            .last()
            .unwrap_or(&DefId::new(0, DefKind::Module));

        let (start_idx, target_segments) = match first {
            x if x == intern_global("self") => (self.current_module(), rest),
            x if x == intern_global("super") => {
                (self.current_path[self.current_path.len() - 2], rest)
            }
            x if x == intern_global("crate") => (self.find_root_crate(self.current_module()), rest),
            _ => (self.current_module(), segments),
        };

        if target_segments.is_empty() {
            return self.module_scopes.get(&start_idx).map(|s| s.def_id);
        }

        // resolve_from_scope 已经处理了可见性，返回的都是可见的
        self.resolve_from_scope(use_mod, target_segments, error_span)
    }

    // 纯函数：不需要 &mut self
    fn check_visibility(definitions: &[Definition], use_module: DefId, target: DefId) -> bool {
        let def = &definitions[target.index as usize];
        match def.visibility {
            Visibility::Public => true,
            Visibility::Private => use_module == def.module_id, // 严格相等
        }
    }

    /// 返回第一个可见元素；全程无解则 None
    fn resolve_from_scope(
        &mut self,
        use_module: DefId,
        segments: &[StringId],
        error_span: Span,
    ) -> Option<DefId> {
        let mut current_module_id: Option<DefId> = None;

        for (i, &seg) in segments.iter().enumerate() {
            let mut found = false;

            // 1. 如果我们已经在一个模块内部，查找该模块的子模块或项
            if let Some(module_id) = current_module_id {
                for def in &self.definitions {
                    if def.module_id == module_id && def.name == seg {
                        if i == segments.len() - 1 {
                            if Self::check_visibility(&self.definitions, use_module, def.def_id) {
                                return Some(def.def_id);
                            }
                            self.diagnostics.push(
                                error(format!(
                                    "模块私有内容 `{}`",
                                    get_global_string(*segments.last().unwrap()).unwrap()
                                ))
                                .with_span(error_span)
                                .build(),
                            );
                            return None;
                        } else if def.kind == DefKind::Module {
                            current_module_id = Some(def.def_id);
                            found = true;
                            break;
                        } else {
                            return None;
                        }
                    }
                }
            }

            // 2. 在当前模块作用域中查找（包括导入映射和直接项）
            if !found {
                for scope in self.module_scopes.values().collect::<Vec<_>>().iter().rev() {
                    if let Some(&id) = scope.import_map.get(&seg) {
                        let def = &self.definitions[id.index as usize];
                        if i == segments.len() - 1 {
                            if Self::check_visibility(&self.definitions, use_module, id) {
                                return Some(id);
                            }
                            self.diagnostics.push(
                                error(format!(
                                    "模块私有内容 `{}`",
                                    get_global_string(*segments.last().unwrap()).unwrap()
                                ))
                                .with_span(error_span)
                                .build(),
                            );
                            return None;
                        } else if def.kind == DefKind::Module {
                            current_module_id = Some(id);
                            found = true;
                            break;
                        } else {
                            return None;
                        }
                    }

                    if let Some(&id) = scope.items.get(&seg) {
                        let def = &self.definitions[id.index as usize];
                        if i == segments.len() - 1 {
                            if Self::check_visibility(&self.definitions, use_module, id) {
                                return Some(id);
                            }
                            self.diagnostics.push(
                                error(format!(
                                    "模块私有内容 `{}`",
                                    get_global_string(*segments.last().unwrap()).unwrap()
                                ))
                                .with_span(error_span)
                                .build(),
                            );
                            return None;
                        } else if def.kind == DefKind::Module {
                            current_module_id = Some(id);
                            found = true;
                            break;
                        } else {
                            return None;
                        }
                    }
                }
            }

            // 3. 检查全局作用域
            if !found {
                if let Some(id) = self.scopes[0].get(seg) {
                    let def = &self.definitions[id.index as usize];
                    if i == segments.len() - 1 {
                        if Self::check_visibility(&self.definitions, use_module, id) {
                            return Some(id);
                        }
                        self.diagnostics.push(
                            error(format!(
                                "模块私有内容 `{}`",
                                get_global_string(*segments.last().unwrap()).unwrap()
                            ))
                            .with_span(error_span)
                            .build(),
                        );
                        return None;
                    } else if def.kind == DefKind::Module {
                        current_module_id = Some(id);
                        found = true;
                    } else {
                        return None;
                    }
                }
            }

            if !found {
                return None;
            }
        }

        None
    }

    // ==================== use & pub-use ====================
    fn record_use(
        &mut self,
        module: DefId,
        alias: StringId,
        path: Vec<StringId>,
        span: Span,
        _vis: Visibility,
    ) {
        self.pending_uses.push(PendingUse {
            module,
            alias,
            path,
            span,
        });
    }
    fn resolve_uses(&mut self) {
        let pending = std::mem::take(&mut self.pending_uses);
        for pending_use in pending {
            if let Some(target) =
                self.resolve_from_scope(pending_use.module, &pending_use.path, pending_use.span)
            {
                let scope = self.module_scopes.get_mut(&pending_use.module).unwrap();
                scope.import_map.insert(pending_use.alias, target);
            } else {
                self.diagnostics.push(
                    error(format!("unresolved use `{:?}`", pending_use.path))
                        .with_span(pending_use.span)
                        .build(),
                );
            }
        }
    }

    fn expand_pub_use(&mut self) {
        let mut q = VecDeque::from([DefId::new(0, DefKind::Module)]);
        while let Some(parent) = q.pop_front() {
            if let Some(children) = self.module_children.get(&parent) {
                for &child in children {
                    // 先收集需要传播的导入
                    let mut imports_to_propagate = Vec::new();
                    if let Some(child_scope) = self.module_scopes.get(&child) {
                        for (&name, &target) in &child_scope.import_map {
                            if self.definitions[target.index as usize].visibility
                                == Visibility::Public
                            {
                                imports_to_propagate.push((name, target));
                            }
                        }
                    }
                    // 再应用到父模块
                    if let Some(parent_scope) = self.module_scopes.get_mut(&parent) {
                        for (name, target) in imports_to_propagate {
                            parent_scope.import_map.insert(name, target);
                        }
                    }
                    q.push_back(child);
                }
            }
        }
    }

    // ==================== 文件加载 ====================
    fn load_module(&mut self, name: StringId, span: Span) -> Option<RawCrate> {
        let name_str = get_global_string(name).unwrap();

        // 测试模式特殊处理
        cfg_if! {
            if #[cfg(test)] {
                if name_str.as_ref() == "inner" {
                    // 手动创建包含priv_fn和pub_fn的items
                    let priv_fn_item = RawItem::Function {
                        attribute: None,
                        visibility: Visibility::Private,
                        name: intern_global("priv_fn"),
                        params: Vec::new(),
                        return_type: None,
                        body: RawBlock {
                            stmts: Vec::new(),
                            tail: None,
                            span,
                        },
                        span,
                    };
                    let pub_fn_item = RawItem::Function {
                        attribute: None,
                        visibility: Visibility::Public,
                        name: intern_global("pub_fn"),
                        params: Vec::new(),
                        return_type: None,
                        body: RawBlock {
                            stmts: Vec::new(),
                            tail: None,
                            span,
                        },
                        span,
                    };
                    let raw_crate = RawCrate {
                        items: vec![priv_fn_item, pub_fn_item],
                    };
                    return Some(raw_crate);
                }
            }
        }

        // 正常文件加载逻辑
        let base = self.main_file_path.parent().unwrap();
        let file = base.join(format!("{}.lt", name_str.as_ref()));
        let dir = base.join(name_str.as_ref()).join("mod.lt");

        let (path, content) = if file.exists() {
            (file.clone(), fs::read_to_string(file))
        } else if dir.exists() {
            (dir.clone(), fs::read_to_string(dir))
        } else {
            self.diagnostics.push(
                error(format!("无法找到 `{}`", name_str.as_ref()))
                    .with_span(span)
                    .build(),
            );
            return None;
        };

        let content = match content {
            Ok(s) => s,
            Err(e) => {
                self.diagnostics
                    .push(error(e.to_string()).with_span(span).build());
                return None;
            }
        };

        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let file_id = self.source_map.add_file(file_name, content, &path);
        let (ast, diagnostics) = parse(&self.source_map, file_id);
        self.diagnostics.extend(diagnostics);
        let (raw_crate, diagnostics) = lower(ast);
        self.diagnostics.extend(diagnostics);
        Some(raw_crate)
    }

    // ==================== 工具 ====================
    fn create_def(
        &mut self,
        name: StringId,
        vis: Visibility,
        kind: DefKind,
        span: Span,
        module_id: DefId,
    ) -> DefId {
        let idx = self.index.fetch_add(1, Ordering::Relaxed);
        let def_id = DefId::new(idx, kind);
        self.definitions.push(Definition {
            def_id,
            visibility: vis,
            name,
            kind,
            module_id,
            span,
        });
        def_id
    }

    fn create_top_def(
        &mut self,
        name: StringId,
        vis: Visibility,
        kind: DefKind,
        span: Span,
        module_id: DefId,
    ) -> DefId {
        let def_id = self.create_def(name, vis, kind, span, module_id);
        let module_scope = self.module_scopes.get_mut(&module_id).unwrap();

        module_scope.items.insert(name, def_id);

        self.insert_ns(module_id, name, def_id);
        self.push_module_child(module_id, def_id);
        def_id
    }

    fn add_module_scope(&mut self, def_id: DefId) {
        self.module_scopes.insert(
            def_id,
            ModuleScope {
                def_id,
                items: FxHashMap::default(),
                import_map: FxHashMap::default(),
            },
        );
        self.current_path.push(def_id);
    }

    fn current_module(&self) -> DefId {
        self.current_path
            .last()
            .copied()
            .unwrap_or_else(|| DefId::new(0, DefKind::Module))
    }
    fn push_module_child(&mut self, parent: DefId, child: DefId) {
        self.module_children
            .entry(parent)
            .or_insert_with(Vec::new)
            .push(child);
    }

    /// 辅助函数：解析类型名称
    fn resolve_type_name(&mut self, name: StringId, span: Span) -> Option<DefId> {
        // 首先检查是否是内置类型
        if let Some(builtin) = &self.builtin {
            let builtin_types = &builtin.types;
            let name_str = get_global_string(name).unwrap();
            match name_str.as_ref() {
                "i8" => return Some(builtin_types.i8),
                "i16" => return Some(builtin_types.i16),
                "i32" => return Some(builtin_types.i32),
                "i64" => return Some(builtin_types.i64),
                "i128" => return Some(builtin_types.i128),
                "isize" => return Some(builtin_types.isize),
                "u8" => return Some(builtin_types.u8),
                "u16" => return Some(builtin_types.u16),
                "u32" => return Some(builtin_types.u32),
                "u64" => return Some(builtin_types.u64),
                "u128" => return Some(builtin_types.u128),
                "usize" => return Some(builtin_types.usize),
                "f32" => return Some(builtin_types.f32),
                "f64" => return Some(builtin_types.f64),
                "bool" => return Some(builtin_types.bool),
                "char" => return Some(builtin_types.char),
                "str" => return Some(builtin_types.str),
                "()" => return Some(builtin_types.unit),
                "!" => return Some(builtin_types.never),
                "RawPtr" => return Some(builtin_types.raw_ptr),
                _ => {}
            }
        }

        let current_module = self.current_module();
        if let Some(def_id) = self.type_ns.get(current_module, name) {
            if Self::check_visibility(&self.definitions, current_module, def_id) {
                return Some(def_id);
            }
            self.diagnostics.push(
                error(format!(
                    "模块私有内容 `{}`",
                    get_global_string(name).unwrap()
                ))
                .with_span(span)
                .build(),
            );
        }

        self.diagnostics.push(
            error(format!(
                "类型 `{}` 未找到",
                get_global_string(name).unwrap().as_ref()
            ))
            .with_span(span)
            .build(),
        );
        None
    }

    fn insert_ns(&mut self, module: DefId, name: StringId, def: DefId) {
        let kind = self.definitions[def.index as usize].kind;
        match kind {
            DefKind::Function | DefKind::Static | DefKind::Const | DefKind::Local => {
                self.value_ns.insert(module, name, def)
            }
            DefKind::Struct | DefKind::Enum | DefKind::Module => {
                self.value_ns.insert(module, name, def);
                self.type_ns.insert(module, name, def);
            }
            _ => {}
        }
    }

    fn flatten_import_map(&self) -> FxHashMap<(DefId, StringId), DefId> {
        let mut map = FxHashMap::default();
        for scope in self.module_scopes.values() {
            for (&name, &target) in &scope.import_map {
                map.insert((scope.def_id, name), target);
            }
        }
        map
    }

    // ==================== 局部作用域 & RHir lowering ====================
    fn enter_scope(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope::new(kind));
    }
    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn lower_to_rhir(&mut self, raw: &RawCrate) -> RCrate {
        let mut items = Vec::new();
        for item in &raw.items {
            items.push(self.lower_item(item));
        }
        RCrate { items }
    }

    fn lower_item(&mut self, item: &RawItem) -> RItem {
        match item {
            RawItem::Module {
                visibility,
                name,
                items,
                span,
                ..
            } => {
                let current_module = self.current_module();
                let def_id = self.module_scopes.get(&current_module).unwrap().items[name];

                // 优先使用内联模块的 items，否则从 raw_module_items 获取
                let krate = if let Some(inline_items) = items {
                    RawCrate {
                        items: inline_items.clone(),
                    }
                } else {
                    self.raw_module_items.get(&def_id).cloned().unwrap()
                };

                self.current_path.push(def_id);
                let mut ritems = Vec::new();
                for item in &krate.items {
                    ritems.push(self.lower_item(item));
                }
                self.current_path.pop();

                RItem::Module {
                    def_id,
                    visibility: *visibility,
                    name: *name,
                    items: ritems,
                    span: *span,
                }
            }

            RawItem::Function {
                visibility,
                name,
                params,
                return_type,
                body,
                span,
                ..
            } => {
                let current_module = self.current_module();
                let fn_def = self.module_scopes.get(&current_module).unwrap().items[name];
                self.enter_scope(ScopeKind::Function);
                let mut rparams = Vec::with_capacity(params.len());
                for p in params {
                    let p_def = self.create_def(
                        p.name,
                        Visibility::Private,
                        DefKind::Local,
                        p.span,
                        fn_def,
                    );
                    self.scopes.last_mut().unwrap().insert(p.name, p_def);
                    self.value_ns.insert(fn_def, p.name, p_def);
                    rparams.push(RParam {
                        def_id: p_def,
                        name: p.name,
                        ty: self.lower_type(&p.ty),
                        span: p.span,
                    });
                }
                let rbody = self.lower_block(body);
                self.exit_scope();
                RItem::Function {
                    def_id: fn_def,
                    visibility: *visibility,
                    name: *name,
                    params: rparams,
                    return_type: return_type.as_ref().map(|t| self.lower_type(t)),
                    body: rbody,
                    span: *span,
                }
            }
            RawItem::Struct {
                visibility,
                name,
                fields,
                span,
                ..
            } => {
                let current_module = self.current_module();
                let def_id = self.module_scopes[&current_module].items[name];
                let rfields = fields
                    .iter()
                    .map(|f| {
                        let f_def = self.create_def(
                            f.name,
                            Visibility::from(f.visibility),
                            DefKind::Field,
                            f.span,
                            def_id,
                        );
                        RField {
                            def_id: f_def,
                            name: f.name,
                            ty: self.lower_type(&f.ty),
                            visibility: Visibility::from(f.visibility),
                            index: f.index,
                            span: f.span,
                        }
                    })
                    .collect();
                RItem::Struct {
                    def_id,
                    visibility: Visibility::from(*visibility),
                    name: *name,
                    fields: rfields,
                    span: *span,
                }
            }
            RawItem::Use {
                visibility,
                path,
                rename,
                span,
                items,
            } => {
                // 处理单个导入 use foo::bar as baz
                if items.is_empty() {
                    let alias = rename.unwrap_or(*path.last().unwrap());
                    let target = match self.resolve_path_internal(path, *span) {
                        Some(def_id) => def_id,
                        None => self.new_ghost(*path.last().unwrap(), *span),
                    };
                    RItem::Use {
                        visibility: Visibility::from(*visibility),
                        alias,
                        target,
                        span: *span,
                    }
                }
                // 处理多个导入 use foo::{a, b, c}
                else {
                    // 创建一个虚拟的 use 项作为容器
                    let ghost_def = self.new_ghost(intern_global("use_group"), *span);

                    // 为每个导入项创建单独的 use
                    for item in items {
                        let alias = item.rename.unwrap_or(item.name);
                        let mut full_path = path.clone();
                        full_path.push(item.name);

                        let target = match self.resolve_path_internal(&full_path, *span) {
                            Some(def_id) => def_id,
                            None => self.new_ghost(item.name, *span),
                        };

                        // 记录导入映射
                        let current_module = self.current_module();
                        let scope = self.module_scopes.get_mut(&current_module).unwrap();
                        scope.import_map.insert(alias, target);
                    }

                    RItem::Use {
                        visibility: Visibility::from(*visibility),
                        alias: intern_global("use_group"),
                        target: ghost_def,
                        span: *span,
                    }
                }
            }

            RawItem::Extern {
                attribute: _,
                visibility,
                abi,
                items,
                span,
            } => {
                let rexterns = items
                    .iter()
                    .map(|i| match i {
                        RawExternItem::Function {
                            visibility: _,
                            is_varidic,
                            name,
                            params,
                            return_type,
                            span,
                        } => {
                            let def_id = self
                                .module_scopes
                                .get(&self.current_module())
                                .unwrap()
                                .items[name];
                            let rparams = params
                                .iter()
                                .map(|p| RParam {
                                    def_id: self.create_def(
                                        p.name,
                                        Visibility::Private,
                                        DefKind::Local,
                                        p.span,
                                        def_id,
                                    ),
                                    name: p.name,
                                    ty: self.lower_type(&p.ty),
                                    span: p.span,
                                })
                                .collect();

                            RExternItem::Function {
                                def_id,
                                name: *name,
                                is_variadic: *is_varidic,
                                params: rparams,
                                return_type: return_type.as_ref().map(|t| self.lower_type(t)),
                                span: *span,
                            }
                        }
                    })
                    .collect();
                RItem::Extern {
                    visibility: Visibility::from(*visibility),
                    abi: *abi,
                    items: rexterns,
                    span: *span,
                }
            }
        }
    }

    fn lower_type(&mut self, t: &RawType) -> RType {
        match t {
            RawType::Named { name, span } => {
                let def_id = match self.resolve_type_name(*name, *span) {
                    Some(def_id) => def_id,
                    None => self.new_ghost(*name, *span),
                };
                RType::Named {
                    id: def_id,
                    span: *span,
                }
            }
            RawType::Generic { name, args, span } => {
                let def_id = match self.resolve_type_name(*name, *span) {
                    Some(def_id) => def_id,
                    None => self.new_ghost(*name, *span),
                };

                let rargs = args.iter().map(|ty| self.lower_type(ty)).collect();
                RType::Generic {
                    id: def_id,
                    args: rargs,
                    span: *span,
                }
            }
            RawType::Tuple { elements, span } => RType::Tuple {
                elements: elements.iter().map(|e| self.lower_type(e)).collect(),
                span: *span,
            },
            RawType::Reference {
                mutable,
                target,
                span,
            } => RType::Reference {
                mutable: *mutable,
                target: Box::new(self.lower_type(target)),
                span: *span,
            },
            RawType::Pointer {
                mutable,
                target,
                span,
            } => RType::Pointer {
                mutable: *mutable,
                target: Box::new(self.lower_type(target)),
                span: *span,
            },
        }
    }

    fn lower_block(&mut self, b: &RawBlock) -> RBlock {
        let mut rstmts = Vec::with_capacity(b.stmts.len());
        for s in &b.stmts {
            rstmts.push(self.lower_stmt(s));
        }
        let tail = b.tail.as_ref().map(|e| Box::new(self.lower_expr(e)));
        RBlock {
            stmts: rstmts,
            tail,
            span: b.span,
        }
    }

    fn lower_stmt(&mut self, s: &RawStmt) -> RStmt {
        match s {
            RawStmt::Expr(e) => RStmt::Expr(Box::new(self.lower_expr(e))),
            RawStmt::Let {
                mutable,
                name,
                ty,
                value,
                span,
            } => {
                let var_def = self.create_def(
                    *name,
                    Visibility::Private,
                    DefKind::Local,
                    *span,
                    self.current_module(),
                );
                self.scopes.last_mut().unwrap().insert(*name, var_def);
                self.value_ns.insert(self.current_module(), *name, var_def);
                RStmt::Let {
                    mutable: *mutable,
                    name: *name,
                    def_id: var_def,
                    ty: ty.as_ref().map(|t| self.lower_type(t)),
                    value: value.as_ref().map(|v| Box::new(self.lower_expr(v))),
                    span: *span,
                }
            }
            RawStmt::Return { value, span } => RStmt::Return {
                value: value.as_ref().map(|v| Box::new(self.lower_expr(v))),
                span: *span,
            },
            RawStmt::Break { value, span } => RStmt::Break {
                value: value.as_ref().map(|v| Box::new(self.lower_expr(v))),
                span: *span,
            },
            RawStmt::Continue { span } => RStmt::Continue { span: *span },
        }
    }

    fn lower_expr(&mut self, e: &RawExpr) -> RExpr {
        match e {
            RawExpr::Literal { value, span } => RExpr::Literal {
                value: value.clone(),
                span: *span,
            },
            RawExpr::Ident { name, span } => {
                if let Some(def) = self.value_ns.get(self.current_module(), *name) {
                    return match def.kind {
                        DefKind::Local => RExpr::Local {
                            def_id: def,
                            span: *span,
                        },
                        _ => RExpr::Global {
                            def_id: def,
                            span: *span,
                        },
                    };
                }
                if let Some(def) = self.scopes.iter().rev().find_map(|s| s.get(*name)) {
                    return RExpr::Local {
                        def_id: def,
                        span: *span,
                    };
                }
                if let Some(&def) = self
                    .module_scopes
                    .get(&self.current_module())
                    .unwrap()
                    .items
                    .get(name)
                {
                    return RExpr::Global {
                        def_id: def,
                        span: *span,
                    };
                }
                if let Some(&def) = self
                    .module_scopes
                    .get(&self.current_module())
                    .unwrap()
                    .import_map
                    .get(name)
                {
                    return RExpr::Global {
                        def_id: def,
                        span: *span,
                    };
                }
                let ghost = self.new_ghost(*name, *span);
                RExpr::Global {
                    def_id: ghost,
                    span: *span,
                }
            }
            RawExpr::PathAccess { segments, span } => {
                let def_id = match self.resolve_path_internal(segments, *span) {
                    Some(def_id) => def_id,
                    None => {
                        let path_str = segments
                            .into_iter()
                            .map(|&id| get_global_string(id).unwrap().to_string())
                            .collect::<Vec<_>>()
                            .join("::");
                        self.diagnostics.push(
                            error(format!("位置路径 `{}`", path_str))
                                .with_span(*span)
                                .build(),
                        );
                        self.new_ghost(*segments.last().unwrap(), *span)
                    }
                };
                RExpr::Global {
                    def_id: def_id,
                    span: *span,
                }
            }
            RawExpr::Binary {
                left,
                right,
                op,
                span,
            } => RExpr::Binary {
                left: Box::new(self.lower_expr(left)),
                right: Box::new(self.lower_expr(right)),
                op: *op,
                span: *span,
            },
            RawExpr::Unary { op, operand, span } => RExpr::Unary {
                op: *op,
                operand: Box::new(self.lower_expr(operand)),
                span: *span,
            },
            RawExpr::Call { callee, args, span } => {
                let lowered_callee = self.lower_expr(callee);

                let def_id = match lowered_callee {
                    RExpr::Global { def_id, .. } => def_id,
                    RExpr::Local { def_id, .. } => def_id,
                    _ => unreachable!(),
                };
                let mut r_args = Vec::new();
                for arg in args {
                    r_args.push(self.lower_expr(arg));
                }

                RExpr::Call {
                    callee: def_id,
                    args: r_args,
                    span: *span,
                }
            }
            RawExpr::Block { block } => {
                self.enter_scope(ScopeKind::Block);
                let lowered_block = self.lower_block(block);
                self.exit_scope();
                RExpr::Block {
                    block: lowered_block,
                }
            }
            RawExpr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => RExpr::If {
                condition: Box::new(self.lower_expr(condition)),
                then_branch: {
                    self.enter_scope(ScopeKind::Block);
                    let block = self.lower_block(then_branch);
                    self.exit_scope();
                    block
                },
                else_branch: else_branch.as_ref().map(|e| {
                    self.enter_scope(ScopeKind::Block);
                    let expr = Box::new(self.lower_expr(e));
                    self.exit_scope();
                    expr
                }),
                span: *span,
            },
            RawExpr::Loop { body, span } => {
                self.enter_scope(ScopeKind::Loop);
                let lowered_body = self.lower_block(body);
                self.exit_scope();
                RExpr::Loop {
                    body: Box::new(lowered_body),
                    span: *span,
                }
            }
            RawExpr::FieldAccess { base, field, span } => {
                let rbase = self.lower_expr(base);

                // 在收集阶段已经为结构体字段创建了定义
                // 现在查找对应字段的定义
                let placeholder_field_def_id = self.new_ghost(*field, *span);

                RExpr::FieldAccess {
                    base: Box::new(rbase),
                    field: RField {
                        def_id: placeholder_field_def_id,
                        name: *field,                    // 字段名称
                        ty: RType::Unknown,              // 占位类型，type checker 会更新
                        visibility: Visibility::Private, // 占位可见性，type checker 会更新
                        index: 0,                        // 占位索引，type checker 会更新
                        span: *span,                     // 使用表达式的 span
                    },
                    def_id: placeholder_field_def_id, // 表达式本身的 def_id
                    span: *span,
                }
            }
            RawExpr::Index {
                indexed,
                index,
                span,
            } => RExpr::Index {
                indexed: Box::new(self.lower_expr(indexed)),
                index: Box::new(self.lower_expr(index)),
                span: *span,
            },
            RawExpr::Tuple { elements, span } => RExpr::Tuple {
                elements: elements.iter().map(|e| self.lower_expr(e)).collect(),
                span: *span,
            },
            RawExpr::Unit { span } => RExpr::Unit { span: *span },
            RawExpr::Grouped { expr, span } => RExpr::Grouped {
                expr: Box::new(self.lower_expr(expr)),
                span: *span,
            },
            RawExpr::To { start, end, span } => RExpr::To {
                start: Box::new(self.lower_expr(start)),
                end: Box::new(self.lower_expr(end)),
                span: *span,
            },
            RawExpr::ToEq { start, end, span } => RExpr::ToEq {
                start: Box::new(self.lower_expr(start)),
                end: Box::new(self.lower_expr(end)),
                span: *span,
            },
            RawExpr::Posifix { operand, op, span } => RExpr::Postfix {
                operand: Box::new(self.lower_expr(operand)),
                op: *op,
                span: *span,
            },
            RawExpr::Assign {
                target,
                op,
                value,
                span,
            } => RExpr::Assign {
                target: Box::new(self.lower_expr(target)),
                op: *op,
                value: Box::new(self.lower_expr(value)),
                span: *span,
            },
            RawExpr::Dereference { expr, span } => RExpr::Dereference {
                expr: Box::new(self.lower_expr(expr)),
                span: *span,
            },
            RawExpr::AddressOf { expr, span } => RExpr::AddressOf {
                expr: Box::new(self.lower_expr(expr)),
                span: *span,
            },
            RawExpr::StructInit {
                r#struct,
                fields,
                span,
            } => {
                let def_id = match self.resolve_from_scope(self.current_module(), r#struct, *span) {
                    Some(def_id) => def_id,
                    None => self.new_ghost(*r#struct.last().unwrap(), *span),
                };

                let rfields = fields
                    .iter()
                    .map(|(name_opt, value)| (*name_opt, self.lower_expr(value)))
                    .collect();

                RExpr::StructInit {
                    def_id: def_id,
                    fields: rfields,
                    span: *span,
                }
            }
            RawExpr::Cast { expr, ty, span } => {
                let expr = self.lower_expr(expr);
                let ty = self.lower_type(ty);
                RExpr::Cast {
                    expr: Box::new(expr),
                    ty,
                    span: *span,
                }
            }
        }
    }

    fn find_root_crate(&self, module: DefId) -> DefId {
        let mut current = module;
        loop {
            // 如果是根模块(index=0)，直接返回
            if current.index == 0 {
                return current;
            }
            // 查找当前模块的父模块
            if let Some(def) = self.definitions.get(current.index as usize) {
                current = def.module_id;
            } else {
                // 如果找不到定义，返回根模块作为fallback
                return DefId::new(0, DefKind::Module);
            }
        }
    }

    fn new_ghost(&mut self, name: StringId, span: Span) -> DefId {
        let idx = self.index.fetch_add(1, Ordering::Relaxed);
        let def_id = DefId::new(idx, DefKind::Ghost);
        let ghost_def = Definition {
            def_id: def_id,
            visibility: Visibility::Private,
            name,
            kind: DefKind::Ghost,
            module_id: DefId::new(0, DefKind::Module), // Ghost 定义属于根模块
            span,
        };
        self.definitions.push(ghost_def);
        def_id
    }
}

pub fn resolve(hir: RawCrate, source_map: &mut SourceMap, field_id: FileId) -> ResolveOutput {
    let resolver = Resolver::new(source_map, field_id);
    resolver.resolve(&hir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use litec_span::{SourceMap, intern_global};

    fn check_files(file: &str) -> ResolveOutput {
        let mut sm = SourceMap::new();
        let main_file_id = sm.add_file("main.lt".into(), file.into(), &PathBuf::from("main.lt"));

        // 在测试模式下，需要解析主文件的内容
        let (ast, ast_diagnostics) = parse(&sm, main_file_id);
        let (raw_crate, lower_diagnostics) = lower(ast);

        dbg!(&raw_crate);
        for diagnostic in ast_diagnostics.into_iter().chain(lower_diagnostics) {
            eprintln!("{}", diagnostic.render(&sm));
        }

        let resolver = Resolver::new(&mut sm, main_file_id);
        resolver.resolve(&raw_crate)
    }

    #[test]
    fn test_private_visibility() {
        let out = check_files("pub fn bar() -> i32 { 42 }");
        dbg!(&out);
        // --- 正常解析到真定义 ---
        let bar_def = out
            .definitions
            .iter()
            .any(|d| d.name == intern_global("bar") && d.kind == DefKind::Function);
        assert!(bar_def, "public item should be resolved normally");
    }
}
