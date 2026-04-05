use litec_ast::{
    ast::*,
    mut_visit::{MutVisitor, walk_mut_block, walk_mut_crate, walk_mut_expr, walk_mut_extern, walk_mut_extern_item, walk_mut_field, walk_mut_fn, walk_mut_generic_param, walk_mut_generic_params, walk_mut_impl, walk_mut_impl_item, walk_mut_item, walk_mut_param, walk_mut_path, walk_mut_path_segment, walk_mut_stmt, walk_mut_struct_expr, walk_mut_struct_expr_field, walk_mut_ty, walk_mut_type_alias, walk_mut_use_tree},
};

pub struct NodeCollector {
    next_id: u32,
}

impl NodeCollector {
    pub fn new() -> Self {
        Self { next_id: 0 }
    }

    pub fn collect(&mut self, krate: &mut Crate) {
        self.visit_crate(krate);
    }

    fn alloc_id(&mut self) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        NodeId::from_raw(id)
    }
}

impl MutVisitor for NodeCollector {
    fn visit_crate(&mut self, krate: &mut Crate) {
        let id = self.alloc_id();
        krate.node_id = id;

        walk_mut_crate(self, krate);
    }

    fn visit_block(&mut self, block: &mut Block) {
        let id = self.alloc_id();
        block.node_id = id;

        walk_mut_block(self, block);
    }

    fn visit_expr(&mut self, expr: &mut Expr) {
        let id = self.alloc_id();
        expr.node_id = id;

        walk_mut_expr(self, expr);
    }

    fn visit_extern(&mut self, ext: &mut Extern) {
        let id = self.alloc_id();
        ext.node_id = id;

        walk_mut_extern(self, ext);
    }

    fn visit_extern_item(&mut self, item: &mut ExternItem) {
        let id = self.alloc_id();
        item.node_id = id;

        walk_mut_extern_item(self, item);
    }

    fn visit_field(&mut self, field: &mut Field) {
        let id = self.alloc_id();
        field.node_id = id;

        walk_mut_field(self, field);
    }

    fn visit_fn(&mut self, func: &mut Fn) {
        let id = self.alloc_id();
        func.node_id = id;

        walk_mut_fn(self, func);
    }

    fn visit_generic_param(&mut self, param: &mut GenericParam) {
        let id = self.alloc_id();
        param.node_id = id;

        walk_mut_generic_param(self, param);
    }

    fn visit_generic_params(&mut self, generics: &mut GenericParams) {
        let id = self.alloc_id();
        generics.node_id = id;

        walk_mut_generic_params(self, generics);
    }

    fn visit_item(&mut self, item: &mut Item) {
        let id = self.alloc_id();
        item.node_id = id;

        walk_mut_item(self, item);
    }

    fn visit_param(&mut self, param: &mut Param) {
        let id = self.alloc_id();
        param.node_id = id;
        
        walk_mut_param(self, param);
    }

    fn visit_path(&mut self, path: &mut Path) {
        let id = self.alloc_id();
        path.node_id = id;

        walk_mut_path(self, path);
    }

    fn visit_path_segment(&mut self, segment: &mut PathSegment) {
        let id = self.alloc_id();
        segment.node_id = id;

        walk_mut_path_segment(self, segment);
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        let id = self.alloc_id();
        stmt.node_id = id;

        walk_mut_stmt(self, stmt);
    }

    fn visit_struct_expr(&mut self, struct_expr: &mut StructExpr) {
        let id = self.alloc_id();
        struct_expr.node_id = id;

        walk_mut_struct_expr(self, struct_expr);
    }

    fn visit_struct_expr_field(&mut self, field: &mut StructExprField) {
        walk_mut_struct_expr_field(self, field);
    }

    fn visit_ty(&mut self, ty: &mut Ty) {
        let id = self.alloc_id();
        ty.node_id = id;

        walk_mut_ty(self, ty);
    }

    fn visit_use_tree(&mut self, use_tree: &mut UseTree) {
        let id = self.alloc_id();
        use_tree.node_id = id;

        walk_mut_use_tree(self, use_tree);
    }

    fn visit_impl(&mut self, impl_: &mut Impl) {
        let id = self.alloc_id();
        impl_.node_id = id;
        
        walk_mut_impl(self, impl_);
    }
    
    fn visit_impl_item(&mut self, impl_item: &mut ImplItem) {
        let id = self.alloc_id();
        impl_item.node_id = id;

        walk_mut_impl_item(self, impl_item);
    }

    fn visit_type_alias(&mut self, type_alias: &mut TypeAlias) {
        let id = self.alloc_id();
        type_alias.node_id = id;

        walk_mut_type_alias(self, type_alias);
    }
}
