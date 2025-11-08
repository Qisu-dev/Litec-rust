mod scope;
mod type_error;

use std::path::PathBuf;

use litec_name_resolver::{ModuleId, Resolver};
use litec_span::{get_global_string, get_global_string_pool, Span, StringId};
use scope::Scope;
use litec_hir::{
    Crate as RawCrate, Expr as RawExpr, Field as RawField, Item as RawItem, LitIntValue, LiteralValue, 
    Visibility as RawVisibility, Type as RawType, Param as RawParam, Block as RawBlock, Stmt as RawStmt
};
use litec_typed_hir::{
    def_id::DefId, ty::{FloatKind, IntKind, Ty}, DefKind, TypedBlock, TypedCrate, TypedExpr, TypedField, TypedItem, TypedParam, TypedStmt, Visibility
};
use crate::{scope::Symbol, type_error::TypeError};

type CheckResult<T> = Result<T, TypeError>;

pub struct TypeChecker {
    scope: Scope,
    next_id: u32,
    resolver: Resolver, // 添加 Resolver 字段
    current_mod_id: ModuleId
}

impl TypeChecker {
    pub fn new(path: PathBuf) -> Self {
        TypeChecker {
            scope: Scope::new(None),
            next_id: 0,
            resolver: Resolver::new(path),
            current_mod_id: 0
        }
    }

    pub fn check_crate(&mut self, raw_crate: RawCrate) -> Result<TypedCrate, Vec<TypeError>> {
        match self.resolver.populate(&raw_crate) {
            Ok(_) => {},
            Err(err) => {
                return Err(err.into_iter().map(|e| TypeError::Error { error: e }).collect())
            }
        }
        
        self.inject_resolver_toplevel();
        self.precompute_function_sigs(&raw_crate);

        let mut items = Vec::new();
        let mut errs = Vec::new();

        for item in raw_crate.items {
            match self.check_item(item) {
                Ok(typed_item) => items.push(typed_item),
                Err(err) => errs.push(err)
            }
        }

        if errs.is_empty() {
            Ok(TypedCrate { items: items })
        } else {
            Err(errs)
        }
    }

    fn check_item(&mut self, raw_item: RawItem) -> CheckResult<TypedItem> {
        match raw_item {
            RawItem::Function { 
                visibility, 
                name, 
                params, 
                return_type, 
                body, 
                span 
            } => {
                let visibility = lower_visibility(visibility);

                self.enter_scope();

                let params: Vec<TypedParam> = params.into_iter()
                    .map(|p| self.check_param(p))
                    .collect::<CheckResult<Vec<TypedParam>>>()?;

                let ret_ty = match return_type {
                    Some(ty) => self.get_type(ty)?,
                    None => Ty::Unit
                };

                let body = self.check_function_body(body, ret_ty.clone())?;

                self.exit_scope();

                let id = *self.get_global().get_id(&name).unwrap();

                let new_symbol = Symbol::Function { name: name.clone(), params_type: params.iter().map(|p| p.ty.clone()).collect(), ret_type: ret_ty.clone(), span: span.clone() };
                self.scope.get_mut_global().replace_symbol(id, new_symbol);

                Ok(TypedItem::Function { def_id: id, visibility: visibility, name: name, params: params, return_ty: ret_ty, body: body, span: span })
            },
            RawItem::Struct { 
                visibility,
                name, 
                fields, 
                span 
            } => {
                let visibility = lower_visibility(visibility);

                let mut typed_fields = Vec::new();

                for field in fields {
                    match self.check_field(field) {
                        Ok(field) => typed_fields.push(field),
                        Err(err) => return Err(err)
                    }
                }

                let id = self.resolver.resolve_path(&[name], self.current_mod_id).unwrap();

                let new_symbol = Symbol::Struct { name: name, fields: typed_fields.iter().map(|field| (field.name, field.def_id, field.ty.clone())).collect(), span: span };
                self.scope.get_mut_global().replace_symbol(id, new_symbol);

                Ok(TypedItem::Struct { def_id: id, visibility: visibility, name: name, fields: typed_fields, span: span })
            },

            RawItem::Use{
                ..
            } => unimplemented!("没有实现 use")
        }
    }

    fn check_field(&mut self, field: RawField) -> CheckResult<TypedField> {
        let visibility = lower_visibility(field.visibility);
        let ty = self.get_type(field.ty)?;
        let id = self.alloc_id(DefKind::Variable);

        Ok(TypedField { visibility: visibility, name: field.name, def_id: id, ty: ty, span: field.span})
    }

