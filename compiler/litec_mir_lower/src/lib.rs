use litec_error::Diagnostic;
use litec_hir::{
    AssignOp, BinOp, IntKind, LiteralIntKind, LiteralValue, Mutability, PosOp, UIntKind, Visibility,
};
use litec_mir::{
    AggregateKind, BasicBlockId, Constant, ConstantKind, Field, GlobalDecl, Local, LocalDecl,
    MirCrate, MirExtern, MirExternFunction, MirExternItem, MirField, MirFunction, MirItem,
    MirModule, MirParam, MirStruct, Operand, Place, PlaceElem, Rvalue, Statement, StatementKind,
    SwitchTargets, Terminator, TerminatorKind,
};
use litec_span::{Span, StringId, intern_global};
use litec_typed_hir::{
    DefKind, TypedBlock, TypedCrate, TypedExpr, TypedExternItem, TypedField, TypedItem, TypedParam,
    TypedStmt, def_id::DefId, ty::Ty,
};
use rustc_hash::FxHashMap;

pub struct Builder {
    typed_crate: TypedCrate,
    // 当前正在构建的函数
    current_function: Option<MirFunction>,
    // 局部变量到 MIR Local 的映射
    local_map: FxHashMap<DefId, Local>,
    // 下一个可用的 Local 索引
    next_local: usize,
    // 当前基本块 ID
    current_block_id: BasicBlockId,
    // 循环上下文 (循环开始块, break 目标块, continue 目标块, 循环值)
    loop_context: Vec<(BasicBlockId, BasicBlockId, BasicBlockId, Place)>,
    diagnostics: Vec<Diagnostic>,
    globals: FxHashMap<DefId, GlobalDecl>,
}

impl Builder {
    pub fn new(typed_crate: TypedCrate) -> Self {
        Self {
            typed_crate,
            current_function: None,
            local_map: FxHashMap::default(),
            next_local: 0,
            current_block_id: BasicBlockId(0),
            loop_context: Vec::new(),
            diagnostics: Vec::new(),
            globals: FxHashMap::default(),
        }
    }

    pub fn build(mut self) -> MirCrate {
        let typed_items = std::mem::take(&mut self.typed_crate.items);
        let mut mir_items = Vec::new();
        for item in typed_items {
            if let Some(item) = self.build_item(item) {
                mir_items.push(item);
            }
        }
        MirCrate {
            items: mir_items,
            globals: self.globals,
            builtin: self.typed_crate.builtin,
            definitions: self.typed_crate.definitions,
        }
    }

    fn build_item(&mut self, item: TypedItem) -> Option<MirItem> {
        match item {
            TypedItem::Function {
                def_id,
                visibility,
                name,
                params,
                return_ty,
                body,
                span,
            } => Some(MirItem::Function(self.build_function(
                def_id, visibility, name, params, return_ty, body, span,
            )?)),

            TypedItem::Struct {
                def_id,
                visibility,
                name,
                fields,
                span,
            } => {
                // 将结构体信息保存到 MIR 中
                Some(MirItem::Struct(MirStruct {
                    def_id,
                    visibility,
                    name,
                    fields: fields
                        .iter()
                        .map(|f| MirField {
                            def_id: f.def_id,
                            name: f.name,
                            ty: f.ty.clone(),
                            visibility: f.visibility,
                            span: f.span,
                        })
                        .collect(),
                    span,
                }))
            }

            TypedItem::Use { .. } => {
                // Use 语句在 MIR 中不需要生成代码
                // 它们只在名称解析阶段有用
                None
            }

            TypedItem::Module {
                def_id,
                visibility,
                name,
                items,
                span,
            } => {
                // 递归处理模块中的所有项
                let mut mir_items = Vec::new();
                for item in items {
                    if let Some(mir_item) = self.build_item(item) {
                        mir_items.push(mir_item);
                    }
                }

                // 如果模块中有生成的 MIR 项，则返回模块
                if mir_items.is_empty() {
                    None
                } else {
                    Some(MirItem::Module(MirModule {
                        def_id,
                        visibility,
                        name,
                        items: mir_items,
                        span,
                    }))
                }
            }

            TypedItem::Extern {
                visibility,
                abi,
                items,
                span,
            } => {
                // 处理外部函数声明
                let mut mir_extern_items = Vec::new();
                for extern_item in items {
                    match extern_item {
                        TypedExternItem::Function {
                            def_id,
                            name,
                            params,
                            is_variadic,
                            return_ty,
                            span,
                        } => {
                            // 外部函数在 MIR 中不需要生成函数体
                            // 只需要记录函数签名
                            mir_extern_items.push(MirExternItem::Function(MirExternFunction {
                                def_id,
                                name,
                                params: params
                                    .iter()
                                    .map(|p| MirParam {
                                        def_id: p.def_id,
                                        name: p.name,
                                        ty: p.ty.clone(),
                                        span: p.span,
                                    })
                                    .collect(),
                                is_variadic: is_variadic,
                                return_ty: Some(return_ty.clone()),
                                span,
                            }));
                        }
                    }
                }

                // 如果有外部项，则返回外部块
                if mir_extern_items.is_empty() {
                    None
                } else {
                    Some(MirItem::Extern(MirExtern {
                        def_id: DefId::new(0, DefKind::Extern), // 使用一个虚拟的 def_id
                        visibility,
                        name: intern_global("extern"), // 使用固定的名称
                        abi,
                        items: mir_extern_items,
                        span,
                    }))
                }
            }
        }
    }

