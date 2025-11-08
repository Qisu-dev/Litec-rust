use litec_span::{Span, StringId};
use litec_typed_hir::{def_id::DefId, ty::Ty};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub enum Symbol {
    Variable {
        name: StringId,
        ty: Ty,
        span: Span
    },
    Function {
        name: StringId,
        params_type: Vec<Ty>,
        ret_type: Ty,
        span: Span
    },
    Struct {
        name: StringId,
        fields: Vec<(StringId, DefId, Ty)>,
        span: Span
    }
}

impl Symbol {
    pub fn name(&self) -> StringId {
        match self {
            Symbol::Variable { name, ..} => *name,
            Symbol::Function { name, .. } => *name,
            Symbol::Struct { name, .. } => *name
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Symbol::Function { span, .. } => *span,
            Symbol::Variable { span, .. } => *span,
            Symbol::Struct { span, .. } => *span,
        }
    }

    pub fn ty(&self) -> Ty {
        match self {
            Symbol::Variable { ty, .. } => ty.clone(),
            Symbol::Function { ret_type, .. } => ret_type.clone(),
            Symbol::Struct { .. } => unreachable!(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Scope {
    symbols: FxHashMap<DefId, Symbol>,
    identifiers: FxHashMap<StringId, DefId>,
    parent: Option<Box<Scope>>,
}

impl Scope {
    pub fn new(parent: Option<Box<Scope>>) -> Self {
        Self {
            symbols: Default::default(),
            identifiers: Default::default(),
            parent,
        }
    }

    pub fn insert_symbol(&mut self, id: DefId, symbol: Symbol) {
        self.identifiers.insert(symbol.name(), id);
        self.symbols.insert(id, symbol);
    }

    pub fn get_id(&self, name: &StringId) -> Option<&DefId> {
        self.identifiers.get(name).or_else(|| {
            self.parent.as_ref().and_then(|parent| parent.get_id(name))
        })
    }

    pub fn get_symbol(&self, id: &DefId) -> Option<&Symbol> {
        self.symbols.get(id).or_else(|| {
            self.parent.as_ref().and_then(|parent| parent.get_symbol(id))
        })
    }

    pub fn get_global(&self) -> &Scope {
        match &self.parent {
            Some(parent) => parent.as_ref().get_global(),
            None => self
        }
    }

    pub fn get_mut_global<'a>(&'a mut self) -> &'a mut Scope {
        if let Some(ref mut parent) = self.parent {
            parent.get_mut_global()
        } else {
            self
        }
    }

    pub fn take_parent(&mut self) -> Option<Box<Scope>> {
        std::mem::take(&mut self.parent)
    }

    pub fn replace_symbol(&mut self, id: DefId, sym: Symbol) {
        if let Some(old) = self.symbols.get_mut(&id) {
            *old = sym;
        }
    }

    pub fn get_current_layer_symbol(&self, name: &DefId) -> bool {
        self.symbols.contains_key(name)
    }

    pub fn get_current_layer_id(&self, name: &StringId) -> Option<&DefId> {
        self.identifiers.get(name)
    }
}