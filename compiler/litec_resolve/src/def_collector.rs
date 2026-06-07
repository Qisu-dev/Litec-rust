use litec_ast::ast::{self, *};
use litec_ast::{ast::NodeId, visit::Visitor};
use litec_hir::adt::{EnumVariantInfo, VariantKind};
use litec_hir::def::{CtorKind, CtorOf, DefKind};
use litec_middle::context::GlobalCtxt;
use litec_span::StringId;
use litec_span::id::{DefId, LocalDefId};

pub struct DefCollector<'a, 'tcx> {
    gcx: &'a mut GlobalCtxt<'tcx>,
    parent_stack: Vec<LocalDefId>,
}

impl<'a, 'tcx> DefCollector<'a, 'tcx> {
    pub fn new(gcx: &'a mut GlobalCtxt<'tcx>) -> Self {
        Self {
            gcx,
            parent_stack: vec![],
        }
    }

    pub fn collect(mut self, ast: &ast::Crate) {
        self.visit_crate(&ast);
    }

    fn push_parent(&mut self, def_id: LocalDefId) {
        self.parent_stack.push(def_id);
    }

    fn pop_parent(&mut self) {
        self.parent_stack.pop();
    }

    fn allocate_def(
        &mut self,
        node_id: NodeId,
        kind: DefKind,
        name: impl Into<StringId>,
        visibility: Visibility,
    ) -> LocalDefId {
        let parent_def_id = self.parent_stack.last().map(|parent| parent.to_def_id());
        let local_id = self
            .gcx
            .create_local(kind, name.into(), parent_def_id, visibility);
        self.gcx.map_node(node_id, local_id);
        local_id
    }

    fn allocate_ctor(
        &mut self,
        ctor_of: CtorOf,
        ctor_kind: CtorKind,
        name: StringId,
        parent_def_id: DefId,
    ) -> LocalDefId {
        let ctor_local_def_id = self.gcx.create_local(
            DefKind::Ctor(ctor_of, ctor_kind),
            name,
            Some(parent_def_id),
            Visibility::Public,
        );
        self.gcx
            .record_ctor(parent_def_id, ctor_local_def_id.to_def_id());
        ctor_local_def_id
    }
}

impl<'a, 'tcx> Visitor for DefCollector<'a, 'tcx> {
    fn visit_crate(&mut self, krate: &Crate) {
        let local_def = self.allocate_def(krate.node_id, DefKind::Crate, "", Visibility::Public);

        self.push_parent(local_def);

        for item in &krate.items {
            self.visit_item(item);
        }

        self.pop_parent();
    }