    fn collect_returns(&mut self, body: &TypedBlock) -> CheckResult<Vec<TypedStmt>> {
        let mut returns: Vec<TypedStmt> = Vec::new();

        for stmt in body.stmts.clone() {
            match stmt {
                TypedStmt::Return { .. } => {
                    returns.push(stmt);
                }
                TypedStmt::Expr(expr) => match *expr {
                    TypedExpr::Block { block , .. } |
                    TypedExpr::Loop { body: block , .. } => {
                        returns.extend(self.collect_returns(&block)?);
                    }
                    TypedExpr::If { then_branch, else_branch, .. } => {
                        returns.extend(self.collect_returns(&then_branch)?);
                        let mut else_branch_ = else_branch;
                        loop {
                            match else_branch_ {
                                Some(expr) => match *expr {
                                    TypedExpr::If { then_branch, else_branch, .. } => {
                                        returns.extend(self.collect_returns(&then_branch)?);
                                        else_branch_ = else_branch;
                                    }
                                    TypedExpr::Block { block, .. } => {
                                        returns.extend(self.collect_returns(&block)?);
                                        else_branch_ = None;
                                    }
                                    _ => unreachable!()
                                }
                                None => break
                            }
                        }
                    }
                    _ => {}
                }
                _ => {}
            }
        }

        Ok(returns)
    }

    fn check_function_body(&mut self, body: RawBlock, ret_ty: Ty) -> CheckResult<TypedBlock> {
        let typed_stmts = Vec::new();

        let body = self.check_block(body)?;
        let returns = self.collect_returns(&body)?;

        for ret in returns {
            match ret {
                TypedStmt::Return { value, .. } => {
                    let ty = match value {
                        Some(expr) => expr.ty().clone(),
                        None => Ty::Unit
                    };

                    self.unify(ret_ty.clone(), ty)?;
                }
                _ => unreachable!()
            }
        }

        let tail = match body.tail {
            Some(expr) => {
                self.unify(expr.ty().clone(), ret_ty.clone())?;
                Some(expr)
            },
            None => None
        };

        Ok(TypedBlock { 
            stmts: typed_stmts, 
            tail, ty: ret_ty, 
            span: body.span 
        })
    }

    fn check_block(&mut self, block: RawBlock) -> CheckResult<TypedBlock> {
        let mut typed_stmts = Vec::new();
        for stmt in block.stmts.into_iter() {
            typed_stmts.push(self.check_stmt(stmt.clone())?);
        }

        let tail = match block.tail {
            Some(expr) => Some(Box::new(self.check_expr(*expr.clone())?)),
            None => None
        };
        let ty = match &tail {
            Some(expr) => expr.ty().clone(),
            None => Ty::Unit
        };
        

        Ok(TypedBlock { stmts: typed_stmts, tail: tail, ty: ty, span: block.span })
    }

    fn check_stmt(&mut self, stmt: RawStmt) -> CheckResult<TypedStmt> {
        match stmt {
            RawStmt::Expr(expr) => {
                Ok(TypedStmt::Expr(
                    Box::new(self.check_expr(*expr)?)
                ))
            },
            RawStmt::Let { name, ty, value, span } => {
                if self.scope.get_current_layer_id(&name).is_some() {
                    return Err(TypeError::RedefineVariable { name: get_global_string(name).unwrap().to_string(), span: span });
                }
                let ty = match ty {
                    Some(ty) => self.get_type(ty)?,
                    None => Ty::Unknown
                };
                let value = match value {
                    Some(expr) => Some(Box::new(self.check_expr(*expr)?)),
                    None => None
                };
                let ty = match value.as_ref() {
                    Some(expr) => {
                        self.unify(ty, expr.ty().clone())?
                    }
                    None => ty
                };
                let id = self.alloc_id(DefKind::Variable);
                self.scope.insert_symbol(id, Symbol::Variable { name: name, ty: ty.clone(), span: span });

                Ok(TypedStmt::Let {
                    name: name, 
                    def_id: id, 
                    ty: ty, 
                    init: value, 
                    span: span 
                })
            },
            RawStmt::Return { value, span } => {
                let value = match value {
                    Some(expr) => Some(Box::new(self.check_expr(*expr)?)),
                    None => None
                };

                Ok(TypedStmt::Return { value: value, span: span })
            },
            RawStmt::Break { value, span } => {
                let value = match value {
                    Some(expr) => Some(Box::new(self.check_expr(*expr)?)),
                    None => None
                };

                Ok(TypedStmt::Break { value: value, span: span })
            },
            RawStmt::Continue { span } => {
                Ok(TypedStmt::Continue { span: span })
            },
        }
    }

