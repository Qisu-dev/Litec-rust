use crate::{
    constraint::{Constraint, TraitRef},
    env::{PolyTy, TypeEnv},
};
use litec_ast::{
    ast::{BinOpKind, Ident, Lit, UnOp},
    token::LiteralKind,
};
use litec_error::error;
use litec_hir::{
    def::{BuiltinTrait, DefKind, Res},
    hir::{self, *},
};
use litec_middle::{
    context::GlobalCtxt,
    ty::{Ty, TypeVarId},
};
use litec_span::{
    Span,
    id::{DefId, HirId},
};
use rustc_hash::{FxHashMap, FxHashSet};

mod constraint;
mod env;

pub fn prim_implements_trait(ty: &Ty, tr: BuiltinTrait) -> bool {
    match (ty, tr) {
        (Ty::Prim(_), BuiltinTrait::Clone | BuiltinTrait::Copy) => true,
        (Ty::Prim(p), BuiltinTrait::Add) => p.is_numeric(),
        (Ty::Prim(p), BuiltinTrait::Sub) => p.is_numeric(),
        (Ty::Prim(p), BuiltinTrait::Mul) => p.is_numeric(),
        (Ty::Prim(p), BuiltinTrait::Div) => p.is_numeric(),
        (Ty::Prim(p), BuiltinTrait::Rem) => p.is_numeric(),
        (Ty::Prim(p), BuiltinTrait::Neg) => p.is_numeric(),
        (Ty::Prim(p), BuiltinTrait::Not) => matches!(p, PrimTy::Bool),
        (Ty::Ptr(_), BuiltinTrait::Deref | BuiltinTrait::Clone | BuiltinTrait::Copy) => true,

        _ => false,
    }
}

#[derive(Debug, Clone)]
pub struct GenericImpl {
    pub trait_def_id: DefId,
    pub self_pattern: Ty,
    pub impl_def_id: DefId,
    pub params: Vec<TypeVarId>,
}

fn bin_op_to_trait_res(op: &BinOpKind) -> Res {
    let trait_ = match op {
        BinOpKind::Add => BuiltinTrait::Add,
        BinOpKind::Sub => BuiltinTrait::Sub,
        BinOpKind::Mul => BuiltinTrait::Mul,
        BinOpKind::Div => BuiltinTrait::Div,
        BinOpKind::Rem => BuiltinTrait::Rem,
        _ => return Res::Err,
    };
    Res::BuiltinTrait(trait_)
}

fn un_op_to_trait_res(op: &UnOp) -> Res {
    let trait_ = match op {
        UnOp::Deref => BuiltinTrait::Deref,
        UnOp::Not => BuiltinTrait::Not,
        UnOp::Neg => BuiltinTrait::Neg,
    };
    Res::BuiltinTrait(trait_)
}

pub struct TypeChecker<'hir> {
    gcx: &'hir mut GlobalCtxt<'hir>,
    locals_stack: Vec<FxHashMap<HirId, Ty>>,
    ret_ty: Option<Ty>,
    env: TypeEnv,
    constraints: Vec<Constraint>,
    type_map: FxHashMap<HirId, Ty>,
    poly_types: FxHashMap<HirId, PolyTy>,

    // 固有方法
    inherent_methods: FxHashMap<DefId, Vec<(Ident, DefId)>>,

    // Trait 定义的方法
    trait_methods: FxHashMap<DefId, Vec<(Ident, DefId)>>,

    // 具体类型实现的 trait（含泛型参数）
    type_trait_impls: FxHashMap<DefId, Vec<(DefId, Vec<Ty>, DefId)>>, // type_def_id -> (trait_def_id, substs, impl_def_id)

    // 泛型 impl 模式（如 impl<T> Trait for *T）
    generic_impls: FxHashMap<DefId, GenericImpl>,

    // 方法查找缓存: (trait_def_id, type_def_id, method_name) -> method_def_id
    trait_impl_methods: FxHashMap<(DefId, DefId, Ident), DefId>,

    // 用户为内置 trait 的实现
    builtin_trait_impls: FxHashMap<(BuiltinTrait, DefId), DefId>,

    trait_generics: FxHashMap<DefId, &'hir Generics<'hir>>,
}

impl<'hir> TypeChecker<'hir> {
    pub fn new(gcx: &'hir mut GlobalCtxt<'hir>) -> Self {
        Self {
            gcx,
            locals_stack: vec![FxHashMap::default()],
            ret_ty: None,
            env: TypeEnv::new(),
            constraints: Vec::new(),
            type_map: FxHashMap::default(),
            poly_types: FxHashMap::default(),
            generic_impls: FxHashMap::default(),
            inherent_methods: FxHashMap::default(),
            trait_methods: FxHashMap::default(),
            trait_impl_methods: FxHashMap::default(),
            builtin_trait_impls: FxHashMap::default(),
            type_trait_impls: FxHashMap::default(),
            trait_generics: FxHashMap::default(),
        }
    }

