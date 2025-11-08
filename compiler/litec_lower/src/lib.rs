use litec_ast::{ast::{
    Block as AstBlock, Crate as AstCrate, Expr as AstExpr, Field as AstField, Item as AstItem, Param as AstParam, 
    Stmt as AstStmt, TypeAnnotation, UseItem, Visibility as AstVisibility, Attribute as AstAttribute
}, token::{LiteralKind, TokenKind}};
use litec_hir::{
    Block as HirBlock, Crate as HirCrate, Expr as HirExpr, Field as HirField, LitFloatValue, LitIntValue, 
    Item as HirItem, LiteralValue, Param as HirParam, Stmt as HirStmt, Attribute as HirAttribute,
    Type as HirType, Visibility as HirVisibility, UseItem as HirUseItem
};
use litec_error::{Diagnostic, DiagnosticBuilder, error};
use litec_span::{Span, StringId, get_global_string, intern_global};

type LowerResult<T> = Result<T, Diagnostic>;

pub fn lower_crate(ast: AstCrate) -> Result<HirCrate, Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let mut items = Vec::new();
    for item in ast.items {
        match lower_item(item) {
            Ok(item) => items.push(item),
            Err(err) => errors.push(err),
        }
    }
    if errors.is_empty() {
        Ok(HirCrate {items: items})
    } else {
        Err(errors)
    }
}
fn lower_item(item: AstItem) -> LowerResult<HirItem> {
    match item {
        AstItem::Function {
            attribute,
            visibility,
            name,
            return_type,
            params,
            body,
            span,
        } => {
            let params = params
                .into_iter()
                .map(|p| lower_param(p))
                .collect::<Result<Vec<_>, _>>()?;
            let return_type = return_type
                .map(|t| lower_type(t))
                .transpose()?;
            let visibility = lower_visibility(visibility);
            let body = lower_block(body)?;
            Ok(HirItem::Function {
                visibility,
                name,
                params,
                return_type,
                body,
                span,
            })
        }
        AstItem::Struct {
            attribute,
            visibility,
            name,
            fields,
            span,
        } => {
            let visibility = lower_visibility(visibility);
            let fields = fields
                .into_iter()
                .map(|f| lower_field(f))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(HirItem::Struct {
                visibility,
                name,
                fields,
                span,
            })
        },
        AstItem::Use { 
            visibility, 
            path, 
            items, 
            rename,
            span 
        } => {
            let visibility = lower_visibility(visibility);
            let items = items.map(|items| {
                items.into_iter()
                    .map(|item| lower_use_item(item))
                    .collect::<Result<Vec<_>, _>>()
            }).transpose()?;
            Ok(HirItem::Use {
                visibility: visibility,
                path: path,
                items: items,
                rename: rename,
                span: span,
            })
        }
        AstItem::Extern { 
            visibility, 
            abi, 
            items, 
            span
        } => {
            todo!()
        }
    }
}

fn lower_use_item(item: UseItem) -> LowerResult<HirUseItem> {
    let items = item.items.map(|items| {
        items.into_iter()
            .map(|item| lower_use_item(item))
            .collect::<Result<Vec<_>, _>>()
    }).transpose()?;
    Ok(HirUseItem {
        name: item.name,
        rename: item.rename,
        items: items,
        span: item.span
    })
}

