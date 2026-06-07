use index_vec::IndexVec;
use litec_ast::ast::{self, Ident, NodeId};
use litec_hir::{def::Res, hir};
use litec_middle::context::TyCtxt;
use litec_span::id::{HirId, ItemLocalId, OwnerId};
use rustc_hash::FxHashMap;

pub struct AstLowering<'hir> {
    tcx: TyCtxt<'hir>,
    owner_stack: Vec<OwnerId>,
    // 每个 owner 当前的局部计数器
    local_counters: FxHashMap<OwnerId, u32>,
    // 收集所有 HIR 节点，按 owner 分组
    owners: FxHashMap<OwnerId, FxHashMap<ItemLocalId, &'hir hir::Node<'hir>>>,

    node_to_hir: FxHashMap<NodeId, HirId>,
}

impl<'hir> AstLowering<'hir> {
    pub fn new(tcx: TyCtxt<'hir>) -> Self {
        Self {
            tcx,
            owner_stack: Vec::new(),
            local_counters: FxHashMap::default(),
            owners: FxHashMap::default(),
            node_to_hir: FxHashMap::default(),
        }
    }

    fn push_owner(&mut self, owner_id: OwnerId) {
        self.owner_stack.push(owner_id);
        self.local_counters.entry(owner_id).or_insert(0);
        self.owners
            .entry(owner_id)
            .or_insert_with(FxHashMap::default);
    }

    fn next_hir_id(&mut self) -> HirId {
        let owner = *self.owner_stack.last().expect("owner为空");
        let counter = self.local_counters.get_mut(&owner).unwrap();
        let id = HirId {
            owner,
            local_id: ItemLocalId::from_raw(*counter),
        };
        *counter += 1;
        id
    }

    fn record_node(&mut self, node: hir::Node<'hir>) -> &'hir hir::Node<'hir> {
        let node_ref = self.tcx.alloc(node);
        let hir_id = node_ref.hir_id();
        let owner_nodes = self.owners.get_mut(&hir_id.owner).unwrap();
        owner_nodes.insert(hir_id.local_id, node_ref);
        node_ref
    }

    fn next_hir_id_for_node(&mut self, node_id: NodeId) -> HirId {
        let hir_id = self.next_hir_id();
        self.node_to_hir.insert(node_id, hir_id);
        hir_id
    }

    #[inline]
    fn pop_owner(&mut self) {
        self.owner_stack.pop();
    }

    pub fn lower_crate(&mut self, crate_: &ast::Crate) -> &'hir hir::Crate<'hir> {
        let items: Vec<_> = crate_
            .items
            .iter()
            .filter(|item| !matches!(&item.kind, ast::ItemKind::Use(..)))
            .map(|item| self.lower_item(item))
            .collect();

        self.tcx.alloc(hir::Crate {
            items: self.tcx.alloc(items),
        })
    }

    fn lower_item(&mut self, item: &ast::Item) -> &'hir hir::Item<'hir> {
        let def_id = self.tcx.def_id_of(item.node_id).unwrap();
        let owner_id = OwnerId(def_id);
        self.push_owner(owner_id);
        let hir_id = self.next_hir_id_for_node(item.node_id);

        let item_kind: hir::ItemKind<'hir> = match &item.kind {
            ast::ItemKind::Fn(fn_) => hir::ItemKind::Fn(self.lower_fn(fn_, item.node_id)),
            ast::ItemKind::Struct(ident, generics, struct_kind) => {
                let generics = self.lower_generics(generics);
                let struct_kind = self.lower_struct_kind(struct_kind);

                hir::ItemKind::Struct(*ident, generics, struct_kind)
            }
            ast::ItemKind::Use(_) => unreachable!("Use should have been filtered"),
            ast::ItemKind::Extern(extern_) => hir::ItemKind::ForeignMod(self.lower_extern(extern_)),
            ast::ItemKind::Module(ident, inline) => {
                hir::ItemKind::Module(*ident, self.lower_module(inline))
            }
            ast::ItemKind::Impl(impl_) => hir::ItemKind::Impl(self.lower_impl(impl_)),
            ast::ItemKind::Trait(ident, generics, items) => {
                let generics = self.lower_generics(generics);
                let items: Vec<_> = items
                    .iter()
                    .map(|item| self.lower_trait_item(item))
                    .collect();

                hir::ItemKind::Trait(*ident, generics, self.tcx.alloc(items))
            }
            ast::ItemKind::TypeAlias(type_alias) => {
                hir::ItemKind::TypeAlias(self.lower_type_alias(type_alias))
            }
            ast::ItemKind::Enum(ident, generics, variants) => {
                let generics = self.lower_generics(generics);
                let variants: Vec<_> = variants
                    .iter()
                    .map(|variant| self.lower_variant(variant))
                    .collect();

                hir::ItemKind::Enum(*ident, generics, self.tcx.alloc(variants))
            }
        };

