use rustc_hash::FxHashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use litec_span::{get_global_string, Span, StringId};
use litec_hir::{Crate as RawCrate, Item as RawItem, UseItem};
use litec_typed_hir::{def_id::DefId, DefKind};
use litec_error::Error;
use litec_lower::lower_crate;
use litec_parse::parser::Parser;

pub type ModuleId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key(ModuleId, StringId); // (父模块, 名字)

#[derive(Debug, Clone)]
struct Module {
    pub id: ModuleId,
    pub path: PathBuf,
    pub bindings: FxHashMap<Key, DefId>,
    pub def_kinds: FxHashMap<DefId, DefKind>,
    pub parent: Option<ModuleId>,
    pub children: Vec<ModuleId>,
}

pub struct Resolver {
    next_id: AtomicU32,
    root_path: PathBuf,
    modules: FxHashMap<ModuleId, Module>, // 存储所有模块的符号表
}

impl Resolver {
    pub fn new(root_path: PathBuf) -> Self {
        let main_module_id = 0;
        let main_module = Module {
            id: main_module_id,
            path: root_path.clone(),
            bindings: FxHashMap::default(),
            def_kinds: FxHashMap::default(),
            parent: None,
            children: Vec::new(),
        };

        let mut modules = FxHashMap::default();
        modules.insert(main_module_id, main_module);

        Resolver {
            next_id: AtomicU32::new(1),
            root_path,
            modules,
        }
    }

    pub fn populate(&mut self, root: &RawCrate) -> Result<(), Vec<Error>> {
        let root_id = 0;
        self.scan_module(root_id, root.items.iter())
    }

    fn scan_module<'a>(&mut self, mod_id: ModuleId, items: impl Iterator<Item = &'a RawItem>) -> Result<(), Vec<Error>> {
        for item in items {
            match item {
                RawItem::Function { name, .. } => {
                    let def_id = self.fresh_def_id(DefKind::Function);
                    self.modules.get_mut(&mod_id).unwrap().bindings.insert(Key(mod_id, *name), def_id);
                    self.modules.get_mut(&mod_id).unwrap().def_kinds.insert(def_id, DefKind::Function);
                }
                RawItem::Struct { name, .. } => {
                    let def_id = self.fresh_def_id(DefKind::Struct);
                    self.modules.get_mut(&mod_id).unwrap().bindings.insert(Key(mod_id, *name), def_id);
                    self.modules.get_mut(&mod_id).unwrap().def_kinds.insert(def_id, DefKind::Struct);
                }
                RawItem::Use { path, items, span, .. } => {
                    // 解析路径并加载模块
                    match self.load_module(path, *span)? {
                        RawCrate { items: module_items } => {
                            // 创建一个新的模块 ID
                            let child_mod_id = self.fresh_module_id();
                            self.modules.get_mut(&mod_id).unwrap().children.push(child_mod_id);

                            // 创建子模块
                            let child_module_path = self.root_path.join(path.iter().map(|id| get_global_string(*id).unwrap().to_string()).collect::<Vec<String>>().join("/"));
                            let mut child_module = Module {
                                id: child_mod_id,
                                path: child_module_path,
                                bindings: FxHashMap::default(),
                                def_kinds: FxHashMap::default(),
                                parent: Some(mod_id),
                                children: Vec::new(),
                            };

                            // 递归解析子模块
                            self.modules.insert(child_mod_id, child_module.clone());
                            self.scan_module(child_mod_id, module_items.iter())?;

                            // 将子模块的符号表合并到当前模块
                            for (key, def_id) in child_module.bindings.drain() {
                                self.modules.get_mut(&mod_id).unwrap().bindings.insert(key, def_id);
                            }
                        }
                    }
                    // 如果有嵌套项，处理它们
                    if let Some(nested_items) = items {
                        for nested_item in nested_items {
                            self.process_use_item(mod_id, nested_item)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn process_use_item(&mut self, mod_id: ModuleId, item: &UseItem) -> Result<(), Vec<Error>> {
        let name = item.name;
        let rename = item.rename.unwrap_or(name);
        match self.resolve_path(&[name], mod_id) {
            Some(def_id) => {
                let module = self.modules.get_mut(&mod_id).unwrap();
                module.bindings.insert(Key(mod_id, rename), def_id);
                if let Some(nested_items) = &item.items {
                    for nested_item in nested_items {
                        self.process_use_item(mod_id, nested_item)?;
                    }
                }
            }
            None => {
                return Err(vec![Error::UnknowPath { 
                    path: vec![get_global_string(name).unwrap().to_string()].join("::"), 
                    span: item.span
                }]);
            }
        }
        Ok(())
    }

    fn fresh_def_id(&self, kind: DefKind) -> DefId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        DefId::new(id, kind)
    }

    fn fresh_module_id(&self) -> ModuleId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        id
    }

    pub fn resolve_path(&self, path: &[StringId], mod_id: ModuleId) -> Option<DefId> {
        let mut current = mod_id;
        for (i, &segment) in path.iter().enumerate() {
            let key = Key(current, segment);
            if let Some(module) = self.modules.get(&current) {
                if let Some(def_id) = module.bindings.get(&key) {
                    if i == path.len() - 1 {
                        return Some(*def_id);
                    }
                    if let Some(&DefKind::Module) = module.def_kinds.get(def_id) {
                        current = self.module_of(*def_id)?;
                    } else {
                        return None;
                    }
                } else if segment == "super".into() {
                    current = self.modules.get(&mod_id).unwrap().parent?; // 向上爬父模块
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        None
    }

    fn module_of(&self, def_id: DefId) -> Option<ModuleId> {
        if let Some(module) = self.modules.values().find(|m| m.bindings.values().any(|&id| id == def_id)) {
            Some(module.id)
        } else {
            None
        }
    }

    fn load_module(&mut self, path: &[StringId], error_span: Span) -> Result<RawCrate, Vec<Error>> {
        let mut current_path = self.root_path.clone();
        for segment in path {
            current_path.push(get_global_string(*segment).unwrap().as_ref());
            if !current_path.exists() {
                return Err(vec![Error::UnknowPath { 
                    path: path.into_iter().map(|id| get_global_string(*id).unwrap().to_string()).collect::<Vec<String>>().join("::"), 
                    span: error_span
                }]);
            }
        }
        let source = match fs::read_to_string(current_path) {
            Ok(src) => src,
            Err(err) => {
                return Err(vec![Error::IOError { error: err.kind(), span: error_span }])
            }
        };
    
        let mut parser = Parser::new(source.as_str());
        let ast = match parser.parse() {
            Ok(ast) => ast,
            Err(err) => return Err(err)
        };
        let hir = lower_crate(ast)?;
    
        Ok(hir)
    }

    /// 返回当前模块所有顶层 (名字, DefId, DefKind)
    pub fn current_toplevel(&self, mod_id: ModuleId) -> Vec<(StringId, DefId, DefKind)> {
        let module = self.modules.get(&mod_id).unwrap();
        module.bindings
              .iter()
              .filter_map(|(Key(_, name), &def_id)| {
                  let kind = module.def_kinds[&def_id];
                  // 只拿函数/结构体
                  match kind {
                      DefKind::Function | DefKind::Struct => Some((*name, def_id, kind)),
                      _ => None,
                  }
              })
              .collect()
    }
}