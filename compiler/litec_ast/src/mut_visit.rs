use crate::ast::*;

pub trait MutVisitor {
    fn visit_crate(&mut self, krate: &mut Crate) {
        walk_mut_crate(self, krate);
    }

    fn visit_item(&mut self, item: &mut Item) {
        walk_mut_item(self, item);
    }

    fn visit_extern_item(&mut self, item: &mut ExternItem) {
        walk_mut_extern_item(self, item);
    }

    fn visit_fn(&mut self, func: &mut Fn) {
        walk_mut_fn(self, func);
    }

    fn visit_extern(&mut self, ext: &mut Extern) {
        walk_mut_extern(self, ext);
    }

    fn visit_use_tree(&mut self, use_tree: &mut UseTree) {
        walk_mut_use_tree(self, use_tree);
    }

    fn visit_param(&mut self, param: &mut Param) {
        walk_mut_param(self, param);
    }

    fn visit_block(&mut self, block: &mut Block) {
        walk_mut_block(self, block);
    }

    fn visit_field(&mut self, field: &mut Field) {
        walk_mut_field(self, field);
    }

    fn visit_expr(&mut self, expr: &mut Expr) {
        walk_mut_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        walk_mut_stmt(self, stmt);
    }

    fn visit_ty(&mut self, ty: &mut Ty) {
        walk_mut_ty(self, ty);
    }

    fn visit_path(&mut self, path: &mut Path) {
        walk_mut_path(self, path);
    }

    fn visit_path_segment(&mut self, segment: &mut PathSegment) {
        walk_mut_path_segment(self, segment);
    }

    fn visit_generic_params(&mut self, generics: &mut Generics) {
        walk_mut_generic_params(self, generics);
    }

    fn visit_generic_param(&mut self, param: &mut Generic) {
        walk_mut_generic_param(self, param);
    }

    fn visit_struct_expr(&mut self, struct_expr: &mut StructExpr) {
        walk_mut_struct_expr(self, struct_expr);
    }

    fn visit_struct_expr_field(&mut self, field: &mut StructExprField) {
        walk_mut_struct_expr_field(self, field);
    }

    fn visit_impl(&mut self, impl_: &mut Impl) {
        walk_mut_impl(self, impl_);
    }

    fn visit_impl_item(&mut self, impl_item: &mut ImplItem) {
        walk_mut_impl_item(self, impl_item);
    }

    fn visit_type_alias(&mut self, type_alias: &mut TypeAlias) {
        walk_mut_type_alias(self, type_alias);
    }

    fn visit_trait_item(&mut self, trait_item: &mut TraitItem) {
        walk_mut_trait_item(self, trait_item);
    }

    fn visit_bounds(&mut self, bounds: &mut Bounds) {
        walk_mut_bounds(self, bounds);
    }

    fn visit_arm(&mut self, arm: &mut Arm) {
        walk_mut_arm(self, arm);
    }

    fn visit_pat(&mut self, pat: &mut Pat) {
        walk_mut_pat(self, pat);
    }

    fn visit_struct_field_pat(&mut self, struct_field_pat: &mut StructFieldPat) {
        walk_mut_struct_field_pat(self, struct_field_pat);
    }

    fn visit_variant(&mut self, variant: &mut Variant) {
        walk_mut_variant(self, variant)
    }

    fn visit_variant_field(&mut self, variant_field: &mut VariantField) {
        walk_mut_variant_field(self, variant_field);
    }

    fn visit_struct_kind(&mut self, struct_kind: &mut StructKind) {
        walk_struct_kind(self, struct_kind);
    }

    fn visit_fn_sig(&mut self, fn_sig: &mut FnSig) {
        walk_mut_fn_sig(self, fn_sig);
    }
}

pub fn walk_mut_crate<V: MutVisitor + ?Sized>(visitor: &mut V, krate: &mut Crate) {
    for item in &mut krate.items {
        visitor.visit_item(item);
    }
}

