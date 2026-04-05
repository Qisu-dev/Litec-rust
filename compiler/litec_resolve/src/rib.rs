use std::hash::BuildHasherDefault;

use indexmap::IndexMap;
use litec_ast::ast::Ident;
use litec_hir::def::Res;
use rustc_hash::FxHasher;

#[derive(Debug, Clone, Copy)]
pub enum RibKind {
    Normal,   // 普通块
    Function, // 函数参数
    Module,
}

pub type FxIndexMap<K, V> = IndexMap<K, V, BuildHasherDefault<FxHasher>>;

#[derive(Debug, Clone)]
pub struct Rib<R = Res> {
    pub bindings: FxIndexMap<Ident, R>,
    pub kind: RibKind,
}

impl<R> Rib<R> {
    pub fn new(kind: RibKind) -> Self {
        Self {
            bindings: Default::default(),
            kind,
        }
    }

    pub fn insert(&mut self, ident: Ident, binding: R) {
        self.bindings.insert(ident, binding);
    }

    pub fn get(&self, ident: &Ident) -> Option<&R> {
        self.bindings.get(ident)
    }
}
