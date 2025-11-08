use litec_span::{get_global_string, get_global_string_pool, Span, StringId};
use litec_mir::{
    AggregateKind, BasicBlock, BasicBlockId, BinOp, FloatKind, IntKind, Literal, LocalDecl, LocalId, MirFunction, 
    Operand, Rvalue, Statement, SwitchTargets, Terminator, Ty, UnOp
};
use litec_typed_hir::{
    def_id::DefId, TypedBlock, TypedCrate, TypedExpr, TypedItem, TypedParam, TypedStmt 
};
use litec_hir::LiteralValue;

// 直接的类型转换函数
fn hir_ty_to_mir_ty(ty: &litec_typed_hir::ty::Ty) -> Ty {
    match ty {
        litec_typed_hir::ty::Ty::Int(kind) => {
            let mir_kind = match kind {
                litec_typed_hir::ty::IntKind::I8 => IntKind::I8,
                litec_typed_hir::ty::IntKind::I16 => IntKind::I16,
                litec_typed_hir::ty::IntKind::I32 => IntKind::I32,
                litec_typed_hir::ty::IntKind::I64 => IntKind::I64,
                litec_typed_hir::ty::IntKind::I128 => IntKind::I128,
                litec_typed_hir::ty::IntKind::U8 => IntKind::U8,
                litec_typed_hir::ty::IntKind::U16 => IntKind::U16,
                litec_typed_hir::ty::IntKind::U32 => IntKind::U32,
                litec_typed_hir::ty::IntKind::U64 => IntKind::U64,
                litec_typed_hir::ty::IntKind::U128 => IntKind::U128,
                litec_typed_hir::ty::IntKind::Usize => IntKind::Usize,
                litec_typed_hir::ty::IntKind::Isize => IntKind::Isize,
                litec_typed_hir::ty::IntKind::Unknown => unreachable!(),
            };
            Ty::Int(mir_kind)
        }
        litec_typed_hir::ty::Ty::Float(kind) => {
            let mir_kind = match kind {
                litec_typed_hir::ty::FloatKind::F32 => FloatKind::F32,
                litec_typed_hir::ty::FloatKind::F64 => FloatKind::F64,
                litec_typed_hir::ty::FloatKind::Unknow => unreachable!(),
            };
            Ty::Float(mir_kind)
        }
        litec_typed_hir::ty::Ty::Bool => Ty::Bool,
        litec_typed_hir::ty::Ty::Unit => Ty::Unit,
        litec_typed_hir::ty::Ty::Never => Ty::Never,
        litec_typed_hir::ty::Ty::Unknown => Ty::Unknown,
        // 处理其他类型
        _ => Ty::Unknown,
    }
}

struct Builder<'a> {
    hir: &'a TypedCrate,
    
    // 当前构建状态
    current_function: Option<MirFunction>,
    current_block: BasicBlockId,
    next_local_id: usize,
    next_block_id: usize,
    
    // 变量映射
    local_map: std::collections::HashMap<DefId, LocalId>,
    
    // 基本块管理
    basic_blocks: Vec<BasicBlock>,
    current_statements: Vec<Statement>,
    
    // 控制流管理
    break_targets: Vec<BasicBlockId>,
    continue_targets: Vec<BasicBlockId>,
}

impl<'a> Builder<'a> {
    pub fn new(hir: &'a TypedCrate) -> Self {
        Self {
            hir,
            current_function: None,
            current_block: BasicBlockId(0),
            next_local_id: 1, // 0 保留给返回值
            next_block_id: 1,
            local_map: Default::default(),
            basic_blocks: Vec::new(),
            current_statements: Vec::new(),
            break_targets: Vec::new(),
            continue_targets: Vec::new()
        }
    }

    // 主构建入口 - 构建整个 crate 的 MIR
    pub fn build_crate(mut self) -> Vec<MirFunction> {
        let mut functions = Vec::new();
        
        for item in &self.hir.items {
            match item {
                TypedItem::Function { def_id, name, params, return_ty, body, span, .. } => {
                    let mir_func = self.build_function(
                        *def_id, *name, params, &hir_ty_to_mir_ty(return_ty), body, *span
                    );
                    functions.push(mir_func);
                }
                TypedItem::Struct { .. } => {
                    // 结构体在 MIR 中主要是类型信息，不生成具体的 MIR 函数
                    // 但可以在这里记录结构体定义以供后续使用
                }
            }
        }
        
        functions
    }