    pub fn check_crate(&mut self, hir_crate: &'hir Crate<'hir>) -> FxHashMap<HirId, Ty> {
        for item in hir_crate.items {
            self.check_item(item);
        }
        self.solve_constraints();
        let mut final_map = FxHashMap::default();
        for (id, ty) in self.type_map.drain() {
            final_map.insert(id, self.env.apply_subst(&ty));
        }
        final_map
    }

    fn parse_trait_substs(&mut self, trait_id: DefId) -> Vec<Ty> {
        if let Some(generic_args) = self.trait_generics.get(&trait_id) {
            let mut substs = Vec::new();
            for param in generic_args.params {
                match &param.kind {
                    GenericParamKind::Type(ty) => substs.push(self.ty_of_hir_type(ty)),
                }
            }
            substs
        } else {
            Vec::new()
        }
    }

    fn collect_definitions(&mut self, item: &'hir Item<'hir>) {
        match &item.kind {
            ItemKind::Trait(_, generics, trait_items) => {
                let trait_def_id = item.def_id;
                self.trait_generics.insert(trait_def_id, generics);
                let mut methods = Vec::new();
                for trait_item in *trait_items {
                    match &trait_item.kind {
                        TraitItemKind::Fn(sig) => {
                            methods.push((sig.name, trait_item.def_id));
                        }
                    }
                }
                self.trait_methods.insert(trait_def_id, methods);
            }

            ItemKind::Struct(_, generics, _) => {
                self.trait_generics.insert(item.def_id, generics);
            }
            ItemKind::Enum(_, generics, _) => {
                self.trait_generics.insert(item.def_id, generics);
            }
            ItemKind::TypeAlias(ta) => {
                self.trait_generics.insert(item.def_id, &ta.generics);
            }
            
            _ => {}
        }
    }

    fn check_item(&mut self, item: &'hir Item<'hir>) {
        match &item.kind {
            ItemKind::Fn(fn_node) => {
                self.check_fn(fn_node, item.def_id);
            }
            ItemKind::Impl(impl_node) => {
                self.check_impl(impl_node, item.def_id);
            }
            _ => {}
        }
    }

    fn check_fn(&mut self, fn_node: &'hir Fn<'hir>, def_id: DefId) {
        self.push_scope();
        let old_ret = self.ret_ty.take();
        let ret_ty = self.ty_of_hir_type(&fn_node.sig.return_type);
        self.ret_ty = Some(ret_ty.clone());

        for param in fn_node.sig.params {
            let ty = self.ty_of_hir_type(&param.ty);
            self.insert_local(param.pat.hir_id, ty);
        }

        let body_ty = self.check_block(fn_node.body);
        if let Err(e) = self.env.unify(&body_ty, &ret_ty) {
            self.gcx.report_err(error(e).with_span(fn_node.body.span));
        }

        self.ret_ty = old_ret;
        self.pop_scope();
    }

    fn check_impl(&mut self, impl_node: &'hir Impl<'hir>, def_id: DefId) {
        let self_ty = self.ty_of_hir_type(&impl_node.self_ty);
        if let Some(trait_path) = impl_node.of_trait {
            if let Res::Def(DefKind::Trait, trait_def_id) = trait_path.res {
                if !impl_node.generics.params.is_empty() {
                    let pattern = self_ty.clone();
                    let params = self.collect_type_vars(&pattern);
                    self.generic_impls.push(GenericImpl {
                        trait_def_id,
                        self_pattern: pattern,
                        impl_def_id: def_id,
                        params,
                    });
                } else {
                    if let Ty::Adt(def_id, _) = self_ty {
                        self.trait_impls.insert((trait_def_id, def_id), def_id);
                    } else {
                        self.gcx.report_err(
                            error("impl self type must be ADT").with_span(impl_node.self_ty.span),
                        );
                    }
                }
            }
        }
    }