    fn check_expr(&mut self, expr: RawExpr) -> CheckResult<TypedExpr> {
        match expr {
            RawExpr::Literal { 
                value: LiteralValue::Int { kind, value: inner}, 
                span 
            } => {
                let new_kind = match kind {
                    LitIntValue::I8 => IntKind::I8,
                    LitIntValue::I16 => IntKind::I16,
                    LitIntValue::I32 => IntKind::I32,
                    LitIntValue::I64 => IntKind::I64,
                    LitIntValue::I128 => IntKind::I128,
                    LitIntValue::Isize => IntKind::Isize,
                    LitIntValue::U8 => IntKind::U8,
                    LitIntValue::U16 => IntKind::U16,
                    LitIntValue::U32 => IntKind::U32,
                    LitIntValue::U64 => IntKind::U64,
                    LitIntValue::U128 => IntKind::U128,
                    LitIntValue::Usize => IntKind::Usize,
                    LitIntValue::Unknown => IntKind::Unknown,
                };

                Ok(TypedExpr::Literal { value: LiteralValue::Int { value: inner, kind: kind }, ty: Ty::Int(new_kind), span: span })
            },
            RawExpr::Literal { 
                value: LiteralValue::Float { value, kind }, 
                span
            } => {
                let new_kind = match kind {
                    litec_hir::LitFloatValue::F32 => FloatKind::F32,
                    litec_hir::LitFloatValue::F64 => FloatKind::F64,
                    litec_hir::LitFloatValue::Unknown => FloatKind::Unknow,
                };

                Ok(TypedExpr::Literal { value: LiteralValue::Float { value: value, kind: kind }, ty: Ty::Float(new_kind), span: span })
            }
            RawExpr::Literal {
                value: LiteralValue::Bool(value),
                span
            } => {
                Ok(TypedExpr::Literal { value: LiteralValue::Bool(value), ty: Ty::Bool, span: span })
            }
            RawExpr::Literal {
                value: LiteralValue::Unit,
                span
            } => {
                Ok(TypedExpr::Literal { value: LiteralValue::Unit, ty: Ty::Unit, span: span })
            }
            RawExpr::Ident { name, span } => {
                match self.get_ident(name) {
                    Some(id) => match self.get_symbol(*id).unwrap() {
                        Symbol::Variable { name, ty, span } => {
                            Ok(TypedExpr::Ident { name: *name, def_id: *id, ty: ty.clone(), span: *span })
                        },
                        Symbol::Function { name, params_type, ret_type, span } => {
                            Ok(TypedExpr::Ident { name: *name, def_id: *id, ty: Ty::Fn { params: params_type.to_vec(), return_ty: Box::new(ret_type.clone()) }, span: *span })
                        },
                        Symbol::Struct { name, fields: _, span } => {
                            Ok(TypedExpr::Ident { name: *name, def_id: *id, ty: Ty::Adt(*id), span: *span })
                        }
                    },
                    None => Err(TypeError::UndefineSymbol { name: get_global_string(name).unwrap().to_string(), span: span })
                }
            },
            RawExpr::Literal {
                value: LiteralValue::Char(c),
                span
            } => {
                Ok(TypedExpr::Literal { value: LiteralValue::Char(c), ty: Ty::Char, span: span })
            }
            RawExpr::Literal { 
                value: LiteralValue::Str(s), 
                span 
            } => {
                Ok(TypedExpr::Literal { value: LiteralValue::Str(s), ty: Ty::Str, span: span })
            }
            RawExpr::Addition { left, right, span } => {
                let values = self.check_binary(*left, *right, span)?;
                
                Ok(TypedExpr::Addition { 
                    left: Box::new(values.0), 
                    right: Box::new(values.1), 
                    ty: values.2, 
                    span: span 
                })
            },
            RawExpr::Subtract { left, right, span } => {
                let values = self.check_binary(*left, *right, span)?;
                
                Ok(TypedExpr::Subtract { 
                    left: Box::new(values.0), 
                    right: Box::new(values.1), 
                    ty: values.2, 
                    span: span 
                })
            },
            RawExpr::Multiply { left, right, span } => {
                let values = self.check_binary(*left, *right, span)?;
                
                Ok(TypedExpr::Multiply { 
                    left: Box::new(values.0), 
                    right: Box::new(values.1), 
                    ty: values.2, 
                    span: span 
                })
            },
            RawExpr::Divide { left, right, span } => {
                let values = self.check_binary(*left, *right, span)?;
                
                Ok(TypedExpr::Divide { 
                    left: Box::new(values.0), 
                    right: Box::new(values.1), 
                    ty: values.2, 
                    span: span 
                })
            },
            RawExpr::Remainder { left, right, span } => {
                let values = self.check_binary(*left, *right, span)?;
                
                Ok(TypedExpr::Remainder { 
                    left: Box::new(values.0), 
                    right: Box::new(values.1), 
                    ty: values.2, 
                    span: span 
                })
            },
            RawExpr::Equal { left, right, span } => {
                let left = self.check_expr(*left)?;
                let right = self.check_expr(*right)?;
                self.unify(left.ty(), right.ty())?;

                Ok(TypedExpr::Equal { left: Box::new(left), right: Box::new(right), ty: Ty::Bool, span: span })
            },
            RawExpr::NotEqual { left, right, span } => {
                let left = self.check_expr(*left)?;
                let right = self.check_expr(*right)?;
                self.unify(left.ty(), right.ty())?;

                Ok(TypedExpr::Equal { left: Box::new(left), right: Box::new(right), ty: Ty::Bool, span: span })
            },
            RawExpr::LessThan { left, right, span } => {
                let left = self.check_expr(*left)?;
                let right = self.check_expr(*right)?;

                let ty = self.unify(left.ty(), right.ty())?;
                self.check_binary_operand_ty(&ty, span)?;

                Ok(TypedExpr::LessThan { left: Box::new(left), right: Box::new(right), ty: Ty::Bool, span: span })
            },
            RawExpr::LessThanOrEqual { left, right, span } => {
                let left = self.check_expr(*left)?;
                let right = self.check_expr(*right)?;

                let ty = self.unify(left.ty(), right.ty())?;
                self.check_binary_operand_ty(&ty, span)?;

                Ok(TypedExpr::LessThanOrEqual { left: Box::new(left), right: Box::new(right), ty: Ty::Bool, span: span })
            },
            RawExpr::GreaterThan { left, right, span } => {
                let left = self.check_expr(*left)?;
                let right = self.check_expr(*right)?;

                let ty = self.unify(left.ty(), right.ty())?;
                self.check_binary_operand_ty(&ty, span)?;

                Ok(TypedExpr::GreaterThan { left: Box::new(left), right: Box::new(right), ty: Ty::Bool, span: span })
            },
            RawExpr::GreaterThanOrEqual { left, right, span } => {
                let left = self.check_expr(*left)?;
                let right = self.check_expr(*right)?;

                let ty = self.unify(left.ty(), right.ty())?;
                self.check_binary_operand_ty(&ty, span)?;

                Ok(TypedExpr::GreaterThanOrEqual { left: Box::new(left), right: Box::new(right), ty: Ty::Bool, span: span })
            },
            RawExpr::LogicalAnd { left, right, span } => {
                let left = self.check_expr(*left)?;
                let right = self.check_expr(*right)?;

                if left.ty() != Ty::Bool || right.ty() != Ty::Bool {
                    return Err(TypeError::ExpectedBoolButFoundTwo { left: left.ty(), right: right.ty() })
                }

                Ok(TypedExpr::LogicalAnd { left: Box::new(left), right: Box::new(right), ty: Ty::Bool, span: span })
            },
            RawExpr::LogicalOr { left, right, span } => {
                let left = self.check_expr(*left)?;
                let right = self.check_expr(*right)?;

                if left.ty() != Ty::Bool || right.ty() != Ty::Bool {
                    return Err(TypeError::ExpectedBoolButFoundTwo { left: left.ty(), right: right.ty() })
                }

                Ok(TypedExpr::LogicalOr { left: Box::new(left), right: Box::new(right), ty: Ty::Bool, span: span })
            },
            RawExpr::LogicalNot { operand, span } => {
                let operand = self.check_expr(*operand)?;

                if operand.ty() != Ty::Bool {
                    return Err(TypeError::ExpectedBoolButFound { operand_ty: operand.ty() });
                }

                Ok(TypedExpr::LogicalNot { operand: Box::new(operand), ty: Ty::Bool, span: span })
            },
            RawExpr::Assign { target, value, span, original_op: _ } => {
                let target = self.check_expr(*target)?;
                let value = self.check_expr(*value)?;
                match &value {
                    TypedExpr::Ident { ty, .. } => {
                        self.unify(target.ty(), ty.clone())?;
                    }
                    _ => unreachable!()
                };

                Ok(TypedExpr::Equal { left: Box::new(target), right: Box::new(value), ty: Ty::Bool, span: span })
            },
            RawExpr::Negate { operand, span } => {
                let operand = self.check_expr(*operand)?;

                if operand.ty() != Ty::Bool {
                    return Err(TypeError::ExpectedBoolButFound { operand_ty: operand.ty() })
                }

                Ok(TypedExpr::Negate { operand: Box::new(operand), ty: Ty::Bool, span: span })
            },
            RawExpr::AddressOf { base, span } => {
                unimplemented!("还没做去地址")
            },
            RawExpr::Call { callee, args, span } => {
                let callee = self.check_expr(*callee)?;

                let mut typed_args = Vec::new();
                for arg in args {
                    match self.check_expr(arg) {
                        Ok(typed_expr) => typed_args.push(typed_expr),
                        Err(e) => return Err(e),
                    }
                }

                if let TypedExpr::Ident { name: _, def_id, ty, span } = callee {
                    match ty {
                        Ty::Fn { params, return_ty } => {
                            if typed_args.len() != params.len() {
                                return Err(TypeError::ArgumentLengthNotEqual { expected_length: params.len(), really_length: typed_args.len() });
                            }

                            for (t1, t2) in params.iter().zip(typed_args.iter()) {
                                self.unify(t1.clone(), t2.ty())?;
                            }

                            Ok(TypedExpr::Call { callee: def_id, args: typed_args, ty: *return_ty, span: span })
                        }
                        _ => {
                            Err(TypeError::ExpectedFunctionButFound { ty: ty, span: span })
                        }
                    }
                } else {
                    Err(TypeError::ExpectedFunctionButFound { ty: callee.ty(), span: callee.span() })
                }
            },
            RawExpr::Block { block } => {
                self.enter_scope();
                let typed_block = self.check_block(block)?;
                self.exit_scope();

                Ok(TypedExpr::Block { block: typed_block })
            },
            RawExpr::If { condition, then_branch, else_branch, span } => {
                let condition = self.check_expr(*condition)?;
                let then_branch = self.check_block(then_branch)?;
                let else_branch = match else_branch {
                    Some(else_branch) => {
                        Some(Box::new(self.check_expr(*else_branch)?))
                    }
                    None => None
                };

                let ty = if else_branch.is_some() {
                    self.unify(then_branch.ty.clone(), else_branch.clone().unwrap().ty())?
                } else {
                    then_branch.ty.clone()
                };

                Ok(TypedExpr::If { condition: Box::new(condition), then_branch: then_branch, else_branch: else_branch, ty: ty, span: span })
            },
            RawExpr::Loop { body, span } => {
                let body = self.check_block(*body)?;

                let ty = self.check_loop_body_ty(&body)?;

                Ok(TypedExpr::Loop { body: body, ty: ty, span: span })
            },
            RawExpr::FieldAccess { base, field, span } => {
                let base = self.check_expr(*base)?;

                let id = match base {
                    TypedExpr::Ident { ref ty, .. } => {
                        if let Ty::Adt(id) = ty {
                            let adt = self.get_global().get_symbol(&id).unwrap();

                            match adt {
                                Symbol::Struct { fields, .. } => {
                                    if let Some((_, id, field_def_id)) = fields.iter().find(|&(s, _, _)| *s == field) {
                                        (*id, field_def_id.clone()) // 找到字段，返回字段的 DefId
                                    } else {
                                        // 字段不存在，返回错误
                                        return Err(TypeError::UndefineField {
                                            base: get_global_string(adt.name()).unwrap().to_string(),
                                            field: get_global_string(field).unwrap().to_string(),
                                        });
                                    }
                                }
                                _ => unreachable!()
                            }
                        } else {
                            unreachable!()
                        }
                    }
                    _ => unreachable!()
                };

                Ok(TypedExpr::FieldAccess { base: Box::new(base), field: field, def_id: id.0, ty: id.1, span: span })
            }
            RawExpr::PathAccess { segments, span } => {
                let id = match self.resolver.resolve_path(&segments, self.current_mod_id) {
                    Some(id) => id,
                    None => {
                        return Err(TypeError::UndefinePath { 
                            path: segments.into_iter().map(|id| get_global_string(id).unwrap().to_string()).collect()
                        });
                    }
                };

                let symbol = self.get_global().get_symbol(&id).unwrap();

                Ok(TypedExpr::PathAccess { def_id: id, ty: symbol.ty(), span: span })
            },
            RawExpr::Grouped { expr, .. } => {
                let expr = self.check_expr(*expr)?;

                Ok(expr)
            },
        }
    }

