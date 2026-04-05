use crate::{ast::*, visit};

/// 用于遍历（只读访问）AST 的 Visitor trait。
pub trait Visitor {
    // 根
    fn visit_crate(&mut self, krate: &Crate) {
        walk_crate(self, krate);
    }

    // 项（Item<ItemKind>）
    fn visit_item(&mut self, item: &Item) {
        walk_item(self, item);
    }

    // 外部项（ExternItem）
    fn visit_extern_item(&mut self, item: &ExternItem) {
        walk_extern_item(self, item);
    }

    // 函数
    fn visit_fn(&mut self, fn_: &Fn) {
        walk_fn(self, fn_);
    }

    // 外部块
    fn visit_extern(&mut self, ext: &Extern) {
        walk_extern(self, ext);
    }

    // use 树
    fn visit_use_tree(&mut self, use_tree: &UseTree) {
        walk_use_tree(self, use_tree);
    }

    // 参数
    fn visit_param(&mut self, param: &Param) {
        walk_param(self, param);
    }

    // 块
    fn visit_block(&mut self, block: &Block) {
        walk_block(self, block);
    }

    // 结构体字段（定义）
    fn visit_field(&mut self, field: &Field) {
        walk_field(self, field);
    }

    // 表达式
    fn visit_expr(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }

    // 语句
    fn visit_stmt(&mut self, stmt: &Stmt) {
        walk_stmt(self, stmt);
    }

    // 类型
    fn visit_ty(&mut self, ty: &Ty) {
        walk_ty(self, ty);
    }

    // 路径
    fn visit_path(&mut self, path: &Path) {
        walk_path(self, path);
    }

    // 路径段
    fn visit_path_segment(&mut self, segment: &PathSegment) {
        walk_path_segment(self, segment);
    }

    // 泛型参数列表
    fn visit_generic_params(&mut self, generic_params: &GenericParams) {
        walk_generic_params(self, generic_params);
    }

    // 泛型参数
    fn visit_generic_param(&mut self, param: &GenericParam) {
        walk_generic_param(self, param);
    }

    // 结构体初始化表达式
    fn visit_struct_expr(&mut self, struct_expr: &StructExpr) {
        walk_struct_expr(self, struct_expr);
    }

    // 结构体初始化字段
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
}

// walk 函数
pub fn walk_crate<V: Visitor + ?Sized>(visitor: &mut V, krate: &Crate) {
    for item in &krate.items {
        visitor.visit_item(item);
    }
}

pub fn walk_item<V: Visitor + ?Sized>(visitor: &mut V, item: &Item) {
    match &item.kind {
        ItemKind::Fn(func) => visitor.visit_fn(func),
        ItemKind::Struct(_ident, generics, fields) => {
            if !generics.params.is_empty() {
                visitor.visit_generic_params(generics);
            }
            for field in fields {
                visitor.visit_field(field);
            }
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
        ItemKind::Trait(_ident, items) => {
            for item in items {
                visitor.visit_trait_item(item);
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
    for param in &fn_.sig.params {
        visitor.visit_param(param);
    }
    match &fn_.sig.return_type {
        FnRetTy::Default(_) => {}
        FnRetTy::Ty(ty) => visitor.visit_ty(ty),
    }
    visitor.visit_generic_params(&fn_.sig.generics);
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
    visitor.visit_ty(&param.ty);
}

pub fn walk_block<V: Visitor + ?Sized>(visitor: &mut V, block: &Block) {
    for stmt in &block.stmts {
        visitor.visit_stmt(stmt);
    }
    if let Some(tail) = &block.tail {
        visitor.visit_expr(tail);
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
            mutability: _,
            variable: _,
            iter,
            body,
        } => {
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
    }
}

pub fn walk_stmt<V: Visitor + ?Sized>(visitor: &mut V, stmt: &Stmt) {
    match &stmt.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => visitor.visit_expr(e),
        StmtKind::Let(_, _, ty, init) => {
            if let Some(ty) = ty {
                visitor.visit_ty(ty);
            }
            if let Some(init) = init {
                visitor.visit_expr(init);
            }
        }
        StmtKind::Return(e) => {
            if let Some(e) = e {
                visitor.visit_expr(e);
            }
        }
        StmtKind::Continue | StmtKind::Break(_) => {}
    }
}

pub fn walk_ty<V: Visitor + ?Sized>(visitor: &mut V, ty: &Ty) {
    match &ty.kind {
        TyKind::Path { path } => visitor.visit_path(path),
        TyKind::Never | TyKind::Unit | TyKind::Infer => {}
        TyKind::Ref { ty: inner, .. } => visitor.visit_ty(inner),
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

pub fn walk_generic_params<V: Visitor + ?Sized>(visitor: &mut V, generics: &GenericParams) {
    for param in &generics.params {
        visitor.visit_generic_param(param);
    }
}

pub fn walk_generic_param<V: Visitor + ?Sized>(_visitor: &mut V, _param: &GenericParam) {
    // 没有子节点
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
    visitor.visit_generic_params(&impl_.generics);
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
    visitor.visit_generic_params(&type_alias.generics);
    visitor.visit_ty(&type_alias.ty);
}

pub fn walk_trait_item<V: Visitor + ?Sized>(visitor: &mut V, trait_item: &TraitItem) {
    match &trait_item.kind {
        TraitItemKind::Fn(fn_) => visitor.visit_fn(fn_),
    }
}