    fn check_expr(&mut self, expr: &'hir Expr<'hir>) -> Ty {
        let ty = match &expr.kind {
            ExprKind::Literal(lit) => self.lit_ty(lit, expr.span),
            ExprKind::QPath(qpath) => {
                let res = self.resolve_qpath(qpath);
                self.ty_of_res(&res)
            }
            ExprKind::Binary(left, op, right) => {
                let left_ty = self.check_expr(left);
                let right_ty = self.check_expr(right);
                let trait_res = bin_op_to_trait_res(&op.value);
                self.constraints.push(Constraint::Trait(
                    TraitRef {
                        trait_res,
                        substs: vec![left_ty.clone()],
                    },
                    left_ty.clone(),
                    expr.span,
                ));
                if let Err(e) = self.env.unify(&left_ty, &right_ty) {
                    self.gcx.report_err(error(e).with_span(expr.span));
                }
                left_ty
            }
            ExprKind::Call(callee, args) => {
                let callee_ty = self.check_expr(callee);
                let arg_tys: Vec<_> = args.iter().map(|a| self.check_expr(a)).collect();
                let ret_ty = self.env.new_var();
                if let Err(e) = self.env.unify(
                    &callee_ty,
                    &Ty::Fn(arg_tys.clone(), Box::new(ret_ty.clone())),
                ) {
                    self.gcx.report_err(error(e).with_span(expr.span));
                }
                ret_ty
            }
            ExprKind::Block(block) => self.check_block(block),
            ExprKind::If(cond, then_block, else_opt) => {
                let cond_ty = self.check_expr(cond);
                if let Err(e) = self.env.unify(&cond_ty, &Ty::Prim(PrimTy::Bool)) {
                    self.gcx.report_err(error(e).with_span(cond.span));
                }
                let then_ty = self.check_block(then_block);
                let else_ty = else_opt
                    .map(|e| self.check_expr(e))
                    .unwrap_or(Ty::Prim(PrimTy::Unit));
                if let Err(e) = self.env.unify(&then_ty, &else_ty) {
                    self.gcx.report_err(error(e).with_span(expr.span));
                }
                then_ty
            }
            ExprKind::Return(expr_opt) => {
                let ret_ty = self.ret_ty.clone().unwrap_or(Ty::Error);
                if let Some(expr) = expr_opt {
                    let e_ty = self.check_expr(expr);
                    if let Err(err) = self.env.unify(&e_ty, &ret_ty) {
                        self.gcx.report_err(error(err).with_span(expr.span));
                    }
                } else if ret_ty != Ty::Prim(PrimTy::Unit) {
                    self.gcx
                        .report_err(error("return expects a value").with_span(expr.span));
                }
                Ty::Prim(PrimTy::Never) // 实际为 Never
            }
            ExprKind::Unary(un_op, expr) => {
                let trait_ref = un_op_to_trait_res(un_op);
                let expr_ty = self.check_expr(expr);
                if !self.trait_satisfied(trait_ref, &expr_ty) {
                    self.gcx
                        .report_err(error("表达式并没重载前缀运算符").with_span(expr.span))
                }
            }
            ExprKind::Grouped(expr) => todo!(),
            ExprKind::Assignment(expr, expr1) => todo!(),
            ExprKind::AssignmentWithOp(expr, spanned, expr1) => todo!(),
            ExprKind::While(expr, block) => todo!(),
            ExprKind::For {
                variable,
                iter,
                body,
            } => todo!(),
            ExprKind::Index(expr, expr1) => todo!(),
            ExprKind::Range(expr, expr1, range_limits) => todo!(),
            ExprKind::Loop(block) => todo!(),
            ExprKind::Field(expr, ident) => todo!(),
            ExprKind::Bool(_) => todo!(),
            ExprKind::Tuple(exprs) => todo!(),
            ExprKind::Unit => todo!(),
            ExprKind::AddressOf(expr) => todo!(),
            ExprKind::StructExpr(struct_expr) => todo!(),
            ExprKind::Cast(expr, ty) => todo!(),
            ExprKind::Match(expr, arms) => todo!(),
            ExprKind::Continue => todo!(),
            ExprKind::Break(expr) => todo!(),
        };
        self.type_map.insert(expr.hir_id, ty.clone());
        ty
    }

    fn check_block(&mut self, block: &'hir Block<'hir>) -> Ty {
        self.push_scope();
        let mut block_ty = Ty::Prim(PrimTy::Unit);
        let stmts_len = block.stmts.len();
        for (i, stmt) in block.stmts.iter().enumerate() {
            let is_last = i == stmts_len - 1;
            match &stmt.kind {
                StmtKind::Expr(expr) if is_last => {
                    block_ty = self.check_expr(expr);
                }
                StmtKind::Expr(expr) => {
                    self.check_expr(expr);
                    block_ty = Ty::Prim(PrimTy::Unit);
                }
                StmtKind::Semi(expr) => {
                    self.check_expr(expr);
                    block_ty = Ty::Prim(PrimTy::Unit);
                }
                _ => {
                    self.check_stmt(stmt);
                }
            }
        }
        self.pop_scope();
        block_ty
    }

