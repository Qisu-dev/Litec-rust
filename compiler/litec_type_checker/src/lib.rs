mod ty_ctxt;

use std::cmp::Ordering;
use std::rc::Rc;

use litec_error::{error, Diagnostic};
use litec_hir::{AbiType, AssignOp, FloatKind, IntKind, LiteralIntKind, UIntKind};
use litec_name_resolver::rhir::{
    LiteralValue, RBlock, RExpr, RExternItem, RItem, RStmt, Visibility as RVisibility,
};
use litec_name_resolver::{rhir, ResolveOutput};
use litec_span::{get_global_string, Span};
use litec_typed_hir::builtins::BuiltinFunction;
use litec_typed_hir::def_id::DefId;
use litec_typed_hir::ty::Ty;
use litec_typed_hir::*;
use ty_ctxt::TypeCtxt;

use crate::ty_ctxt::VarState;

macro_rules! check_builtin_type {
    ($def_id:expr, $builtin_types:expr, { $($name:ident => $ty:expr),* $(,)? }) => {
        $(
            if $def_id == $builtin_types.$name {
                return Some($ty);
            }
        )*
    };
}

pub struct TypeChecker {
    env: TypeCtxt,
    diagnostics: Vec<Diagnostic>,
    resolve_output: ResolveOutput,

    function_return_tys: Vec<Ty>,
    loop_break_tys: Vec<Ty>,
}

impl TypeChecker {
    pub fn new(mut resolve_output: ResolveOutput) -> Self {
        let diagnostics = std::mem::take(&mut resolve_output.diagnostics);
        Self {
            env: TypeCtxt::new(),
            diagnostics: diagnostics,
            resolve_output: resolve_output,
            function_return_tys: Vec::new(),
            loop_break_tys: Vec::new(),
        }
    }

    pub fn check_crate(mut self) -> (TypedCrate, Vec<Diagnostic>) {
        let item_refs: Vec<_> = std::mem::take(&mut self.resolve_output.rhir.items);
        let mut typed_items = Vec::new();

        for item in item_refs {
            if let Some(typed_item) = self.check_item(&item) {
                typed_items.push(typed_item);
            }
        }

        (
            TypedCrate {
                items: typed_items,
                builtin: self.resolve_output.builtin,
                definitions: self
                    .resolve_output
                    .definitions
                    .into_iter()
                    .map(|definition| Definition {
                        def_id: definition.def_id,
                        name: definition.name,
                        span: definition.span,
                    })
                    .collect(),
            },
            self.diagnostics,
        )
    }
    fn check_item(&mut self, ritem: &RItem) -> Option<TypedItem> {
        match ritem {
            RItem::Function {
                def_id,
                visibility,
                name,
                params,
                return_type,
                body,
                span,
            } => {
                let mut typed_params = Vec::new();
                for param in params {
                    let param_ty = self.check_type(&param.ty)?;
                    self.env.insert(param.def_id, param_ty.clone());
                    typed_params.push(TypedParam {
                        name: param.name,
                        def_id: param.def_id,
                        ty: param_ty,
                        span: param.span,
                    });
                }

                let return_ty = if get_global_string(*name).unwrap() == "main".into() {
                    match return_type {
                        Some(ty) => match self.check_type(ty)? {
                            Ty::Int(IntKind::I32) => Ty::Int(IntKind::I32),
                            _ => {
                                self.diagnostics
                                    .push(error("main函数必须返回i32").with_span(*span).build());
                                return None;
                            }
                        },
                        None => Ty::Int(IntKind::I32),
                    }
                } else {
                    match return_type {
                        Some(ty) => self.check_type(ty)?,
                        None => Ty::Unit,
                    }
                };

                self.function_return_tys.push(return_ty.clone());

                let fn_ty = Ty::Fn {
                    params: typed_params.iter().map(|p| p.ty.clone()).collect(),
                    return_ty: Box::new(return_ty.clone()),
                };

                self.env.insert(*def_id, fn_ty);

                let typed_body = self.check_block(body)?;
                self.function_return_tys.pop();

                Some(TypedItem::Function {
                    def_id: *def_id,
                    visibility: *visibility,
                    name: *name,
                    params: typed_params,
                    return_ty,
                    body: typed_body,
                    span: *span,
                })
            }
            RItem::Struct {
                def_id,
                visibility,
                name,
                fields,
                span,
            } => {
                let mut typed_fields = Vec::new();
                for field in fields {
                    let field_ty = self.check_type(&field.ty)?;
                    self.env.insert(field.def_id, field_ty.clone());

                    // 创建 TypedField 对象
                    typed_fields.push(TypedField {
                        name: field.name,
                        def_id: field.def_id,
                        ty: field_ty,
                        visibility: field.visibility,
                        index: field.index,
                        span: field.span,
                    });
                }

                let struct_ty = Ty::Adt(*def_id);
                self.env.insert(*def_id, struct_ty.clone());

                Some(TypedItem::Struct {
                    def_id: *def_id,
                    visibility: *visibility,
                    name: *name,
                    fields: typed_fields, // 使用 TypedField 类型
                    span: *span,
                })
            }
            RItem::Use {
                visibility,
                alias,
                target,
                span,
            } => Some(TypedItem::Use {
                visibility: *visibility,
                alias: *alias,
                target: *target,
                span: *span,
            }),

            RItem::Extern {
                visibility,
                abi,
                items,
                span,
            } => {
                // 对于 extern 块中的每个项，我们需要进行类型检查
                let mut typed_items = Vec::new();
                for item in items {
                    if let Some(typed_item) = self.check_extern_item(item) {
                        match abi {
                            AbiType::Lite => match &typed_item {
                                TypedExternItem::Function {
                                    def_id,
                                    name,
                                    params,
                                    is_variadic,
                                    return_ty,
                                    span,
                                } => {
                                    self.resolve_output.builtin.functions.push(BuiltinFunction {
                                        name: *name,
                                        def_id: *def_id,
                                        params: params.clone(),
                                        is_variadic: *is_variadic,
                                        ret: return_ty.clone(),
                                        span: *span,
                                    });
                                }
                            },
                            _ => {}
                        }
                        typed_items.push(typed_item);
                    }
                }

                Some(TypedItem::Extern {
                    visibility: *visibility,
                    abi: *abi,
                    items: typed_items,
                    span: *span,
                })
            }

            RItem::Module {
                def_id,
                visibility,
                name,
                items,
                span,
            } => {
                let mut typed_items = Vec::new();
                for item in items {
                    if let Some(typed_item) = self.check_item(item) {
                        typed_items.push(typed_item);
                    }
                }

                Some(TypedItem::Module {
                    def_id: *def_id,
                    visibility: *visibility,
                    name: *name,
                    items: typed_items,
                    span: *span,
                })
            }
        }
    }

    fn check_extern_item(&mut self, rextern_item: &RExternItem) -> Option<TypedExternItem> {
        // 检查参数类型
        match rextern_item {
            RExternItem::Function {
                def_id,
                name,
                is_variadic,
                params,
                return_type,
                span,
            } => {
                let mut typed_params = Vec::new();
                for param in params {
                    let param_ty = self.check_type(&param.ty)?;
                    typed_params.push(TypedParam {
                        name: param.name,
                        def_id: param.def_id,
                        ty: param_ty,
                        span: param.span,
                    });
                }

                // 检查返回类型
                let return_ty = match return_type {
                    Some(ty) => self.check_type(ty)?,
                    None => Ty::Unit,
                };

                // 创建函数类型
                let fn_ty = Ty::ExternFn {
                    is_varidic: *is_variadic,
                    params: typed_params.iter().map(|p| p.ty.clone()).collect(),
                    return_ty: Box::new(return_ty.clone()),
                };

                // 注册到类型环境
                self.env.insert(*def_id, fn_ty);

                Some(TypedExternItem::Function {
                    def_id: *def_id,
                    name: *name,
                    params: typed_params,
                    is_variadic: *is_variadic,
                    return_ty,
                    span: *span,
                })
            }
        }
    }