    fn collect_breaks(&mut self, body: &TypedBlock) -> CheckResult<Vec<TypedStmt>> {
        let mut breaks = Vec::new();

        for stmt in &body.stmts {
            match stmt {
                TypedStmt::Break { .. } => {
                    breaks.push(stmt.clone());
                }
                TypedStmt::Expr(expr) => match *expr.clone() {
                    TypedExpr::If { then_branch, else_branch, .. } => {
                        breaks.extend(self.collect_breaks(&then_branch)?);
                        let mut else_branch_ = else_branch;

                        loop {
                            match else_branch_ {
                                Some(expr) => match *expr {
                                    TypedExpr::If { then_branch, else_branch, .. } => {
                                        breaks.extend(self.collect_breaks(&then_branch)?);
                                        else_branch_ = else_branch;
                                    }
                                    _ => unreachable!()
                                }
                                None => break
                            }
                        }
                    }
                    TypedExpr::Loop { body, .. } => {
                        breaks.extend(self.collect_breaks(&body)?);
                    }
                    TypedExpr::Block { block } => {
                        breaks.extend(self.collect_breaks(&block)?);
                    }
                    _ => {}
                }
                _ => {}
            }
        }

        Ok(breaks)
    }

    fn check_loop_body_ty(&mut self, body: &TypedBlock) -> CheckResult<Ty> {
        let breaks = self.collect_breaks(body)?;

        let mut ty = Ty::Never;

        for break_ in breaks {
            match break_ {
                TypedStmt::Break { value, .. } => {
                    let break_ty = match value {
                        Some(value) => value.ty(),
                        None => Ty::Unit
                    };

                    ty = self.unify(ty, break_ty)?;
                }
                _ => unreachable!()
            }
        }

        Ok(ty)
    }