    // 构建单个函数
    fn build_function(
        &mut self,
        def_id: DefId,
        name: StringId,
        params: &[TypedParam],
        return_ty: &Ty,
        body: &TypedBlock,
        span: Span,
    ) -> MirFunction {
        // 重置构建器状态
        self.reset_builder_state();

        // 初始化函数
        let mut function = MirFunction {
            def_id,
            name,
            locals: Vec::new(),
            basic_blocks: Vec::new(),
            span,
        };
        
        // 创建返回位置 (local 0)
        function.locals.push(LocalDecl {
            ty: return_ty.clone(),
            name: Some(StringId::from("return")),
            span,
        });

        // 为参数创建局部变量
        for param in params {
            let id = self.alloc_local_id();

            function.locals.push(LocalDecl {
                ty: hir_ty_to_mir_ty(&param.ty),
                name: Some(param.name),
                span: param.span,
            });
            
            self.local_map.insert(param.def_id, id);
        }

        self.current_function = Some(function);
        self.start_block(BasicBlockId(0)); // 入口块

        // 构建函数体
        self.build_block(body);

        let function = self.finish_function();

        if get_global_string(name).unwrap().as_ref() == "main" {
            return self.process_main_function(function);
        }

        function
    }

    /// 处理主函数的 MIR 生成
    fn process_main_function(&self, mut mir_func: MirFunction) -> MirFunction {
        // 确保主函数有正确的返回类型
        if let Some(local) = mir_func.locals.first_mut() {
            // 如果主函数返回 Unit，我们将其改为返回 i32
            if matches!(local.ty, Ty::Unit) {
                dbg!("  🔄 将主函数返回类型从 Unit 改为 i32");
                local.ty = Ty::Int(IntKind::I32);
            }
        }

        // 确保主函数有返回语句
        self.ensure_main_has_return(&mut mir_func);

        mir_func
    }

    /// 确保主函数有返回语句
    fn ensure_main_has_return(&self, mir_func: &mut MirFunction) {
        for bb in &mut mir_func.basic_blocks {
            if let Some(Terminator::Return { value, span }) = &bb.terminator {
                // 已经有返回语句，检查返回值
                if let Operand::Literal(Literal::Unit) = value {
                    dbg!("  🔄 将主函数返回 Unit 改为返回 0");
                    // 将 return () 改为 return 0
                    bb.terminator = Some(Terminator::Return {
                        value: Operand::Literal(Literal::I32(0)),
                        span: span.clone(),
                    });
                }
                return;
            }
        }
        
        // 如果没有返回语句，添加一个返回 0
        println!("  ➕ 为主函数添加默认返回语句");
        if let Some(last_bb) = mir_func.basic_blocks.last_mut() {
            last_bb.terminator = Some(Terminator::Return {
                value: Operand::Literal(Literal::I32(0)),
                span: Span::default(),
            });
        }
    }

    fn alloc_local_id(&mut self) -> LocalId {
        let id = LocalId(self.next_local_id);
        self.next_local_id += 1;
        id
    }

    // 构建语句块
    fn build_block(&mut self, block: &TypedBlock) {
        // 构建所有语句
        for stmt in &block.stmts {
            self.build_stmt(stmt);
        }
        
        // 构建尾表达式（如果有）
        if let Some(expr) = &block.tail {
            let result = self.build_expr(expr);
            
            // 将尾表达式的结果存储到返回位置
            self.assign(
                LocalId(0), // 返回位置
                Rvalue::Use(result),
                expr.span()
            );

            self.return_(Operand::Local(LocalId(0)), expr.span());
        } else if block.stmts.is_empty() || !self.has_terminator() {
            self.return_(Operand::Literal(Literal::Unit), block.span);
        }
    }

    // 构建语句
    fn build_stmt(&mut self, stmt: &TypedStmt) {
        match stmt {
            TypedStmt::Expr(expr) => {
                // 表达式语句：计算表达式但丢弃结果
                let _operand = self.build_expr(expr);
            }
            TypedStmt::Let { name, def_id, ty, init, span } => {
                let local_id = self.declare_local(*name, hir_ty_to_mir_ty(ty), *span);
                self.local_map.insert(*def_id, local_id);
                
                if let Some(init_expr) = init {
                    let operand = self.build_expr(init_expr);
                    self.assign(local_id, Rvalue::Use(operand), init_expr.span());
                }
            }
            TypedStmt::Return { value, span } => {
                let return_value = if let Some(expr) = value {
                    self.build_expr(expr)
                } else {
                    Operand::Literal(Literal::Unit)
                };
                self.return_(return_value, *span);
            }
            TypedStmt::Break { value, span } => {
                if let Some(&target) = self.break_targets.last() {
                    // 简化处理：break 跳转到循环结束块
                    if let Some(expr) = value {
                        let _ = self.build_expr(expr); // 计算表达式但暂时不处理值
                    }
                    self.goto(target, *span);
                }
            }
            TypedStmt::Continue { span } => {
                if let Some(&target) = self.continue_targets.last() {
                    self.goto(target, *span);
                }
            }
        }
    }

