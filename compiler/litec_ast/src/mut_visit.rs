use crate::{ast::*, visit};

/// 用于遍历并修改 AST 的 MutVisitor trait。
pub trait MutVisitor {
    // 根
    fn visit_crate(&mut self, krate: &mut Crate) {
        walk_mut_crate(self, krate);
    }

    // 项（Item<ItemKind>）
    fn visit_item(&mut self, item: &mut Item) {
        walk_mut_item(self, item);
    }

    // 外部项（ExternItem）
    fn visit_extern_item(&mut self, item: &mut ExternItem) {
        walk_mut_extern_item(self, item);
    }

    // 函数
    fn visit_fn(&mut self, func: &mut Fn) {
        walk_mut_fn(self, func);
    }

    // 外部块
    fn visit_extern(&mut self, ext: &mut Extern) {
        walk_mut_extern(self, ext);
    }

    // use 树
    fn visit_use_tree(&mut self, use_tree: &mut UseTree) {
        walk_mut_use_tree(self, use_tree);
    }

    // 参数
    fn visit_param(&mut self, param: &mut Param) {
        walk_mut_param(self, param);
    }

    // 块
    fn visit_block(&mut self, block: &mut Block) {
        walk_mut_block(self, block);
    }

    // 结构体字段（定义）
    fn visit_field(&mut self, field: &mut Field) {
        walk_mut_field(self, field);
    }

    // 表达式
    fn visit_expr(&mut self, expr: &mut Expr) {
        walk_mut_expr(self, expr);
    }

    // 语句
    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        walk_mut_stmt(self, stmt);
    }

    // 类型
    fn visit_ty(&mut self, ty: &mut Ty) {
        walk_mut_ty(self, ty);
    }

    // 路径
    fn visit_path(&mut self, path: &mut Path) {
        walk_mut_path(self, path);
    }

    // 路径段
    fn visit_path_segment(&mut self, segment: &mut PathSegment) {
        walk_mut_path_segment(self, segment);
    }

    // 泛型参数列表
    fn visit_generic_params(&mut self, generics: &mut GenericParams) {
        walk_mut_generic_params(self, generics);
    }

    // 泛型参数
    fn visit_generic_param(&mut self, param: &mut GenericParam) {
        walk_mut_generic_param(self, param);
    }

    // 结构体初始化表达式
    fn visit_struct_expr(&mut self, struct_expr: &mut StructExpr) {
        walk_mut_struct_expr(self, struct_expr);
    }

    // 结构体初始化字段
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
}

// walk 函数
pub fn walk_mut_crate<V: MutVisitor + ?Sized>(visitor: &mut V, krate: &mut Crate) {
    for item in &mut krate.items {
        visitor.visit_item(item);
    }
}

pub fn walk_mut_item<V: MutVisitor + ?Sized>(visitor: &mut V, item: &mut Item) {
    match &mut item.kind {
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
        ItemKind::Impl(impl_) => visitor.visit_impl(impl_),
        ItemKind::TypeAlias(type_alias) => visitor.visit_type_alias(type_alias),
        ItemKind::Trait(ident, items) => {
            for item in items {
                visitor.visit_trait_item(item);
            }
        }
    }
}

pub fn walk_mut_extern_item<V: MutVisitor + ?Sized>(visitor: &mut V, item: &mut ExternItem) {
    match &mut item.kind {
        ExternItemKind::Fn(func) => visitor.visit_fn(func),
    }
}

pub fn walk_mut_fn<V: MutVisitor + ?Sized>(visitor: &mut V, func: &mut Fn) {
    for param in &mut func.sig.params {
        visitor.visit_param(param);
    }
    match &mut func.sig.return_type {
        FnRetTy::Default(_) => {}
        FnRetTy::Ty(ty) => visitor.visit_ty(ty),
    }
    if let Some(body) = &mut func.body {
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
    visitor.visit_ty(&mut param.ty);
}

pub fn walk_mut_block<V: MutVisitor + ?Sized>(visitor: &mut V, block: &mut Block) {
    for stmt in &mut block.stmts {
        visitor.visit_stmt(stmt);
    }
    if let Some(tail) = &mut block.tail {
        visitor.visit_expr(tail);
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

pub fn walk_mut_stmt<V: MutVisitor + ?Sized>(visitor: &mut V, stmt: &mut Stmt) {
    match &mut stmt.kind {
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

pub fn walk_mut_ty<V: MutVisitor + ?Sized>(visitor: &mut V, ty: &mut Ty) {
    match &mut ty.kind {
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

pub fn walk_mut_generic_params<V: MutVisitor + ?Sized>(
    visitor: &mut V,
    generics: &mut GenericParams,
) {
    for param in &mut generics.params {
        visitor.visit_generic_param(param);
    }
}

pub fn walk_mut_generic_param<V: MutVisitor + ?Sized>(_visitor: &mut V, _param: &mut GenericParam) {
    // 没有子节点
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
        TraitItemKind::Fn(fn_) => visitor.visit_fn(fn_),
    }
}