    fn check_stmt(&mut self, stmt: &'hir Stmt<'hir>) -> Ty {
        match &stmt.kind {
            StmtKind::Expr(expr) => self.check_expr(expr),
            StmtKind::Semi(expr) => {
                self.check_expr(expr);
                Ty::Prim(PrimTy::Unit)
            }
            StmtKind::Let(pat, ty_opt, init) => {
                let init_ty = init.map(|expr| self.check_expr(expr));
                if let Some(ty_node) = ty_opt {
                    let annot_ty = self.ty_of_hir_type(ty_node);
                    if let Some(init_ty) = &init_ty
                        && let Err(e) = self.env.unify(init_ty, &annot_ty)
                    {
                        self.gcx.report_err(error(e).with_span(ty_node.span));
                    }
                }
                if let Some(init_ty) = &init_ty {
                    let free_vars = self.free_vars_in_env();
                    let poly = self.env.generalize(init_ty, &free_vars);
                    self.bind_pat(pat, &init_ty, Some(poly));
                }
                Ty::Prim(PrimTy::Unit)
            }
            StmtKind::Defer(_) => Ty::Prim(PrimTy::Unit),
        }
    }

    fn bind_pat(&mut self, pat: &'hir Pat<'hir>, ty: &Ty, poly: Option<PolyTy>) {
        match &pat.kind {
            PatKind::Ident(_, _) => {
                self.insert_local(pat.hir_id, ty.clone());
                if let Some(p) = poly {
                    self.poly_types.insert(pat.hir_id, p);
                }
            }
            PatKind::Tuple(pats) => {
                if let Ty::Tuple(elems) = ty {
                    for (p, elem_ty) in pats.iter().zip(elems.iter()) {
                        self.bind_pat(p, elem_ty, None);
                    }
                } else {
                    self.gcx
                        .report_err(error("pattern type mismatch").with_span(pat.span));
                }
            }
            _ => {}
        }
    }

    fn ty_of_hir_type(&mut self, ty_node: &'hir hir::Ty<'hir>) -> Ty {
        match &ty_node.kind {
            TyKind::QPath(qpath) => {
                let res = self.resolve_qpath(qpath);
                self.ty_of_res(&res)
            }
            TyKind::SelfTy => self.env.new_var(),
            TyKind::Ptr { ty, .. } => Ty::Ptr(Box::new(self.ty_of_hir_type(ty))),
            TyKind::Array { elem, .. } => self.ty_of_hir_type(elem),
            TyKind::Tuple { elems } => {
                Ty::Tuple(elems.iter().map(|e| self.ty_of_hir_type(e)).collect())
            }
            TyKind::FnPtr { inputs, output } => Ty::Fn(
                inputs.iter().map(|i| self.ty_of_hir_type(i)).collect(),
                Box::new(self.ty_of_hir_type(output)),
            ),
            _ => Ty::Error,
        }
    }

    fn ty_of_res(&self, res: &Res<HirId>) -> Ty {
        match res {
            Res::PrimTy(p) => Ty::Prim(*p),
            Res::Def(DefKind::Struct, def_id) => Ty::Adt(*def_id, vec![]),
            Res::Def(DefKind::Enum, def_id) => Ty::Adt(*def_id, vec![]),
            Res::Def(DefKind::Fn, def_id) => Ty::Fn(vec![], Box::new(Ty::Prim(PrimTy::Unit))),
            Res::Local(hir_id) => {
                if let Some(ty) = self.lookup_local(*hir_id) {
                    ty.clone()
                } else {
                    Ty::Error
                }
            }
            Res::BuiltinTrait(_) => Ty::Error,
            _ => Ty::Error,
        }
    }

    fn resolve_qpath(&self, qpath: &'hir QPath<'hir>) -> Res<HirId> {
        match qpath {
            QPath::Resolved(path) => path.res.clone(),
            _ => Res::Err,
        }
    }