    // 构建表达式 - 返回操作数
    fn build_expr(&mut self, expr: &TypedExpr) -> Operand {
        match expr {
            // 字面量
            TypedExpr::Literal { value, ty: _, span } => {
                self.build_literal(value, *span)
            }
            
            // 标识符（变量）
            TypedExpr::Ident { def_id, ty: _, span: _, .. } => {
                if let Some(&local_id) = self.local_map.get(def_id) {
                    Operand::Local(local_id)
                } else {
                    // 全局变量或静态变量
                    Operand::Static(*def_id)
                }
            }
            
            // 算术运算
            TypedExpr::Addition { left, right, ty, span } => {
                self.build_binary_op(BinOp::Add, left, right, &hir_ty_to_mir_ty(ty), *span)
            }
            TypedExpr::Subtract { left, right, ty, span } => {
                self.build_binary_op(BinOp::Sub, left, right, &hir_ty_to_mir_ty(ty), *span)
            }
            TypedExpr::Multiply { left, right, ty, span } => {
                self.build_binary_op(BinOp::Mul, left, right, &hir_ty_to_mir_ty(ty), *span)
            }
            TypedExpr::Divide { left, right, ty, span } => {
                self.build_binary_op(BinOp::Div, left, right, &hir_ty_to_mir_ty(ty), *span)
            }
            TypedExpr::Remainder { left, right, ty, span } => {
                self.build_binary_op(BinOp::Rem, left, right, &hir_ty_to_mir_ty(ty), *span)
            }
            
            // 比较运算
            TypedExpr::Equal { left, right, ty: _, span } => {
                self.build_binary_op(BinOp::Eq, left, right, &Ty::Bool, *span)
            }
            TypedExpr::NotEqual { left, right, ty: _, span } => {
                self.build_binary_op(BinOp::Ne, left, right, &Ty::Bool, *span)
            }
            TypedExpr::LessThan { left, right, ty: _, span } => {
                self.build_binary_op(BinOp::Lt, left, right, &Ty::Bool, *span)
            }
            TypedExpr::LessThanOrEqual { left, right, ty: _, span } => {
                self.build_binary_op(BinOp::Le, left, right, &Ty::Bool, *span)
            }
            TypedExpr::GreaterThan { left, right, ty: _, span } => {
                self.build_binary_op(BinOp::Gt, left, right, &Ty::Bool, *span)
            }
            TypedExpr::GreaterThanOrEqual { left, right, ty: _, span } => {
                self.build_binary_op(BinOp::Ge, left, right, &Ty::Bool, *span)
            }
            
            // 逻辑运算
            TypedExpr::LogicalAnd { left, right, ty, span } => {
                self.build_binary_op(BinOp::And, left, right, &hir_ty_to_mir_ty(ty), *span)
            }
            TypedExpr::LogicalOr { left, right, ty, span } => {
                self.build_binary_op(BinOp::Or, left, right, &hir_ty_to_mir_ty(ty), *span)
            }
            TypedExpr::LogicalNot { operand, ty, span } => {
                self.build_unary_op(UnOp::Not, operand, &hir_ty_to_mir_ty(ty), *span)
            }
            
            // 一元运算
            TypedExpr::Negate { operand, ty, span } => {
                self.build_unary_op(UnOp::Neg, operand, &hir_ty_to_mir_ty(ty), *span)
            }
            
            // 赋值
            TypedExpr::Assign { target, value, ty: _, span } => {
                self.build_assign(target, value, *span)
            }
            
            // 函数调用
            TypedExpr::Call { callee, args, ty, span } => {
                self.build_call(*callee, args, &hir_ty_to_mir_ty(ty), *span)
            }
            
            // 控制流
            TypedExpr::If { condition, then_branch, else_branch, ty, span } => {
                self.build_if(condition, then_branch, else_branch, &hir_ty_to_mir_ty(ty), *span)
            }
            TypedExpr::Loop { body, ty: _, span } => {
                self.build_loop(body, *span)
            }
            
            // 块表达式
            TypedExpr::Block { block } => {
                self.build_block_expr(block)
            }
            
            // 其他表达式（简化处理）
            TypedExpr::Dereference { expr, ty, span } => {
                // 简化：直接返回操作数
                self.build_expr(expr)
            }
            TypedExpr::AddressOf { base, mutable: _, ty, span } => {
                // 简化：直接返回操作数
                self.build_expr(base)
            }
            TypedExpr::FieldAccess { base, field: _, def_id: _, ty, span } => {
                // 简化：直接返回基操作数
                self.build_expr(base)
            }
            TypedExpr::PathAccess { def_id, ty: _, span } => {
                // 全局路径访问
                Operand::Static(*def_id)
            }
        }
    }