    fn check_block(&mut self, rblock: &RBlock) -> Option<TypedBlock> {
        let mut stmts = Vec::new();
        for stmt in &rblock.stmts {
            if let Some(typed_stmt) = self.check_stmt(stmt) {
                stmts.push(typed_stmt);
            }
        }

        let tail = match &rblock.tail {
            Some(expr) => {
                let typed_expr = self.check_expr(expr)?;
                Some(Box::new(typed_expr))
            }
            None => None,
        };

        Some(TypedBlock {
            stmts,
            tail: tail.clone(),
            ty: tail.as_ref().map(|e| e.ty()).unwrap_or(Ty::Unit),
            span: rblock.span,
        })
    }

    fn check_stmt(&mut self, rstmt: &RStmt) -> Option<TypedStmt> {
        match rstmt {
            RStmt::Expr(expr) => {
                let typed_expr = self.check_expr(expr)?;
                Some(TypedStmt::Expr(Box::new(typed_expr)))
            }
            RStmt::Let {
                mutable,
                name,
                def_id,
                ty,
                value,
                span,
            } => {
                let (init, actual_ty) = match value {
                    Some(init_expr) => {
                        let typed_init = self.check_expr(init_expr)?;
                        if let Some(def_id) = self.get_def_id(&typed_init) {
                            if self.env.get_var_state(def_id) == Some(&VarState::Moved) {
                                self.diagnostics.push(
                                    error("变量已经被移动")
                                        .with_span(*span)
                                        .with_label(*span, "变量已经被移动，不能再使用")
                                        .build(),
                                );
                                return None;
                            } else {
                                if !typed_init.ty().is_copyable() {
                                    self.env.set_var_state(def_id, VarState::Moved);
                                }
                            }
                        }
                        let init_ty = typed_init.ty();
                        let mut actual_ty = init_ty;

                        // 如果声明了类型，检查是否匹配
                        if let Some(decl_ty) = ty {
                            let expected_ty = self.check_type(decl_ty)?;
                            match self.unify(&actual_ty, &expected_ty) {
                                Some(unified_ty) => actual_ty = unified_ty,
                                None => {
                                    self.diagnostics.push(type_mismatch(
                                        &expected_ty,
                                        &actual_ty,
                                        *span,
                                    ));
                                    return None;
                                }
                            }
                        }

                        self.env.insert(*def_id, actual_ty.clone());
                        (Some(Box::new(typed_init)), actual_ty)
                    }
                    None => {
                        // 没有初始化表达式，需要有类型声明
                        if let Some(decl_ty) = ty {
                            let declared_ty = self.check_type(decl_ty)?;
                            self.env.insert(*def_id, declared_ty.clone());
                            self.env.set_var_state(*def_id, VarState::UnInitialized);
                            (None, declared_ty)
                        } else {
                            // 错误：没有类型声明也没有初始化表达式
                            self.diagnostics.push(
                                error("缺少类型声明")
                                    .with_span(*span)
                                    .with_label(*span, "变量需要类型声明或初始化表达式")
                                    .build(),
                            );
                            return None;
                        }
                    }
                };

                Some(TypedStmt::Let {
                    mutable: *mutable,
                    name: *name,
                    def_id: *def_id,
                    ty: actual_ty,
                    init,
                    span: *span,
                })
            }
            RStmt::Return { value, span } => {
                let (typed_value, ty) = match value {
                    Some(expr) => {
                        let typed_expr = self.check_expr(expr)?;
                        let ty = typed_expr.ty();
                        (Some(Box::new(typed_expr)), ty)
                    }
                    None => (None, Ty::Unit),
                };

                let current_function_return_ty = self.function_return_tys.last().cloned().unwrap();

                if self.unify(&current_function_return_ty, &ty).is_none() {
                    self.diagnostics
                        .push(type_mismatch(&current_function_return_ty, &ty, *span));
                    return None;
                }

                Some(TypedStmt::Return {
                    value: typed_value,
                    span: *span,
                })
            }
            RStmt::Break { value, span } => {
                let typed_value = match value {
                    Some(expr) => {
                        let typed_expr = self.check_expr(expr)?;
                        Some(Box::new(typed_expr))
                    }
                    None => None,
                };

                let ty = typed_value.as_ref().map(|e| e.ty()).unwrap_or(Ty::Unit);
                self.loop_break_tys.push(ty.clone());

                Some(TypedStmt::Break {
                    value: typed_value,
                    ty: ty,
                    span: *span,
                })
            }
            RStmt::Continue { span } => Some(TypedStmt::Continue {
                ty: Ty::Unit,
                span: *span,
            }),
        }
    }