    fn lit_ty(&self, lit: &Lit, span: Span) -> Ty {
        match lit.kind {
            LiteralKind::Integer => {
                if let Some(suffix) = lit.suffix {
                    let suffix_str = suffix.to_string();
                    match suffix_str.as_str() {
                        "i8" => Ty::Prim(PrimTy::Int(IntTy::I8)),
                        "i16" => Ty::Prim(PrimTy::Int(IntTy::I16)),
                        "i32" => Ty::Prim(PrimTy::Int(IntTy::I32)),
                        "i64" => Ty::Prim(PrimTy::Int(IntTy::I64)),
                        "i128" => Ty::Prim(PrimTy::Int(IntTy::I128)),
                        "isize" => Ty::Prim(PrimTy::Int(IntTy::Isize)),
                        "u8" => Ty::Prim(PrimTy::Uint(UintTy::U8)),
                        "u16" => Ty::Prim(PrimTy::Uint(UintTy::U16)),
                        "u32" => Ty::Prim(PrimTy::Uint(UintTy::U32)),
                        "u64" => Ty::Prim(PrimTy::Uint(UintTy::U64)),
                        "u128" => Ty::Prim(PrimTy::Uint(UintTy::U128)),
                        _ => {
                            self.gcx.report_err(error("未知的后缀").with_span(span));
                            Ty::Error
                        }
                    }
                } else {
                    Ty::Prim(PrimTy::Int(IntTy::I32))
                }
            }
            LiteralKind::Float => {
                if let Some(suffix) = lit.suffix {
                    let suffix_str = suffix.to_string();
                    match suffix_str.as_str() {
                        "f32" => Ty::Prim(PrimTy::Float(FloatTy::F32)),
                        "f64" => Ty::Prim(PrimTy::Float(FloatTy::F64)),
                        _ => {
                            self.gcx.report_err(error("未知的后缀").with_span(span));
                            Ty::Error
                        }
                    }
                } else {
                    Ty::Prim(PrimTy::Float(FloatTy::F32))
                }
            }
            LiteralKind::Char => Ty::Prim(PrimTy::Char),
            LiteralKind::Str => Ty::Prim(PrimTy::Str),
        }
    }

    fn push_scope(&mut self) {
        self.locals_stack.push(FxHashMap::default());
    }

    fn pop_scope(&mut self) {
        self.locals_stack.pop();
    }

    fn insert_local(&mut self, hir_id: HirId, ty: Ty) {
        self.locals_stack
            .last_mut()
            .expect("no scope")
            .insert(hir_id, ty);
    }

    fn lookup_local(&self, hir_id: HirId) -> Option<&Ty> {
        for scope in self.locals_stack.iter().rev() {
            if let Some(ty) = scope.get(&hir_id) {
                return Some(ty);
            }
        }
        None
    }

    fn free_vars_in_env(&self) -> FxHashSet<TypeVarId> {
        let mut set = FxHashSet::default();
        for scope in &self.locals_stack {
            for ty in scope.values() {
                self.env.collect_vars(ty, &mut set);
            }
        }
        set
    }

    fn collect_type_vars(&self, ty: &Ty) -> Vec<TypeVarId> {
        let mut set = FxHashSet::default();
        self.env.collect_vars(ty, &mut set);
        set.into_iter().collect()
    }

    fn solve_constraints(&mut self) {
        let mut constraints = std::mem::take(&mut self.constraints);
        for constraint in constraints.drain(..) {
            match constraint {
                Constraint::Eq(a, b, span) => {
                    if let Err(e) = self.env.unify(&a, &b) {
                        self.gcx.report_err(error(e).with_span(span));
                    }
                }
                Constraint::Trait(trait_ref, ty, span) => {
                    let concrete = self.env.apply_subst(&ty);
                    if !self.trait_satisfied(&trait_ref, &concrete) {
                        self.gcx.report_err(
                            error(format!(
                                "type {} does not implement trait {:?}",
                                concrete, trait_ref.trait_res
                            ))
                            .with_span(span),
                        );
                    }
                }
            }
        }
    }

    fn trait_satisfied(&self, trait_ref: &TraitRef, ty: &Ty) -> bool {
        match &trait_ref.trait_res {
            Res::BuiltinTrait(tr) => {
                if let Ty::Adt(def_id, _) = ty {
                    if self.builtin_trait_impls.contains_key(&(*tr, *def_id)) {
                        return true;
                    }
                }
                prim_implements_trait(ty, *tr)
            }
            Res::Def(DefKind::Trait, trait_def_id) => {
                if let Ty::Adt(def_id, _) = ty {
                    if self.trait_impls.contains_key(&(*trait_def_id, *def_id)) {
                        return true;
                    }
                }
                for generic_impl in &self.generic_impls {
                    if generic_impl.trait_def_id == *trait_def_id {
                        let mut env_clone = self.env.clone();
                        if env_clone.unify(&generic_impl.self_pattern, ty).is_ok() {
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn trait_arity(&self, trait_def_id: DefId) -> Option<usize> {
        self.trait_generics
            .get(&trait_def_id)
            .map(|params| params.len())
    }
}