    // === 具体表达式构建方法 ===

    fn build_literal(&self, value: &LiteralValue, span: Span) -> Operand {
        match value {
            LiteralValue::Int { value, kind } => {
                        match kind {
                            litec_hir::LitIntValue::I8 => Operand::Literal(Literal::I8(*value as i8)),
                            litec_hir::LitIntValue::I16 => Operand::Literal(Literal::I16(*value as i16)),
                            litec_hir::LitIntValue::I32 => Operand::Literal(Literal::I32(*value as i32)),
                            litec_hir::LitIntValue::I64 => Operand::Literal(Literal::I64(*value as i64)),
                            litec_hir::LitIntValue::I128 => Operand::Literal(Literal::I128(*value as i128)),
                            litec_hir::LitIntValue::Isize => Operand::Literal(Literal::Isize(*value as isize)),
                            litec_hir::LitIntValue::U8 => Operand::Literal(Literal::U8(*value as u8)),
                            litec_hir::LitIntValue::U16 => Operand::Literal(Literal::U16(*value as u16)),
                            litec_hir::LitIntValue::U32 => Operand::Literal(Literal::U32(*value as u32)),
                            litec_hir::LitIntValue::U64 => Operand::Literal(Literal::U64(*value as u64)),
                            litec_hir::LitIntValue::U128 => Operand::Literal(Literal::U128(*value as u128)),
                            litec_hir::LitIntValue::Usize => Operand::Literal(Literal::Usize(*value as usize)),
                            litec_hir::LitIntValue::Unknown => unreachable!(),
                        }
                    },
            LiteralValue::Float { value, kind } => {
                        match kind {
                            litec_hir::LitFloatValue::F32 => Operand::Literal(Literal::F32(*value as f32)),
                            litec_hir::LitFloatValue::F64 => Operand::Literal(Literal::F64(*value as f64)),
                            litec_hir::LitFloatValue::Unknown => unreachable!(),
                        }
                    },
            LiteralValue::Bool(value) => Operand::Literal(Literal::Bool(*value)),
            LiteralValue::Unit => Operand::Literal(Literal::Unit),
            LiteralValue::Str(string_id) => Operand::Literal(Literal::Str(*string_id)),
            LiteralValue::Char(c) => Operand::Literal(Literal::Char(*c)),
        }
    }

    fn build_binary_op(
        &mut self,
        op: BinOp,
        left: &TypedExpr,
        right: &TypedExpr,
        result_ty: &Ty,
        span: Span,
    ) -> Operand {
        let left_op = self.build_expr(left);
        let right_op = self.build_expr(right);
        let temp = self.new_temp(result_ty.clone(), span);
        
        self.assign(temp, Rvalue::Binary(op, left_op, right_op), span);
        Operand::Local(temp)
    }

    fn build_unary_op(
        &mut self,
        op: UnOp,
        operand: &TypedExpr,
        result_ty: &Ty,
        span: Span,
    ) -> Operand {
        let op_operand = self.build_expr(operand);
        let temp = self.new_temp(result_ty.clone(), span);
        
        self.assign(temp, Rvalue::Unary(op, op_operand), span);
        Operand::Local(temp)
    }

    fn build_assign(
        &mut self,
        target: &TypedExpr,
        value: &TypedExpr,
        span: Span,
    ) -> Operand {
        let value_op = self.build_expr(value);
        
        // 简化：假设目标是一个局部变量
        if let TypedExpr::Ident { def_id, .. } = target {
            if let Some(&local_id) = self.local_map.get(def_id) {
                self.assign(local_id, Rvalue::Use(value_op), span);
                return Operand::Literal(Literal::Unit);
            }
        }
        
        // 如果目标不是简单局部变量，返回单位值
        Operand::Literal(Literal::Unit)
    }