    fn check_binary(&mut self, left: RawExpr, right: RawExpr, span: Span) -> CheckResult<(TypedExpr, TypedExpr, Ty)> {
        let left = self.check_expr(left)?;
        let right = self.check_expr(right)?;

        let ty = self.unify(left.ty().clone(), right.ty().clone())?;
        self.check_binary_operand_ty(&ty, span)?;

        Ok((left, right, ty))
    }

    fn check_binary_operand_ty(&self, ty: &Ty, span: Span) -> CheckResult<()>{
        match ty {
            Ty::Int(_) | Ty::Float(_) => {
                Ok(())
            }
            _ => {
                Err(TypeError::BinaryOperandError { ty: ty.clone(), span: span })
            }
        }
    }

    fn check_param(&mut self, raw_param: RawParam) -> CheckResult<TypedParam> {
        let ty = self.get_type(raw_param.ty)?;
        if self.get_ident(raw_param.name).is_some(){
            return Err(TypeError::RedefineVariable { name: get_global_string(raw_param.name).unwrap().to_string(), span: raw_param.span })
        }
        let id = self.alloc_id(DefKind::Variable);
        self.scope.insert_symbol(id, Symbol::Variable { name: raw_param.name, ty: ty.clone(), span: raw_param.span });

        Ok(TypedParam {
            name: raw_param.name,
            def_id: id,
            ty: ty,
            span: raw_param.span
        })
    }

