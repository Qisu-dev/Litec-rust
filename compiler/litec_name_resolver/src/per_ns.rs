use rustc_hash::FxHashMap;
use litec_typed_hir::def_id::DefId;
use litec_span::StringId;

#[derive(Debug)]
pub struct PerNs<T> {
    inner: FxHashMap<(DefId, StringId), T>,
}

impl<T> PerNs<T> {
    pub fn new() -> Self {
        Self {
            inner: FxHashMap::default(),
        }
    }
    pub fn get(&self, module: DefId, name: StringId) -> Option<T>
    where
        T: Copy,
    {
        self.inner.get(&(module, name)).copied()
    }
    pub fn insert(&mut self, module: DefId, name: StringId, val: T) {
        self.inner.insert((module, name), val);
    }
}