    fn build_call(
        &mut self,
        callee: DefId,
        args: &[TypedExpr],
        result_ty: &Ty,
        span: Span,
    ) -> Operand {
        let arg_operands: Vec<Operand> = args.iter()
            .map(|arg| self.build_expr(arg))
            .collect();
        
        // 创建临时变量存储结果
        let temp = self.new_temp(result_ty.clone(), span);
        
        // 简化处理：将函数调用视为聚合操作
        self.assign(
            temp,
            Rvalue::Aggregate(AggregateKind::Adt(callee), arg_operands),
            span,
        );
        
        Operand::Local(temp)
    }

    fn build_if(&mut self, condition: &TypedExpr, then_branch: &TypedBlock, else_branch: &Option<Box<TypedExpr>>, result_ty: &Ty, span: Span) -> Operand {
        let cond_op = self.build_expr(condition);
        
        let then_block = self.new_block();
        let else_block = self.new_block();
        let merge_block = self.new_block();
        
        let result_temp = self.new_temp(result_ty.clone(), span);
        
        // 条件跳转 - 设置当前块的终结符
        self.switch(
            cond_op,
            SwitchTargets {
                values: vec![1], // true = 1
                targets: vec![then_block],
                otherwise: else_block,
            },
            span,
        );

        // 构建 then 分支
        self.start_block(then_block);
        let then_result = self.build_block_expr(then_branch);
        self.assign(result_temp, Rvalue::Use(then_result), then_branch.span);
        self.goto(merge_block, then_branch.span);

        // 构建 else 分支  
        self.start_block(else_block);
        let else_result = if let Some(else_expr) = else_branch {
            self.build_expr(else_expr)
        } else {
            Operand::Literal(Literal::Unit)
        };
        self.assign(result_temp, Rvalue::Use(else_result), span);
        self.goto(merge_block, span);

        // 构建合并块
        self.start_block(merge_block);

        Operand::Local(result_temp)
    }

    fn build_loop(&mut self, body: &TypedBlock, span: Span) -> Operand {
        let loop_header = self.new_block();
        let loop_body = self.new_block();
        let loop_end = self.new_block();
        
        // 设置循环控制目标
        self.break_targets.push(loop_end);
        self.continue_targets.push(loop_header);
        
        // 跳转到循环头
        self.goto(loop_header, span);
        
        // 循环头
        self.start_block(loop_header);
        self.goto(loop_body, span);
        
        // 循环体
        self.start_block(loop_body);
        self.build_block(body);
        // 循环体结束后跳回头部（除非有 break）
        if !self.has_terminator() {
            self.goto(loop_header, body.span);
        }
        
        // 循环结束块
        self.start_block(loop_end);
        
        // 恢复循环控制目标
        self.break_targets.pop();
        self.continue_targets.pop();
        
        // loop 表达式返回 never 类型
        Operand::Literal(Literal::Never)
    }

    fn build_block_expr(&mut self, block: &TypedBlock) -> Operand {
        // 构建语句
        for stmt in &block.stmts {
            self.build_stmt(stmt);
        }
        
        // 返回尾表达式的结果
        if let Some(expr) = &block.tail {
            self.build_expr(expr)
        } else {
            Operand::Literal(Literal::Unit)
        }
    }

    // === 基础构建方法 ===

    fn reset_builder_state(&mut self) {
        self.current_function = None;
        self.current_block = BasicBlockId(0);
        self.next_local_id = 1;
        self.next_block_id = 1;
        self.local_map.clear();
        self.basic_blocks.clear();
        self.current_statements.clear();
        self.break_targets.clear();
        self.continue_targets.clear();
    }

    fn start_block(&mut self, id: BasicBlockId) {
        // 如果切换到新块，完成当前块
        if id != self.current_block {
            self.finish_block();
        }
        self.current_block = id;
        self.current_statements.clear();
    }

    fn finish_block(&mut self) {
        // 检查是否已经存在这个块
        let existing_block = self.basic_blocks.iter()
            .position(|b| b.id == self.current_block);
        
        if let Some(index) = existing_block {
            // 更新已存在的块
            let block = &mut self.basic_blocks[index];
            block.statements.extend(std::mem::take(&mut self.current_statements));
        } else if !self.current_statements.is_empty() || self.has_terminator_for_current_block() {
            // 创建新块
            let block = BasicBlock {
                id: self.current_block,
                statements: std::mem::take(&mut self.current_statements),
                terminator: self.get_terminator_for_current_block(),
            };
            self.basic_blocks.push(block);
        }
    }