    fn get_type(&mut self, ty: RawType) -> CheckResult<Ty> {
        match ty {
            RawType::Named { name, span } => {
                if let Some(builtin_type) = self.resolve_builtin_type(name) {
                    return Ok(builtin_type);
                }
                // let scope = self.current_scope();

                if let Some(id) = self.resolver.resolve_path(&[name], self.current_mod_id) {
                    match &id.kind {
                        DefKind::Struct => Ok(Ty::Adt(id)),
                        DefKind::Function => {
                            let symbol = self.get_global().get_symbol(&id).unwrap();
                            Err(TypeError::ExpectedTypeButFoundFunction { name: get_global_string(symbol.name()).unwrap().to_string(), span: span })                            
                        }
                        DefKind::Variable => {
                            let symbol = self.get_global().get_symbol(&id).unwrap();
                            Err(TypeError::ExpectedTypeButFoundVariable { name: get_global_string(symbol.name()).unwrap().to_string(), span: span })                            
                        }
                        _ => {
                            Err(TypeError::UnknowTypeAsType { kind: id.kind, span: span })
                        }
                    }
                } else {
                    Err(TypeError::UndefineSymbol { name: get_global_string(name).unwrap().to_string(), span: span })
                }
            },
        }
    }

    fn resolve_builtin_type(&self, name: StringId) -> Option<Ty> {
        match get_global_string(name) {
            Some(str) => match str.as_ref() {
                "isize" => Some(Ty::Int(IntKind::Isize)),
                "i8" => Some(Ty::Int(IntKind::I8)),
                "i16" => Some(Ty::Int(IntKind::I16)),
                "i32" => Some(Ty::Int(IntKind::I32)),
                "i64" => Some(Ty::Int(IntKind::I64)),
                "128" => Some(Ty::Int(IntKind::I128)),
                "f32" => Some(Ty::Float(FloatKind::F32)),
                "f64" => Some(Ty::Float(FloatKind::F64)),
                "bool" => Some(Ty::Bool),
                "str" => Some(Ty::Str),
                _ => None,
            },
            None => None
        }
    }

    fn enter_scope(&mut self) {
        // 使用 std::mem::take 获取当前作用域，并将其包装在 Box 中
        let current_scope = Box::new(std::mem::take(&mut self.scope));
        // 创建一个新的作用域，将当前作用域作为父作用域
        let new_scope = Scope::new(Some(current_scope));
        // 将新的作用域设置为当前作用域
        self.scope = new_scope;
    }

    fn exit_scope(&mut self) {
        if let Some(parent) = self.scope.take_parent() {
            self.scope = *parent;
        }
    }

    fn alloc_id(&mut self, kind: DefKind) -> DefId {
        let id = DefId::new(self.next_id, kind);
        self.next_id += 1;
        id
    }

    fn unify(&mut self, t1: Ty, t2: Ty) -> CheckResult<Ty> {
        match (t1, t2) {
            (Ty::Int(k1), Ty::Int(k2)) if k1 != IntKind::Unknown && k2 != IntKind::Unknown && k1 == k2 => {
                Ok(Ty::Int(k1))
            },
            (Ty::Int(k1), Ty::Int(k2)) if k1 != IntKind::Unknown && k2 == IntKind::Unknown => {
                Ok(Ty::Int(k1))
            },
            (Ty::Int(k1), Ty::Int(k2)) if k1 == IntKind::Unknown && k2 != IntKind::Unknown => {
                Ok(Ty::Int(k2))
            },
            (Ty::Int(k1), Ty::Int(k2)) if k1 == IntKind::Unknown && k2 == IntKind::Unknown => {
                Ok(Ty::Int(IntKind::I32))
            },

            (Ty::Float(k1), Ty::Float(k2)) if k1 != FloatKind::Unknow && k2 != FloatKind::Unknow && k1 == k2 => {
                Ok(Ty::Float(k1))
            },
            (Ty::Float(k1), Ty::Float(k2)) if k1 != FloatKind::Unknow && k2 == FloatKind::Unknow => {
                Ok(Ty::Float(k1))
            },
            (Ty::Float(k1), Ty::Float(k2)) if k1 == FloatKind::Unknow && k2 != FloatKind::Unknow => {
                Ok(Ty::Float(k2))
            },
            (Ty::Float(k1), Ty::Float(k2)) if k1 == FloatKind::Unknow && k2 == FloatKind::Unknow => {
                Ok(Ty::Float(FloatKind::F32))
            },

            (Ty::Unknown, other) if other != Ty::Unknown => {
                Ok(other)
            }

            (other, Ty::Unknown) if other != Ty::Unknown => {
                Ok(other)
            }

            (Ty::Unit, Ty::Unit) => Ok(Ty::Unit),
            
            (Ty::Str, Ty::Str) => Ok(Ty::Str),

            (t1, t2) => Err(TypeError::TypeMismatch { t1: t1, t2: t2 })
        }
    }