    fn visit_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Fn(fn_) => {
                self.visit_generics(&fn_.sig.generics);
                self.allocate_def(item.node_id, DefKind::Fn, fn_.sig.name, item.visibility);
            }
            ItemKind::Struct(ident, generics, kind) => {
                let struct_def =
                    self.allocate_def(item.node_id, DefKind::Struct, *ident, item.visibility);
                self.visit_generics(generics);

                let ctor_kind = match kind {
                    StructKind::Unit => Some(CtorKind::Const),
                    StructKind::Tuple(_) => Some(CtorKind::Fn),
                    StructKind::Struct(_) => None,
                };
                if let Some(ck) = ctor_kind {
                    self.allocate_ctor(CtorOf::Struct, ck, ident.text, struct_def.to_def_id());
                }
            }
            ItemKind::Use(_) => {}
            ItemKind::Extern(extern_) => {
                self.visit_extern(extern_);
            }
            ItemKind::Module(ident, inline) => {
                let local_def_id =
                    self.allocate_def(item.node_id, DefKind::Module, *ident, item.visibility);

                self.push_parent(local_def_id);
                match inline {
                    Inline::External(items) | Inline::Inline(items) => {
                        for item in items {
                            self.visit_item(item);
                        }
                    }
                }
                self.pop_parent();
            }
            ItemKind::Impl(impl_) => {
                let impl_def_id = self.allocate_def(
                    item.node_id,
                    match &impl_.of_trait {
                        Some(_) => DefKind::TraitImpl,
                        None => DefKind::Impl,
                    },
                    "",
                    Visibility::Public,
                );

                self.visit_generics(&impl_.generics);

                self.push_parent(impl_def_id);
                for item in &impl_.items {
                    self.visit_impl_item(item);
                }
                self.pop_parent();
            }
            ItemKind::Trait(ident, generics, items) => {
                let local_def_id =
                    self.allocate_def(item.node_id, DefKind::Trait, *ident, item.visibility);

                self.visit_generics(generics);

                self.push_parent(local_def_id);
                for item in items {
                    self.visit_trait_item(item);
                }
                self.pop_parent();
            }
            ItemKind::TypeAlias(type_alias) => {
                self.allocate_def(
                    item.node_id,
                    DefKind::TyAlias,
                    type_alias.name,
                    item.visibility,
                );
            }
            ItemKind::Enum(ident, generics, variants) => {
                let enum_def =
                    self.allocate_def(item.node_id, DefKind::Enum, *ident, item.visibility);
                let enum_def_id = enum_def.to_def_id();

                self.visit_generics(generics);

                let mut variant_infos = Vec::new();

                self.push_parent(enum_def);
                for variant in variants {
                    let variant_def = self.allocate_def(
                        variant.node_id,
                        DefKind::Variant,
                        variant.ident,
                        item.visibility,
                    );
                    let variant_def_id = variant_def.to_def_id();

                    let (kind, need_ctor) = match &variant.data {
                        VariantData::Unit => (VariantKind::Unit, true),
                        VariantData::Tuple(_) => (VariantKind::Tuple, true),
                        VariantData::Struct(_) => (VariantKind::Struct, false),
                    };

                    if need_ctor {
                        let ctor_kind = match kind {
                            VariantKind::Unit => CtorKind::Const,
                            VariantKind::Tuple => CtorKind::Fn,
                            _ => unreachable!(),
                        };
                        self.allocate_ctor(
                            CtorOf::Variant,
                            ctor_kind,
                            variant.ident.text,
                            variant_def_id,
                        );
                    }

                    variant_infos.push(EnumVariantInfo {
                        name: variant.ident,
                        variant_def_id,
                        kind,
                    });
                }
                self.pop_parent();

                let variant_slice = self.gcx.alloc_slice_clone(variant_infos.as_slice());
                self.gcx.record_variant_infos(enum_def_id, variant_slice);
            }
        }
    }

    fn visit_generics(&mut self, generic_params: &Generics) {
        for param in &generic_params.params {
            self.allocate_def(
                param.node_id,
                DefKind::TyParam,
                param.name.text,
                Visibility::Public,
            );
        }
    }

    fn visit_trait_item(&mut self, trait_item: &TraitItem) {
        match &trait_item.kind {
            TraitItemKind::Fn(sig) => {
                self.allocate_def(
                    trait_item.node_id,
                    DefKind::ExternFn,
                    sig.name,
                    Visibility::Public,
                );
            }
            TraitItemKind::Type(type_alias) => {
                self.allocate_def(
                    trait_item.node_id,
                    DefKind::TyAlias,
                    type_alias.name,
                    Visibility::Public,
                );
            }
        }
    }

    fn visit_extern(&mut self, ext: &Extern) {
        for item in &ext.items {
            self.visit_extern_item(item);
        }
    }

    fn visit_extern_item(&mut self, item: &ExternItem) {
        match &item.kind {
            ExternItemKind::Fn(fn_) => {
                self.allocate_def(
                    fn_.node_id,
                    DefKind::ExternFn,
                    fn_.sig.name,
                    item.visibility,
                );
            }
        }
    }

    fn visit_impl_item(&mut self, impl_item: &ImplItem) {
        match &impl_item.kind {
            ImplItemKind::Fn(fn_) => {
                self.allocate_def(
                    impl_item.node_id,
                    DefKind::ImplFn,
                    fn_.sig.name,
                    impl_item.visibility,
                );
            }
            ImplItemKind::Type(ty) => {
                self.allocate_def(
                    impl_item.node_id,
                    DefKind::ImplTy,
                    ty.name,
                    impl_item.visibility,
                );
            }
        }
    }
}