    fn build_function(
        &mut self,
        def_id: DefId,
        visibility: Visibility,
        name: StringId,
        params: Vec<TypedParam>,
        return_ty: Ty,
        body: TypedBlock,
        span: Span,
    ) -> Option<MirFunction> {
        let mir_function = MirFunction {
            def_id,
            local_decls: Vec::new(),
            basic_blocks: Vec::new(),
            args: Vec::with_capacity(params.len()),
            return_ty: return_ty,
            span,
        };

        self.current_function = Some(mir_function);
        self.local_map.clear();
        self.next_local = 0;

        for param in &params {
            let local = self.new_local(param.ty.clone(), Mutability::Const, Some(param.name), span);
            self.local_map.insert(param.def_id, local);
            self.current_function.as_mut().unwrap().args.push(local);
        }

        let entry_block = self.new_basic_block(span);
        self.set_current_block(entry_block);

        let tail_place = self.build_block(&body);

        self.terminate(Terminator {
            kind: TerminatorKind::Return {
                value: Operand::Move(tail_place),
                is_explicit: false,
            },
            span: span,
        });

        let mir_function = self.current_function.take().unwrap();
        Some(mir_function)
    }

    fn build_block(&mut self, block: &TypedBlock) -> Place {
        for stmt in &block.stmts {
            self.build_stmt(&stmt);
        }

        if let Some(tail_expr) = &block.tail {
            let tail_place = self.build_expr(tail_expr);
            tail_place
        } else {
            let tail_place = Place::local(
                self.new_local(Ty::Unit, Mutability::Const, None, block.span),
                block.ty.clone(),
            );
            tail_place
        }
    }

    fn build_stmt(&mut self, stmt: &TypedStmt) {
        match stmt {
            TypedStmt::Expr(typed_expr) => {
                // 表达式语句：构建表达式但不使用其结果
                self.build_expr(typed_expr);
            }
            TypedStmt::Let {
                mutable,
                name,
                def_id,
                ty,
                init,
                span,
            } => {
                // 创建新的局部变量
                let local = self.new_local(ty.clone(), *mutable, Some(*name), *span);
                self.local_map.insert(*def_id, local);

                // 如果有初始化表达式，则构建并赋值
                if let Some(init_expr) = init {
                    let init_place = self.build_expr(init_expr);
                    self.assign(
                        Place::local(local, init_expr.ty()),
                        Rvalue::Use(
                            self.move_or_copy_operand(&init.as_ref().unwrap().ty(), init_place),
                        ),
                        *span,
                    );
                }
            }
            TypedStmt::Return { value, span } => {
                // 返回语句：构建返回值并终止当前基本块
                let operand = match value {
                    Some(expr) => Operand::Copy(self.build_expr(expr)),
                    None => Operand::Constant(Constant {
                        kind: ConstantKind::Unit,
                        span: *span,
                    }),
                };
                self.terminate(Terminator {
                    kind: TerminatorKind::Return {
                        value: operand,
                        is_explicit: true,
                    },
                    span: *span,
                });
            }
            TypedStmt::Break { value, ty: _, span } => {
                // 获取循环上下文中的 break 目标块
                let break_target = self.loop_context.last().expect("break outside of loop").1;

                // 如果有返回值，则构建并赋值
                if let Some(expr) = value {
                    let value_place = self.build_expr(expr);
                    let result_place = self.loop_context.last().unwrap().3.clone();
                    self.assign(result_place, Rvalue::Use(Operand::Copy(value_place)), *span);
                }

                // 跳转到 break 目标块
                self.terminate(Terminator {
                    kind: TerminatorKind::Goto {
                        target: break_target,
                    },
                    span: *span,
                });
            }
            TypedStmt::Continue { ty, span } => {
                // 获取循环上下文中的 continue 目标块
                let continue_target = self
                    .loop_context
                    .last()
                    .expect("continue outside of loop")
                    .2;

                // 跳转到 continue 目标块
                self.terminate(Terminator {
                    kind: TerminatorKind::Goto {
                        target: continue_target,
                    },
                    span: *span,
                });
            }
        }
    }

