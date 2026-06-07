use litec_ast::ast::Ident;
use litec_span::id::DefId;

#[derive(Debug, Clone)]
pub struct EnumVariantInfo {
    pub name: Ident,
    pub variant_def_id: DefId,
    pub kind: VariantKind,
}

#[derive(Debug, Clone, Copy)]
pub enum VariantKind {
    Unit,
    Tuple,
    Struct,
}
