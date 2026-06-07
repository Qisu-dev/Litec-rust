use crate::{def::DefKind, def_path::DefPathKind};
use litec_ast::ast::Visibility;
use litec_span::{StringId, id::DefId};

#[derive(Debug, Clone)]
pub struct DefData {
    pub kind: DefKind,
    pub name: StringId,
    pub parent: Option<DefId>,
    pub path_data: DefPathKind,
    pub visibility: Visibility,
}