fn lower_block( block: AstBlock) -> LowerResult<HirBlock> {
    let stmts = block
        .stmts
        .into_iter()
        .map(|s| lower_stmt(s))
        .collect::<Result<Vec<_>, _>>()?;
    let tail = if let Some(expr) = block.tail {
        Some(Box::new(lower_expr(*expr)?))
    } else {
        None
    };
    Ok(HirBlock {
        stmts,
        tail,
        span: block.span,
    })
}
fn lower_field( field: AstField) -> LowerResult<HirField> {
    let ty = lower_type(field.ty)?;
    let visibility = lower_visibility(field.visibility);
    Ok(HirField {
        name: field.name,
        ty,
        visibility,
        span: field.span,
    })
}
fn lower_expr(expr: AstExpr) -> LowerResult<HirExpr> {
    match expr {
        AstExpr::Block { block } => {
            let block = lower_block(block)?;
            Ok(HirExpr::Block { block })
        }
        AstExpr::Binary { left, op, right, span } => {
            let left = Box::new(lower_expr(*left)?);
            let right = Box::new(lower_expr(*right)?);
            let expr = match op {
                litec_ast::ast::BinOp::Add => todo!(),
                litec_ast::ast::BinOp::Subtract => todo!(),
                litec_ast::ast::BinOp::Multiply => todo!(),
                litec_ast::ast::BinOp::Divide => todo!(),
                litec_ast::ast::BinOp::Remainder => todo!(),
                litec_ast::ast::BinOp::Equal => todo!(),
                litec_ast::ast::BinOp::NotEqual => todo!(),
                litec_ast::ast::BinOp::LessThan => todo!(),
                litec_ast::ast::BinOp::LessEqual => todo!(),
                litec_ast::ast::BinOp::GreaterThan => todo!(),
                litec_ast::ast::BinOp::GreaterEqual => todo!(),
                litec_ast::ast::BinOp::LogicalAnd => todo!(),
                litec_ast::ast::BinOp::LogicalOr => todo!(),
                litec_ast::ast::BinOp::BitAnd => todo!(),
                litec_ast::ast::BinOp::BitOr => todo!(),
                litec_ast::ast::BinOp::BitXor => todo!(),
                litec_ast::ast::BinOp::ShiftLeft => todo!(),
                litec_ast::ast::BinOp::ShiftRight => todo!(),
            };
            Ok(expr)
        }
        AstExpr::Unary { op, operand, span } => {
            let operand = Box::new(lower_expr(*operand)?);
            let expr = match op {
                TokenKind::Bang => HirExpr::LogicalNot { operand, span },
                TokenKind::Minus => HirExpr::Negate { operand, span },
                _ => {
                    return Err(Error::InvalidOperatorTypes {
                        op,
                        span,
                    });
                }
            };
            Ok(expr)
        }
        AstExpr::Posifix { op, expr, span } => {
            let operand = Box::new(lower_expr(*expr)?);
            let value = match op {
                TokenKind::PlusPlus => {
                    let one = HirExpr::Literal {
                        value: LiteralValue::Int {
                            value: 1,
                            kind: LitIntValue::Unknown,
                        },
                        span: Span::new(0, 0),
                    };
                    HirExpr::Addition {
                        left: operand.clone(),
                        right: Box::new(one),
                        span,
                    }
                }
                TokenKind::MinusMinus => {
                    let one = HirExpr::Literal {
                        value: LiteralValue::Int {
                            value: 1,
                            kind: LitIntValue::Unknown,
                        },
                        span: Span::new(0, 0),
                    };
                    HirExpr::Subtract {
                        left: operand.clone(),
                        right: Box::new(one),
                        span,
                    }
                }
                _ => {
                    return Err(Error::InvalidOperatorTypes {
                        op,
                        span,
                    });
                }
            };
            Ok(HirExpr::Assign {
                target: operand,
                value: Box::new(value),
                span,
                original_op: Some(op),
            })
        }
        AstExpr::Literal {
            kind: LiteralKind::Int { base },
            value,
            suffix,
            span,
        } => {
            let s = get_global_string(value).unwrap();
            let radix = base as u32;
            match i128::from_str_radix(&s, radix) {
                Ok(num) => {
                    let int_kind = if let Some(suffix_sid) = suffix {
                        let suffix_str = get_global_string(suffix_sid).unwrap();
                        match suffix_str.as_ref() {
                            "i8" => LitIntValue::I8,
                            "i16" => LitIntValue::I16,
                            "i32" => LitIntValue::I32,
                            "i64" => LitIntValue::I64,
                            "i128" => LitIntValue::I128,
                            "isize" => LitIntValue::Isize,
                            "u8" => LitIntValue::U8,
                            "u16" => LitIntValue::U16,
                            "u32" => LitIntValue::U32,
                            "u64" => LitIntValue::U64,
                            "u128" => LitIntValue::U128,
                            "usize" => LitIntValue::Usize,
                            _ => {
                                return Err(Error::InvalidLiteralSuffix {
                                    suffix: suffix_str.as_ref().to_string(),
                                    span,
                                });
                            }
                        }
                    } else {
                        LitIntValue::Unknown
                    };
                    if !is_value_in_range(num, int_kind.clone()) {
                        return Err(Error::IntegerLiteralOutOfRange {
                            value: s.to_string(),
                            ty: format!("{:?}", int_kind),
                            span,
                        });
                    }
                    Ok(HirExpr::Literal {
                        value: LiteralValue::Int {
                            value: num,
                            kind: int_kind,
                        },
                        span,
                    })
                }
                Err(e) => Err(Error::IntegerLiteralParseError {
                    literal: s.to_string(),
                    base: radix,
                    error: e.to_string(),
                    span,
                }),
            }
        }
        // 在 lower 函数中添加浮点数字面量处理
        AstExpr::Literal {
            kind: LiteralKind::Float { .. },
            value,
            suffix,
            span,
        } => {
            let s = get_global_string(value).unwrap();
        
            // 解析为 f64（浮点数默认使用 f64 类型）
            let num = match s.parse::<f64>() {
                Ok(num) => num,
                Err(e) => {
                    return Err(Error::FloatLiteralParseError {
                        literal: s.to_string(),
                        error: e.to_string(),
                        span,
                    });
                }
            };
        
            // 确定目标浮点类型
            let float_kind = if let Some(suffix_sid) = suffix {
                let suffix_str = get_global_string(suffix_sid).unwrap();
                match suffix_str.as_ref() {
                    "f32" => LitFloatValue::F32,
                    "f64" => LitFloatValue::F64,
                    _ => {
                        return Err(Error::InvalidFloatSuffix {
                            suffix: suffix_str.as_ref().to_string(),
                            span,
                        });
                    }
                }
            } else {
                LitFloatValue::Unknown // 默认类型
            };
        
            // 检查数值是否在目标类型范围内
            match float_kind {
                LitFloatValue::F32 => {
                    if num < f32::MIN as f64 || num > f32::MAX as f64 {
                        return Err(Error::FloatLiteralOutOfRange {
                            value: s.to_string(),
                            ty: "f32".to_string(),
                            span,
                        });
                    }
                }
                LitFloatValue::F64 | _ => {
                    // f64 范围很大，通常不会超出，但保留检查
                    if num < f64::MIN || num > f64::MAX {
                        return Err(Error::FloatLiteralOutOfRange {
                            value: s.to_string(),
                            ty: "f64".to_string(),
                            span,
                        });
                    }
                }
            }
        
            // 构建 HIR 浮点数字面量
            Ok(HirExpr::Literal {
                value: LiteralValue::Float {
                    value: num,
                    kind: float_kind,
                },
                span,
            })
        }
        AstExpr::Literal { 
            kind: LiteralKind::Str { .. }, 
            value, 
            span ,
            ..
        } => {
            let s = get_global_string(value).unwrap();
            if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                let inner = &s[1..s.len()-1];
                Ok(HirExpr::Literal { 
                    value: LiteralValue::Str(intern_global(inner)), 
                    span 
                })
            } else {
                Err(Error::InvalidStringLiteral {
                    literal: s.as_ref().to_string(),
                    span,
                })
            }
        }
        // 字符字面量的正确处理（如果需要）
        AstExpr::Literal { 
            kind: LiteralKind::Char { .. }, 
            value, 
            span ,
            ..
        } => {
            let s = get_global_string(value).unwrap();
            if s.len() == 3 && s.starts_with('\'') && s.ends_with('\'') {
                let c = s.chars().nth(1).unwrap();
                Ok(HirExpr::Literal { 
                    value: LiteralValue::Char(c), 
                    span 
                })
            } else {
                Err(Error::InvalidCharLiteral {
                    literal: s.as_ref().to_string(),
                    span,
                })
            }
        }
        AstExpr::Ident { name, span } => Ok(HirExpr::Ident { name, span }),
        AstExpr::Grouped { expr, span } => {
            let expr = lower_expr(*expr)?;
            Ok(HirExpr::Grouped { expr: Box::new(expr), span })
        }
        AstExpr::Assignment { target, op, value, span } => {
            let target = Box::new(lower_expr(*target)?);
            let value = Box::new(lower_expr(*value)?);
            Ok(HirExpr::Assign {
                target,
                value,
                span,
                original_op: Some(op),
            })
        }
        AstExpr::Call { callee, args, span } => {
            let callee = Box::new(lower_expr(*callee)?);
            let args = args
                .into_iter()
                .map(|arg| lower_expr(arg))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(HirExpr::Call {
                callee,
                args,
                span,
            })
        }
        AstExpr::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => {
            let condition = Box::new(lower_expr(*condition)?);
            let then_branch = lower_block(then_branch)?;
            let else_branch = if else_branch.is_some() {
                Some(Box::new(lower_expr(*else_branch.unwrap())?))
            } else {
                None
            };
            Ok(HirExpr::If {
                condition,
                then_branch,
                else_branch,
                span,
            })
        }
        AstExpr::While { condition, body, span } => {
            let condition = Box::new(lower_expr(*condition)?);
            let body = lower_block(body)?;
            let body = {
                HirBlock {
                    stmts: Vec::new(),
                    tail: Some(
                        Box::new(
                            HirExpr::If { 
                                condition: condition, 
                                then_branch: body, 
                                else_branch: Some(
                                    Box::new(
                                        HirExpr::Block { 
                                            block: HirBlock { 
                                                stmts: vec![
                                                    HirStmt::Break { 
                                                        value: None, 
                                                        span: span 
                                                    }
                                                ], 
                                                tail: None, 
                                                span
                                            } 
                                        }
                                    )
                                ), 
                                span: span 
                            }
                        )
                    ),
                    span: span
                }
            };
            Ok(HirExpr::Loop {
                body: Box::new(body),
                span: span,
            })
        }
        AstExpr::For {
            ..
        } => {
            panic!("之后实现")
        }
        AstExpr::Loop { body, span } => {
            let body = lower_block(body)?;
            Ok(HirExpr::Loop { body: Box::new(body), span })
        }
        AstExpr::FieldAccess { base, name, span } => {
            let base = Box::new(lower_expr(*base)?);
            
            Ok(HirExpr::FieldAccess { base: base, field: name, span: span })
        }
        AstExpr::PathAccess { segments, span } => {
            Ok(HirExpr::PathAccess { segments: segments, span: span })
        }
        AstExpr::Bool { value, span } => Ok(HirExpr::Literal {
            value: LiteralValue::Bool(value),
            span,
        }),
    }
}
fn lower_stmt(stmt: AstStmt) -> LowerResult<HirStmt> {
    match stmt {
        AstStmt::Expr { expr } => {
            let expr = lower_expr(*expr)?;
            Ok(HirStmt::Expr(Box::new(expr)))
        }
        AstStmt::Let { mutable, name, ty, value, span } => {
            let ty = ty.map(|t| lower_type(t)).transpose()?;
            let value = value
                .map(|v| lower_expr(v))
                .transpose()?
                .map(Box::new);
            Ok(HirStmt::Let {
                name,
                ty,
                value,
                span,
            })
        }
        AstStmt::Return { value, span } => {
            let value = value
                .map(|v| lower_expr(v))
                .transpose()?
                .map(Box::new);
            Ok(HirStmt::Return { value, span })
        }
        AstStmt::Continue { span } => Ok(HirStmt::Continue { span }),
        AstStmt::Break { value, span } => {
            let value = value
                .map(|v| lower_expr(v))
                .transpose()?
                .map(Box::new);
            Ok(HirStmt::Break { value, span })
        }
    }
}
fn lower_param(param: AstParam) -> LowerResult<HirParam> {
    let ty = lower_type(param.ty)?;
    Ok(HirParam {
        name: param.name,
        ty,
        span: param.span,
    })
}
fn lower_visibility( visibility: AstVisibility) -> HirVisibility {
    match visibility {
        AstVisibility::Public => HirVisibility::Public,
        AstVisibility::Private => HirVisibility::Private,
    }
}
fn lower_type( ty: TypeAnnotation) -> LowerResult<HirType> {
    match ty {
        TypeAnnotation::Ident { name, span } => Ok(HirType::Named { name, span }),
    }
}