    fn has_terminator_for_current_block(&self) -> bool {
        self.basic_blocks.iter()
            .find(|b| b.id == self.current_block)
            .and_then(|b| b.terminator.as_ref())
            .is_some()
    }

    fn get_terminator_for_current_block(&self) -> Option<Terminator> {
        self.basic_blocks.iter()
            .find(|b| b.id == self.current_block)
            .and_then(|b| b.terminator.clone())
    }

    fn has_terminator(&self) -> bool {
        // 检查当前块是否已经有终结符
        self.basic_blocks.iter()
            .find(|b| b.id == self.current_block)
            .and_then(|b| b.terminator.as_ref())
            .is_some()
    }

    fn declare_local(&mut self, name: StringId, ty: Ty, span: Span) -> LocalId {
        let local_id = self.next_local_id;
        self.next_local_id += 1;

        if let Some(function) = &mut self.current_function {
            function.locals.push(LocalDecl {
                ty,
                name: Some(name),
                span,
            });
        }

        litec_mir::LocalId(local_id)
    }

    fn new_temp(&mut self, ty: Ty, span: Span) -> LocalId {
        self.declare_local(StringId::from("temp"), ty, span)
    }

    fn new_block(&mut self) -> BasicBlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        litec_mir::BasicBlockId(id)
    }

    fn assign(&mut self, dest: LocalId, rvalue: Rvalue, span: Span) {
        self.current_statements.push(Statement::Assign {
            dest,
            rvalue,
            span,
        });
    }

    fn goto(&mut self, target: BasicBlockId, span: Span) {
        self.set_terminator(Terminator::Goto { target, span });
    }

    fn return_(&mut self, value: Operand, span: Span) {
        self.set_terminator(Terminator::Return { value, span });
    }

    fn switch(&mut self, discr: Operand, targets: SwitchTargets, span: Span) {
        self.set_terminator(Terminator::Switch { discr, targets, span });
    }

    fn set_terminator(&mut self, terminator: Terminator) {
        // 先完成当前块（处理语句）
        self.finish_block();
        
        // 设置终结符
        if let Some(block) = self.basic_blocks.iter_mut()
            .find(|b| b.id == self.current_block) {
            block.terminator = Some(terminator);
        } else {
            // 创建新块并设置终结符
            let block = BasicBlock {
                id: self.current_block,
                statements: Vec::new(),
                terminator: Some(terminator),
            };
            self.basic_blocks.push(block);
        }
    }

    fn finish_function(&mut self) -> MirFunction {
        // 完成最后一个块
        self.finish_block();

        let mut function = self.current_function.take().expect("No current function");
        function.basic_blocks = std::mem::take(&mut self.basic_blocks);
        
        function
    }
}

