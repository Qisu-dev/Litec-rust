#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefKind {
    Function,
    Struct,
    Variable,
    Parameter,
    Field,
    Module,
    Constant,
    ExternFunction,
    Ghost,
    Static,
    Const,
    Local,
    Enum,
    Extern,
    Intrinsic,
    BuiltinType,
}

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

impl From<DefId> for usize {
    fn from(value: DefId) -> Self {
        value.index as usize
    }
}