pub fn walk_mut_item<V: MutVisitor + ?Sized>(visitor: &mut V, item: &mut Item) {
    match &mut item.kind {
        ItemKind::Fn(func) => visitor.visit_fn(func),
        ItemKind::Struct(_ident, generics, struct_kind) => {
            if !generics.params.is_empty() {
                visitor.visit_generic_params(generics);
            }
            visitor.visit_struct_kind(struct_kind);
        }
        ItemKind::Use(use_tree) => visitor.visit_use_tree(use_tree),
        ItemKind::Extern(ext) => visitor.visit_extern(ext),
        ItemKind::Module(_ident, inline) => match inline {
            Inline::External(items) | Inline::Inline(items) => {
                for item in items {
                    visitor.visit_item(item);
                }
            }
        },
        ItemKind::Impl(impl_) => visitor.visit_impl(impl_),
        ItemKind::TypeAlias(type_alias) => visitor.visit_type_alias(type_alias),
        ItemKind::Trait(_ident, generics, items) => {
            visitor.visit_generic_params(generics);
            for item in items {
                visitor.visit_trait_item(item);
            }
        }
        ItemKind::Enum(_, generic_params, variants) => {
            visitor.visit_generic_params(generic_params);
            for variant in variants {
                visitor.visit_variant(variant);
            }
        }
    }
}

pub fn walk_mut_extern_item<V: MutVisitor + ?Sized>(visitor: &mut V, item: &mut ExternItem) {
    match &mut item.kind {
        ExternItemKind::Fn(func) => visitor.visit_fn(func),
    }
}

pub fn walk_mut_fn<V: MutVisitor + ?Sized>(visitor: &mut V, fn_: &mut Fn) {
    visitor.visit_fn_sig(&mut fn_.sig);
    if let Some(body) = &mut fn_.body {
        visitor.visit_block(body);
    }
}

pub fn walk_mut_extern<V: MutVisitor + ?Sized>(visitor: &mut V, ext: &mut Extern) {
    for item in &mut ext.items {
        visitor.visit_extern_item(item);
    }
}

pub fn walk_mut_use_tree<V: MutVisitor + ?Sized>(visitor: &mut V, use_tree: &mut UseTree) {
    visitor.visit_path(&mut use_tree.prefix);
    match &mut use_tree.kind {
        UseTreeKind::Simple(_) => {}
        UseTreeKind::Nested(trees, _) => {
            for tree in trees {
                visitor.visit_use_tree(tree);
            }
        }
        UseTreeKind::Glob => {}
    }
}

pub fn walk_mut_param<V: MutVisitor + ?Sized>(visitor: &mut V, param: &mut Param) {
    match &mut param.kind {
        ParamKind::Normal(pat, ty) => {
            visitor.visit_pat(pat);
            visitor.visit_ty(ty);
        }
        ParamKind::SelfPtr(_) => {}
        ParamKind::SelfValue(_) => {}
    }
}

pub fn walk_mut_block<V: MutVisitor + ?Sized>(visitor: &mut V, block: &mut Block) {
    for stmt in &mut block.stmts {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_mut_field<V: MutVisitor + ?Sized>(visitor: &mut V, field: &mut Field) {
    visitor.visit_ty(&mut field.ty);
}

pub fn walk_mut_expr<V: MutVisitor + ?Sized>(visitor: &mut V, expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::Binary(l, _, r) => {
            visitor.visit_expr(l);
            visitor.visit_expr(r);
        }
        ExprKind::Unary(_, e) => visitor.visit_expr(e),
        ExprKind::Literal(_) => {}
        ExprKind::Grouped(e) => visitor.visit_expr(e),
        ExprKind::Assignment(l, r) => {
            visitor.visit_expr(l);
            visitor.visit_expr(r);
        }
        ExprKind::AssignmentWithOp(l, _, r) => {
            visitor.visit_expr(l);
            visitor.visit_expr(r);
        }
        ExprKind::Call(callee, args) => {
            visitor.visit_expr(callee);
            for arg in args {
                visitor.visit_expr(arg);
            }
        }
        ExprKind::Block(b) => visitor.visit_block(b),
        ExprKind::If(cond, then, else_opt) => {
            visitor.visit_expr(cond);
            visitor.visit_block(then);
            if let Some(else_expr) = else_opt {
                visitor.visit_expr(else_expr);
            }
        }
        ExprKind::While(cond, body) => {
            visitor.visit_expr(cond);
            visitor.visit_block(body);
        }
        ExprKind::For {
            variable,
            iter,
            body,
        } => {
            visitor.visit_pat(variable);
            visitor.visit_expr(iter);
            visitor.visit_block(body);
        }
        ExprKind::Index(base, index) => {
            visitor.visit_expr(base);
            visitor.visit_expr(index);
        }
        ExprKind::Range(start, end, _) => {
            visitor.visit_expr(start);
            visitor.visit_expr(end);
        }
        ExprKind::Loop(body) => visitor.visit_block(body),
        ExprKind::Field(e, _) => visitor.visit_expr(e),
        ExprKind::Path(p) => visitor.visit_path(p),
        ExprKind::Bool(_) => {}
        ExprKind::Tuple(elems) => {
            for elem in elems {
                visitor.visit_expr(elem);
            }
        }
        ExprKind::Unit => {}
        ExprKind::AddressOf(e) => visitor.visit_expr(e),
        ExprKind::StructExpr(s) => visitor.visit_struct_expr(s),
        ExprKind::Cast(e, ty) => {
            visitor.visit_expr(e);
            visitor.visit_ty(ty);
        }
        ExprKind::Match(expr, arms) => {
            visitor.visit_expr(expr);

            for arm in arms {
                visitor.visit_arm(arm);
            }
        }
        ExprKind::Return(expr) => {
            if let Some(expr) = expr {
                visitor.visit_expr(expr);
            }
        }
        ExprKind::Continue => {}
        ExprKind::Break(expr) => {
            if let Some(expr) = expr {
                visitor.visit_expr(expr);
            }
        }
    }
}

pub fn walk_mut_stmt<V: MutVisitor + ?Sized>(visitor: &mut V, stmt: &mut Stmt) {
    match &mut stmt.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => visitor.visit_expr(e),
        StmtKind::Let(pat, ty, init) => {
            visitor.visit_pat(pat);
            if let Some(ty) = ty {
                visitor.visit_ty(ty);
            }
            if let Some(init) = init {
                visitor.visit_expr(init);
            }
        }
        StmtKind::Defer(expr) => visitor.visit_expr(expr),
    }
}

