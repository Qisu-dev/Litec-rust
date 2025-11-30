use litec_ast::{
    ast::{
        Attribute as AstAttribute, AttributeKind as AstAttributeKind, Block as AstBlock, Crate as AstCrate, 
        Expr as AstExpr, ExternItem, Field as AstField, Item as AstItem, Param as AstParam, Stmt as AstStmt, 
        Type, UseItem, Visibility as AstVisibility, ExternItem as AstExternItem
    }, 
    token::{
        Base, LiteralKind
    }
};
use litec_hir::{
    AssignOp, Attribute as HirAttribute, AttributeKind as HirAttributeKind, BinOp, Block as HirBlock, 
    Crate as HirCrate, Expr as HirExpr, Field as HirField, Item as HirItem, LitFloatValue, LitIntValue, 
    LiteralValue, Mutability as HirMutability, Param as HirParam, PosOp, Stmt as HirStmt, Type as HirType, 
    UnOp, UseItem as HirUseItem, Visibility as HirVisibility, AbiType as HirAbiType, ExternItem as HirExternItem
};
use litec_error::{Diagnostic, error};
use litec_span::{Span, StringId, get_global_string, intern_global};
use rustc_hash::FxHashMap;

pub struct Lower {
    pub krate: AstCrate,
    pub diagnostics: Vec<Diagnostic>
}

impl Lower {
    pub fn new(krate: AstCrate) -> Self {
        Self {
            krate: krate,
            diagnostics: Vec::new()
        }
    }

    pub fn low(&mut self) -> (HirCrate, Vec<Diagnostic>) {
        let krate_items = std::mem::take(&mut self.krate.items);
        let mut items = Vec::new();
        for item in krate_items.into_iter() {
            match self.low_item(item) {
                Some(item) => items.push(item),
                None => {
                    
                }
            }
        }

        (
            HirCrate {
                items: items
            },
            std::mem::take(&mut self.diagnostics)
        )
    }

