use crate::{
    def::DefKind,
    def_path::{DefPathHash, DefPathKind},
    def_table::StableCrateId,
};
use litec_ast::ast::Visibility;
use litec_span::intern_global;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CrateMetadata {
    pub stable_crate_id: StableCrateId,
    pub crate_name: String,
    pub defs: Vec<SerDefData>, // 所有公开定义的元数据
    pub def_path_hash_to_index: Vec<(DefPathHash, u32)>, // 哈希 → 原始 DefIndex
    pub deps: Vec<StableCrateId>, // 直接依赖的 crate 稳定 ID
    pub crate_kind: CrateKind,
}

#[derive(Serialize, Deserialize)]
pub enum CrateKind {
    Lib,
    Bin,
}

#[derive(Serialize, Deserialize)]
pub struct SerDefData {
    pub kind: DefKind,
    pub name: String,                // 原始字符串（反序列化时 intern）
    pub parent: Option<DefPathHash>, // 父定义的稳定哈希（跨 crate）
    pub path_data: SerDefPathKind,
    pub visibility: Visibility,
}

#[derive(Serialize, Deserialize)]
pub enum SerDefPathKind {
    CrateRoot,
    Module(String),
    ExternCrate(String),
    Struct(String),
    Enum(String),
    Trait(String),
    TyAlias(String),
    Fn(String),
    Const(String),
    Static(String),
    Impl,
    TraitImpl,
    ImplFn(String),
    ImplTy(String),
    Variant(String),
    TyParam(String),
    Ctor,
}

impl From<DefPathKind> for SerDefPathKind {
    fn from(value: DefPathKind) -> Self {
        match value {
            DefPathKind::CrateRoot => SerDefPathKind::CrateRoot,
            DefPathKind::Module(id) => SerDefPathKind::Module(id.to_string()),
            DefPathKind::Impl => SerDefPathKind::Impl,
            DefPathKind::TraitImpl => SerDefPathKind::TraitImpl,
            DefPathKind::ImplFn(id) => SerDefPathKind::ImplFn(id.to_string()),
            DefPathKind::ImplTy(id) => SerDefPathKind::ImplTy(id.to_string()),
            DefPathKind::Variant(id) => SerDefPathKind::Variant(id.to_string()),
            DefPathKind::ExternCrate(id) => SerDefPathKind::ExternCrate(id.to_string()),
            DefPathKind::Struct(id) => SerDefPathKind::Struct(id.to_string()),
            DefPathKind::Enum(id) => SerDefPathKind::Enum(id.to_string()),
            DefPathKind::Trait(id) => SerDefPathKind::Trait(id.to_string()),
            DefPathKind::TyAlias(id) => SerDefPathKind::TyAlias(id.to_string()),
            DefPathKind::Fn(id) => SerDefPathKind::Fn(id.to_string()),
            DefPathKind::Const(id) => SerDefPathKind::Const(id.to_string()),
            DefPathKind::Static(id) => SerDefPathKind::Static(id.to_string()),
            DefPathKind::Ctor => SerDefPathKind::Ctor,
            DefPathKind::TyParam(id) => SerDefPathKind::TyParam(id.to_string()),
        }
    }
}

impl From<SerDefPathKind> for DefPathKind {
    fn from(value: SerDefPathKind) -> Self {
        match value {
            SerDefPathKind::CrateRoot => DefPathKind::CrateRoot,
            SerDefPathKind::Module(s) => DefPathKind::Module(intern_global(&s)),
            SerDefPathKind::Impl => DefPathKind::Impl,
            SerDefPathKind::TraitImpl => DefPathKind::TraitImpl,
            SerDefPathKind::ImplFn(s) => DefPathKind::ImplFn(intern_global(&s)),
            SerDefPathKind::ImplTy(s) => DefPathKind::ImplTy(intern_global(&s)),
            SerDefPathKind::Variant(s) => DefPathKind::Variant(intern_global(&s)),
            SerDefPathKind::ExternCrate(s) => DefPathKind::ExternCrate(intern_global(&s)),
            SerDefPathKind::Struct(s) => DefPathKind::Struct(intern_global(&s)),
            SerDefPathKind::Enum(s) => DefPathKind::Enum(intern_global(&s)),
            SerDefPathKind::Trait(s) => DefPathKind::Trait(intern_global(&s)),
            SerDefPathKind::TyAlias(s) => DefPathKind::TyAlias(intern_global(&s)),
            SerDefPathKind::Fn(s) => DefPathKind::Fn(intern_global(&s)),
            SerDefPathKind::Const(s) => DefPathKind::Const(intern_global(&s)),
            SerDefPathKind::Static(s) => DefPathKind::Static(intern_global(&s)),
            SerDefPathKind::Ctor => DefPathKind::Ctor,
            SerDefPathKind::TyParam(s) => DefPathKind::TyParam(intern_global(&s)),
        }
    }
}