        let item = self.tcx.alloc(hir::Item {
            hir_id,
            def_id: def_id.to_def_id(),
            visibility: item.visibility,
            kind: item_kind,
            span: item.span,
        });
        let node = hir::Node::Item(item);
        self.record_node(node);

        self.pop_owner();
        item
    }

    fn lower_variant(&mut self, ast_variant: &ast::Variant) -> &'hir hir::Variant<'hir> {
        let hir_id = self.next_hir_id_for_node(ast_variant.node_id);
        let def_id = self.tcx.def_id_of(ast_variant.node_id).unwrap().to_def_id();
        let name = ast_variant.ident;
        let span = ast_variant.span;
        let (data, ctor_def_id) = match &ast_variant.data {
            ast::VariantData::Unit => {
                let ctor = self.tcx.ctor_of(def_id);
                (hir::VariantData::Unit, ctor)
            }
            ast::VariantData::Tuple(tys) => {
                let mut hir_tys = Vec::new();
                for ty in tys {
                    hir_tys.push(self.lower_ty(ty));
                }
                let ty_slice = self.tcx.alloc_slice_copy(&hir_tys);
                let ctor = self.tcx.ctor_of(def_id);
                (hir::VariantData::Tuple(ty_slice), ctor)
            }
            ast::VariantData::Struct(fields) => {
                let mut hir_fields = Vec::new();
                for (idx, field) in fields.iter().enumerate() {
                    let field_hir_id = self.next_hir_id_for_node(field.node_id);
                    let ty = self.lower_ty(&field.ty);
                    let field = self.tcx.alloc(hir::Field {
                        hir_id: field_hir_id,
                        name: field.name,
                        ty,
                        visibility: ast::Visibility::Inherited,
                        index: idx as u32,
                        span: field.span,
                    });
                    self.record_node(hir::Node::Field(field));
                    hir_fields.push(&*field);
                }
                let field_slice = self.tcx.alloc(hir_fields);
                // 结构体变体没有默认构造函数
                (hir::VariantData::Struct(field_slice), None)
            }
        };
        let variant = hir::Variant {
            hir_id,
            def_id,
            name,
            data,
            ctor_def_id,
            span,
        };
        let variant_ref = self.tcx.alloc(variant);
        self.record_node(hir::Node::Variant(variant_ref));
        variant_ref
    }

    fn lower_type_alias(&mut self, type_alias: &ast::TypeAlias) -> &'hir hir::TypeAlias<'hir> {
        let generics = self.lower_generics(&type_alias.generics);
        let ty = self.lower_ty(&type_alias.ty);
        self.tcx.alloc(hir::TypeAlias {
            name: type_alias.name,
            generics,
            ty,
        })
    }

    fn lower_trait_item(&mut self, ast_item: &ast::TraitItem) -> &'hir hir::TraitItem<'hir> {
        match &ast_item.kind {
            ast::TraitItemKind::Fn(fn_) => {
                let sig = self.lower_fn_sig(fn_);
                self.tcx.alloc(hir::TraitItem::Fn(sig))
            }
            ast::TraitItemKind::Type(type_alias) => {
                let type_alias = self.lower_type_alias(type_alias);
                self.tcx.alloc(hir::TraitItem::TypeAlias(type_alias))
            }
        }
    }

    fn lower_impl(&mut self, ast_impl: &ast::Impl) -> &'hir hir::Impl<'hir> {
        let generics = self.lower_generics(&ast_impl.generics);

        let of_trait = ast_impl.of_trait.as_ref().map(|path| self.lower_path(path));

        let self_ty = self.lower_ty(&ast_impl.self_ty);

        let mut items = Vec::new();
        for ast_item in &ast_impl.items {
            let item = self.lower_impl_item(ast_item);
            items.push(item);
        }
        let items_slice = self.tcx.alloc_slice_copy(&items);

        self.tcx.alloc(hir::Impl {
            generics,
            of_trait,
            self_ty,
            items: items_slice,
        })
    }

    fn lower_impl_item(&mut self, ast_item: &ast::ImplItem) -> &'hir hir::ImplItem<'hir> {
        match &ast_item.kind {
            ast::ImplItemKind::Fn(fn_) => {
                let fn_node = self.lower_fn(fn_, ast_item.node_id);
                self.tcx.alloc(hir::ImplItem::Fn(fn_node))
            }
            ast::ImplItemKind::Type(ty) => {
                let generics = self.lower_generics(&ty.generics);
                let ty_node = self.lower_ty(&ty.ty);
                self.tcx
                    .alloc(hir::ImplItem::TypeAlias(ty.name, generics, ty_node))
            }
        }
    }

    fn lower_module(&mut self, inline: &ast::Inline) -> &'hir hir::Mod<'hir> {
        let items = match inline {
            ast::Inline::Inline(items) | ast::Inline::External(items) => {
                let mut ids = Vec::new();
                for ast_item in items {
                    ids.push(self.lower_item(ast_item));
                }
                self.tcx.alloc(ids)
            }
        };
        self.tcx.alloc(hir::Mod { items: items })
    }

    fn lower_foreign_item(&mut self, ast_item: &ast::ExternItem) -> &'hir hir::ForeignItem<'hir> {
        let hir_id = self.next_hir_id_for_node(ast_item.node_id);
        let def_id = self.tcx.def_id_of(ast_item.node_id).unwrap().to_def_id();
        let name = match &ast_item.kind {
            ast::ExternItemKind::Fn(fn_) => fn_.sig.name,
        };
        let vis = ast_item.visibility;
        let span = ast_item.span;
        let kind = match &ast_item.kind {
            ast::ExternItemKind::Fn(fn_) => {
                let sig = self.lower_fn_sig(&fn_.sig);
                hir::ForeignItemKind::Fn(sig)
            }
        };
        let item = hir::ForeignItem {
            hir_id,
            def_id,
            name,
            vis,
            span,
            kind,
        };
        self.tcx.alloc(item)
    }

    fn lower_extern(&mut self, ast_ext: &ast::Extern) -> &'hir hir::ForeignMod<'hir> {
        let mut items = Vec::new();
        for ast_item in &ast_ext.items {
            items.push(self.lower_foreign_item(ast_item));
        }
        let items_slice = self.tcx.alloc_slice_copy(&items);
        self.tcx.alloc(hir::ForeignMod {
            abi: ast_ext.abi,
            items: items_slice,
        })
    }

    fn lower_struct_kind(&mut self, kind: &ast::StructKind) -> &'hir hir::StructKind<'hir> {
        match kind {
            ast::StructKind::Unit => self.tcx.alloc(hir::StructKind::Unit),
            ast::StructKind::Tuple(tys) => {
                let mut hir_tys = Vec::new();
                for ty in tys {
                    hir_tys.push(self.lower_ty(ty));
                }
                let ty_slice = self.tcx.alloc_slice_copy(&hir_tys);
                self.tcx.alloc(hir::StructKind::Tuple(ty_slice))
            }
            ast::StructKind::Struct(fields) => {
                let mut hir_fields = Vec::new();
                for (idx, field) in fields.iter().enumerate() {
                    let hir_id = self.next_hir_id_for_node(field.node_id);
                    let name = field.name;
                    let ty = self.lower_ty(&field.ty);
                    let field = self.tcx.alloc(hir::Field {
                        hir_id,
                        name,
                        ty,
                        visibility: field.visibility,
                        index: idx as u32,
                        span: field.span,
                    });
                    self.record_node(hir::Node::Field(field));
                    hir_fields.push(&*field);
                }
                let field_slice = self.tcx.alloc_slice_copy(&hir_fields);
                self.tcx.alloc(hir::StructKind::Struct(field_slice))
            }
        }
    }

    fn lower_fn(&mut self, fn_: &ast::Fn, node_id: NodeId) -> &'hir hir::Fn<'hir> {
        let def_id = self.tcx.def_id_of(node_id).unwrap();
        let owner_id = OwnerId(def_id);
        self.push_owner(owner_id);
        let sig = self.lower_fn_sig(&fn_.sig);

        let body = self.lower_block(fn_.body.as_ref().unwrap());

        self.pop_owner();

        self.tcx.alloc(hir::Fn { sig: sig, body })
    }

    fn lower_fn_sig(&mut self, sig: &ast::FnSig) -> &'hir hir::FnSig<'hir> {
        let generics = self.lower_generics(&sig.generics);

        let params: Vec<&'hir hir::Param<'hir>> = sig
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();
        let params = self.tcx.alloc_slice_copy(&params);

        let return_ty = match &sig.return_type {
            ast::FnRetTy::Default(span) => {
                let hir_id = self.next_hir_id();
                self.tcx.alloc(hir::Ty {
                    hir_id,
                    span: *span,
                    kind: self.tcx.alloc(hir::TyKind::Unit),
                })
            }
            ast::FnRetTy::Ty(ty) => self.lower_ty(&ty),
        };

        self.tcx.alloc(hir::FnSig {
            name: sig.name,
            generics,
            params,
            return_type: return_ty,
            is_variadic: sig.is_variadic,
        })
    }

    fn lower_generics(&mut self, generics: &ast::Generics) -> &'hir hir::Generics<'hir> {
        let mut hir_generics = Vec::new();
        for generic in &generics.params {
            hir_generics.push(self.lower_generic(generic));
        }

        let params_slice = self.tcx.alloc_slice_copy(&hir_generics);
        self.tcx.alloc(hir::Generics {
            params: params_slice,
            span: generics.span,
        })
    }

    fn lower_generic(&mut self, generic: &ast::Generic) -> &'hir hir::GenericParam<'hir> {
        let hir_id = self.next_hir_id();
        let def_id = self.tcx.def_id_of(generic.node_id).unwrap().to_def_id();
        let name = generic.name;
        let bounds = if let Some(bounds) = &generic.bounds {
            Some(self.lower_bounds(bounds))
        } else {
            None
        };
        let param = hir::GenericParam {
            hir_id,
            def_id,
            name,
            kind: hir::GenericParamKind::Ty,
            bounds,
            span: generic.span,
        };
        let param_ref = self.tcx.alloc(param);
        let node = hir::Node::GenericParam(param_ref);
        self.record_node(node);
        param_ref
    }

    fn lower_bounds(&mut self, ast_bounds: &ast::Bounds) -> &'hir hir::Bounds<'hir> {
        let mut hir_bounds = Vec::new();

        for bound in &ast_bounds.bounds {
            hir_bounds.push(self.lower_path(&bound));
        }

        let hir_bounds = self.tcx.alloc_slice_copy(&hir_bounds);

        self.tcx.alloc(hir::Bounds {
            bounds: hir_bounds,
            span: ast_bounds.span,
        })
    }

    fn lower_param(&mut self, param: &ast::Param) -> &'hir hir::Param<'hir> {
        let hir_id = self.next_hir_id_for_node(param.node_id);
        let span = param.span;
        let param = match &param.kind {
            ast::ParamKind::Normal(ast_pat, ast_ty) => {
                let pat = self.lower_pat(ast_pat);
                let ty = self.lower_ty(ast_ty);
                hir::Param {
                    hir_id,
                    pat,
                    ty,
                    span,
                    is_self: false,
                    self_kind: None,
                }
            }
            ast::ParamKind::SelfValue(_) => {
                // 为 pat 分配独立的 HirId
                let pat_hir_id = self.next_hir_id();
                let ident = Ident::new("self".into(), span);
                let pat = self.tcx.alloc(hir::Pat {
                    hir_id: pat_hir_id,
                    span,
                    kind: self
                        .tcx
                        .alloc(hir::PatKind::Ident(ast::Mutability::Immutable, ident)),
                });
                // 为 ty 分配另一个 HirId
                let ty_hir_id = self.next_hir_id();
                let ty = self.tcx.alloc(hir::Ty {
                    hir_id: ty_hir_id,
                    span,
                    kind: self.tcx.alloc(hir::TyKind::SelfTy),
                });
                hir::Param {
                    hir_id,
                    pat,
                    ty,
                    span,
                    is_self: true,
                    self_kind: Some(hir::SelfKind::Value),
                }
            }
            ast::ParamKind::SelfPtr(mutability) => {
                let pat_hir_id = self.next_hir_id();
                let ident = Ident::new("self".into(), span);
                let pat = self.tcx.alloc(hir::Pat {
                    hir_id: pat_hir_id,
                    span,
                    kind: self
                        .tcx
                        .alloc(hir::PatKind::Ident(ast::Mutability::Immutable, ident)),
                });
                let self_ty_hir_id = self.next_hir_id();
                let self_ty = self.tcx.alloc(hir::Ty {
                    hir_id: self_ty_hir_id,
                    span,
                    kind: self.tcx.alloc(hir::TyKind::SelfTy),
                });
                let ptr_ty_hir_id = self.next_hir_id();
                let ty = self.tcx.alloc(hir::Ty {
                    hir_id: ptr_ty_hir_id,
                    span,
                    kind: self.tcx.alloc(hir::TyKind::Ptr {
                        mutability: *mutability,
                        ty: self_ty,
                    }),
                });
                hir::Param {
                    hir_id,
                    pat,
                    ty,
                    span,
                    is_self: true,
                    self_kind: Some(hir::SelfKind::Pointer),
                }
            }
        };

        let param = self.tcx.alloc(param);

        self.record_node(hir::Node::Param(param));

        param
    }

    fn lower_pat(&mut self, ast_pat: &ast::Pat) -> &'hir hir::Pat<'hir> {
        let kind = match &ast_pat.kind {
            ast::PatKind::Wild => hir::PatKind::Wild,
            ast::PatKind::Ident(mutability, ident) => hir::PatKind::Ident(*mutability, *ident),
            ast::PatKind::Tuple(pats) => {
                let pats: Vec<_> = pats.iter().map(|pat| self.lower_pat(pat)).collect();

                hir::PatKind::Tuple(self.tcx.alloc(pats))
            }
            ast::PatKind::Struct(path, struct_field_pats, bool_) => {
                let path = self.lower_path(path);

                let struct_field_pats: Vec<_> = struct_field_pats
                    .iter()
                    .map(|field| self.lower_struct_field_pat(field))
                    .collect();

                hir::PatKind::Struct(path, self.tcx.alloc(struct_field_pats), *bool_)
            }
            ast::PatKind::Enum(path, pat) => {
                let path = self.lower_path(path);

                let pat = pat.as_ref().map(|pat| self.lower_pat(&pat));

                hir::PatKind::Enum(path, pat)
            }
            ast::PatKind::Lit(lit) => hir::PatKind::Lit(*lit),
            ast::PatKind::Range(left, right, range_limits) => {
                let left = self.lower_expr(left);
                let right = self.lower_expr(right);
                hir::PatKind::Range(left, right, *range_limits)
            }
            ast::PatKind::Or(pats) => {
                let pats: Vec<_> = pats.iter().map(|pat| self.lower_pat(pat)).collect();
                hir::PatKind::Or(self.tcx.alloc(pats))
            }
        };
        let hir_id = self.next_hir_id_for_node(ast_pat.node_id);
        let pat = self.tcx.alloc(hir::Pat {
            hir_id,
            span: ast_pat.span,
            kind: self.tcx.alloc(kind),
        });

        self.record_node(hir::Node::Pat(pat));

        pat
    }

    fn lower_struct_field_pat(
        &mut self,
        field: &ast::StructFieldPat,
    ) -> &'hir hir::StructFieldPat<'hir> {
        let name = field.name;

        let pat = self.lower_pat(&field.pat);
        let span = field.span;
        self.tcx.alloc(hir::StructFieldPat { name, pat, span })
    }

    fn lower_expr(&mut self, expr: &ast::Expr) -> &'hir hir::Expr<'hir> {
        let kind = match &expr.kind {
            ast::ExprKind::Binary(left, op, right) => {
                let left = self.lower_expr(left);
                let right = self.lower_expr(right);
                hir::ExprKind::Binary(left, *op, right)
            }
            ast::ExprKind::Unary(un_op, expr) => {
                let expr = self.lower_expr(expr);
                hir::ExprKind::Unary(*un_op, expr)
            }
            ast::ExprKind::Literal(lit) => hir::ExprKind::Literal(*lit),
            ast::ExprKind::Grouped(expr) => hir::ExprKind::Grouped(self.lower_expr(expr)),
            ast::ExprKind::Assignment(var, value) => {
                let var = self.lower_expr(var);
                let value = self.lower_expr(value);
                hir::ExprKind::Assignment(var, value)
            }
            ast::ExprKind::AssignmentWithOp(var, op, value) => {
                let var = self.lower_expr(var);
                let value = self.lower_expr(value);
                hir::ExprKind::AssignmentWithOp(var, *op, value)
            }
            ast::ExprKind::Call(fn_, args) => {
                let fn_ = self.lower_expr(fn_);
                let args: Vec<_> = args.iter().map(|arg| self.lower_expr(arg)).collect();
                hir::ExprKind::Call(fn_, self.tcx.alloc(args))
            }
            ast::ExprKind::Block(block) => {
                let block = self.lower_block(block);
                hir::ExprKind::Block(block)
            }
            ast::ExprKind::If(condition, block, else_branch) => {
                let condition = self.lower_expr(condition);
                let block = self.lower_block(block);
                let else_branch = else_branch
                    .as_ref()
                    .map(|else_branch| self.lower_expr(else_branch));
                hir::ExprKind::If(condition, block, else_branch)
            }
            ast::ExprKind::While(condition, block) => {
                let condition = self.lower_expr(condition);
                let block = self.lower_block(block);
                hir::ExprKind::While(condition, block)
            }
            ast::ExprKind::For {
                variable,
                iter,
                body,
            } => {
                let variable = self.lower_pat(variable);
                let iter = self.lower_expr(iter);
                let body = self.lower_block(body);

                hir::ExprKind::For {
                    variable: variable,
                    iter,
                    body,
                }
            }
            ast::ExprKind::Index(indexed, index) => {
                let indexed = self.lower_expr(indexed);
                let index = self.lower_expr(index);
                hir::ExprKind::Index(indexed, index)
            }
            ast::ExprKind::Range(start, end, range_limits) => {
                let start = self.lower_expr(start);
                let end = self.lower_expr(end);

                hir::ExprKind::Range(start, end, *range_limits)
            }
            ast::ExprKind::Loop(block) => {
                let block = self.lower_block(block);

                hir::ExprKind::Loop(block)
            }
            ast::ExprKind::Field(expr, ident) => {
                let expr = self.lower_expr(expr);

                hir::ExprKind::Field(expr, *ident)
            }
            ast::ExprKind::Path(path) => {
                let qpath = self.lower_qpath(path);
                hir::ExprKind::QPath(self.tcx.alloc(qpath))
            }
            ast::ExprKind::Bool(bool_) => hir::ExprKind::Bool(*bool_),
            ast::ExprKind::Tuple(exprs) => {
                let exprs: Vec<_> = exprs.iter().map(|expr| self.lower_expr(expr)).collect();

                hir::ExprKind::Tuple(self.tcx.alloc(exprs))
            }
            ast::ExprKind::Unit => hir::ExprKind::Unit,
            ast::ExprKind::AddressOf(expr) => hir::ExprKind::AddressOf(self.lower_expr(expr)),
            ast::ExprKind::StructExpr(struct_expr) => {
                hir::ExprKind::StructExpr(self.lower_struct_expr(struct_expr))
            }
            ast::ExprKind::Cast(expr, ty) => {
                let expr = self.lower_expr(expr);
                let ty = self.lower_ty(ty);

                hir::ExprKind::Cast(expr, ty)
            }
            ast::ExprKind::Match(scrutinee, arms) => {
                let scrutinee = self.lower_expr(scrutinee);
                let mut hir_arms = Vec::new();
                for arm in arms {
                    hir_arms.push(self.lower_arm(arm));
                }
                let arms_slice = self.tcx.alloc_slice_copy(&hir_arms);
                hir::ExprKind::Match(scrutinee, arms_slice)
            }
            ast::ExprKind::Return(expr) => {
                hir::ExprKind::Return(expr.as_ref().map(|expr| self.lower_expr(expr)))
            }
            ast::ExprKind::Continue => hir::ExprKind::Continue,
            ast::ExprKind::Break(expr) => {
                hir::ExprKind::Break(expr.as_ref().map(|expr| self.lower_expr(expr)))
            }
        };
        let hir_id = self.next_hir_id_for_node(expr.node_id);

        let expr = self.tcx.alloc(hir::Expr {
            hir_id,
            span: expr.span,
            kind: self.tcx.alloc(kind),
        });

        self.record_node(hir::Node::Expr(expr));

        expr
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> &'hir hir::Stmt<'hir> {
        let kind = match &stmt.kind {
            ast::StmtKind::Expr(expr) => hir::StmtKind::Expr(self.lower_expr(expr)),
            ast::StmtKind::Semi(expr) => hir::StmtKind::Semi(self.lower_expr(expr)),
            ast::StmtKind::Let(pat, ty, expr) => {
                let pat = self.lower_pat(pat);
                let ty = ty.as_ref().map(|ty| self.lower_ty(ty));
                let value = expr.as_ref().map(|expr| self.lower_expr(expr));

                hir::StmtKind::Let(pat, ty, value)
            }
            ast::StmtKind::Defer(expr) => {
                let expr = self.lower_expr(expr);

                hir::StmtKind::Defer(expr)
            }
        };
        let hir_id = self.next_hir_id_for_node(stmt.node_id);

        let stmt = self.tcx.alloc(hir::Stmt {
            hir_id,
            span: stmt.span,
            kind: self.tcx.alloc(kind),
        });

        self.record_node(hir::Node::Stmt(stmt));

        stmt
    }

    fn lower_arm(&mut self, ast_arm: &ast::Arm) -> &'hir hir::Arm<'hir> {
        let hir_id = self.next_hir_id_for_node(ast_arm.node_id);
        let pat = self.lower_pat(&ast_arm.pat);
        let guard = ast_arm.guard.as_ref().map(|e| self.lower_expr(e));
        let body = self.lower_expr(&ast_arm.body);
        self.tcx.alloc(hir::Arm {
            hir_id,
            pat,
            guard,
            body,
        })
    }

    fn lower_struct_expr(&mut self, ast_expr: &ast::StructExpr) -> &'hir hir::StructExpr<'hir> {
        let path = self.lower_path(&ast_expr.path);
        let mut fields = Vec::new();
        for ast_field in &ast_expr.fields {
            let name = ast_field.name;
            let value = self.lower_expr(&ast_field.value);
            let field = hir::StructExprField {
                name,
                value,
                is_shorthand: ast_field.is_shorthand,
                span: ast_field.span,
            };
            fields.push(&*self.tcx.alloc(field));
        }
        let fields = self.tcx.alloc_slice_copy(&fields);
        self.tcx.alloc(hir::StructExpr { path, fields })
    }

    fn lower_ty(&mut self, ty: &ast::Ty) -> &'hir hir::Ty<'hir> {
        let kind = match &ty.kind {
            ast::TyKind::Path { path } => {
                let qpath = self.lower_qpath(path);
                hir::TyKind::QPath(self.tcx.alloc(qpath))
            }
            ast::TyKind::Never => hir::TyKind::Never,
            ast::TyKind::Unit => hir::TyKind::Unit,
            ast::TyKind::Ptr { mutability, ty } => {
                let ty = self.lower_ty(ty);
                hir::TyKind::Ptr {
                    mutability: *mutability,
                    ty,
                }
            }
            ast::TyKind::Array { elem, len } => {
                let elem = self.lower_ty(elem);
                let len = self.lower_expr(len);

                hir::TyKind::Array { elem, len }
            }
            ast::TyKind::Slice { elem } => {
                let elem = self.lower_ty(elem);

                hir::TyKind::Slice { elem }
            }
            ast::TyKind::Tuple { elems } => {
                let elems: Vec<_> = elems.iter().map(|elem| self.lower_ty(elem)).collect();

                hir::TyKind::Tuple {
                    elems: self.tcx.alloc(elems),
                }
            }
            ast::TyKind::FnPtr { inputs, output } => {
                let inputs: Vec<_> = inputs.iter().map(|input| self.lower_ty(input)).collect();
                let output = self.lower_ty(output);

                hir::TyKind::FnPtr {
                    inputs: self.tcx.alloc(inputs),
                    output,
                }
            }
            ast::TyKind::SelfTy => hir::TyKind::SelfTy,
        };
        let hir_id = self.next_hir_id_for_node(ty.node_id);

        let ty = self.tcx.alloc(hir::Ty {
            hir_id,
            span: ty.span,
            kind: self.tcx.alloc(kind),
        });

        self.record_node(hir::Node::Ty(ty));

        ty
    }

    fn lower_block(&mut self, block: &ast::Block) -> &'hir hir::Block<'hir> {
        let stmts: Vec<_> = block
            .stmts
            .iter()
            .map(|stmt| self.lower_stmt(stmt))
            .collect();
        let hir_id = self.next_hir_id_for_node(block.node_id);

        let block = self.tcx.alloc(hir::Block {
            hir_id,
            stmts: self.tcx.alloc(stmts),
            span: block.span,
        });

        self.record_node(hir::Node::Block(block));

        block
    }

    fn lower_path(&mut self, path: &ast::Path) -> &'hir hir::Path<'hir> {
        match self.lower_qpath(path) {
            hir::QPath::Resolved(p) => p,
            hir::QPath::TypeRelative(..) => self.tcx.alloc(hir::Path {
                res: Res::Err,
                segments: &[],
                span: path.span,
            }),
        }
    }

    fn lower_qpath(&mut self, path: &ast::Path) -> hir::QPath<'hir> {
        if let Some(partial) = self.tcx.partial_resolution(path.node_id).cloned() {
            let base_res = self.lower_res(partial.base);
            let remaining_segments: Vec<_> = partial
                .remaining
                .iter()
                .map(|seg| self.lower_path_segment(seg))
                .collect();
            let remaining_segments = self.tcx.alloc(remaining_segments);
            return hir::QPath::TypeRelative(base_res, remaining_segments);
        }
        let res = self
            .tcx
            .resolution(path.node_id)
            .cloned()
            .expect("missing resolution for path");
        let res_hir = self.lower_res(res);
        let segments: Vec<_> = path
            .segments
            .iter()
            .map(|seg| self.lower_path_segment(seg))
            .collect();
        let segments = self.tcx.alloc(segments);
        let path_node = self.tcx.alloc(hir::Path {
            res: res_hir,
            segments,
            span: path.span,
        });
        hir::QPath::Resolved(path_node)
    }

    fn lower_res(&self, res: Res<NodeId>) -> Res<HirId> {
        match res {
            Res::Def(def_kind, def_id) => Res::Def(def_kind, def_id),
            Res::Local(node_id) => Res::Local(self.node_to_hir[&node_id]),
            Res::PrimTy(prim_ty) => Res::PrimTy(prim_ty),
            Res::SelfTyParam { trait_ } => Res::SelfTyParam { trait_ },
            Res::SelfTyAlias { alias_to } => Res::SelfTyAlias { alias_to },
            Res::SelfCtor(def_id) => Res::SelfCtor(def_id),
            Res::BuiltinTrait(builtin_trait) => Res::BuiltinTrait(builtin_trait),
            Res::Err => Res::Err,
        }
    }

    fn lower_path_segment(
        &mut self,
        path_segment: &ast::PathSegment,
    ) -> &'hir hir::PathSegment<'hir> {
        let generic_args = path_segment
            .generic_args
            .as_ref()
            .map(|args| self.lower_generic_args(args));

        self.tcx.alloc(hir::PathSegment {
            ident: path_segment.name,
            generic_args,
            span: path_segment.span,
        })
    }

    fn lower_generic_args(
        &mut self,
        generic_args: &ast::GenericArgs,
    ) -> &'hir hir::GenericArgs<'hir> {
        let mut args = Vec::new();

        for arg in &generic_args.args {
            args.push(self.lower_generic_arg(arg));
        }

        let args = self.tcx.alloc_slice_copy(&args);

        self.tcx.alloc(hir::GenericArgs {
            args,
            span: generic_args.span,
        })
    }

    fn lower_generic_arg(&mut self, generic_arg: &ast::GenericArg) -> &'hir hir::GenericArg<'hir> {
        match generic_arg {
            ast::GenericArg::Type(ty) => {
                let ty = self.lower_ty(ty);
                self.tcx.alloc(hir::GenericArg::Type(ty))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use litec_ast::ast::{BinOp, BinOpKind};
    use litec_hir::hir::*;
    use litec_middle::context::GlobalCtxt;
    use litec_parse::{node_collector::NodeCollector, parser::Parser};
    use litec_resolve::{Resolver, def_collector::DefCollector};
    use litec_session::Session;
    use litec_span::SourceMap;
    use std::path::Path;

    /// 辅助函数：解析、名称解析、lowering，然后在闭包中验证 HIR
    fn with_lowered<F>(src: &str, f: F)
    where
        F: FnOnce(&TyCtxt, &Crate),
    {
        let source_map = SourceMap::new();
        let session = Session::new(source_map);
        let file = session.mut_source_map().add_file(
            "test".into(),
            src.to_string(),
            &Path::new("test.lt"),
        );
        let parser = Parser::new(&session, file);
        let mut krate = parser.parse();
        let mut node_collector = NodeCollector::new();
        node_collector.collect(&mut krate);

        let bump = Bump::new();
        let mut gcx = GlobalCtxt::new(&session, &bump);
        let def_collector = DefCollector::new(&mut gcx);
        def_collector.collect(&krate);

        let mut resolver = Resolver::new(gcx.ty_ctxt());
        resolver.resolve_crate(&krate);
        let resolve_output = resolver.take_output();
        gcx.set_resolve_output(resolve_output);
        let tcx = gcx.ty_ctxt();

        let mut lowerer = AstLowering::new(tcx);
        let hir_crate = lowerer.lower_crate(&krate);
        f(&tcx, hir_crate);
    }

    #[test]
    fn test_lower_fn() {
        let src = r#"
            fn add(a: i32, b: i32) -> i32 {
                a + b
            }
        "#;
        with_lowered(src, |_tcx, hir_crate| {
            assert_eq!(hir_crate.items.len(), 1);
            let item = hir_crate.items[0];
            assert!(matches!(item.kind, ItemKind::Fn(_)));
            if let ItemKind::Fn(fn_node) = &item.kind {
                // 检查函数签名（注意：Ident 的 span 无法直接比较，只检查名称）
                assert_eq!(fn_node.sig.name.to_string(), "add");
                assert_eq!(fn_node.sig.params.len(), 2);
                // 检查返回类型（简化：只检查是否为 Path 类型）
                match fn_node.sig.return_type.kind {
                    TyKind::QPath(qpath) => match qpath {
                        QPath::Resolved(path) => match path.res {
                            Res::PrimTy(PrimTy::Int(IntTy::I32)) => {}
                            _ => panic!("type should be i32"),
                        },
                        QPath::TypeRelative(..) => {
                            panic!("qpath should be resolved")
                        }
                    },
                    _ => panic!("return type should be QPath"),
                }
                // 检查函数体
                let body = fn_node.body;
                assert_eq!(body.stmts.len(), 1);
                let tail = body
                    .stmts
                    .last()
                    .expect("tail expression should be present");
                match tail.kind {
                    StmtKind::Expr(Expr {
                        kind: ExprKind::Binary(_, BinOp { value, .. }, _),
                        ..
                    }) => {
                        assert!(*value == BinOpKind::Add, "binary op code should be add");
                    }
                    _ => panic!("expected binary expression"),
                }
            } else {
                panic!("expected function item");
            }
        });
    }

    #[test]
    fn test_lower_struct() {
        let src = r#"
            struct Point {
                x: i32,
                y: i32,
            }
            struct Unit;
            struct Tuple(i32, i32);
        "#;
        with_lowered(src, |_tcx, hir_crate| {
            assert_eq!(hir_crate.items.len(), 3);
            // Point
            let point_item = &hir_crate.items[0];
            if let ItemKind::Struct(ident, generics, struct_kind) = &point_item.kind {
                assert_eq!(ident.to_string(), "Point");
                assert_eq!(generics.params.len(), 0);
                match struct_kind {
                    StructKind::Struct(fields) => {
                        assert_eq!(fields.len(), 2);
                        let field0 = &fields[0];
                        assert_eq!(field0.name.to_string(), "x");
                        // 字段类型为 i32，应表现为 Path 或 QPath
                        match field0.ty.kind {
                            TyKind::QPath(_) => {}
                            _ => panic!("field type should be QPath"),
                        }
                    }
                    _ => panic!("expected struct with named fields"),
                }
            } else {
                panic!("expected struct item");
            }
            // Unit
            let unit_item = &hir_crate.items[1];
            if let ItemKind::Struct(ident, _generics, struct_kind) = &unit_item.kind {
                assert_eq!(ident.to_string(), "Unit");
                assert!(matches!(struct_kind, StructKind::Unit));
            }
            // Tuple
            let tuple_item = &hir_crate.items[2];
            if let ItemKind::Struct(ident, _generics, struct_kind) = &tuple_item.kind {
                assert_eq!(ident.to_string(), "Tuple");
                match struct_kind {
                    StructKind::Tuple(tys) => {
                        assert_eq!(tys.len(), 2);
                        match tys[0].kind {
                            TyKind::QPath(_) => {}
                            _ => panic!("tuple field type should be QPath"),
                        }
                    }
                    _ => panic!("expected tuple struct"),
                }
            }
        });
    }
}