    fn get_global(&self) -> &Scope {
        self.scope.get_global()
    }

    fn get_ident(&self, name: StringId) -> Option<&DefId> {
        self.scope.get_id(&name)
    }

    fn get_symbol(&self, id: DefId) -> Option<&Symbol> {
        self.scope.get_symbol(&id)
    }

    fn inject_resolver_toplevel(&mut self) {
        for (name_id, def_id, kind) in self.resolver.current_toplevel(self.current_mod_id) {
            match kind {
                DefKind::Function => {
                    self.scope.insert_symbol(
                        def_id,
                        Symbol::Function {
                            name: name_id,
                            params_type: vec![],
                            ret_type: Ty::Unknown,
                            span: Span::dummy(),
                        },
                    );
                }
                DefKind::Struct => {
                    self.scope.insert_symbol(
                        def_id,
                        Symbol::Struct {
                            name: name_id,
                            fields: vec![],
                            span: Span::dummy(),
                        },
                    );
                }
                _ => {}
            }
        }
    }

    /// 预计算当前模块所有函数的**签名**（参数 + 返回），并替换空壳
    fn precompute_function_sigs(&mut self, raw: &RawCrate) {
        for item in &raw.items {
            if let RawItem::Function { name, params, return_type, span, .. } = item {
                let def_id = self.resolver.resolve_path(&[*name], self.current_mod_id).unwrap();

                // 计算参数类型
                let param_tys = params
                    .iter()
                    .map(|p| self.get_type(p.ty.clone()).unwrap_or(Ty::Unknown))
                    .collect::<Vec<_>>();

                // 计算返回类型
                let ret_ty = return_type
                    .as_ref()
                    .map(|t| self.get_type(t.clone()).unwrap_or(Ty::Unknown))
                    .unwrap_or(Ty::Unit);

                // 替换空壳
                let full = Symbol::Function {
                    name: *name,
                    params_type: param_tys,
                    ret_type: ret_ty,
                    span: *span,
                };
                self.scope.replace_symbol(def_id, full);
            }
        }
    }
}

pub fn check(raw_crate: RawCrate, path: PathBuf) -> Result<TypedCrate, Vec<TypeError>> {
    let mut type_checker = TypeChecker::new(path);
    type_checker.check_crate(raw_crate)
}