    fn check_expr(&mut self, rexpr: &RExpr) -> Option<TypedExpr> {
        match rexpr {
            RExpr::Literal { value, span } => {
                let ty = match value {
                    LiteralValue::Int { kind, .. } => match kind {
                        LiteralIntKind::Signed(int_kind) => Ty::Int(*int_kind),
                        LiteralIntKind::Unsigned(uint_kind) => Ty::UInt(*uint_kind),
                    },
                    LiteralValue::Float { value: _, kind } => Ty::Float(*kind),
                    LiteralValue::Bool(_) => Ty::Bool,
                    LiteralValue::Str(_) => Ty::Str,
                    LiteralValue::Char(_) => Ty::Char,
                    LiteralValue::Unit => Ty::Unit,
                };

                Some(TypedExpr::Literal {
                    value: value.clone(),
                    ty,
                    span: *span,
                })
            }
            RExpr::Local { def_id, span } => {
                match self.env.get_var_state(*def_id) {
                    Some(VarState::Moved) => {
                        self.diagnostics
                            .push(error(format!("使用已移动的变量")).with_span(*span).build());
                        return None;
                    }
                    Some(VarState::UnInitialized) => {
                        self.diagnostics.push(
                            error(format!("使用未初始化的变量"))
                                .with_span(*span)
                                .build(),
                        );
                        return None;
                    }
                    _ => {}
                }

                match self.env.get(*def_id) {
                    Some(ty) => Some(TypedExpr::Local {
                        def_id: *def_id,
                        ty: ty.clone(),
                        span: *span,
                    }),
                    None => {
                        // 错误：未定义的变量
                        self.diagnostics.push(undefined_variable(
                            &self.get_definition_name(*def_id),
                            *span,
                        ));
                        return None;
                    }
                }
            }
            RExpr::Global { def_id, span } => {
                if def_id.kind == DefKind::Ghost {
                    self.diagnostics
                        .push(error("不能在表达式中使用幽灵变量").with_span(*span).build());
                    return None;
                }

                match self.env.get(*def_id) {
                    Some(ty) => Some(TypedExpr::Global {
                        def_id: *def_id,
                        ty: ty.clone(),
                        span: *span,
                    }),
                    None => {
                        // 错误：未定义的全局变量
                        self.diagnostics.push(undefined_variable(
                            &self.get_definition_name(*def_id),
                            *span,
                        ));
                        None
                    }
                }
            }
            RExpr::Binary {
                left,
                op,
                right,
                span,
            } => {
                let typed_left = self.check_expr(left)?;
                let typed_right = self.check_expr(right)?;

                let left_ty = typed_left.ty();
                let right_ty = typed_right.ty();

                // 类型检查
                let result_ty = match op {
                    rhir::BinOp::Add
                    | rhir::BinOp::Sub
                    | rhir::BinOp::Mul
                    | rhir::BinOp::Div
                    | rhir::BinOp::Rem => {
                        // 算术运算：要求两边类型相同，且为数值类型
                        if !left_ty.is_numeric() || !right_ty.is_numeric() {
                            self.diagnostics.push(
                                error("算术运算需要数值类型")
                                    .with_span(*span)
                                    .with_label(
                                        *span,
                                        format!(
                                            "操作数类型 {:?} 和 {:?} 不支持算术运算",
                                            left_ty, right_ty
                                        ),
                                    )
                                    .build(),
                            );
                        } else if self.unify(&left_ty, &right_ty).is_none() {
                            self.diagnostics
                                .push(type_mismatch(&left_ty, &right_ty, *span));
                        }

                        // 返回左侧类型或统一的数值类型
                        left_ty
                    }
                    rhir::BinOp::Eq
                    | rhir::BinOp::Ne
                    | rhir::BinOp::Lt
                    | rhir::BinOp::Le
                    | rhir::BinOp::Gt
                    | rhir::BinOp::Ge => {
                        // 比较运算：要求两边类型相同，返回bool
                        if self.unify(&left_ty, &right_ty).is_none() {
                            self.diagnostics
                                .push(type_mismatch(&left_ty, &right_ty, *span));
                        }

                        Ty::Bool
                    }
                    rhir::BinOp::And | rhir::BinOp::Or => {
                        // 逻辑运算：要求两边都是bool类型
                        if left_ty != Ty::Bool || right_ty != Ty::Bool {
                            self.diagnostics.push(
                                error("逻辑运算需要bool类型")
                                    .with_span(*span)
                                    .with_label(
                                        *span,
                                        format!(
                                            "操作数类型 {:?} 和 {:?} 不支持逻辑运算",
                                            left_ty, right_ty
                                        ),
                                    )
                                    .build(),
                            );
                        }

                        Ty::Bool
                    }
                    _ => Ty::Unknown,
                };

                Some(TypedExpr::Binary {
                    left: Box::new(typed_left),
                    op: *op,
                    right: Box::new(typed_right),
                    ty: result_ty,
                    span: *span,
                })
            }
            RExpr::Unary { op, operand, span } => {
                let typed_operand = self.check_expr(operand)?;
                let operand_ty = typed_operand.ty();

                let result_ty = match op {
                    rhir::UnOp::Neg => {
                        if !operand_ty.is_numeric() {
                            self.diagnostics.push(
                                error("负号运算需要数值类型")
                                    .with_span(*span)
                                    .with_label(
                                        *span,
                                        format!("操作数类型 {:?} 不支持负号运算", operand_ty),
                                    )
                                    .build(),
                            );
                        }
                        operand_ty
                    }
                    rhir::UnOp::Not => {
                        if operand_ty != Ty::Bool {
                            self.diagnostics.push(
                                error("逻辑非运算需要bool类型")
                                    .with_span(*span)
                                    .with_label(
                                        *span,
                                        format!("操作数类型 {:?} 不支持逻辑非运算", operand_ty),
                                    )
                                    .build(),
                            );
                        }
                        Ty::Bool
                    }
                };

                Some(TypedExpr::Unary {
                    op: *op,
                    operand: Box::new(typed_operand),
                    ty: result_ty,
                    span: *span,
                })
            }
            RExpr::Call { callee, args, span } => {
                let fn_ty = match self.env.get(*callee) {
                    Some(ty) => ty.clone(),
                    None => {
                        self.diagnostics.push(
                            error(format!(
                                "未知的函数 `{}`",
                                self.get_definition_name(*callee)
                            ))
                            .with_span(*span)
                            .build(),
                        );
                        return None;
                    }
                };
                let (params_ty, return_ty) = match fn_ty {
                    Ty::Fn { params, return_ty } => {
                        if args.len() != params.len() {
                            self.diagnostics.push(
                                error("函数调用参数数量不匹配")
                                    .with_span(*span)
                                    .with_label(
                                        *span,
                                        format!(
                                            "函数期望 {} 个参数，但实际提供了 {} 个参数",
                                            params.len(),
                                            args.len()
                                        ),
                                    )
                                    .build(),
                            );
                            return None;
                        }
                        (params, return_ty)
                    }
                    Ty::ExternFn {
                        is_varidic,
                        params,
                        return_ty,
                    } => {
                        if args.len() != params.len() && !is_varidic {
                            self.diagnostics.push(
                                error("函数调用参数数量不匹配")
                                    .with_span(*span)
                                    .with_label(
                                        *span,
                                        format!(
                                            "函数期望 {} 个参数，但实际提供了 {} 个参数",
                                            params.len(),
                                            args.len()
                                        ),
                                    )
                                    .build(),
                            );
                            return None;
                        }
                        (params, return_ty)
                    }
                    ty => {
                        self.diagnostics.push(
                            error("函数调用需要函数类型")
                                .with_span(*span)
                                .with_label(
                                    *span,
                                    format!("调用类型为 {:?}", ty),
                                )
                                .build(),
                        );

                        return None;
                    }
                };

                let mut typed_args = Vec::new();
                let mut params_iter = params_ty.iter();

                for arg in args {
                    let typed_arg = self.check_expr(arg)?;

                    if let Some(expected_ty) = params_iter.next() {
                        if self.unify(&typed_arg.ty(), expected_ty).is_none() {
                            self.diagnostics
                                .push(error("函数调用参数类型不匹配").with_span(*span).build());
                            return None;
                        }
                    }

                    typed_args.push(typed_arg);
                }

                Some(TypedExpr::Call {
                    callee: *callee,
                    args: typed_args,
                    ty: *return_ty,
                    span: *span,
                })
            }
            RExpr::Block { block } => {
                let typed_block = self.check_block(block)?;
                let ty = typed_block.ty.clone();
                Some(TypedExpr::Block {
                    block: typed_block,
                    ty: ty,
                    span: block.span,
                })
            }
            RExpr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                let typed_condition = self.check_expr(condition)?;
                if typed_condition.ty() != Ty::Bool {
                    self.diagnostics.push(
                        error("if条件需要bool类型")
                            .with_span(condition.span())
                            .with_label(
                                condition.span(),
                                format!("条件表达式类型为 {:?}", typed_condition.ty()),
                            )
                            .build(),
                    );
                }

                let typed_then = self.check_block(then_branch)?;

                let result_ty = match else_branch {
                    Some(else_expr) => {
                        let typed_else = self.check_expr(else_expr)?;
                        if self.unify(&typed_then.ty, &typed_else.ty()).is_none() {
                            self.diagnostics.push(
                                error("if-else分支类型不匹配")
                                    .with_span(*span)
                                    .with_label(
                                        then_branch.span,
                                        format!("then分支类型为 {:?}", typed_then.ty),
                                    )
                                    .with_label(
                                        else_expr.span(),
                                        format!("else分支类型为 {:?}", typed_else.ty()),
                                    )
                                    .build(),
                            );
                        }
                        typed_then.ty.clone()
                    }
                    None => Ty::Unit,
                };

                Some(TypedExpr::If {
                    condition: Box::new(typed_condition),
                    then_branch: typed_then,
                    else_branch: match else_branch {
                        Some(else_expr) => Some(Box::new(self.check_expr(else_expr)?)),
                        None => None,
                    },
                    ty: result_ty,
                    span: *span,
                })
            }
            RExpr::Loop { body, span } => {
                let typed_body = self.check_block(body)?;

                let mut actual_ty = Ty::Never;
                for ty in std::mem::take(&mut self.loop_break_tys) {
                    match self.unify(&actual_ty, &ty) {
                        Some(ty) => actual_ty = ty,
                        None => {
                            self.diagnostics
                                .push(error("循环的break语句类型不匹配").with_span(*span).build());
                            return None;
                        }
                    }
                }
                Some(TypedExpr::Loop {
                    body: typed_body,
                    ty: Ty::Never, // 循环的类型是Never，因为它不会正常返回
                    span: *span,
                })
            }
            RExpr::Postfix { operand, op, span } => {
                let typed_operand = self.check_expr(operand)?;
                let operand_ty = typed_operand.ty();

                let result_ty = match op {
                    rhir::PosOp::Plus | rhir::PosOp::Sub => {
                        // 增量/减量操作：要求操作数是数值类型
                        if !operand_ty.is_numeric() {
                            self.diagnostics.push(
                                error("增量/减量操作需要数值类型")
                                    .with_span(*span)
                                    .with_label(
                                        operand.span(),
                                        format!("操作数类型为 {:?}", operand_ty),
                                    )
                                    .build(),
                            );
                        }
                        operand_ty // 返回原类型
                    }
                };

                Some(TypedExpr::Postfix {
                    operand: Box::new(typed_operand),
                    op: *op,
                    ty: result_ty,
                    span: *span,
                })
            }
            RExpr::Assign {
                target,
                op,
                value,
                span,
            } => {
                let typed_target = self.check_expr(target)?;
                let typed_value = self.check_expr(value)?;

                let target_ty = typed_target.ty();
                let value_ty = typed_value.ty();
                if let Some(def_id) = self.get_def_id(&typed_value) {
                    match *self.env.get_var_state(def_id).unwrap() {
                        VarState::Moved => {
                            self.diagnostics
                                .push(error("不能对已移动的变量进行赋值").with_span(*span).build());
                            return None;
                        }
                        VarState::UnInitialized => {
                            self.env.set_var_state(def_id, VarState::Initialized);
                        }
                        _ => {}
                    }
                }

                // 检查赋值操作的类型兼容性
                if self.unify(&target_ty, &value_ty).is_none() {
                    self.diagnostics.push(
                        error("赋值操作类型不匹配")
                            .with_span(*span)
                            .with_label(target.span(), format!("目标类型为 {:?}", target_ty))
                            .with_label(value.span(), format!("值类型为 {:?}", value_ty))
                            .build(),
                    );
                }

                match op {
                    AssignOp::Simple => {
                        if !value_ty.is_copyable() {
                            if self.is_lvalue(value) {
                                if let Some(def_id) = self.get_def_id(&typed_value) {
                                    self.env.set_var_state(def_id, VarState::Moved);
                                }
                            }
                        }
                        if self.unify(&target_ty, &value_ty).is_none() {
                            self.diagnostics.push(
                                error("相等/不相等操作类型不匹配")
                                    .with_span(*span)
                                    .with_label(
                                        target.span(),
                                        format!("目标类型为 {:?}", target_ty),
                                    )
                                    .with_label(value.span(), format!("值类型为 {:?}", value_ty))
                                    .build(),
                            );
                        }
                    }
                    AssignOp::Add
                    | AssignOp::Sub
                    | AssignOp::Mul
                    | AssignOp::Div
                    | AssignOp::Rem => {
                        // 算术复合赋值：要求操作数类型兼容
                        if !target_ty.is_numeric() || !value_ty.is_numeric() {
                            self.diagnostics.push(
                                error("复合赋值操作需要数值类型")
                                    .with_span(*span)
                                    .with_label(
                                        *span,
                                        format!(
                                            "操作数类型 {:?} 和 {:?} 不支持复合赋值",
                                            target_ty, value_ty
                                        ),
                                    )
                                    .build(),
                            );
                        }
                    }
                    AssignOp::BitAnd
                    | AssignOp::BitOr
                    | AssignOp::BitXor
                    | AssignOp::Shl
                    | AssignOp::Shr => {
                        // 位运算复合赋值：要求操作数类型兼容
                        if !target_ty.is_integer() || !value_ty.is_integer() {
                            self.diagnostics.push(
                                error("位运算复合赋值操作需要整数类型")
                                    .with_span(*span)
                                    .with_label(
                                        *span,
                                        format!(
                                            "操作数类型 {:?} 和 {:?} 不支持位运算复合赋值",
                                            target_ty, value_ty
                                        ),
                                    )
                                    .build(),
                            );
                        }
                    }
                }

                Some(TypedExpr::Assign {
                    target: Box::new(typed_target),
                    value: Box::new(typed_value),
                    op: *op,
                    ty: Ty::Unit, // 赋值表达式返回Unit
                    span: *span,
                })
            }
            RExpr::FieldAccess {
                base,
                field,
                def_id: _,
                span,
            } => {
                let typed_base = self.check_expr(base)?;
                let base_ty = typed_base.ty();

                if let Some(def_id) = self.get_def_id(&typed_base) {
                    if self.env.get_parent_state(def_id) == Some(VarState::Moved) {
                        self.diagnostics.push(
                            error(format!(
                                "`{}` 的值已被移动",
                                get_global_string(field.name).unwrap().as_ref()
                            ))
                            .with_span(*span)
                            .with_label(*span, "字段已被移动")
                            .build(),
                        );
                        return None;
                    }
                }

                // 检查字段访问：表达式必须是结构体类型
                let (field_ty, actual_field_def_id, index) = match &base_ty {
                    Ty::Adt(struct_def_id) => {
                        match self.resolve_output.struct_fields.get(struct_def_id) {
                            Some(fields) => {
                                // 关键：用 enumerate() 获取 index
                                match fields
                                    .iter()
                                    .enumerate()
                                    .find(|(_, f)| f.name == field.name)
                                {
                                    Some((index, struct_field)) => {
                                        let field_type = struct_field.ty.clone();
                                        let field_def_id = struct_field.def_id;

                                        if struct_field.visibility == RVisibility::Private {
                                            self.diagnostics.push(
                                                error(format!(
                                                    "字段 `{}` 是私有的",
                                                    get_global_string(field.name).unwrap().as_ref()
                                                ))
                                                .with_span(*span)
                                                .with_label(*span, "私有字段")
                                                .build(),
                                            );
                                            return None;
                                        }

                                        (self.check_type(&field_type)?, field_def_id, index as u32)
                                    }
                                    None => {
                                        self.diagnostics.push(
                                            error(format!(
                                                "结构体中不存在字段 `{}`",
                                                get_global_string(field.name).unwrap().as_ref()
                                            ))
                                            .with_span(*span)
                                            .build(),
                                        );
                                        return None;
                                    }
                                }
                            }
                            None => {
                                self.diagnostics.push(
                                    error(format!("未找到结构体定义 {:?}", struct_def_id))
                                        .with_span(*span)
                                        .with_label(*span, "结构体定义未找到")
                                        .build(),
                                );
                                return None;
                            }
                        }
                    }
                    _ => {
                        self.diagnostics.push(
                            error("字段访问需要结构体类型")
                                .with_span(base.span())
                                .with_label(
                                    base.span(),
                                    format!("类型 {:?} 不支持字段访问", base_ty),
                                )
                                .build(),
                        );
                        return None;
                    }
                };

                Some(TypedExpr::FieldAccess {
                    base: Box::new(typed_base),
                    field: TypedField {
                        def_id: actual_field_def_id, // 更新为实际字段的 def_id
                        name: field.name,
                        ty: field_ty.clone(),         // 字段类型
                        visibility: field.visibility, // 保持原始可见性
                        index: index,
                        span: field.span, // 保持原始位置信息
                    },
                    def_id: actual_field_def_id,
                    ty: field_ty,
                    span: *span,
                })
            }
            RExpr::Index {
                indexed,
                index,
                span,
            } => {
                let typed_indexed = self.check_expr(indexed)?;
                let typed_index = self.check_expr(index)?;

                let indexed_ty = typed_indexed.ty();
                let index_ty = typed_index.ty();

                // 检查索引类型必须是整数
                if index_ty != Ty::UInt(UIntKind::Usize) {
                    self.diagnostics.push(
                        error("索引必须是整数类型")
                            .with_span(index.span())
                            .with_label(index.span(), format!("索引类型为 {:?}", index_ty))
                            .build(),
                    );
                }

                // 检查容器类型必须是数组、切片或向量
                let element_ty = match &indexed_ty {
                    Ty::Array { element, .. } => element.as_ref().clone(),
                    _ => {
                        self.diagnostics.push(
                            error("索引操作需要数组、切片或向量类型")
                                .with_span(indexed.span())
                                .with_label(
                                    indexed.span(),
                                    format!("类型 {:?} 不支持索引操作", indexed_ty),
                                )
                                .build(),
                        );
                        Ty::Unknown
                    }
                };

                Some(TypedExpr::Index {
                    indexed: Box::new(typed_indexed),
                    index: Box::new(typed_index),
                    ty: element_ty,
                    span: *span,
                })
            }
            RExpr::Tuple { elements, span } => {
                let mut typed_elements = Vec::new();
                let mut element_tys = Vec::new();

                for element in elements {
                    let typed_element = self.check_expr(element)?;
                    typed_elements.push(typed_element.clone());
                    element_tys.push(typed_element.ty());
                }

                let tuple_ty = Ty::Tuple(element_tys);

                Some(TypedExpr::Tuple {
                    elements: typed_elements,
                    ty: tuple_ty,
                    span: *span,
                })
            }
            RExpr::Unit { span } => Some(TypedExpr::Unit {
                ty: Ty::Unit,
                span: *span,
            }),
            RExpr::To { start, end, span } => {
                let typed_start = self.check_expr(start)?;
                let typed_end = self.check_expr(end)?;

                let start_ty = typed_start.ty();
                let end_ty = typed_end.ty();

                // 检查范围边界类型
                if self.unify(&start_ty, &end_ty).is_none() {
                    self.diagnostics.push(
                        error("范围起始值和结束值类型不匹配")
                            .with_span(*span)
                            .with_label(start.span(), format!("起始值类型为 {:?}", start_ty))
                            .with_label(end.span(), format!("结束值类型为 {:?}", end_ty))
                            .build(),
                    );
                }

                if !start_ty.is_numeric() {
                    self.diagnostics.push(
                        error("范围操作需要数值类型")
                            .with_span(*span)
                            .with_label(*span, format!("范围类型为 {:?}", start_ty))
                            .build(),
                    );
                }

                Some(TypedExpr::To {
                    start: Box::new(typed_start),
                    end: Box::new(typed_end),
                    ty: Ty::Range {
                        ty: Box::new(start_ty),
                    },
                    span: *span,
                })
            }
            RExpr::ToEq { start, end, span } => {
                let typed_start = self.check_expr(start)?;
                let typed_end = self.check_expr(end)?;

                let start_ty = typed_start.ty();
                let end_ty = typed_end.ty();

                // 检查范围边界类型
                if self.unify(&start_ty, &end_ty).is_none() {
                    self.diagnostics.push(
                        error("范围起始值和结束值类型不匹配")
                            .with_span(*span)
                            .with_label(start.span(), format!("起始值类型为 {:?}", start_ty))
                            .with_label(end.span(), format!("结束值类型为 {:?}", end_ty))
                            .build(),
                    );
                }

                if !start_ty.is_numeric() {
                    self.diagnostics.push(
                        error("范围操作需要数值类型")
                            .with_span(*span)
                            .with_label(*span, format!("范围类型为 {:?}", start_ty))
                            .build(),
                    );
                }

                Some(TypedExpr::ToEq {
                    start: Box::new(typed_start),
                    end: Box::new(typed_end),
                    ty: Ty::Range {
                        ty: Box::new(start_ty),
                    },
                    span: *span,
                })
            }
            RExpr::Grouped { expr, span } => {
                let typed_expr = self.check_expr(expr)?;
                let ty = typed_expr.ty();
                Some(TypedExpr::Grouped {
                    expr: Box::new(typed_expr),
                    ty: ty,
                    span: *span,
                })
            }
            RExpr::AddressOf { expr, span } => {
                // 检查操作数是否为左值
                if !self.is_lvalue(expr) {
                    self.diagnostics.push(
                        error("只能对左值取地址")
                            .with_span(*span)
                            .with_label(*span, "表达式不是左值，无法取地址")
                            .build(),
                    );
                    return None;
                }

                let typed_expr = self.check_expr(expr)?;
                let operand_ty = typed_expr.ty();

                // 返回指向操作数类型的指针类型
                let result_ty = Ty::Ptr(Box::new(operand_ty));

                Some(TypedExpr::AddressOf {
                    expr: Box::new(typed_expr),
                    ty: result_ty,
                    span: *span,
                })
            }
            RExpr::Dereference { expr, span } => {
                let typed_expr = self.check_expr(expr)?;
                let operand_ty = typed_expr.ty();

                // 检查操作数是否为指针类型或引用类型
                let result_ty = match &operand_ty {
                    Ty::Ptr(target_ty) => target_ty.as_ref().clone(),
                    Ty::Ref { to: target_ty, .. } => target_ty.as_ref().clone(),
                    _ => {
                        self.diagnostics.push(
                            error("解引用需要指针类型或引用类型")
                                .with_span(*span)
                                .with_label(
                                    *span,
                                    format!("操作数类型 {:?} 不是指针类型或引用类型", operand_ty),
                                )
                                .build(),
                        );
                        return None;
                    }
                };

                Some(TypedExpr::Dereference {
                    expr: Box::new(typed_expr),
                    ty: result_ty,
                    span: *span,
                })
            }
            RExpr::StructInit {
                def_id,
                fields,
                span,
            } => {
                let struct_def_id = match def_id.kind {
                    DefKind::Ghost => {
                        return None;
                    }
                    DefKind::Struct => *def_id,
                    _ => {
                        self.diagnostics.push(
                            error(format!(
                                "类型 `{}` 不是结构体",
                                self.get_definition_name(*def_id)
                            ))
                            .with_span(*span)
                            .build(),
                        );
                        return None;
                    }
                };

                // 从环境中获取结构体类型
                let struct_ty = match self.env.get(struct_def_id) {
                    Some(ty) => ty.clone(),
                    None => {
                        self.diagnostics.push(
                            error(format!("结构体类型未定义: {:?}", struct_def_id))
                                .with_span(*span)
                                .build(),
                        );
                        return None;
                    }
                };

                // 检查是否是 Adt 类型
                let struct_fields = match &struct_ty {
                    Ty::Adt(def_id) => {
                        // 从 resolve_output 中获取结构体字段信息
                        match self.resolve_output.struct_fields.get(def_id) {
                            Some(fields) => fields.clone(), // 克隆字段信息，避免借用冲突
                            None => {
                                self.diagnostics.push(
                                    error(format!("结构体字段信息未找到: {:?}", def_id))
                                        .with_span(*span)
                                        .build(),
                                );
                                return None;
                            }
                        }
                    }
                    _ => {
                        self.diagnostics.push(
                            error(format!("类型 {:?} 不是结构体", struct_ty))
                                .with_span(*span)
                                .build(),
                        );
                        return None;
                    }
                };

                // 解析字段表达式并检查类型
                let mut typed_fields = Vec::new();

                for (name, field_value) in fields {
                    // 查找字段定义并检查类型
                    let field_def = match struct_fields.iter().find(|f| f.name == *name) {
                        Some(def) => def,
                        None => {
                            self.diagnostics.push(
                                error(format!(
                                    "结构体中不存在字段 `{}`",
                                    get_global_string(*name).unwrap().as_ref()
                                ))
                                .with_span(field_value.span())
                                .build(),
                            );
                            return None;
                        }
                    };

                    // 检查并统一类型
                    let typed_value = self.check_expr(field_value)?;
                    let value_ty = typed_value.ty();
                    let field_ty = field_def.ty.clone();
                    let field_ty_checked = self.check_type(&field_ty)?;

                    if self.unify(&field_ty_checked, &value_ty).is_none() {
                        self.diagnostics.push(
                            error("结构体字段类型不匹配")
                                .with_span(field_value.span())
                                .with_label(
                                    field_value.span(),
                                    format!(
                                        "期望类型 {:?}，但找到类型 {:?}",
                                        field_ty_checked, value_ty
                                    ),
                                )
                                .build(),
                        );
                        return None;
                    }

                    typed_fields.push((*name, typed_value));
                }

                // 检查是否所有必需字段都被初始化
                let initialized_fields: std::collections::HashSet<_> =
                    typed_fields.iter().map(|(name, _)| name).collect();

                for field_def in struct_fields {
                    if !initialized_fields.contains(&field_def.name) {
                        self.diagnostics.push(
                            error(format!(
                                "结构体字段 `{}` 未初始化",
                                get_global_string(field_def.name).unwrap().as_ref()
                            ))
                            .with_span(*span)
                            .build(),
                        );
                        return None;
                    }
                }

                Some(TypedExpr::StructInit {
                    def_id: struct_def_id,
                    fields: typed_fields,
                    ty: struct_ty,
                    span: *span,
                })
            }
            RExpr::Cast { expr, ty, span } => {
                let expr = self.check_expr(expr)?;
                let expr_ty = expr.ty();
                let ty = self.check_type(ty)?;

                let cast_kind = match self.check_cast(&expr_ty, &ty) {
                    Some(kind) => kind,
                    None => {
                        self.diagnostics.push(
                            error(format!(
                                "类型 `{:?}` 与类型 `{:?}` 不支持类型转换",
                                expr_ty, ty
                            ))
                            .with_span(*span)
                            .build(),
                        );
                        return None;
                    }
                };

                Some(TypedExpr::Cast {
                    expr: Box::new(expr),
                    kind: cast_kind,
                    ty: ty,
                    span: *span,
                })
            }
        }
    }

    fn check_type(&mut self, rtype: &rhir::RType) -> Option<Ty> {
        use litec_name_resolver::rhir::RType;
        match rtype {
            // 命名类型（结构体、枚举等）
            RType::Named { id, span: _ } => {
                // 检查是否是基本类型
                if let Some(basic_ty) = self.check_basic_type(*id) {
                    return Some(basic_ty);
                }

                // 检查是否是 Ghost 类型
                if id.kind == DefKind::Ghost {
                    return None;
                }

                // 返回 ADT 类型
                Some(Ty::Adt(*id))
            }

            // 泛型类型
            RType::Generic { id, args, span } => {
                let def_kind = self.resolve_output.definitions[id.index as usize].kind;
                if def_kind == DefKind::Ghost {
                    self.diagnostics.push(
                        error(format!(
                            "类型 `{}` 未找到",
                            get_global_string(
                                self.resolve_output.definitions[id.index as usize].name
                            )
                            .unwrap()
                        ))
                        .with_span(*span)
                        .build(),
                    );
                    return None;
                }

                let mut arg_tys = Vec::with_capacity(args.len());
                for arg in args {
                    arg_tys.push(self.check_type(arg)?);
                }

                Some(Ty::Adt(*id))
            }

            // 元组类型
            RType::Tuple { elements, span: _ } => {
                let mut v = Vec::with_capacity(elements.len());
                for e in elements {
                    v.push(self.check_type(e)?);
                }
                Some(Ty::Tuple(v))
            }

            // 引用类型
            RType::Reference {
                mutable,
                target,
                span: _,
            } => {
                let target_ty = self.check_type(target)?;
                let mutable_bool = match mutable {
                    litec_hir::Mutability::Mut => true,
                    litec_hir::Mutability::Const => false,
                };
                Some(Ty::Ref {
                    mutable: mutable_bool,
                    to: Box::new(target_ty),
                })
            }

            // 指针类型
            RType::Pointer {
                mutable: _,
                target,
                span: _,
            } => {
                let target_ty = self.check_type(target)?;
                Some(Ty::Ptr(Box::new(target_ty)))
            }

            // 未知类型
            RType::Unknown => Some(Ty::Unknown),
        }
    }

    // 辅助方法：检查基本类型
    fn check_basic_type(&self, def_id: DefId) -> Option<Ty> {
        let builtin_types = &self.resolve_output.builtin.types;
        check_builtin_type!(def_id, builtin_types, {
            i8 => Ty::Int(IntKind::I8),
            i16 => Ty::Int(IntKind::I16),
            i32 => Ty::Int(IntKind::I32),
            i64 => Ty::Int(IntKind::I64),
            i128 => Ty::Int(IntKind::I128),
            isize => Ty::Int(IntKind::Isize),
            u8 => Ty::UInt(UIntKind::U8),
            u16 => Ty::UInt(UIntKind::U16),
            u32 => Ty::UInt(UIntKind::U32),
            u64 => Ty::UInt(UIntKind::U64),
            u128 => Ty::UInt(UIntKind::U128),
            usize => Ty::UInt(UIntKind::Usize),
            f32 => Ty::Float(FloatKind::F32),
            f64 => Ty::Float(FloatKind::F64),
            bool => Ty::Bool,
            char => Ty::Char,
            str => Ty::Str,
            unit => Ty::Unit,
            never => Ty::Never,
            raw_ptr => Ty::RawPtr,
        });

        None
    }

    fn unify(&mut self, ty1: &Ty, ty2: &Ty) -> Option<Ty> {
        match (ty1, ty2) {
            (Ty::Int(_), Ty::Float(kind)) | (Ty::Float(kind), &Ty::Int(_)) => {
                Some(Ty::Float(*kind))
            }
            (Ty::Int(a), Ty::Int(b)) => Some(Ty::Int(*a.max(b))),
            (Ty::UInt(a), Ty::UInt(b)) => Some(Ty::UInt(*a.max(b))),
            (Ty::Int(_), Ty::UInt(_)) | (Ty::UInt(_), Ty::Int(_)) => None,
            (Ty::Unknown, ty) | (ty, Ty::Unknown) => Some(ty.clone()),
            (Ty::Never, ty) | (ty, Ty::Never) => Some(ty.clone()),
            (ty1, ty2) if ty1 == ty2 => Some(ty1.clone()),

            _ => None,
        }
    }

    fn is_lvalue(&self, expr: &RExpr) -> bool {
        match expr {
            RExpr::Local { .. } | RExpr::Global { .. } => true,
            RExpr::Dereference { .. } => true,
            RExpr::FieldAccess { .. } => true,
            RExpr::Index { .. } => true,
            RExpr::Grouped { expr, .. } => self.is_lvalue(expr),
            _ => false,
        }
    }

    fn check_cast(&self, from: &Ty, to: &Ty) -> Option<CastKind> {
        use IntKind as S;
        use IntKind::*;
        use UIntKind as U;

        match (from, to) {
            // 相同类型
            _ if from == to => Some(CastKind::Identity),

            // 有符号 -> 有符号
            (Ty::Int(a), Ty::Int(b)) => match a.bit_width().cmp(&b.bit_width()) {
                Ordering::Less => Some(CastKind::SignExtend),
                Ordering::Greater => Some(CastKind::Truncate),
                Ordering::Equal => Some(CastKind::Identity),
            },

            // 无符号 -> 无符号
            (Ty::UInt(a), Ty::UInt(b)) => match a.bit_width().cmp(&b.bit_width()) {
                Ordering::Less => Some(CastKind::ZeroExtend),
                Ordering::Greater => Some(CastKind::Truncate),
                Ordering::Equal => Some(CastKind::Identity),
            },

            // 有符号 -> 无符号
            (Ty::Int(a), Ty::UInt(b)) => match a.bit_width().cmp(&b.bit_width()) {
                Ordering::Less => Some(CastKind::SignExtend), // i8 -> u32: 符号扩展
                Ordering::Greater => Some(CastKind::Truncate), // i64 -> u32: 截断
                Ordering::Equal => Some(CastKind::Bitcast),   // i32 -> u32: 重新解释
            },

            // 无符号 -> 有符号
            (Ty::UInt(a), Ty::Int(b)) => match a.bit_width().cmp(&b.bit_width()) {
                Ordering::Less => Some(CastKind::ZeroExtend), // u8 -> i32: 零扩展
                Ordering::Greater => Some(CastKind::Truncate), // u64 -> i32: 截断
                Ordering::Equal => Some(CastKind::Bitcast),   // u32 -> i32: 重新解释
            },

            // 整数 -> 浮点
            (Ty::Int(_), Ty::Float(_)) => Some(CastKind::IntToFloat),
            (Ty::UInt(_), Ty::Float(_)) => Some(CastKind::UintToFloat),

            // 浮点 -> 整数
            (Ty::Float(_), Ty::Int(_)) => Some(CastKind::FloatToInt),
            (Ty::Float(_), Ty::UInt(_)) => Some(CastKind::FloatToUint),

            // 浮点之间
            (Ty::Float(a), Ty::Float(b)) => {
                if a.bits() < b.bits() {
                    Some(CastKind::FloatPromote)
                } else {
                    Some(CastKind::FloatDemote)
                }
            }

            // 指针转换
            (Ty::Ptr(_), Ty::Ptr(_)) => Some(CastKind::PtrToPtr),
            (Ty::RawPtr, Ty::Ptr(_)) => Some(CastKind::PtrToPtr),
            (Ty::Ptr(_), Ty::RawPtr) => Some(CastKind::PtrToPtr),

            // 指针 <-> usize
            (Ty::Ptr(_), Ty::UInt(U::Usize)) => Some(CastKind::PtrToInt),
            (Ty::UInt(U::Usize), Ty::Ptr(_)) => Some(CastKind::IntToPtr),

            // 危险：指针 <-> 任意整数（截断/扩展）
            (Ty::Ptr(_), Ty::Int(_)) => Some(CastKind::PtrToInt),
            (Ty::Int(_), Ty::Ptr(_)) => Some(CastKind::IntToPtr),

            _ => None,
        }
    }

    fn get_definition_name(&self, def_id: DefId) -> Rc<str> {
        get_global_string(self.resolve_output.definitions[def_id.index as usize].name).unwrap()
    }

    fn get_def_id(&self, expr: &TypedExpr) -> Option<DefId> {
        match expr {
            TypedExpr::Local { def_id, .. } => Some(*def_id),
            TypedExpr::Global { def_id, .. } => Some(*def_id),
            TypedExpr::Dereference { expr, .. } => self.get_def_id(expr),
            TypedExpr::FieldAccess { def_id, .. } => Some(*def_id),
            TypedExpr::Index { indexed, .. } => self.get_def_id(indexed),
            TypedExpr::Grouped { expr, .. } => self.get_def_id(expr),
            _ => None,
        }
    }
}