fn lower_literal_value(
    kind: LiteralKind,
    value: StringId,
    suffix: Option<StringId>,
    span: Span,
) -> LowerResult<LiteralValue> {
    match kind {
        LiteralKind::Int { base } => {
            let s = get_global_string(value).unwrap();
            let radix = base as u32;
            
            // 解析整数值
            let parsed_value = i128::from_str_radix(&s, radix)
                .map_err(|e| {
                    error("数字解析失败")
                        .with_span(span)
                        .build()
                })?;
            
            // 处理后缀并创建相应的 LitIntValue
            let int_value = if let Some(suffix_id) = suffix {
                let suffix_str = get_global_string(suffix_id).unwrap();
                match suffix_str.as_ref() {
                    "i8" => {
                        if parsed_value < i8::MIN as i128 || parsed_value > i8::MAX as i128 {
                            return Err(error("超出i8数字范围")
                                .with_span(span)
                                .build()
                            );
                        }
                        LitIntValue::I8(parsed_value as i8)
                    }
                    "i16" => {
                        if parsed_value < i16::MIN as i128 || parsed_value > i16::MAX as i128 {
                            return Err(error("超出i16数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitIntValue::I16(parsed_value as i16)
                    }
                    "i32" => {
                        if parsed_value < i32::MIN as i128 || parsed_value > i32::MAX as i128 {
                            return Err(error("超出i32数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitIntValue::I32(parsed_value as i32)
                    }
                    "i64" => {
                        if parsed_value < i64::MIN as i128 || parsed_value > i64::MAX as i128 {
                            return Err(error("超出i64数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitIntValue::I64(parsed_value as i64)
                    }
                    "i128" => LitIntValue::I128(parsed_value),
                    "isize" => {
                        if cfg!(target_pointer_width = "64") {
                            if parsed_value < i64::MIN as i128 || parsed_value > i64::MAX as i128 {
                                return Err(error("超出isize数字范围")
                                .with_span(span)
                                .build());
                            }
                            LitIntValue::Isize(parsed_value as isize)
                        } else {
                            if parsed_value < i32::MIN as i128 || parsed_value > i32::MAX as i128 {
                                return Err(error("超出isize数字范围")
                                .with_span(span)
                                .build());
                            }
                            LitIntValue::Isize(parsed_value as isize)
                        }
                    }
                    "u8" => {
                        if parsed_value < 0 || parsed_value > u8::MAX as i128 {
                            return Err(error("超出u8数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitIntValue::U8(parsed_value as u8)
                    }
                    "u16" => {
                        if parsed_value < 0 || parsed_value > u16::MAX as i128 {
                            return Err(error("超出i8数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitIntValue::U16(parsed_value as u16)
                    }
                    "u32" => {
                        if parsed_value < 0 || parsed_value > u32::MAX as i128 {
                            return Err(error("超出u32数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitIntValue::U32(parsed_value as u32)
                    }
                    "u64" => {
                        if parsed_value < 0 || parsed_value > u64::MAX as i128 {
                            return Err(error("超出u64数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitIntValue::U64(parsed_value as u64)
                    }
                    "u128" => {
                        if parsed_value < 0 {
                            return Err(error("超出u128数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitIntValue::U128(parsed_value as u128)
                    }
                    "usize" => {
                        if cfg!(target_pointer_width = "64") {
                            if parsed_value < 0 || parsed_value > u64::MAX as i128 {
                                return Err(error("超出usize数字范围")
                                .with_span(span)
                                .build());
                            }
                            LitIntValue::Usize(parsed_value as usize)
                        } else {
                            if parsed_value < 0 || parsed_value > u32::MAX as i128 {
                                return Err(error("超出usize数字范围")
                                .with_span(span)
                                .build());
                            }
                            LitIntValue::Usize(parsed_value as usize)
                        }
                    }
                    _ => {
                        return Err(error("位置数字后缀")
                                .with_span(span)
                                .build());
                    }
                }
            } else {
                // 无后缀的情况，使用 Unknown
                if parsed_value < i16::MIN as i128 || parsed_value > i16::MAX as i128 {
                    return Err(error("超出i64数字范围")
                                .with_span(span)
                                .build());
                }
                LitIntValue::Unknown(parsed_value as i16)
            };
            
            Ok(LiteralValue::Int {
                value: int_value,
            })
        }
        LiteralKind::Float { .. } => {
            let s = get_global_string(value).unwrap();
            
            // 解析浮点数值
            let parsed_value = s.parse::<f64>()
                .map_err(|e| {
                    error("超出u32数字范围")
                                .with_help(e.to_string())
                                .with_span(span)
                                .build()
                })?;
            
            // 处理后缀并创建相应的 LitFloatValue
            let float_value = if let Some(suffix_id) = suffix {
                let suffix_str = get_global_string(suffix_id).unwrap();
                match suffix_str.as_ref() {
                    "f32" => {
                        if parsed_value < f32::MIN as f64 || parsed_value > f32::MAX as f64 {
                            return Err(error("超出u32数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitFloatValue::F32(parsed_value as f32)
                    }
                    "f64" => {
                        if parsed_value < f64::MIN || parsed_value > f64::MAX {
                            return Err(error("超出f64数字范围")
                                .with_span(span)
                                .build());
                        }
                        LitFloatValue::F64(parsed_value)
                    }
                    _ => {
                        return Err(error("位置数字后缀")
                                .with_span(span)
                                .build());
                    }
                }
            } else {
                // 无后缀的情况，使用 Unknown
                if parsed_value < f32::MIN as f64 || parsed_value > f32::MAX as f64 {
                    return Err(error("超出f32数字范围")
                                .with_span(span)
                                .build());
                }
                LitFloatValue::Unknown(parsed_value as f32)
            };
            
            Ok(LiteralValue::Float {
                value: float_value,
            })
        }
        LiteralKind::Str { terminated } => {
            if !terminated {
                return Err(error("未关闭的字符串")
                    .with_span(span)
                    .build());
            }
            let s = get_global_string(value).unwrap();
            if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                let inner = &s[1..s.len()-1];
                Ok(LiteralValue::Str(intern_global(inner)))
            } else {
                Err(error("非法字符串")
                                .with_span(span)
                                .build())
            }
        }
        LiteralKind::Char { terminated } => {
            if !terminated {
                return Err(error("字符未关闭")
                                .with_span(span)
                                .build());
            }
            let s = get_global_string(value).unwrap();
            if s.len() == 3 && s.starts_with('\'') && s.ends_with('\'') {
                let c = s.chars().nth(1).unwrap();
                Ok(LiteralValue::Char(c))
            } else {
                Err(error("非法字符")
                                .with_span(span)
                                .build())
            }
        }
    }
}