    fn low_item(&mut self, item: AstItem) -> Option<HirItem> {
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
                let attribute = match attribute {
                    Some(attribute) => Some(self.low_attribute(attribute)?),
                    None => None
                };
                let mut _params = Vec::new();
                for param in params {
                    _params.push(self.low_param(param)?);
                }
                let return_type = match return_type {
                    Some(ty) => Some(self.low_type(ty)?),
                    None => None
                };
                let visibility = self.low_visibility(visibility);
                let body = self.low_block(body)?;
                Some(HirItem::Function {
                    attribute,
                    visibility,
                    name,
                    params: _params,
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
                let attribute = match attribute {
                    Some(attribute) => Some(self.low_attribute(attribute)?),
                    None => None
                };
                let visibility = self.low_visibility(visibility);
                let mut _fields = Vec::new();
                for field in fields {
                    _fields.push(self.low_field(field)?);
                }
                Some(HirItem::Struct {
                    attribute,
                    visibility,
                    name,
                    fields: _fields,
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
                let visibility = self.low_visibility(visibility);
                let mut _items: Vec<HirUseItem> = Vec::new();
                for item in items {
                    _items.push(self.low_use_item(item)?);
                }
                Some(HirItem::Use {
                    visibility: visibility,
                    path: path,
                    items: _items,
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
                let visibility = self.low_visibility(visibility);
                let abi = HirAbiType::from(abi);

                let mut _items = Vec::new();

                for item in items {
                    _items.push(self.low_extern_item(item)?);
                }
                Some(HirItem::Extern { 
                    visibility, 
                    abi, 
                    items: _items, 
                    span 
                })
            }
        }
    }

    fn low_extern_item(&mut self, item: AstExternItem) -> Option<HirExternItem> {
        match item {
            AstExternItem::Function { name, params, return_type, span } => {
                let mut _params = Vec::new();

                for param in params {
                    _params.push(self.low_param(param)?);
                }

                let return_type = match return_type {
                    Some(ty) => Some(self.low_type(ty)?),
                    None => None
                };

                Some(HirExternItem::Function { 
                    name: name, 
                    params: _params, 
                    return_type: return_type, 
                    span: span 
                })
            }
        }
    }

    fn low_attribute(&mut self, attribute: AstAttribute) -> Option<HirAttribute> {
        let kind = self.low_attribute_kind(attribute.kind)?;
        Some(HirAttribute { name: attribute.name, kind: kind, span: attribute.span })
    }

    fn low_attribute_kind(&mut self, kind: AstAttributeKind)-> Option<HirAttributeKind> {
        match kind {
            AstAttributeKind::Simple => Some(HirAttributeKind::Simple),
            AstAttributeKind::Positional(args) => {
                 let mut _args = Vec::new();
 
                 for arg in args {
                     _args.push(self.low_expr(arg)?);
                 }
 
                 Some(HirAttributeKind::Positional(_args))
            }
            AstAttributeKind::Named(map) => {
                let mut _map = FxHashMap::default();
                for (key, value) in map.into_iter() {
                    _map.insert(key, self.low_expr(value)?);
                }

                Some(HirAttributeKind::Named(_map))
            }
            AstAttributeKind::Mixed { positional, named } => {
                let mut _args = Vec::new();

                for arg in positional {
                    _args.push(self.low_expr(arg)?);
                }

                let mut _map = FxHashMap::default();

                for (key, value) in named.into_iter() {
                    _map.insert(key, self.low_expr(value)?);
                }

                Some(HirAttributeKind::Mixed { 
                    positional: _args, 
                    named: _map 
                })
            }
        }
    }

    fn low_use_item(&self, item: UseItem) -> Option<HirUseItem> {
        let mut items = Vec::new();
        for item in item.items {
            items.push(self.low_use_item(item)?);
        }
        Some(HirUseItem {
            name: item.name,
            rename: item.rename,
            items: items,
            span: item.span
        })
    }

    fn low_block(&mut self, block: AstBlock) -> Option<HirBlock> {
        let mut stmts = Vec::new();
        for stmt in block.stmts {
            stmts.push(self.low_stmt(stmt)?);
        }
        let tail = if let Some(expr) = block.tail {
            Some(Box::new(self.low_expr(*expr)?))
        } else {
            None
        };
        Some(HirBlock {
            stmts,
            tail,
            span: block.span,
        })
    }

    fn low_field(&mut self, field: AstField) -> Option<HirField> {
        let ty = self.low_type(field.ty)?;
        let visibility = self.low_visibility(field.visibility);
        Some(HirField {
            name: field.name,
            ty,
            visibility,
            span: field.span,
        })
    }

    fn low_expr(&mut self, expr: AstExpr) -> Option<HirExpr> {
        match expr {
            AstExpr::Unit { span } => {
                Some(HirExpr::Unit { span })
            }
            AstExpr::Tuple { elements, span } => {
                let mut _elements = Vec::new();

                for element in elements {
                    _elements.push(self.low_expr(element)?);
                }

                Some(HirExpr::Tuple { elements: _elements, span })
            }
            AstExpr::ToEq { strat, end, span } => {
                let start = Box::new(self.low_expr(*strat)?);
                let end = Box::new(self.low_expr(*end)?);

                Some(HirExpr::ToEq { start, end, span })
            }
            AstExpr::To { strat, end, span } => {
                let start = Box::new(self.low_expr(*strat)?);
                let end = Box::new(self.low_expr(*end)?);

                Some(HirExpr::To { start, end, span })
            }
            AstExpr::Index { indexed, index, span } => {
                let indexed = Box::new(self.low_expr(*indexed)?);
                let index = Box::new(self.low_expr(*index)?);

                Some(HirExpr::Index { indexed, index, span })
            }
            AstExpr::Block { block } => {
                let block = self.low_block(block)?;
                Some(HirExpr::Block { block })
            }
            AstExpr::Binary { left, op, right, span } => {
                let left = Box::new(self.low_expr(*left)?);
                let right = Box::new(self.low_expr(*right)?);
                let op = BinOp::from(op);
                Some(HirExpr::Binary { 
                    left, 
                    right, 
                    op, 
                    span 
                })
            }
            AstExpr::Unary { op, operand, span } => {
                let operand = Box::new(self.low_expr(*operand)?);
                let op = UnOp::from(op);
                Some(HirExpr::Unary { 
                    op, 
                    operand, 
                    span 
                })
            }
            AstExpr::Posifix { op, expr, span } => {
                let operand = Box::new(self.low_expr(*expr)?);
                let op = PosOp::from(op);
                Some(HirExpr::Posifix { 
                    operand, 
                    op, 
                    span 
                })
            }
            AstExpr::Literal {
                kind: LiteralKind::Int { base },
                value,
                suffix,
                span,
            } => {
                let literal = self.low_int_literal_value(
                    base,
                    value, 
                    suffix, 
                    span
                )?;

                Some(HirExpr::Literal { value: literal, span })
            }
            // 在 lower 函数中添加浮点数字面量处理
            AstExpr::Literal {
                kind: LiteralKind::Float { base },
                value,
                suffix,
                span,
            } => {
                if base != Base::Decimal {
                    self.diagnostics.push(error("并不支持非十进制浮点数")
                                            .with_span(span)
                                            .build());
                    return None;
                }
                let literal = self.low_float_literal_value(
                    value, 
                    suffix, 
                    span
                )?;

                Some(HirExpr::Literal { value: literal, span })
            }
            AstExpr::Literal { 
                kind: LiteralKind::Str { .. }, 
                value, 
                suffix,
                span ,
            } => {
                if suffix.is_some() {
                    self.diagnostics.push(error("字符串不应有后缀")
                                .with_span(span)
                                .build());
                    return None;
                }
                let s = get_global_string(value).unwrap();
                if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                    let inner = &s[1..s.len()-1];
                    Some(HirExpr::Literal { 
                        value: LiteralValue::Str(intern_global(inner)), 
                        span 
                    })
                } else {
                    self.diagnostics.push(error("非法字符串")
                            .with_span(span)
                            .build());
                    return None;
                }
            }
            // 字符字面量的正确处理（如果需要）
            AstExpr::Literal { 
                kind: LiteralKind::Char { .. }, 
                value, 
                suffix,
                span ,
            } => {
                if suffix.is_some() {
                    self.diagnostics.push(error("字符不应有后缀")
                                .with_span(span)
                                .build());
                    return None;
                }
                let s = get_global_string(value).unwrap();
                if s.len() == 3 && s.starts_with('\'') && s.ends_with('\'') {
                    let c = s.chars().nth(1).unwrap();
                    Some(HirExpr::Literal { 
                        value: LiteralValue::Char(c), 
                        span 
                    })
                } else {
                    self.diagnostics.push(error("非法字符")
                            .with_span(span)
                            .build());
                    return None;
                }
            }
            AstExpr::Ident { name, span } => Some(HirExpr::Ident { name, span }),
            AstExpr::Grouped { expr, span } => {
                let expr = self.low_expr(*expr)?;
                Some(HirExpr::Grouped { expr: Box::new(expr), span })
            }
            AstExpr::Assignment { target, op, value, span } => {
                let target = Box::new(self.low_expr(*target)?);
                let value = Box::new(self.low_expr(*value)?);
                let op = AssignOp::from(op);
                Some(HirExpr::Assign {
                    target,
                    value,
                    op,
                    span,
                })
            }
            AstExpr::Call { callee, args, span } => {
                let callee = Box::new(self.low_expr(*callee)?);
                let mut _args = Vec::new();
                
                for arg in args {
                    _args.push(self.low_expr(arg)?);
                }

                Some(HirExpr::Call {
                    callee: callee,
                    args: _args,
                    span: span,
                })
            }
            AstExpr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                let condition = Box::new(self.low_expr(*condition)?);
                let then_branch = self.low_block(then_branch)?;
                let else_branch = if else_branch.is_some() {
                    Some(Box::new(self.low_expr(*else_branch.unwrap())?))
                } else {
                    None
                };
                Some(HirExpr::If {
                    condition,
                    then_branch,
                    else_branch,
                    span,
                })
            }
            AstExpr::While { condition, body, span } => {
                let condition = Box::new(self.low_expr(*condition)?);
                let body = self.low_block(body)?;
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
                Some(HirExpr::Loop {
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
                let body = self.low_block(body)?;
                Some(HirExpr::Loop { body: Box::new(body), span })
            }
            AstExpr::FieldAccess { base, name, span } => {
                let base = Box::new(self.low_expr(*base)?);

                Some(HirExpr::FieldAccess { base: base, field: name, span: span })
            }
            AstExpr::PathAccess { segments, span } => {
                Some(HirExpr::PathAccess { segments: segments, span: span })
            }
            AstExpr::Bool { value, span } => Some(HirExpr::Literal {
                value: LiteralValue::Bool(value),
                span,
            })
        }
    }
    fn low_stmt(&mut self, stmt: AstStmt) -> Option<HirStmt> {
        match stmt {
            AstStmt::Expr { expr } => {
                let expr = self.low_expr(*expr)?;
                Some(HirStmt::Expr(Box::new(expr)))
            }
            AstStmt::Let { mutable, name, ty, value, span } => {
                let mutable = HirMutability::from(mutable);
                let ty = ty.map(|t| self.low_type(t))?;
                let value = match value {
                    Some(value) => Some(self.low_expr(value)?),
                    None => None
                };
                Some(HirStmt::Let {
                    mutable,
                    name,
                    ty,
                    value,
                    span,
                })
            }
            AstStmt::Return { value, span } => {
                let value = match value {
                    Some(value) => Some(self.low_expr(value)?),
                    None => None
                };
                Some(HirStmt::Return { value, span })
            }
            AstStmt::Continue { span } => Some(HirStmt::Continue { span }),
            AstStmt::Break { value, span } => {
                let value = match value {
                    Some(value) => Some(self.low_expr(value)?),
                    None => None
                };
                Some(HirStmt::Break { value, span })
            }
        }
    }

    fn low_param(&mut self, param: AstParam) -> Option<HirParam> {
        let ty = self.low_type(param.ty)?;
        Some(HirParam {
            name: param.name,
            ty,
            span: param.span,
        })
    }

    fn low_visibility(&mut self, visibility: AstVisibility) -> HirVisibility {
        match visibility {
            AstVisibility::Public => HirVisibility::Public,
            AstVisibility::Private => HirVisibility::Private,
        }
    }

    fn low_type(&mut self, ty: Type) -> Option<HirType> {
        match ty {
            Type::Ident { name, span } => Some(HirType::Named { name, span }),
            Type::Generic { name, args, span } => {
                let mut _args = Vec::new();
                for ty in args {
                    _args.push(self.low_type(ty)?);
                }
                
                Some(HirType::Generic { 
                    name, 
                    args: _args, 
                    span 
                })
            }
            Type::Pointer { mutable, target, span } => {
                let mutable = HirMutability::from(mutable);
                let target = Box::new(self.low_type(*target)?);
                Some(HirType::Pointer { 
                    mutable: mutable, 
                    target: target, 
                    span 
                })
            }
            Type::Tuple { elements, span } => {
                let mut _elements = Vec::new();

                for ty in elements {
                    _elements.push(self.low_type(ty)?);
                }

                Some(HirType::Tuple {
                    elements: _elements,
                    span: span
                })
            }
            Type::Reference { mutable, target, span } => {
                let mutable = HirMutability::from(mutable);
                let target = Box::new(self.low_type(*target)?);

                Some(HirType::Reference {
                    mutable: mutable,
                    target: target,
                    span: span
                })
            }
        }
    }

    fn low_int_literal_value(
        &mut self,
        base: Base,
        value: StringId,
        suffix: Option<StringId>,
        span: Span,
    ) -> Option<LiteralValue> {
        let s = get_global_string(value).unwrap();
        let radix = base as u32;

        // 解析整数值
        let parsed_value = match i128::from_str_radix(&s, radix) {
            Ok(value) => value,
            Err(e) => {
                self.diagnostics.push(error("数字解析失败")
                    .with_help(e.to_string())
                    .with_span(span)
                    .build());
                return None;
            }
        };
        
        // 处理后缀并创建相应的 LitIntValue
        let int_value = if let Some(suffix_id) = suffix {
            let suffix_str = get_global_string(suffix_id).unwrap();
            match suffix_str.as_ref() {
                "i8" => {
                    if parsed_value < i8::MIN as i128 || parsed_value > i8::MAX as i128 {
                        self.diagnostics.push(error("超出i8数字范围")
                            .with_span(span)
                            .build()
                        );
                    }
                    LitIntValue::I8(parsed_value as i8)
                }
                "i16" => {
                    if parsed_value < i16::MIN as i128 || parsed_value > i16::MAX as i128 {
                        self.diagnostics.push(error("超出i16数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitIntValue::I16(parsed_value as i16)
                }
                "i32" => {
                    if parsed_value < i32::MIN as i128 || parsed_value > i32::MAX as i128 {
                        self.diagnostics.push(error("超出i32数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitIntValue::I32(parsed_value as i32)
                }
                "i64" => {
                    if parsed_value < i64::MIN as i128 || parsed_value > i64::MAX as i128 {
                        self.diagnostics.push(error("超出i64数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitIntValue::I64(parsed_value as i64)
                }
                "i128" => LitIntValue::I128(parsed_value),
                "isize" => {
                    if cfg!(target_pointer_width = "64") {
                        if parsed_value < i64::MIN as i128 || parsed_value > i64::MAX as i128 {
                            self.diagnostics.push(error("超出isize数字范围")
                            .with_span(span)
                            .build());
                        }
                        LitIntValue::Isize(parsed_value as isize)
                    } else {
                        if parsed_value < i32::MIN as i128 || parsed_value > i32::MAX as i128 {
                            self.diagnostics.push(error("超出isize数字范围")
                            .with_span(span)
                            .build());
                        }
                        LitIntValue::Isize(parsed_value as isize)
                    }
                }
                "u8" => {
                    if parsed_value < 0 || parsed_value > u8::MAX as i128 {
                        self.diagnostics.push(error("超出u8数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitIntValue::U8(parsed_value as u8)
                }
                "u16" => {
                    if parsed_value < 0 || parsed_value > u16::MAX as i128 {
                        self.diagnostics.push(error("超出i8数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitIntValue::U16(parsed_value as u16)
                }
                "u32" => {
                    if parsed_value < 0 || parsed_value > u32::MAX as i128 {
                        self.diagnostics.push(error("超出u32数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitIntValue::U32(parsed_value as u32)
                }
                "u64" => {
                    if parsed_value < 0 || parsed_value > u64::MAX as i128 {
                        self.diagnostics.push(error("超出u64数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitIntValue::U64(parsed_value as u64)
                }
                "u128" => {
                    if parsed_value < 0 {
                        self.diagnostics.push(error("超出u128数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitIntValue::U128(parsed_value as u128)
                }
                "usize" => {
                    if cfg!(target_pointer_width = "64") {
                        if parsed_value < 0 || parsed_value > u64::MAX as i128 {
                            self.diagnostics.push(error("超出usize数字范围")
                            .with_span(span)
                            .build());
                        }
                        LitIntValue::Usize(parsed_value as usize)
                    } else {
                        if parsed_value < 0 || parsed_value > u32::MAX as i128 {
                            self.diagnostics.push(error("超出usize数字范围")
                            .with_span(span)
                            .build());
                        }
                        LitIntValue::Usize(parsed_value as usize)
                    }
                }
                _ => {
                    self.diagnostics.push(error("位置数字后缀")
                            .with_span(span)
                            .build());
                    return None;
                }
            }
        } else {
            // 无后缀的情况，使用 Unknown
            if parsed_value < i16::MIN as i128 || parsed_value > i16::MAX as i128 {
                self.diagnostics.push(error("超出i64数字范围")
                            .with_span(span)
                            .build());
            }
            LitIntValue::Unknown(parsed_value as i16)
        };

        Some(LiteralValue::Int {
            value: int_value,
        })
    }

    fn low_float_literal_value(
        &mut self,
        value: StringId,
        suffix: Option<StringId>,
        span: Span,
    ) -> Option<LiteralValue> {
        let s = get_global_string(value).unwrap();
        // 解析浮点数值
        let parsed_value = match s.parse::<f64>() {
            Ok(value) => value,
            Err(e) => {
                self.diagnostics.push(error("数字解析失败")
                    .with_help(e.to_string())
                    .with_span(span)
                    .build());
                return None;
            }
        };
        
        // 处理后缀并创建相应的 LitFloatValue
        let float_value = if let Some(suffix_id) = suffix {
            let suffix_str = get_global_string(suffix_id).unwrap();
            match suffix_str.as_ref() {
                "f32" => {
                    if parsed_value < f32::MIN as f64 || parsed_value > f32::MAX as f64 {
                        self.diagnostics.push(error("超出u32数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitFloatValue::F32(parsed_value as f32)
                }
                "f64" => {
                    if parsed_value < f64::MIN || parsed_value > f64::MAX {
                        self.diagnostics.push(error("超出f64数字范围")
                            .with_span(span)
                            .build());
                    }
                    LitFloatValue::F64(parsed_value)
                }
                _ => {
                    self.diagnostics.push(error("位置数字后缀")
                            .with_span(span)
                            .build());
                    return None;
                }
            }
        } else {
            // 无后缀的情况，使用 Unknown
            if parsed_value < f32::MIN as f64 || parsed_value > f32::MAX as f64 {
                self.diagnostics.push(error("超出f32数字范围")
                            .with_span(span)
                            .build());
            }
            LitFloatValue::Unknown(parsed_value as f32)
        };

        Some(LiteralValue::Float {
            value: float_value,
        })
    }
}

pub fn low(krate: AstCrate) -> (HirCrate, Vec<Diagnostic>) {
    let mut lower = Lower::new(krate);
    lower.low()
}

#[cfg(test)]
mod tests {
    use super::*;
    use litec_ast::{ast::{
        Block, Expr, Mutability, Stmt, Type
    }, token::TokenKind};
    use litec_span::{Location, Span};

    // 创建一个虚拟的 Span 用于测试
    fn dummy_span() -> Span {
        Span::new(Location::default(), Location::default(), litec_span::FileId(0))
    }

    #[test]
    fn test_lower_function() {
        // 创建一个简单的函数 AST: fn main() -> i32 { 42 }
        let ast_function = AstItem::Function {
            attribute: None,
            visibility: AstVisibility::Public,
            name: intern_global("main"),
            return_type: Some(Type::Ident { 
                name: intern_global("i32"), 
                span: dummy_span() 
            }),
            params: Vec::new(),
            body: Block {
                stmts: vec![
                    Stmt::Return {
                        value: Some(Expr::Literal {
                            kind: LiteralKind::Int { base: Base::Decimal },
                            value: intern_global("42"),
                            suffix: None,
                            span: dummy_span(),
                        }),
                        span: dummy_span(),
                    }
                ],
                tail: None,
                span: dummy_span(),
            },
            span: dummy_span(),
        };

        let ast_crate = AstCrate {
            items: vec![ast_function],
        };

        let mut lower = Lower::new(ast_crate);
        let (hir_crate, diagnostics) = lower.low();

        // 检查没有错误
        assert!(diagnostics.is_empty(), "Unexpected diagnostics: {:?}", diagnostics);
        
        // 检查 HIR 项数量
        assert_eq!(hir_crate.items.len(), 1);
        
        // 检查函数转换
        if let HirItem::Function { name, return_type, params, body, .. } = &hir_crate.items[0] {
            assert_eq!(*name, intern_global("main"));
            assert!(return_type.is_some());
            assert!(params.is_empty());
            
            // 检查函数体中的返回语句
            if let HirStmt::Return { value, .. } = &body.stmts[0] {
                assert!(value.is_some());
                if let Some(HirExpr::Literal { value: literal_value, .. }) = value.as_ref() {
                    if let LiteralValue::Int { value: int_value } = literal_value {
                        // 检查整数字面量值
                        match int_value {
                            LitIntValue::Unknown(val) => assert_eq!(*val, 42),
                            _ => panic!("Expected Unknown integer literal"),
                        }
                    } else {
                        panic!("Expected integer literal");
                    }
                } else {
                    panic!("Expected literal expression in return");
                }
            } else {
                panic!("Expected return statement");
            }
        } else {
            panic!("Expected function item");
        }
    }

    #[test]
    fn test_lower_struct() {
        // 创建一个结构体 AST
        let ast_struct = AstItem::Struct {
            attribute: None,
            visibility: AstVisibility::Public,
            name: intern_global("Point"),
            fields: vec![
                AstField {
                    name: intern_global("x"),
                    ty: Type::Ident { 
                        name: intern_global("i32"), 
                        span: dummy_span() 
                    },
                    visibility: AstVisibility::Private,
                    span: dummy_span(),
                },
                AstField {
                    name: intern_global("y"),
                    ty: Type::Ident { 
                        name: intern_global("i32"), 
                        span: dummy_span() 
                    },
                    visibility: AstVisibility::Private,
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };

        let ast_crate = AstCrate {
            items: vec![ast_struct],
        };

        let mut lower = Lower::new(ast_crate);
        let (hir_crate, diagnostics) = lower.low();

        assert!(diagnostics.is_empty(), "Unexpected diagnostics: {:?}", diagnostics);
        assert_eq!(hir_crate.items.len(), 1);
        
        if let HirItem::Struct { name, fields, .. } = &hir_crate.items[0] {
            assert_eq!(*name, intern_global("Point"));
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, intern_global("x"));
            assert_eq!(fields[1].name, intern_global("y"));
        } else {
            panic!("Expected struct item");
        }
    }

    #[test]
    fn test_lower_literals() {
        // 测试各种字面量的转换
        let test_cases = vec![
            (
                Expr::Literal {
                    kind: LiteralKind::Int { base: Base::Decimal },
                    value: intern_global("123"),
                    suffix: None,
                    span: dummy_span(),
                },
                "整数 123"
            ),
            (
                Expr::Literal {
                    kind: LiteralKind::Float { base: Base::Decimal },
                    value: intern_global("3.14"),
                    suffix: None,
                    span: dummy_span(),
                },
                "浮点数 3.14"
            ),
            (
                Expr::Literal {
                    kind: LiteralKind::Str { terminated: true },
                    value: intern_global("\"hello\""),
                    suffix: None,
                    span: dummy_span(),
                },
                "字符串 \"hello\""
            ),
            (
                Expr::Bool {
                    value: true,
                    span: dummy_span(),
                },
                "布尔值 true"
            ),
        ];

        for (expr, description) in test_cases {
            let ast_function = AstItem::Function {
                attribute: None,
                visibility: AstVisibility::Public,
                name: intern_global("test"),
                return_type: None,
                params: Vec::new(),
                body: Block {
                    stmts: vec![Stmt::Expr {
                        expr: Box::new(expr),
                    }],
                    tail: None,
                    span: dummy_span(),
                },
                span: dummy_span(),
            };

            let ast_crate = AstCrate {
                items: vec![ast_function],
            };

            let mut lower = Lower::new(ast_crate);
            let (hir_crate, diagnostics) = lower.low();

            assert!(
                diagnostics.is_empty(), 
                "Failed to lower {}: {:?}", 
                description, 
                diagnostics
            );
            assert_eq!(hir_crate.items.len(), 1);
        }
    }

    #[test]
    fn test_lower_binary_expression() {
        // 测试二元表达式: 1 + 2
        let binary_expr = Expr::Binary {
            left: Box::new(Expr::Literal {
                kind: LiteralKind::Int { base: Base::Decimal },
                value: intern_global("1"),
                suffix: None,
                span: dummy_span(),
            }),
            op: TokenKind::Add,
            right: Box::new(Expr::Literal {
                kind: LiteralKind::Int { base: Base::Decimal },
                value: intern_global("2"),
                suffix: None,
                span: dummy_span(),
            }),
            span: dummy_span(),
        };

        let ast_function = AstItem::Function {
            attribute: None,
            visibility: AstVisibility::Public,
            name: intern_global("test_binary"),
            return_type: None,
            params: Vec::new(),
            body: Block {
                stmts: vec![Stmt::Expr {
                    expr: Box::new(binary_expr),
                }],
                tail: None,
                span: dummy_span(),
            },
            span: dummy_span(),
        };

        let ast_crate = AstCrate {
            items: vec![ast_function],
        };

        let mut lower = Lower::new(ast_crate);
        let (hir_crate, diagnostics) = lower.low();

        assert!(diagnostics.is_empty(), "Unexpected diagnostics: {:?}", diagnostics);
        assert_eq!(hir_crate.items.len(), 1);
        
        // 检查二元表达式是否正确转换
        if let HirItem::Function { body, .. } = &hir_crate.items[0] {
            if let HirStmt::Expr(expr) = &body.stmts[0] {
                if let HirExpr::Binary { left, right, op, .. } = &**expr {
                    assert!(matches!(op, BinOp::Add));
                    // 可以进一步检查左右操作数
                } else {
                    panic!("Expected binary expression");
                }
            } else {
                panic!("Expected expression statement");
            }
        }
    }

    #[test]
    fn test_lower_variable_declaration() {
        // 测试变量声明: let x: i32 = 10;
        let ast_function = AstItem::Function {
            attribute: None,
            visibility: AstVisibility::Public,
            name: intern_global("test_var"),
            return_type: None,
            params: Vec::new(),
            body: Block {
                stmts: vec![Stmt::Let {
                    mutable: Mutability::Const,
                    name: intern_global("x"),
                    ty: Some(Type::Ident { 
                        name: intern_global("i32"), 
                        span: dummy_span() 
                    }),
                    value: Some(Expr::Literal {
                        kind: LiteralKind::Int { base: Base::Decimal },
                        value: intern_global("10"),
                        suffix: None,
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                }],
                tail: None,
                span: dummy_span(),
            },
            span: dummy_span(),
        };

        let ast_crate = AstCrate {
            items: vec![ast_function],
        };

        let mut lower = Lower::new(ast_crate);
        let (hir_crate, diagnostics) = lower.low();

        assert!(diagnostics.is_empty(), "Unexpected diagnostics: {:?}", diagnostics);
        assert_eq!(hir_crate.items.len(), 1);
        
        if let HirItem::Function { body, .. } = &hir_crate.items[0] {
            if let HirStmt::Let { name, ty, value, .. } = &body.stmts[0] {
                assert_eq!(*name, intern_global("x"));
                assert!(ty.is_some());
                assert!(value.is_some());
            } else {
                panic!("Expected let statement");
            }
        }
    }

    #[test]
    fn test_lower_error_handling() {
        // 测试错误处理：无效的浮点数基数
        let invalid_float = Expr::Literal {
            kind: LiteralKind::Float { base: Base::Hexadecimal }, // 十六进制浮点数，应该报错
            value: intern_global("0x1.0"),
            suffix: None,
            span: dummy_span(),
        };

        let ast_function = AstItem::Function {
            attribute: None,
            visibility: AstVisibility::Public,
            name: intern_global("test_error"),
            return_type: None,
            params: Vec::new(),
            body: Block {
                stmts: vec![Stmt::Expr {
                    expr: Box::new(invalid_float),
                }],
                tail: None,
                span: dummy_span(),
            },
            span: dummy_span(),
        };

        let ast_crate = AstCrate {
            items: vec![ast_function],
        };

        let mut lower = Lower::new(ast_crate);
        let (hir_crate, diagnostics) = lower.low();

        // 应该产生错误诊断
        assert!(!diagnostics.is_empty(), "Expected error diagnostics for invalid float base");
        // 错误项应该被跳过（返回 None）
        assert_eq!(hir_crate.items.len(), 0);
    }
}