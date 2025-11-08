use crate::DefKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId {
    pub index: u32,
    pub kind: DefKind
}

impl DefId {
    pub const fn new(index: u32, kind: DefKind) -> Self {
        DefId { index, kind }
    }
}