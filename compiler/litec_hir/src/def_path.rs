use litec_span::StringId;
use rustc_stable_hash::{FromStableHash, SipHasher128Hash, StableSipHasher128};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefPathKind {
    CrateRoot,
    Module(StringId),
    ExternCrate(StringId),
    Struct(StringId),
    Enum(StringId),
    Trait(StringId),
    TyAlias(StringId),
    Fn(StringId),
    Const(StringId),
    Static(StringId),
    Impl,
    TraitImpl,
    ImplFn(StringId),
    ImplTy(StringId),
    Variant(StringId),
    TyParam(StringId),
    Ctor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefPath {
    pub segments: Vec<DefPathKind>,
}

impl DefPath {
    pub fn stable_hash(&self) -> DefPathHash {
        let mut hasher = StableSipHasher128::new();
        for seg in &self.segments {
            seg.hash(&mut hasher);
        }
        hasher.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DefPathHash(pub u128);

impl FromStableHash for DefPathHash {
    type Hash = SipHasher128Hash;

    fn from(SipHasher128Hash([high, low]): Self::Hash) -> Self {
        Self((high as u128) << 64 | low as u128)
    }
}