    fn build_expr(&mut self, expr: &TypedExpr) -> Place {
        match expr {
            TypedExpr::Literal { value, ty, span } => {
                let place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );
                self.assign(
                    place.clone(),
                    Rvalue::Use(Operand::Constant(Constant {
                        kind: ConstantKind::Literal {
                            value: value.clone(),
                            ty: ty.clone(),
                        },
                        span: *span,
                    })),
                    *span,
                );
                place
            }
            TypedExpr::Local { def_id, ty, .. } => {
                let local = self.local_map.get(&def_id).unwrap();
                Place::local(*local, ty.clone())
            }
            TypedExpr::Global { def_id, ty, span } => {
                todo!()
            }
            TypedExpr::Binary {
                left,
                op,
                right,
                ty,
                span,
            } => {
                let left_place = self.build_expr(left);
                let right_place = self.build_expr(right);

                let place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                self.assign(
                    place.clone(),
                    Rvalue::BinaryOp(*op, Operand::Copy(left_place), Operand::Copy(right_place)),
                    *span,
                );

                place
            }
            TypedExpr::Unary {
                op,
                operand,
                ty,
                span,
            } => {
                let opearnd_place = self.build_expr(operand);

                let place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                self.assign(
                    place.clone(),
                    Rvalue::UnaryOp(*op, Operand::Copy(opearnd_place)),
                    *span,
                );

                place
            }
            TypedExpr::Postfix {
                operand,
                op,
                ty,
                span,
            } => {
                let operand_place = self.build_expr(operand);
                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                // 先保存原始值
                self.assign(
                    result_place.clone(),
                    Rvalue::Use(Operand::Copy(operand_place.clone())),
                    *span,
                );

                // 然后执行后缀操作
                match op {
                    PosOp::Plus => {
                        self.assign(
                            operand_place,
                            Rvalue::BinaryOp(
                                BinOp::Add,
                                Operand::Copy(result_place.clone()),
                                Operand::Constant(Constant {
                                    kind: ConstantKind::Literal {
                                        value: LiteralValue::Int {
                                            value: 1,
                                            kind: LiteralIntKind::Signed(IntKind::I32),
                                        },
                                        ty: ty.clone(),
                                    },
                                    span: *span,
                                }),
                            ),
                            *span,
                        );
                    }
                    PosOp::Sub => {
                        self.assign(
                            operand_place,
                            Rvalue::BinaryOp(
                                BinOp::Sub,
                                Operand::Copy(result_place.clone()),
                                Operand::Constant(Constant {
                                    kind: ConstantKind::Literal {
                                        value: LiteralValue::Int {
                                            value: 1,
                                            kind: LiteralIntKind::Signed(IntKind::I32),
                                        },
                                        ty: ty.clone(),
                                    },
                                    span: *span,
                                }),
                            ),
                            *span,
                        );
                    }
                }

                result_place
            }
            TypedExpr::Assign {
                target,
                op,
                value,
                ty,
                span,
            } => {
                let target_place = self.build_expr(target);
                let value_place = self.build_expr(value);

                match op {
                    AssignOp::Simple => {
                        if value.ty().is_copyable() {
                            // Copy 类型：使用 Copy 操作数
                            self.assign(
                                target_place.clone(),
                                Rvalue::Use(Operand::Copy(value_place)),
                                *span,
                            );
                        } else {
                            // 非 Copy 类型：使用 Move 操作数
                            self.assign(
                                target_place.clone(),
                                Rvalue::Use(Operand::Move(value_place)),
                                *span,
                            );
                        }
                    }
                    _ => {
                        // 复合赋值：先计算新值，再赋给目标
                        let temp_place = Place::local(
                            self.new_local(ty.clone(), Mutability::Const, None, *span),
                            ty.clone(),
                        );
                        self.assign(
                            temp_place.clone(),
                            Rvalue::BinaryOp(
                                self.assign_op_to_bin_op(*op),
                                Operand::Copy(target_place.clone()),
                                Operand::Copy(value_place),
                            ),
                            *span,
                        );
                        self.assign(
                            target_place.clone(),
                            Rvalue::Use(Operand::Copy(temp_place)),
                            *span,
                        );
                    }
                }

                target_place
            }
            TypedExpr::AddressOf { expr, ty, span } => {
                let expr_place = self.build_expr(expr);
                let address_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Mut, None, *span),
                    ty.clone(),
                );
                self.assign(address_place.clone(), Rvalue::address_of(expr_place), *span);
                address_place
            }
            TypedExpr::Dereference { expr, ty, .. } => {
                let inner_place = self.build_place(expr); // 递归获取 place
                Place {
                    local: inner_place.local,
                    projection: [inner_place.projection, vec![PlaceElem::Deref]].concat(),
                    ty: ty.clone(),
                }
            }
            TypedExpr::Call {
                callee,
                args,
                ty,
                span,
            } => {
                let mut operands = Vec::new();
                for arg in args.iter() {
                    let arg_place = self.build_expr(arg);

                    let operand = if arg.ty().is_copyable() {
                        Operand::Copy(arg_place)
                    } else {
                        Operand::Move(arg_place)
                    };
                    operands.push(operand);
                }

                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );
                let next_block = self.new_basic_block(*span);

                self.terminate(Terminator {
                    kind: TerminatorKind::Call {
                        function: *callee,
                        args: operands,
                        destination: result_place.clone(),
                        target: next_block,
                    },
                    span: *span,
                });

                self.set_current_block(next_block);
                result_place
            }
            TypedExpr::Block {
                block,
                ty: _,
                span: _,
            } => {
                let result_place = self.build_block(&block);

                result_place
            }
            TypedExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
                span,
            } => {
                let then_block = self.new_basic_block(*span);
                let else_block = self.new_basic_block(*span);
                let merge_block = self.new_basic_block(*span);

                // 构建条件表达式
                let condition_place = self.build_expr(condition);

                // 设置当前块的终止器为条件跳转
                self.terminate(Terminator {
                    kind: TerminatorKind::SwitchInt {
                        discr: Operand::Copy(condition_place),
                        targets: SwitchTargets::if_else(then_block, else_block),
                    },
                    span: *span,
                });

                // 构建then分支
                self.set_current_block(then_block);
                let then_place = self.build_block(then_branch);

                // 跳转到merge块
                if self.current_block_terminator().is_none() {
                    self.terminate(Terminator {
                        kind: TerminatorKind::Goto {
                            target: merge_block,
                        },
                        span: *span,
                    });
                }

                // 构建else分支
                self.set_current_block(else_block);
                let else_place = match else_branch {
                    Some(else_expr) => self.build_expr(else_expr),
                    None => Place::local(
                        self.new_local(ty.clone(), Mutability::Const, None, *span),
                        ty.clone(),
                    ),
                };

                // 跳转到merge块
                if self.current_block_terminator().is_none() {
                    self.terminate(Terminator {
                        kind: TerminatorKind::Goto {
                            target: merge_block,
                        },
                        span: *span,
                    });
                }

                // 在merge块创建结果变量
                self.set_current_block(merge_block);
                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                // 根据是否有else分支决定如何赋值
                if else_branch.is_some() {
                    // 有else分支，需要根据条件选择 then_place 或 else_place
                    // 重新计算条件表达式用于选择
                    let condition_place = self.build_expr(condition);

                    // 创建临时变量存储 else_place 的值
                    let temp_else_place = Place::local(
                        self.new_local(ty.clone(), Mutability::Const, None, *span),
                        ty.clone(),
                    );
                    self.assign(
                        temp_else_place.clone(),
                        Rvalue::Use(Operand::Copy(else_place)),
                        *span,
                    );

                    // 创建临时变量存储 then_place 的值
                    let temp_then_place = Place::local(
                        self.new_local(ty.clone(), Mutability::Const, None, *span),
                        ty.clone(),
                    );
                    self.assign(
                        temp_then_place.clone(),
                        Rvalue::Use(Operand::Copy(then_place)),
                        *span,
                    );

                    // 创建新块处理 else 情况
                    let else_assign_block = self.new_basic_block(*span);

                    // 先设置当前块为 merge 块并赋值 then 的结果
                    self.set_current_block(merge_block);
                    self.assign(
                        result_place.clone(),
                        Rvalue::Use(Operand::Copy(temp_then_place)),
                        *span,
                    );

                    // 设置 else_assign_block 的内容
                    self.set_current_block(else_assign_block);
                    self.assign(
                        result_place.clone(),
                        Rvalue::Use(Operand::Copy(temp_else_place)),
                        *span,
                    );
                    self.terminate(Terminator {
                        kind: TerminatorKind::Goto {
                            target: merge_block,
                        },
                        span: *span,
                    });

                    // 回到 merge 块并设置终止器
                    self.set_current_block(merge_block);
                    self.terminate(Terminator {
                        kind: TerminatorKind::SwitchInt {
                            discr: Operand::Copy(condition_place),
                            targets: SwitchTargets::if_else(
                                merge_block,       // 条件为真，保持当前值
                                else_assign_block, // 条件为假，跳转到 else_assign_block
                            ),
                        },
                        span: *span,
                    });
                } else {
                    self.assign(
                        result_place.clone(),
                        Rvalue::Use(Operand::Move(then_place)),
                        *span,
                    );
                }

                result_place
            }
            TypedExpr::Loop { body, ty, span } => {
                // 创建循环的基本块
                let loop_header = self.new_basic_block(*span);
                let loop_body = self.new_basic_block(*span);
                let loop_exit = self.new_basic_block(*span);

                // 跳转到循环头部
                self.terminate(Terminator {
                    kind: TerminatorKind::Goto {
                        target: loop_header,
                    },
                    span: *span,
                });

                // 设置当前块为循环头部并跳转到循环体
                self.set_current_block(loop_header);
                self.terminate(Terminator {
                    kind: TerminatorKind::Goto { target: loop_body },
                    span: *span,
                });

                let loop_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Mut, None, *span),
                    ty.clone(),
                );
                // 将循环上下文推入栈中
                self.loop_context
                    .push((loop_header, loop_exit, loop_header, loop_place.clone()));

                // 构建循环体
                self.set_current_block(loop_body);
                self.build_block(body);

                // 如果循环体没有终止器，则跳转回循环头部
                if self.current_block_terminator().is_none() {
                    self.terminate(Terminator {
                        kind: TerminatorKind::Goto {
                            target: loop_header,
                        },
                        span: *span,
                    });
                }

                // 从循环上下文栈中弹出当前循环上下文
                self.loop_context.pop();

                // 设置当前块为循环出口
                self.set_current_block(loop_exit);

                // 创建结果变量
                self.assign(
                    loop_place.clone(),
                    Rvalue::Use(Operand::Constant(Constant {
                        kind: ConstantKind::Unit,
                        span: *span,
                    })),
                    *span,
                );

                loop_place
            }
            TypedExpr::FieldAccess { base, field, .. } => {
                // 构建基础表达式获取结构体实例
                let base_place = self.build_expr(base);

                base_place.field_access(field.clone())
            }
            TypedExpr::Index {
                indexed,
                index,
                ty,
                span,
            } => {
                // 构建被索引的表达式获取数组/切片的Place
                let indexed_place = self.build_expr(indexed);

                // 构建索引表达式获取索引值的Place
                let index_place = self.build_expr(index);

                match index_place {
                    Place {
                        local, projection, ..
                    } if projection.is_empty() => indexed_place.index(local),
                    _ => {
                        let temp_local = self.new_local(
                            Ty::UInt(UIntKind::Usize),
                            Mutability::Const,
                            None,
                            *span,
                        );
                        self.assign(
                            Place::local(temp_local, indexed.ty()),
                            Rvalue::Use(Operand::Copy(index_place)),
                            *span,
                        );
                        indexed_place.index(temp_local)
                    }
                }
            }
            TypedExpr::Tuple { elements, ty, span } => {
                // 构建所有元素的Place
                let element_places: Vec<Place> =
                    elements.iter().map(|e| self.build_expr(e)).collect();

                // 创建结果变量
                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                // 使用Rvalue::Aggregate创建元组
                self.assign(
                    result_place.clone(),
                    Rvalue::Aggregate(
                        AggregateKind::Tuple,
                        element_places
                            .iter()
                            .map(|p| Operand::Copy(p.clone()))
                            .collect(),
                    ),
                    *span,
                );

                result_place
            }
            TypedExpr::Unit { ty, span } => {
                // 创建结果变量
                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                // 赋值为Unit常量
                self.assign(
                    result_place.clone(),
                    Rvalue::Use(Operand::Constant(Constant {
                        kind: ConstantKind::Unit,
                        span: *span,
                    })),
                    *span,
                );

                result_place
            }
            TypedExpr::To {
                start,
                end,
                ty,
                span,
            } => {
                // 构建起始和结束表达式
                let start_place = self.build_expr(start);
                let end_place = self.build_expr(end);

                // 创建结果变量
                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                // 创建范围值
                self.assign(
                    result_place.clone(),
                    Rvalue::Aggregate(
                        AggregateKind::Array(ty.clone()),
                        vec![Operand::Copy(start_place), Operand::Copy(end_place)],
                    ),
                    *span,
                );

                result_place
            }
            TypedExpr::ToEq {
                start,
                end,
                ty,
                span,
            } => {
                // 构建起始和结束表达式
                let start_place = self.build_expr(start);
                let end_place = self.build_expr(end);

                // 创建结果变量
                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                // 创建包含式范围值
                self.assign(
                    result_place.clone(),
                    Rvalue::Aggregate(
                        AggregateKind::Array(ty.clone()),
                        vec![Operand::Copy(start_place), Operand::Copy(end_place)],
                    ),
                    *span,
                );

                result_place
            }
            TypedExpr::Grouped { expr, .. } => self.build_expr(expr),
            TypedExpr::StructInit {
                def_id,
                fields,
                ty,
                span,
            } => {
                // 构建所有字段的Place
                let field_operands: Vec<Operand> = fields
                    .iter()
                    .map(|(_, expr)| {
                        let expr_place = self.build_expr(expr);
                        Operand::Copy(expr_place)
                    })
                    .collect();

                // 创建结果变量
                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                self.assign(
                    result_place.clone(),
                    Rvalue::Aggregate(AggregateKind::Adt(*def_id, vec![]), field_operands),
                    *span,
                );

                result_place
            }
            TypedExpr::Cast {
                expr,
                kind,
                ty,
                span,
            } => {
                let source_place = self.build_expr(expr);

                let result_place = Place::local(
                    self.new_local(ty.clone(), Mutability::Const, None, *span),
                    ty.clone(),
                );

                let rvalue = Rvalue::Cast(*kind, source_place);

                self.assign(result_place.clone(), rvalue, *span);

                result_place
            }
        }
    }

    fn build_place(&mut self, expr: &TypedExpr) -> Place {
        match expr {
            TypedExpr::Local { def_id, ty, .. } => {
                let local = self.local_map.get(def_id).unwrap();
                Place::local(*local, ty.clone())
            }
            TypedExpr::Dereference { expr, ty, .. } => {
                let inner = self.build_place(expr);
                Place {
                    local: inner.local,
                    projection: [inner.projection, vec![PlaceElem::Deref]].concat(),
                    ty: ty.clone(),
                }
            }
            TypedExpr::FieldAccess { base, field, .. } => {
                let base_place = self.build_place(base);
                Place {
                    local: base_place.local,
                    projection: [base_place.projection, vec![PlaceElem::Field(field.clone())]]
                        .concat(),
                    ty: field.ty.clone(),
                }
            }
            // 其他表达式不能作为左值
            _ => panic!("Cannot use as place: {:?}", expr),
        }
    }

    fn new_local(
        &mut self,
        ty: Ty,
        mutablility: Mutability,
        name: Option<StringId>,
        span: Span,
    ) -> Local {
        let local = Local(self.next_local);
        self.next_local += 1;
        self.current_function
            .as_mut()
            .unwrap()
            .local_decls
            .push(LocalDecl {
                ty,
                mutability: mutablility,
                name,
                span,
            });
        local
    }

    fn new_global(
        &mut self,
        def_id: DefId,
        name: StringId,
        ty: Ty,
        init: Option<Constant>,
        span: Span,
    ) {
        self.globals.insert(
            def_id,
            GlobalDecl {
                def_id,
                name,
                ty,
                init,
                span,
            },
        );
    }

    fn new_basic_block(&mut self, span: Span) -> BasicBlockId {
        self.current_function
            .as_mut()
            .unwrap()
            .new_basic_block(span)
    }

    fn set_current_block(&mut self, block_id: BasicBlockId) {
        self.current_block_id = block_id;
    }

    fn current_block_terminator(&self) -> Option<&Terminator> {
        if let Some(func) = &self.current_function {
            Some(&func.basic_block(self.current_block_id).terminator)
        } else {
            None
        }
    }

    fn assign(&mut self, place: Place, rvalue: Rvalue, span: Span) {
        if let Some(func) = &mut self.current_function {
            func.basic_block_mut(self.current_block_id)
                .statements
                .push(Statement {
                    span,
                    kind: StatementKind::Assign { place, rvalue },
                });
        }
    }

    fn terminate(&mut self, terminator: Terminator) {
        if let Some(func) = &mut self.current_function {
            func.basic_block_mut(self.current_block_id).terminator = terminator;
        }
    }

    fn assign_op_to_bin_op(&self, op: AssignOp) -> BinOp {
        match op {
            AssignOp::Add => BinOp::Add,
            AssignOp::Sub => BinOp::Sub,
            AssignOp::Mul => BinOp::Mul,
            AssignOp::Div => BinOp::Div,
            AssignOp::Rem => BinOp::Rem,
            AssignOp::BitAnd => BinOp::BitAnd,
            AssignOp::BitOr => BinOp::BitOr,
            AssignOp::BitXor => BinOp::BitXor,
            AssignOp::Shl => BinOp::Shl,
            AssignOp::Shr => BinOp::Shr,
            _ => BinOp::Add,
        }
    }

    fn move_or_copy_operand(&self, ty: &Ty, place: Place) -> Operand {
        if ty.is_copyable() {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    fn get_def_id_name(&self, def_id: DefId) -> String {
        self.typed_crate.definitions[def_id.index as usize]
            .name
            .to_string()
    }
}

pub fn build(typed_crate: TypedCrate) -> MirCrate {
    Builder::new(typed_crate).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use litec_lower::lower;
    use litec_name_resolver::Resolver;
    use litec_parse::parser::parse;
    use litec_span::SourceMap;
    use litec_type_checker::{TypeChecker, check};
    use std::path::PathBuf;

    // 辅助函数：从源代码生成 MIR
    fn mir_from_source(source: &str) -> MirCrate {
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "test.lt".to_string(),
            source.to_string(),
            &PathBuf::from("test.lt"),
        );

        // 解析
        let (ast, diagnostics) = parse(&mut source_map, file_id);
        for diagnostic in &diagnostics {
            println!("{}", diagnostic.render(&source_map))
        }
        assert!(diagnostics.is_empty());

        let (hir, diagnostics) = lower(ast);
        for diagnostic in &diagnostics {
            println!("{}", diagnostic.render(&source_map))
        }
        assert!(diagnostics.is_empty());

        // 名称解析
        let resolver = Resolver::new(&mut source_map, file_id);
        let resolve_output = resolver.resolve(&hir);

        // 类型检查
        let type_checker = TypeChecker::new(resolve_output);
        let (typed_crate, diagnostics) = type_checker.check_crate();
        for diagnostic in &diagnostics {
            println!("{}", diagnostic.render(&source_map))
        }
        assert!(diagnostics.is_empty());

        // MIR 生成
        let builder = Builder::new(typed_crate);
        builder.build()
    }

    #[test]
    fn test_simple_function() {
        let source = r#"
        fn add(a: i32, b: i32) -> i32 {
            a + b
        }
        "#;

        let mir_crate = mir_from_source(source);

        // 验证 MIR 中有一个函数
        assert_eq!(mir_crate.items.len(), 1);

        // 验证函数是 add
        if let MirItem::Function(func) = &mir_crate.items[0] {
            assert_eq!(func.args.len(), 2); // 两个参数
            assert_eq!(func.local_decls.len(), 3); // 两个参数 + 一个返回值
            assert_eq!(func.basic_blocks.len(), 1); // 一个基本块
        } else {
            panic!("Expected a function item");
        }
    }

    #[test]
    fn test_if_expression() {
        let source = r#"
        fn max(a: i32, b: i32) -> i32 {
            if a > b { 
                a
            } else { 
                b
            }
        }
        "#;

        let mir_crate = mir_from_source(source);

        if let MirItem::Function(func) = &mir_crate.items[0] {
            // if 表达式应该生成多个基本块
            assert!(func.basic_blocks.len() > 1);

            // 验证有条件跳转
            let has_conditional = func.basic_blocks.iter().any(|bb| {
                matches!(
                    &bb.terminator,
                    Terminator {
                        kind: TerminatorKind::SwitchInt { .. },
                        span: _
                    }
                )
            });
            assert!(has_conditional, "Expected a conditional terminator");
        } else {
            panic!("Expected a function item");
        }
    }

    #[test]
    fn test_loop() {
        let source = r#"
        fn sum(n: i32) -> i32 {
            let mut result = 0;
            let mut i = 0;
            loop {
                if i >= n { break result; }
                result = result + i;
                i = i + 1;
            }
        }
        "#;

        let mir_crate = mir_from_source(source);

        if let MirItem::Function(func) = &mir_crate.items[0] {
            // 循环应该生成多个基本块
            assert!(func.basic_blocks.len() > 1);

            // 验证有循环结构
            let has_loop = func.basic_blocks.iter().any(|bb| {
                matches!(
                    &bb.terminator,
                    Terminator {
                        kind: TerminatorKind::Goto { .. },
                        span: _
                    }
                )
            });
            assert!(has_loop, "Expected a loop structure");
        } else {
            panic!("Expected a function item");
        }
    }

    #[test]
    fn test_struct() {
        let source = r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn new_point(x: i32, y: i32) -> Point {
            Point { x: x, y: y }
        }
        "#;

        let mir_crate = mir_from_source(source);

        // 验证 MIR 中有一个结构体和一个函数
        assert_eq!(mir_crate.items.len(), 2);

        // 验证结构体
        if let MirItem::Struct(struct_def) = &mir_crate.items[0] {
            assert_eq!(struct_def.fields.len(), 2);
        } else {
            panic!("Expected a struct item");
        }
    }

    #[test]
    fn test_extern_function() {
        let source = r#"
        extern "C" {
            fn printf(fmt: str, ...) -> i32;
        }

        fn main() {
            printf("Hello, world!\n");
        }
        "#;

        let mir_crate = mir_from_source(source);

        // 验证 MIR 中有一个外部块和一个函数
        assert_eq!(mir_crate.items.len(), 2);

        // 验证外部块
        if let MirItem::Extern(extern_block) = &mir_crate.items[0] {
            assert_eq!(extern_block.items.len(), 1);
        } else {
            panic!("Expected an extern block");
        }
    }

    #[test]
    fn test_main_function() {
        let source = r#"
        fn main() -> i32 {
            42
        }
        "#;

        let mir_crate = mir_from_source(source);

        // 验证 MIR 中有一个函数
        assert_eq!(mir_crate.items.len(), 1);

        // 验证函数是 main
        if let MirItem::Function(func) = &mir_crate.items[0] {
            assert_eq!(
                func.return_ty,
                Ty::Int(IntKind::I32)
            );
        } else {
            panic!("Expected a function item");
        }
    }

    #[test]
    fn test_program() {
        let source = r#"
        extern "C" {
            fn printf(fmt: str, ...) -> i32;
        }
            
        fn main() {
            let a = -1;
            let b = a as u32;
            printf("%d\n", b);
        }
        "#;

        let _mir_crate = mir_from_source(source);
    }
}
