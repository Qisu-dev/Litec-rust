use crate::ast::*;

pub trait Visitor {
    fn visit_crate(&mut self, krate: &Crate) {
        walk_crate(self, krate);
    }

    fn visit_item(&mut self, item: &Item) {
        walk_item(self, item);
    }

    fn visit_extern_item(&mut self, item: &ExternItem) {
        walk_extern_item(self, item);
    }

    fn visit_fn(&mut self, fn_: &Fn) {
        walk_fn(self, fn_);
    }

    fn visit_extern(&mut self, ext: &Extern) {
        walk_extern(self, ext);
    }

    fn visit_use_tree(&mut self, use_tree: &UseTree) {
        walk_use_tree(self, use_tree);
    }

    fn visit_param(&mut self, param: &Param) {
        walk_param(self, param);
    }

    fn visit_block(&mut self, block: &Block) {
        walk_block(self, block);
    }

    fn visit_field(&mut self, field: &Field) {
        walk_field(self, field);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_ty(&mut self, ty: &Ty) {
        walk_ty(self, ty);
    }

    fn visit_path(&mut self, path: &Path) {
        walk_path(self, path);
    }

    fn visit_path_segment(&mut self, segment: &PathSegment) {
        walk_path_segment(self, segment);
    }

    fn visit_generics(&mut self, generic_params: &Generics) {
        walk_generic_params(self, generic_params);
    }

    fn visit_generic_param(&mut self, param: &Generic) {
        walk_generic_param(self, param);
    }

    fn visit_struct_expr(&mut self, struct_expr: &StructExpr) {
        walk_struct_expr(self, struct_expr);
    }

    fn visit_struct_expr_field(&mut self, field: &StructExprField) {
        walk_struct_expr_field(self, field);
    }

    fn visit_impl(&mut self, impl_: &Impl) {
        walk_impl(self, impl_);
    }

    fn visit_impl_item(&mut self, impl_item: &ImplItem) {
        walk_impl_item(self, impl_item);
    }

    fn visit_type_alias(&mut self, type_alias: &TypeAlias) {
        walk_type_alias(self, type_alias);
    }

    fn visit_trait_item(&mut self, trait_item: &TraitItem) {
        walk_trait_item(self, trait_item);
    }

    fn visit_bounds(&mut self, bounds: &Bounds) {
        walk_bounds(self, bounds);
    }

    fn visit_arm(&mut self, arm: &Arm) {
        walk_arm(self, arm);
    }

    fn visit_pat(&mut self, pat: &Pat) {
        walk_pat(self, pat);
    }

    fn visit_struct_field_pat(&mut self, struct_field_pat: &StructFieldPat) {
        walk_struct_field_pat(self, struct_field_pat);
    }

    fn visit_variant(&mut self, variant: &Variant) {
        walk_variant(self, variant);
    }

    fn visit_variant_field(&mut self, variant_field: &VariantField) {
        walk_variant_field(self, variant_field);
    }

    fn visit_struct_kind(&mut self, struct_kind: &StructKind) {
        walk_struct_kind(self, struct_kind);
    }

    fn visit_fn_sig(&mut self, fn_sig: &FnSig) {
        walk_fn_sig(self, fn_sig);
    }
}

pub fn walk_crate<V: Visitor + ?Sized>(visitor: &mut V, krate: &Crate) {
    for item in &krate.items {
        visitor.visit_item(item);
    }
}

pub fn walk_item<V: Visitor + ?Sized>(visitor: &mut V, item: &Item) {
    match &item.kind {
        ItemKind::Fn(func) => visitor.visit_fn(func),
        ItemKind::Struct(_ident, generics, struct_kind) => {
            if !generics.params.is_empty() {
                visitor.visit_generics(generics);
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
        ItemKind::Impl(impl_) => visitor.visit_impl(&impl_),
        ItemKind::TypeAlias(type_alias) => visitor.visit_type_alias(type_alias),
        ItemKind::Trait(_ident, generics, items) => {
            visitor.visit_generics(generics);
            for item in items {
                visitor.visit_trait_item(item);
            }
        }
        ItemKind::Enum(_, generic_params, variants) => {
            visitor.visit_generics(generic_params);
            for variant in variants {
                visitor.visit_variant(variant);
            }
        }
    }
}

pub fn walk_extern_item<V: Visitor + ?Sized>(visitor: &mut V, item: &ExternItem) {
    match &item.kind {
        ExternItemKind::Fn(func) => visitor.visit_fn(func),
    }
}

pub fn walk_fn<V: Visitor + ?Sized>(visitor: &mut V, fn_: &Fn) {
    visitor.visit_fn_sig(&fn_.sig);
    if let Some(body) = &fn_.body {
        visitor.visit_block(body);
    }
}

pub fn walk_extern<V: Visitor + ?Sized>(visitor: &mut V, ext: &Extern) {
    for item in &ext.items {
        visitor.visit_extern_item(item);
    }
}

pub fn walk_use_tree<V: Visitor + ?Sized>(visitor: &mut V, use_tree: &UseTree) {
    visitor.visit_path(&use_tree.prefix);
    match &use_tree.kind {
        UseTreeKind::Simple(_) => {}
        UseTreeKind::Nested(trees, _) => {
            for tree in trees {
                visitor.visit_use_tree(tree);
            }
        }
        UseTreeKind::Glob => {}
    }
}

pub fn walk_param<V: Visitor + ?Sized>(visitor: &mut V, param: &Param) {
    match &param.kind {
        ParamKind::Normal(pat, ty) => {
            visitor.visit_pat(pat);
            visitor.visit_ty(ty);
        }
        ParamKind::SelfPtr(_) => {}
        ParamKind::SelfValue(_) => {}
    }
}

pub fn walk_block<V: Visitor + ?Sized>(visitor: &mut V, block: &Block) {
    for stmt in &block.stmts {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_field<V: Visitor + ?Sized>(visitor: &mut V, field: &Field) {
    visitor.visit_ty(&field.ty);
}

pub fn walk_expr<V: Visitor + ?Sized>(visitor: &mut V, expr: &Expr) {
    match &expr.kind {
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

pub fn walk_stmt<V: Visitor + ?Sized>(visitor: &mut V, stmt: &Stmt) {
    match &stmt.kind {
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

pub fn walk_ty<V: Visitor + ?Sized>(visitor: &mut V, ty: &Ty) {
    match &ty.kind {
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

pub fn walk_path<V: Visitor + ?Sized>(visitor: &mut V, path: &Path) {
    for seg in &path.segments {
        visitor.visit_path_segment(seg);
    }
}

pub fn walk_path_segment<V: Visitor + ?Sized>(visitor: &mut V, segment: &PathSegment) {
    if let Some(generic_args) = &segment.generic_args {
        for arg in &generic_args.args {
            match arg {
                GenericArg::Type(ty) => visitor.visit_ty(ty),
            }
        }
    }
}

pub fn walk_generic_params<V: Visitor + ?Sized>(visitor: &mut V, generics: &Generics) {
    for param in &generics.params {
        visitor.visit_generic_param(param);
    }
}

pub fn walk_generic_param<V: Visitor + ?Sized>(visitor: &mut V, param: &Generic) {
    if let Some(bounds) = &param.bounds {
        visitor.visit_bounds(bounds);
    }
}

pub fn walk_struct_expr<V: Visitor + ?Sized>(visitor: &mut V, struct_expr: &StructExpr) {
    visitor.visit_path(&struct_expr.path);
    for field in &struct_expr.fields {
        visitor.visit_struct_expr_field(field);
    }
}

pub fn walk_struct_expr_field<V: Visitor + ?Sized>(visitor: &mut V, field: &StructExprField) {
    visitor.visit_expr(&field.value);
}

pub fn walk_impl<V: Visitor + ?Sized>(visitor: &mut V, impl_: &Impl) {
    visitor.visit_generics(&impl_.generics);
    if let Some(trait_) = &impl_.of_trait {
        visitor.visit_path(trait_);
    }
    visitor.visit_ty(&impl_.self_ty);
    for impl_item in &impl_.items {
        visitor.visit_impl_item(impl_item);
    }
}

pub fn walk_impl_item<V: Visitor + ?Sized>(visitor: &mut V, impl_item: &ImplItem) {
    match &impl_item.kind {
        ImplItemKind::Fn(fn_) => visitor.visit_fn(fn_),
        ImplItemKind::Type(type_alias) => visitor.visit_type_alias(type_alias),
    }
}

pub fn walk_type_alias<V: Visitor + ?Sized>(visitor: &mut V, type_alias: &TypeAlias) {
    visitor.visit_generics(&type_alias.generics);
    visitor.visit_ty(&type_alias.ty);
}

pub fn walk_trait_item<V: Visitor + ?Sized>(visitor: &mut V, trait_item: &TraitItem) {
    match &trait_item.kind {
        TraitItemKind::Fn(fn_) => visitor.visit_fn_sig(fn_),
        TraitItemKind::Type(ty_alias) => visitor.visit_type_alias(ty_alias),
    }
}

pub fn walk_bounds<V: Visitor + ?Sized>(visitor: &mut V, bounds: &Bounds) {
    for bound in &bounds.bounds {
        visitor.visit_path(bound);
    }
}

pub fn walk_arm<V: Visitor + ?Sized>(visitor: &mut V, arm: &Arm) {
    visitor.visit_expr(&arm.body);
}

pub fn walk_pat<V: Visitor + ?Sized>(visitor: &mut V, pat: &Pat) {
    match &pat.kind {
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

pub fn walk_struct_field_pat<V: Visitor + ?Sized>(
    visitor: &mut V,
    struct_field_pat: &StructFieldPat,
) {
    visitor.visit_pat(&struct_field_pat.pat);
}

pub fn walk_variant<V: Visitor + ?Sized>(visitor: &mut V, variant: &Variant) {
    match &variant.data {
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

pub fn walk_variant_field<V: Visitor + ?Sized>(visitor: &mut V, variant_field: &VariantField) {
    visitor.visit_ty(&variant_field.ty);
}

pub fn walk_struct_kind<V: Visitor + ?Sized>(visitor: &mut V, struct_kind: &StructKind) {
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

pub fn walk_fn_sig<V: Visitor + ?Sized>(visitor: &mut V, fn_sig: &FnSig) {
    for param in &fn_sig.params {
        visitor.visit_param(param);
    }
    match &fn_sig.return_type {
        FnRetTy::Default(_) => {}
        FnRetTy::Ty(ty) => visitor.visit_ty(ty),
    }
}