pub fn walk_mut_ty<V: MutVisitor + ?Sized>(visitor: &mut V, ty: &mut Ty) {
    match &mut ty.kind {
        TyKind::Path { path } => visitor.visit_path(path),
        TyKind::Never | TyKind::Unit => {}
        TyKind::Ptr { ty: inner, .. } => visitor.visit_ty(inner),
        TyKind::Array { elem, len } => {
            visitor.visit_ty(elem);
            visitor.visit_expr(len);
        }
        TyKind::Slice { elem } => visitor.visit_ty(elem),
        TyKind::Tuple { elems } => {
            for elem in elems {
                visitor.visit_ty(elem);
            }
        }
        TyKind::FnPtr { inputs, output } => {
            for input in inputs {
                visitor.visit_ty(input);
            }
            visitor.visit_ty(output);
        }
        TyKind::SelfTy => {}
    }
}

pub fn walk_mut_path<V: MutVisitor + ?Sized>(visitor: &mut V, path: &mut Path) {
    for seg in &mut path.segments {
        visitor.visit_path_segment(seg);
    }
}

pub fn walk_mut_path_segment<V: MutVisitor + ?Sized>(visitor: &mut V, segment: &mut PathSegment) {
    if let Some(generic_args) = &mut segment.generic_args {
        for arg in &mut generic_args.args {
            match arg {
                GenericArg::Type(ty) => visitor.visit_ty(ty),
            }
        }
    }
}

pub fn walk_mut_generic_params<V: MutVisitor + ?Sized>(visitor: &mut V, generics: &mut Generics) {
    for param in &mut generics.params {
        visitor.visit_generic_param(param);
    }
}

pub fn walk_mut_generic_param<V: MutVisitor + ?Sized>(visitor: &mut V, param: &mut Generic) {
    if let Some(bounds) = &mut param.bounds {
        visitor.visit_bounds(bounds);
    }
}

pub fn walk_mut_struct_expr<V: MutVisitor + ?Sized>(visitor: &mut V, struct_expr: &mut StructExpr) {
    visitor.visit_path(&mut struct_expr.path);
    for field in &mut struct_expr.fields {
        visitor.visit_struct_expr_field(field);
    }
}

pub fn walk_mut_struct_expr_field<V: MutVisitor + ?Sized>(
    visitor: &mut V,
    field: &mut StructExprField,
) {
    visitor.visit_expr(&mut field.value);
}