/// 便捷的类型错误创建函数
pub fn type_mismatch(expected: &Ty, found: &Ty, span: Span) -> Diagnostic {
    error(format!("类型 `{:?}` 和类型 `{:?}` 不匹配", expected, found))
        .with_span(span)
        .with_help("请检查表达式的类型")
        .build()
}

pub fn undefined_variable(name: &str, span: Span) -> Diagnostic {
    error(format!("未定义的变量 `{}`", name))
        .with_span(span)
        .with_help("请确保变量已声明或拼写正确")
        .build()
}

pub fn check(resolve_output: ResolveOutput) -> (TypedCrate, Vec<Diagnostic>) {
    let checker = TypeChecker::new(resolve_output);
    checker.check_crate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use litec_span::SourceMap;
    use std::path::PathBuf;

    fn parse_and_resolve(input: &str) -> (SourceMap, ResolveOutput) {
        use litec_lower::lower;
        use litec_parse::parser::parse;

        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "test.lt".to_string(),
            input.to_string(),
            &PathBuf::from("test.lt"),
        );

        let (ast, parse_diagnostics) = parse(&source_map, file_id);
        for diagnostic in &parse_diagnostics {
            println!("{}", diagnostic.render(&source_map));
        }
        assert!(
            parse_diagnostics.is_empty(),
            "Parse errors: {:?}",
            parse_diagnostics
        );

        let (raw_crate, lower_diagnostics) = lower(ast);
        for diagnostic in &lower_diagnostics {
            println!("{}", diagnostic.render(&source_map));
        }
        assert!(
            lower_diagnostics.is_empty(),
            "Lower errors: {:?}",
            lower_diagnostics
        );

        let resolver = litec_name_resolver::Resolver::new(&mut source_map, file_id);
        let resolve_output = resolver.resolve(&raw_crate);
        (source_map, resolve_output)
    }

    fn type_check(input: &str) -> (TypedCrate, Vec<Diagnostic>) {
        let (source_map, resolve_output) = parse_and_resolve(input);
        let checker = TypeChecker::new(resolve_output);
        let (typed_crate, diagnostics) = checker.check_crate();
        for diagnostic in &diagnostics {
            println!("{}", diagnostic.render(&source_map));
        }
        (typed_crate, diagnostics)
    }

    #[test]
    fn test_basic_types() {
        let input = r#"
        fn main() {
            let x: i32 = 42;
            let y: bool = true;
            let z: f64 = 3.14;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_type_mismatch() {
        let input = r#"
        fn main() {
            let x: i32 = true;  // 类型不匹配
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(!diagnostics.is_empty(), "Expected type mismatch error");
        assert!(diagnostics.iter().any(|d| d.message.contains("不匹配")));
    }

    #[test]
    fn test_arithmetic_operations() {
        let input = r#"
        fn main() {
            let a = 5 + 3;
            let b = 10 - 2;
            let c = 4 * 6;
            let d = 20 / 4;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_invalid_arithmetic_operation() {
        let input = r#"
        fn main() {
            let a = 5 + true;  // 类型不匹配：整数与布尔值相加
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            !diagnostics.is_empty(),
            "Expected type error for invalid arithmetic operation"
        );
    }

    #[test]
    fn test_comparison_operations() {
        let input = r#"
        fn main() {
            let a = 5 == 5;
            let b = 10 > 5;
            let c = 3 < 7;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_struct_definition() {
        let input = r#"
        struct Point {
            x: i32,
            y: i32,
        }
        
        fn main() {
            let p = Point { x: 1, y: 2 };
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_struct_field_access() {
        let input = r#"
        struct Point {
            pub x: i32,
            pub y: i32,
        }
        
        fn main() {
            let p = Point { x: 1, y: 2 };
            let x_val = p.x;
            let y_val = p.y;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_invalid_struct_field_access() {
        let input = r#"
        struct Point {
            x: i32,
            y: i32,
        }
        
        fn main() {
            let p = Point { x: 1, y: 2 };
            let z_val = p.z;  // 不存在的字段
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            !diagnostics.is_empty(),
            "Expected error for accessing non-existent field"
        );
    }

    #[test]
    fn test_function_definition() {
        let input = r#"
        fn add(x: i32, y: i32) -> i32 {
            x + y
        }
        
        fn main() {
            let result = add(5, 3);
        }
        "#;

        let (krate, diagnostics) = type_check(input);

        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_function_call_type_mismatch() {
        let input = r#"
        fn add(x: i32, y: i32) -> i32 {
            x + y
        }
        
        fn main() {
            let result = add(5, true);  // 第二个参数类型不匹配
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            !diagnostics.is_empty(),
            "Expected type error for function call"
        );
    }

    #[test]
    fn test_if_expression() {
        let input = r#"
        fn main() {
            let x = if true { 1 } else { 2 };
            let y = if 5 > 3 { 10 } else { 20 };
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_invalid_if_else_branches() {
        let input = r#"
        fn main() {
            let x = if true { 1 } else { true };  // 分支类型不匹配
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            !diagnostics.is_empty(),
            "Expected type error for mismatched if-else branches"
        );
    }

    #[test]
    fn test_loop_expression() {
        let input = r#"
        fn main() {
            loop {
                break;
            }
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_tuple_types() {
        let input = r#"
        fn main() {
            let t = (1, true, 3.14);
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_range_expression() {
        let input = r#"
        fn main() {
            let r = 1 .. 10;
            let r2 = 1 ..= 10;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_undefined_variable() {
        let input = r#"
        fn main() {
            let x = y;  // y未定义
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            !diagnostics.is_empty(),
            "Expected error for undefined variable"
        );
    }

    #[test]
    fn test_literal_types() {
        let input = r#"
        fn main() {
            let int_val = 42;
            let float_val = 3.14;
            let bool_val = true;
            let unit_val = ();
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_reference_and_deref() {
        let input = r#"
        fn main() {
            let x = 42;
            let y = &x;  // 取引用
            let z = *y;  // 解引用
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_unary_operations() {
        let input = r#"
        fn main() {
            let a = -5;
            let b = !true;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_assignment_operations() {
        let input = r#"
        fn main() {
            let mut x = 5;
            x = 10;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_module_definition() {
        let input = r#"
        mod inner {
            pub fn public_fn() -> i32 { 42 }
            fn private_fn() -> i32 { 43 }
        }
        
        fn main() {
            let x = inner::public_fn();
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_nested_modules() {
        let input = r#"
        mod outer {
            mod inner {
                pub fn foo() -> i32 { 42 }
            }
            
            pub fn bar() -> i32 { inner::foo() }
        }
        
        fn main() {
            let x = outer::bar();
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_cross_module_access() {
        let input = r#"
        mod utils {
            pub fn add(x: i32, y: i32) -> i32 { x + y }
        }
        
        fn main() {
            let result = utils::add(5, 3);
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_private_access_error() {
        let input = r#"
        mod inner {
            fn private_fn() -> i32 { 43 }
        }
        
        fn main() {
            let x = inner::private_fn();
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for private access"
        );
        assert!(
            diagnostics.iter().any(|d| d.message.contains("私有")),
            "Expected private access error or undefined error"
        );
    }

    #[test]
    fn test_module_struct_access() {
        let input = r#"
        mod types {
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }
        }
        
        fn main() {
            let p = types::Point { x: 1, y: 2 };
            let x_val = p.x;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_module_private_struct_field() {
        let input = r#"
        mod types {
            pub struct Point {
                pub x: i32,
                y: i32,
            }
        }
        
        fn main() {
            let p = types::Point { x: 1, y: 2 };
            let y_val = p.y;
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for private field access"
        );
    }

    #[test]
    fn test_module_function_call() {
        let input = r#"
        mod math {
            pub fn square(x: i32) -> i32 { x * x }
        }
        
        fn main() {
            let result = math::square(5);
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_module_function_type_mismatch() {
        let input = r#"
        mod math {
            pub fn add(x: i32, y: i32) -> i32 { x + y }
        }
        
        fn main() {
            let result = math::add(5, true);
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(!diagnostics.is_empty(), "Expected type mismatch error");
        assert!(
            diagnostics.iter().any(|d| d.message.contains("类型不匹配")),
            "Expected type mismatch error"
        );
    }

    #[test]
    fn test_module_use_statement() {
        let input = r#"
        mod utils {
            pub fn helper() -> i32 { 42 }
        }
        
        use utils::helper;
        
        fn main() {
            let result = helper();
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_module_nested_use() {
        let input = r#"
        mod outer {
            mod inner {
                pub fn foo() -> i32 { 42 }
            }
            
            pub use inner::foo;
        }
        
        use outer::foo;
        
        fn main() {
            let result = foo();
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_module_extern_functions() {
        let input = r#"
        extern "C" {
            pub fn external_func(x: i32) -> i32;
        }
        
        fn main() {
            let result = external_func(42);
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(
            diagnostics.is_empty(),
            "Unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_module_extern_type_mismatch() {
        let input = r#"
        extern "C" {
            fn external_func(x: i32) -> i32;
        }
        
        fn main() {
            let result = external_func(true);
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(!diagnostics.is_empty(), "Expected type mismatch error");
        assert!(
            diagnostics.iter().any(|d| d.message.contains("类型不匹配")),
            "Expected type mismatch error"
        );
    }

    #[test]
    fn test_move_semantics() {
        let input = r#"
        struct Point {
            pub x: i32,
            pub y: i32,
        }
        
        fn main() {
            let p1 = Point { x: 1, y: 2 };
            let p2 = p1;  // 移动 p1
            let x = p1.x;  // 错误：使用已移动的值
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(!diagnostics.is_empty(), "Expected move error");
        assert!(diagnostics.iter().any(|d| d.message.contains("移动")));
    }

    #[test]
    fn test_copy_types() {
        let input = r#"
        fn main() {
            let x = 42;
            let y = x;  // 复制，不移动
            let z = x;  // 可以继续使用 x
        }
        "#;

        let (_, diagnostics) = type_check(input);
        assert!(diagnostics.is_empty(), "Unexpected diagnostics");
    }
}