#[inline]
fn lower_visibility(raw_visibility: RawVisibility) -> Visibility {
    match raw_visibility {
        RawVisibility::Public => Visibility::Public,
        RawVisibility::Private => Visibility::Private,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use litec_hir::{
        Crate as RawCrate, Item as RawItem, Visibility as RawVisibility, Param as RawParam, 
        Type as RawType, Expr as RawExpr, LiteralValue, LitIntValue, Stmt as RawStmt, Block as RawBlock
    };

    #[test]
    fn test_function_definition_and_call() {
        let path = PathBuf::from("test");
        let mut type_checker = TypeChecker::new(path);

        let raw_crate = RawCrate {
            items: vec![
                RawItem::Function {
                    visibility: RawVisibility::Public,
                    name: StringId::from("add"),
                    params: vec![
                        RawParam {
                            name: StringId::from("a"),
                            ty: RawType::Named { name: StringId::from("i32"), span: Span::default() },
                            span: Span::default()
                        },
                        RawParam {
                            name: StringId::from("b"),
                            ty: RawType::Named { name: StringId::from("i32"), span: Span::default() },
                            span: Span::default()
                        }
                    ],
                    return_type: Some(RawType::Named { name: StringId::from("i32"), span: Span::default() }),
                    body: RawBlock {
                        stmts: vec![],
                        tail: Some(Box::new(RawExpr::Addition {
                            left: Box::new(RawExpr::Ident { name: StringId::from("a"), span: Span::default() }),
                            right: Box::new(RawExpr::Ident { name: StringId::from("b"), span: Span::default() }),
                            span: Span::default()
                        })),
                        span: Span::default()
                    },
                    span: Span::default()
                },
                RawItem::Function {
                    visibility: RawVisibility::Public,
                    name: StringId::from("main"),
                    params: vec![],
                    return_type: Some(RawType::Named { name: StringId::from("i32"), span: Span::default() }),
                    body: RawBlock {
                        stmts: vec![],
                        tail: Some(Box::new(RawExpr::Call {
                            callee: Box::new(RawExpr::Ident { name: StringId::from("add"), span: Span::default() }),
                            args: vec![
                                RawExpr::Literal {
                                    value: LiteralValue::Int { kind: LitIntValue::I32, value: 10 },
                                    span: Span::default()
                                },
                                RawExpr::Literal {
                                    value: LiteralValue::Int { kind: LitIntValue::I32, value: 20 },
                                    span: Span::default()
                                }
                            ],
                            span: Span::default()
                        })),
                        span: Span::default()
                    },
                    span: Span::default()
                }
            ]
        };

        let result = type_checker.check_crate(raw_crate);
        if let Err(errs) = &result {
            for e in errs {
                println!("❌ {:?}", e);
            }
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_scope_management() {
        let path = PathBuf::from("test");
        let mut type_checker = TypeChecker::new(path);

        let raw_crate = RawCrate {
            items: vec![
                RawItem::Function {
                    visibility: RawVisibility::Public,
                    name: StringId::from("test_function"),
                    params: vec![],
                    return_type: Some(RawType::Named { name: StringId::from("i32"), span: Span::default() }),
                    body: RawBlock {
                        stmts: vec![
                            RawStmt::Let {
                                name: StringId::from("x"),
                                ty: Some(RawType::Named { name: StringId::from("i32"), span: Span::default() }),
                                value: Some(Box::new(RawExpr::Literal {
                                    value: LiteralValue::Int { kind: LitIntValue::I32, value: 10 },
                                    span: Span::default()
                                })),
                                span: Span::default()
                            },
                            RawStmt::Expr(
                                Box::new(RawExpr::Block {
                                    block: RawBlock {
                                        stmts: vec![
                                            RawStmt::Let {
                                                name: StringId::from("x"),
                                                ty: Some(RawType::Named { name: StringId::from("i32"), span: Span::default() }),
                                                value: Some(Box::new(RawExpr::Literal {
                                                    value: LiteralValue::Int { kind: LitIntValue::I32, value: 20 },
                                                    span: Span::default()
                                                })),
                                                span: Span::default()
                                            }
                                        ],
                                        tail: None,
                                        span: Span::default()
                                    },
                                })
                            )
                        ],
                        tail: Some(Box::new(RawExpr::Ident { name: StringId::from("x"), span: Span::default() })),
                        span: Span::default()
                    },
                    span: Span::default()
                }
            ]
        };

        let result = type_checker.check_crate(raw_crate);
        if let Err(errs) = &result {
            for e in errs {
                println!("❌ {:?}", e);
            }
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_let_type_inference() {
        let path = PathBuf::from("test");
        let mut type_checker = TypeChecker::new(path);

        let raw_crate = RawCrate {
            items: vec![
                RawItem::Function {
                    visibility: RawVisibility::Public,
                    name: StringId::from("test_function"),
                    params: vec![],
                    return_type: Some(RawType::Named { name: StringId::from("i32"), span: Span::default() }),
                    body: RawBlock {
                        stmts: vec![
                            RawStmt::Let {
                                name: StringId::from("x"),
                                ty: None, // 未显式指定类型
                                value: Some(Box::new(RawExpr::Literal {
                                    value: LiteralValue::Int { kind: LitIntValue::I32, value: 10 },
                                    span: Span::default()
                                })),
                                span: Span::default()
                            },
                            RawStmt::Let {
                                name: StringId::from("y"),
                                ty: Some(RawType::Named { name: StringId::from("i32"), span: Span::default() }),
                                value: Some(Box::new(RawExpr::Literal {
                                    value: LiteralValue::Int { kind: LitIntValue::I32, value: 20 },
                                    span: Span::default()
                                })),
                                span: Span::default()
                            },
                            RawStmt::Let {
                                name: StringId::from("z"),
                                ty: Some(RawType::Named { name: StringId::from("i32"), span: Span::default() }),
                                value: Some(Box::new(RawExpr::Literal {
                                    value: LiteralValue::Str("hello".into()),
                                    span: Span::default()
                                })),
                                span: Span::default()
                            }
                        ],
                        tail: Some(Box::new(RawExpr::Literal {
                            value: LiteralValue::Int { kind: LitIntValue::I32, value: 10 },
                            span: Span::default()
                        })),
                        span: Span::default()
                    },
                    span: Span::default()
                }
            ]
        };

        let result = type_checker.check_crate(raw_crate);
        if let Err(errs) = &result {
            for e in errs {
                println!("❌ {:?}", e);
            }
        }
        assert!(result.is_err()); // 因为 z 的类型不匹配，期望失败
    }
}