pub fn walk_mut_impl<V: MutVisitor + ?Sized>(visitor: &mut V, impl_: &mut Impl) {
    visitor.visit_generic_params(&mut impl_.generics);
    if let Some(trait_) = &mut impl_.of_trait {
        visitor.visit_path(trait_);
    }
    visitor.visit_ty(&mut impl_.self_ty);
    for impl_item in &mut impl_.items {
        visitor.visit_impl_item(impl_item);
    }
}

pub fn walk_mut_impl_item<V: MutVisitor + ?Sized>(visitor: &mut V, impl_item: &mut ImplItem) {
    match &mut impl_item.kind {
        ImplItemKind::Fn(fn_) => visitor.visit_fn(fn_),
        ImplItemKind::Type(type_alias) => visitor.visit_type_alias(type_alias),
    }
}

pub fn walk_mut_type_alias<V: MutVisitor + ?Sized>(visitor: &mut V, type_alias: &mut TypeAlias) {
    visitor.visit_generic_params(&mut type_alias.generics);
    visitor.visit_ty(&mut type_alias.ty);
}

pub fn walk_mut_trait_item<V: MutVisitor + ?Sized>(visitor: &mut V, trait_item: &mut TraitItem) {
    match &mut trait_item.kind {
        TraitItemKind::Fn(fn_) => visitor.visit_fn_sig(fn_),
        TraitItemKind::Type(ty_alias) => visitor.visit_type_alias(ty_alias),
    }
}

pub fn walk_mut_bounds<V: MutVisitor + ?Sized>(visitor: &mut V, bounds: &mut Bounds) {
    for bound in &mut bounds.bounds {
        visitor.visit_path(bound);
    }
}

pub fn walk_mut_arm<V: MutVisitor + ?Sized>(visitor: &mut V, arm: &mut Arm) {
    visitor.visit_expr(&mut arm.body);
}

pub fn walk_mut_pat<V: MutVisitor + ?Sized>(visitor: &mut V, pat: &mut Pat) {
    match &mut pat.kind {
        PatKind::Enum(path, pat) => {
            visitor.visit_path(path);
            if let Some(pat) = pat {
                visitor.visit_pat(pat);
            }
        }
        PatKind::Ident(_, _) => {}
        PatKind::Wild => {}
        PatKind::Tuple(pats) => {
            for pat in pats {
                visitor.visit_pat(pat);
            }
        }
        PatKind::Struct(path, struct_field_pats, _) => {
            visitor.visit_path(path);
            for struct_field_pat in struct_field_pats {
                visitor.visit_struct_field_pat(struct_field_pat);
            }
        }
        PatKind::Lit(_) => {}
        PatKind::Range(start, end, _) => {
            visitor.visit_expr(start);
            visitor.visit_expr(end);
        }
        PatKind::Or(pats) => {
            for pat in pats {
                visitor.visit_pat(pat);
            }
        }
    }
}

pub fn walk_mut_struct_field_pat<V: MutVisitor + ?Sized>(
    visitor: &mut V,
    struct_field_pat: &mut StructFieldPat,
) {
    visitor.visit_pat(&mut struct_field_pat.pat);
}

pub fn walk_mut_variant<V: MutVisitor + ?Sized>(visitor: &mut V, variant: &mut Variant) {
    match &mut variant.data {
        VariantData::Struct(fields) => {
            for field in fields {
                visitor.visit_variant_field(field);
            }
        }
        VariantData::Tuple(tys) => {
            for ty in tys {
                visitor.visit_ty(ty);
            }
        }
        VariantData::Unit => {}
    }
}

pub fn walk_mut_variant_field<V: MutVisitor + ?Sized>(
    visitor: &mut V,
    variant_field: &mut VariantField,
) {
    visitor.visit_ty(&mut variant_field.ty);
}

pub fn walk_struct_kind<V: MutVisitor + ?Sized>(visitor: &mut V, struct_kind: &mut StructKind) {
    match struct_kind {
        StructKind::Unit => {}
        StructKind::Tuple(items) => {
            for item in items {
                visitor.visit_ty(item);
            }
        }
        StructKind::Struct(fields) => {
            for field in fields {
                visitor.visit_field(field);
            }
        }
    }
}

pub fn walk_mut_fn_sig<V: MutVisitor + ?Sized>(visitor: &mut V, fn_sig: &mut FnSig) {
    for param in &mut fn_sig.params {
        visitor.visit_param(param);
    }
    match &mut fn_sig.return_type {
        FnRetTy::Default(_) => {}
        FnRetTy::Ty(ty) => visitor.visit_ty(ty),
    }
}