// 公共接口
pub fn build_mir(hir: &TypedCrate) -> Vec<MirFunction> {
    let builder = Builder::new(hir);
    builder.build_crate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use litec_span::{Span, StringId};
    use litec_typed_hir::{TypedBlock, TypedCrate, TypedExpr, TypedItem, TypedParam, TypedStmt, Visibility};
    use litec_hir::LiteralValue;

    // 创建一个虚拟的 Span 用于测试
    fn dummy_span() -> Span {
        Span::default() // 或者 Span { start: 0, end: 0 }
    }

    // 创建一个虚拟的 StringId 用于测试
    fn dummy_string_id() -> StringId {
        StringId(0)
    }

    // 创建一个虚拟的 DefId 用于测试
    fn dummy_def_id() -> DefId {
        DefId { 
            index: 0, 
            kind: litec_typed_hir::DefKind::Function 
        }
    }

    #[test]
    fn test_build_simple_function() {
        // 构建一个简单的 HIR 函数：fn add() -> i32 { 1 + 2 }
        let hir = TypedCrate {
            items: vec![
                TypedItem::Function {
                    def_id: dummy_def_id(),
                    visibility: Visibility::Public,
                    name: dummy_string_id(),
                    params: Vec::new(),
                    return_ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                    body: TypedBlock {
                        stmts: Vec::new(),
                        tail: Some(Box::new(TypedExpr::Addition {
                            left: Box::new(TypedExpr::Literal {
                                value: LiteralValue::Int { value: 1, kind: litec_hir::LitIntValue::I32 },
                                ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                span: dummy_span(),
                            }),
                            right: Box::new(TypedExpr::Literal {
                                value: LiteralValue::Int { value: 2, kind: litec_hir::LitIntValue::I32 },
                                ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                span: dummy_span(),
                            }),
                            ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                            span: dummy_span(),
                        })),
                        ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }
            ],
        };

        // 构建 MIR
        let functions = build_mir(&hir);
        
        // 严格的验证
        assert_eq!(functions.len(), 1, "Should generate exactly one function");
        
        let mir_func = &functions[0];
        assert_eq!(mir_func.def_id, dummy_def_id(), "Function DefId should match");
        assert_eq!(mir_func.name, dummy_string_id(), "Function name should match");
        
        // 验证基本块
        assert!(!mir_func.basic_blocks.is_empty(), "Function should have at least one basic block");
        
        let entry_block = &mir_func.basic_blocks[0];
        println!("Entry block statements: {:?}", entry_block.statements);
        println!("Entry block terminator: {:?}", entry_block.terminator);
        
        // 严格的语句检查
        assert_eq!(entry_block.statements.len(), 2, "Entry block should have exactly 2 statements");
        
        // 检查第一个语句：计算 1 + 2
        if let Statement::Assign { dest: LocalId(1), rvalue: Rvalue::Binary(BinOp::Add, Operand::Literal(Literal::I32(1)), Operand::Literal(Literal::I32(2))), .. } = &entry_block.statements[0] {
            // 正确：第一个语句计算 1 + 2
        } else {
            panic!("First statement should compute 1 + 2 and store to LocalId(1)");
        }
        
        // 检查第二个语句：存储结果到返回位置
        if let Statement::Assign { dest: LocalId(0), rvalue: Rvalue::Use(Operand::Local(LocalId(1))), .. } = &entry_block.statements[1] {
            // 正确：第二个语句存储到返回位置
        } else {
            panic!("Second statement should store result to return position LocalId(0)");
        }
        
        // 严格的终结符检查
        assert!(entry_block.terminator.is_some(), "Entry block must have a terminator");
        
        if let Some(Terminator::Return { value: Operand::Local(LocalId(0)), .. }) = &entry_block.terminator {
            // 正确：返回 LocalId(0) 的值
        } else {
            panic!("Entry block terminator should be Return with value from LocalId(0)");
        }
        
        // 验证返回类型
        assert_eq!(mir_func.locals[0].ty, Ty::Int(IntKind::I32), "Return type should be i32");
    }

    #[test]
    fn test_build_function_with_variables() {
        // 构建一个更复杂的函数：fn test() -> i32 { let x = 5; let y = 10; x + y }
        let x_def_id = DefId { 
            index: 1, 
            kind: litec_typed_hir::DefKind::Variable 
        };
        let y_def_id = DefId { 
            index: 2, 
            kind: litec_typed_hir::DefKind::Variable 
        };
        
        let hir = TypedCrate {
            items: vec![
                TypedItem::Function {
                    def_id: dummy_def_id(),
                    visibility: Visibility::Public,
                    name: dummy_string_id(),
                    params: Vec::new(),
                    return_ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                    body: TypedBlock {
                        stmts: vec![
                            TypedStmt::Let {
                                name: dummy_string_id(),
                                def_id: x_def_id,
                                ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                init: Some(Box::new(TypedExpr::Literal {
                                    value: LiteralValue::Int { value: 5, kind: litec_hir::LitIntValue::I32 },
                                    ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                    span: dummy_span(),
                                })),
                                span: dummy_span(),
                            },
                            TypedStmt::Let {
                                name: dummy_string_id(),
                                def_id: y_def_id,
                                ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                init: Some(Box::new(TypedExpr::Literal {
                                    value: LiteralValue::Int { value: 10, kind: litec_hir::LitIntValue::I32 },
                                    ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                    span: dummy_span(),
                                })),
                                span: dummy_span(),
                            },
                        ],
                        tail: Some(Box::new(TypedExpr::Addition {
                            left: Box::new(TypedExpr::Ident {
                                name: dummy_string_id(),
                                def_id: x_def_id,
                                ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                span: dummy_span(),
                            }),
                            right: Box::new(TypedExpr::Ident {
                                name: dummy_string_id(),
                                def_id: y_def_id,
                                ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                span: dummy_span(),
                            }),
                            ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                            span: dummy_span(),
                        })),
                        ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }
            ],
        };

        let functions = build_mir(&hir);
        assert_eq!(functions.len(), 1, "Should generate exactly one function");
        
        let mir_func = &functions[0];
        
        // 严格的局部变量检查
        assert!(mir_func.locals.len() >= 3, "Should have at least return position + x + y locals");
        
        // 检查基本块结构
        assert!(!mir_func.basic_blocks.is_empty(), "Should have at least one basic block");
        
        let entry_block = &mir_func.basic_blocks[0];
        assert!(entry_block.terminator.is_some(), "Entry block must have a terminator");
        
        // 验证有变量赋值和计算语句
        assert!(!entry_block.statements.is_empty(), "Should have statements for variable assignments and computation");
    }

    #[test]
    fn test_build_function_with_parameters() {
        // 构建带参数的函数：fn add(a: i32, b: i32) -> i32 { a + b }
        let a_def_id = DefId { 
            index: 1, 
            kind: litec_typed_hir::DefKind::Variable 
        };
        let b_def_id = DefId { 
            index: 2, 
            kind: litec_typed_hir::DefKind::Variable 
        };
        
        let hir = TypedCrate {
            items: vec![
                TypedItem::Function {
                    def_id: dummy_def_id(),
                    visibility: Visibility::Public,
                    name: dummy_string_id(),
                    params: vec![
                        TypedParam {
                            name: dummy_string_id(),
                            def_id: a_def_id,
                            ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                            span: dummy_span(),
                        },
                        TypedParam {
                            name: dummy_string_id(),
                            def_id: b_def_id,
                            ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                            span: dummy_span(),
                        },
                    ],
                    return_ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                    body: TypedBlock {
                        stmts: Vec::new(),
                        tail: Some(Box::new(TypedExpr::Addition {
                            left: Box::new(TypedExpr::Ident {
                                name: dummy_string_id(),
                                def_id: a_def_id,
                                ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                span: dummy_span(),
                            }),
                            right: Box::new(TypedExpr::Ident {
                                name: dummy_string_id(),
                                def_id: b_def_id,
                                ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                                span: dummy_span(),
                            }),
                            ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                            span: dummy_span(),
                        })),
                        ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }
            ],
        };

        let functions = build_mir(&hir);
        assert_eq!(functions.len(), 1, "Should generate exactly one function");
        
        let mir_func = &functions[0];
        
        // 严格的参数检查
        assert!(mir_func.locals.len() >= 3, "Should have at least return position + a + b locals");
        
        // 检查基本块结构
        assert!(!mir_func.basic_blocks.is_empty(), "Should have at least one basic block");
        
        let entry_block = &mir_func.basic_blocks[0];
        assert!(entry_block.terminator.is_some(), "Entry block must have a terminator");
        
        // 验证有参数使用和计算语句
        assert!(!entry_block.statements.is_empty(), "Should have statements for parameter usage and computation");
    }

    // 辅助函数：打印 MIR 的调试信息
    fn debug_print_mir(functions: &[MirFunction]) {
        for (i, func) in functions.iter().enumerate() {
            println!("Function {}: {:?}", i, func.name);
            println!("  DefId: {:?}", func.def_id);
            println!("  Locals: {}", func.locals.len());
            for (j, local) in func.locals.iter().enumerate() {
                println!("    Local {}: {:?} ({:?})", j, local.ty, local.name);
            }
            println!("  Basic blocks: {}", func.basic_blocks.len());
            for block in &func.basic_blocks {
                println!("    Block {:?}:", block.id);
                println!("      Statements: {}", block.statements.len());
                for stmt in &block.statements {
                    println!("        {:?}", stmt);
                }
                println!("      Terminator: {:?}", block.terminator);
            }
            println!();
        }
    }

    #[test]
    fn test_debug_output() {
        // 构建一个简单函数用于调试输出
        let hir = TypedCrate {
            items: vec![
                TypedItem::Function {
                    def_id: dummy_def_id(),
                    visibility: Visibility::Public,
                    name: dummy_string_id(),
                    params: Vec::new(),
                    return_ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                    body: TypedBlock {
                        stmts: Vec::new(),
                        tail: Some(Box::new(TypedExpr::Literal {
                            value: LiteralValue::Int { value: 42, kind: litec_hir::LitIntValue::I32 },
                            ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                            span: dummy_span(),
                        })),
                        ty: litec_typed_hir::ty::Ty::Int(litec_typed_hir::ty::IntKind::I32),
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }
            ],
        };

        let functions = build_mir(&hir);
        debug_print_mir(&functions);
        
        // 严格的验证
        assert_eq!(functions.len(), 1);
        let mir_func = &functions[0];
        assert!(!mir_func.basic_blocks.is_empty());
        
        let entry_block = &mir_func.basic_blocks[0];
        assert!(entry_block.terminator.is_some(), "Debug function must have terminator");
    }
}