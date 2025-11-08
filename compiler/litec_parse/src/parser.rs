use litec_error::{
    error, expected_identifier, unclosed_delimiter, unterminated_char, unterminated_string, Diagnostic, DiagnosticBuilder
};
use litec_ast::{ast::{AssignOp, Attribute, Block, Crate, Expr, Field, Item, Param, Stmt, TypeAnnotation, UseItem, Visibility}, token::{LiteralKind, Token, TokenKind}};
use litec_span::{intern_global, FileId, SourceFile, SourceMap, StringId};
use crate::lexer::Lexer;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
enum Precedence {
    Lowest,
    Assignment,   // =
    LogicalOr,    // ||
    LogicalAnd,   // &&
    Range,            // ..
    Equality,     // ==, !=
    Comparison,   // <, >, <=, >=
    Term,         // +, -
    Factor,       // *, /, %
    Unary,        // !
    Call,         // 函数调用
    Member,      // ., []
    Path         // ::
}

impl Precedence {
    fn from_token_kind(kind: &TokenKind) -> Precedence {
        match kind {
            TokenKind::Eq => Precedence::Assignment,
            TokenKind::Or => Precedence::LogicalOr,
            TokenKind::And => Precedence::LogicalAnd,
            TokenKind::To => Precedence::Range,        // 新增：范围运算符
            TokenKind::EqEq | TokenKind::NotEq => Precedence::Equality,
            TokenKind::Lt | TokenKind::Gt | TokenKind::LtEq | TokenKind::GtEq => Precedence::Comparison,
            TokenKind::Plus | TokenKind::Minus => Precedence::Term,
            TokenKind::Star | TokenKind::Slash | TokenKind::Remainder => Precedence::Factor,
            TokenKind::Bang => Precedence::Unary, // 前缀运算符
            TokenKind::OpenParen => Precedence::Call,
            TokenKind::Dot | TokenKind::OpenBracket => Precedence::Member,
            TokenKind::PathAccess => Precedence::Path,
            _ => Precedence::Lowest,
        }
    }
}

pub struct Parser<'src> {
    lexer: Lexer<'src>,
    current_token: Token<'src>,
    diagnostics: Vec<Diagnostic>
}

impl<'src> Parser<'src> {
    pub fn new(source: &'src SourceFile, file_id: FileId) -> Result<Self, Diagnostic> {
        let mut lexer = Lexer::new(source, file_id);
        let current_token = lexer.advance_token()?; // 使用 ? 传播错误
        
        Ok(Parser {
            lexer,
            current_token,
            diagnostics: Vec::new(),
        })
    }

    fn advance(&mut self) {
        loop {
            match self.lexer.advance_token() {
                Ok(token) => {
                    self.current_token = token;
                    return;
                }
                Err(err) => {
                    self.diagnostics.push(err);
                    self.lexer.advance_token();
                }
            }
        }
    }

    #[inline]
    fn expect(&mut self, kind: TokenKind, err: Diagnostic) -> Option<Token<'src>> {
        if self.current_token.kind == kind {
            let token = self.current_token.clone();
            self.advance();
            Some(token)
        } else {
            self.diagnostics.push(err);
            None
        }
    }

    fn sync_to(&mut self, recovery_tokens: &[TokenKind]) {
        let mut skipped = 0;
        const MAX_SKIP: usize = 20;

        while skipped < MAX_SKIP && self.current_token.kind != TokenKind::Eof {
            // 如果遇到同步点，停止恢复
            if recovery_tokens.contains(&self.current_token.kind) {
                return;
            }
            
            // 如果遇到更高层级的同步点，也停止
            if self. is_item_start() {
                return;
            }
            
            self.advance();
            skipped += 1;
        }
    }

    /// 项级别的同步点
    fn sync_to_item(&mut self) {
        self.sync_to(&[
            TokenKind::Hash,
            TokenKind::Pub,
            TokenKind::Priv,
            TokenKind::Fn
        ]);
    }

    /// 语句级别的同步点
    fn sync_to_stmt(&mut self) {
        self.sync_to(&[
            TokenKind::CloseBrace,
            TokenKind::Let,
            TokenKind::Return,
            TokenKind::If,
            TokenKind::While,
        ]);
    }

    /// 表达式级别的同步点
    fn sync_to_expr(&mut self) {
        self.sync_to(&[
            TokenKind::Comma,
            TokenKind::Semi,
            TokenKind::CloseParen,
            TokenKind::CloseBracket,
            TokenKind::CloseBrace,
        ]);
    }

    /// 检查是否是项的开始
    fn is_item_start(&self) -> bool {
        matches!(
            self.current_token.kind,
            TokenKind::Fn |
            TokenKind::Struct |
            TokenKind::Use |
            TokenKind::Pub |
            TokenKind::Priv |
            TokenKind::Hash
        )
    }

    pub fn parse(&mut self) -> (Crate, Vec<Diagnostic>) {
        let mut items = Vec::new();
        
        while self.current_token.kind != TokenKind::Eof {
            match self.parse_item() {
                Some(stmt) => items.push(stmt),
                None => {
                    self.sync_to_item();
                }
            }
        }
        
        (Crate::new(items), self.take_diagnostics())
    }

    // 获取所有诊断信息（消费它们）
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    fn parse_attribute(&mut self) -> Option<Option<Attribute>> {
        let span = if self.current_token.kind == TokenKind::Hash {
            let span = self.current_token.span;
            self.advance();
            span
        } else {
            return Some(None);
        };

        self.expect(TokenKind::OpenBracket, error("期待`[`")
            .with_span(self.current_token.span)
            .with_help("添加一个`[`")
            .build())?;

        let name = self.expect(
            TokenKind::Ident,
            self.expect_identifier_error().build()
        )?;
        let name = intern_global(name.text);

        let args = if self.current_token.kind == TokenKind::OpenParen {
            self.parse_attribute_args()?
        } else {
            None
        };

        // 期待 `]`
        let close_bracket_span = self.expect(
            TokenKind::CloseBracket, 
            error("未闭合的属性").with_span(self.current_token.span).build()
        )?.span;
        
        Some(Some(Attribute {
            name: name,
            args: args,
            span: span.extend_to(close_bracket_span),
        }))
    }

    fn parse_attribute_args(&mut self) -> Option<Option<Vec<StringId>>> {
        self.expect(TokenKind::OpenParen, error("期待 `(`").with_span(self.current_token.span).build())?;
        
        let mut args = Vec::new();
        
        // 解析逗号分隔的标识符列表
        loop {
            let ident = self.expect(
                TokenKind::Ident,
                error("期待标识符").with_span(self.current_token.span).build()
            )?;
            args.push(intern_global(ident.text));
            
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        
        self.expect(TokenKind::CloseParen, error("未闭合的参数列表").with_span(self.current_token.span).build())?;
        
        Some(Some(args))
    }

    fn parse_item(&mut self) -> Option<Item> {
        let mut visibility = Visibility::Private;

        let attribute = self.parse_attribute()?;

        if self.eat(TokenKind::Pub) {
            visibility = Visibility::Public;
        } else if self.eat(TokenKind::Priv) {
            visibility = Visibility::Private;
        }

        match self.current_token.kind {
            TokenKind::Fn => self.parse_fn_item(attribute, visibility),
            TokenKind::Struct => self.parse_struct_item(attribute, visibility),
            TokenKind::Use => {
                let result = match self.parse_use_item(visibility) {
                    Some(result) => result,
                    None => {
                        self.sync_to_item();
                        return None;
                    }
                };

                self.expect(TokenKind::Semi, self.expect_semi_error().with_span(self.current_token.span).build())?;
                Some(result)
            },
            _ => {
                self.diagnostics.push(error("期待一个`item`")
                                    .with_span(self.current_token.span)
                                    .build());
                None
            }
        }
        
    }

    fn parse_use_item(&mut self, visibility: Visibility) -> Option<Item> {
        let span = self.current_token.span;
        self.advance(); // 消耗 `use`

        let path = self.parse_path()?;
        let mut items = None;
        let mut rename = None;

        // 处理大括号形式的 use { item1, item2 }
        if self.eat(TokenKind::OpenBrace) {
            let mut use_items = Vec::new();
            while self.current_token.kind != TokenKind::CloseBrace && self.current_token.kind != TokenKind::Eof {
                use_items.push(self.parse_use_item_inner()?);

                // 可选逗号
                if self.current_token.kind == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::CloseBrace, error("未闭合的 use 项")
                .with_span(self.current_token.span)
                .build())?;
            items = Some(use_items);
        } 
        // 处理重命名形式 use path as rename
        else if self.eat(TokenKind::As) {
            let rename_token = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;
            rename = Some(intern_global(rename_token.text));
        }

        Some(Item::Use {
            visibility,
            path,
            items,
            rename,
            span: span.extend_to(self.current_token.span),
        })
    }

    fn parse_use_item_inner(&mut self) -> Option<UseItem> {
        let name_token = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;
        let name = intern_global(name_token.text);
        let mut rename = None;
        let mut items = None;

        let start_span = name_token.span;
        let mut end_span = name_token.span;

        // 处理重命名：item as Rename
        if self.eat(TokenKind::As) {
            let rename_token = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;
            rename = Some(intern_global(rename_token.text));
            end_span = rename_token.span;
        }
        // 处理嵌套：item { subitem1, subitem2 }
        else if self.eat(TokenKind::OpenBrace) {
            let mut use_items = Vec::new();
            while self.current_token.kind != TokenKind::CloseBrace && self.current_token.kind != TokenKind::Eof {
                use_items.push(self.parse_use_item_inner()?);

                // 可选逗号
                if self.current_token.kind == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            let close_brace = self.expect(TokenKind::CloseBrace, unclosed_delimiter("大括号", self.current_token.span).build())?;
            items = Some(use_items);
            end_span = close_brace.span;
        }

        Some(UseItem {
            name,
            rename,
            items,
            span: start_span.extend_to(end_span),
        })
    }

    fn parse_path(&mut self) -> Option<Vec<StringId>> {
        let mut path = Vec::new();
        
        // 解析第一个路径段
        if self.current_token.kind == TokenKind::Ident {
            path.push(intern_global(self.current_token.text));
            self.advance();
        } else {
            self.diagnostics.push(error("期待路径")
                .with_span(self.current_token.span)
                .build());
            return None;
        }

        // 解析后续的 ::segment
        while self.current_token.kind == TokenKind::PathAccess {
            self.advance(); // 消耗 ::

            if self.current_token.kind == TokenKind::Ident {
                path.push(intern_global(self.current_token.text));
                self.advance();
            } else {
                self.diagnostics.push(error("期待标识符")
                    .with_span(self.current_token.span)
                    .build());

                return None;
            }
        }

        Some(path)
    }

    fn parse_struct_item(&mut self, attribute: Option<Attribute>, visibility: Visibility) -> Option<Item> {
        let span = self.current_token.span;
        self.advance();
        
        let name = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;

        self.expect(TokenKind::OpenBrace, error("期待大括号").with_span(self.current_token.span).build())?;
        
        let mut fields: Vec<Field> = Vec::new();
        while self.current_token.kind != TokenKind::CloseBrace && self.current_token.kind != TokenKind::Eof {
            fields.push(self.parse_field()?);

            self.eat(TokenKind::Comma);
        }

        let close = self.expect(TokenKind::CloseBrace, unclosed_delimiter("大括号", self.current_token.span)
            .with_label(span, "开始的大括号")
            .build())?.span;

        Some(Item::Struct { attribute: attribute, visibility: visibility, name: intern_global(name.text), fields: fields, span: span.extend_to(close) })
    }

    fn parse_field(&mut self) -> Option<Field> {
        let span = self.current_token.span;
        let mut flag = Visibility::Private;
        match self.current_token.kind {
            TokenKind::Pub => {
                flag = Visibility::Public;
                self.advance();
            }
            TokenKind::Priv => {
                flag = Visibility::Private;
                self.advance();
            }
            _ => {}
        };

        let name = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;
        
        self.expect(TokenKind::Colon, error("期待冒号")
            .with_span(self.current_token.span)
            .build())?;

        let ty = self.parse_type()?;
        let ty_span = ty.span();

        Some(Field {
            name: intern_global(name.text),
            ty: ty,
            visibility: flag,
            span: span.extend_to(ty_span)
        })
    }

    fn parse_fn_item(&mut self, attribute: Option<Attribute>, visibility: Visibility) -> Option<Item> {
        let span = self.current_token.span;
        self.advance();

        let name = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;

        let start_paren_span = self.expect(TokenKind::OpenParen, error("期待括号")
            .with_span(self.current_token.span)
            .build())?.span;

        let mut params: Vec<Param> = Vec::new();
        if self.current_token.kind != TokenKind::CloseParen {
            loop {
                params.push(self.parse_param()?);

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(TokenKind::CloseParen, error("期待括号")
            .with_span(self.current_token.span)
            .with_label(start_paren_span, "开始时的括号")
            .build())?;

        let mut return_type: Option<TypeAnnotation> = None;
        if self.eat(TokenKind::Arrow) {
            return_type = Some(self.parse_type()?);
        }

        let block = self.parse_block()?;
        let block_span = block.span;

        Some(Item::Function {
            attribute: attribute,
            visibility: visibility,
            name: intern_global(name.text),
            return_type: return_type,
            params: params,
            body: block,
            span: span.extend_to(block_span)
        })
    }

    fn parse_param(&mut self) -> Option<Param> {
        let name = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;
        self.expect(TokenKind::Colon, error("期待冒号")
            .with_span(self.current_token.span)
            .build())?;
        let ty = self.parse_type()?;
        let name_span = name.span;
        let ty_span = ty.span();

        Some(Param {
            name: intern_global(name.text),
            ty,
            span: name_span.extend_to(ty_span)
        })
    }

    fn parse_type(&mut self) -> Option<TypeAnnotation> {
        let start_span = self.current_token.span;
        
        match &self.current_token.kind {
            TokenKind::Ident => {
                // 解析标识符类型
                let token = self.current_token.clone();
                self.advance();
                Some(TypeAnnotation::Ident {
                    name: intern_global(token.text),
                    span: start_span.extend_to(token.span),
                })
            }
            // 可以添加更多类型解析逻辑
            _ => {
                self.diagnostics.push(error("expect type")
                    .with_span(self.current_token.span)
                    .build());
                None
            }
        }
    }

    fn parse_expression(&mut self) -> Option<Expr> {
        self.parse_expression_with_precedence(Precedence::Lowest)
    }

    fn parse_expression_with_precedence(&mut self, precedence: Precedence) -> Option<Expr> {
        let mut left = self.parse_prefix()?;
        
        while self.current_token.kind != TokenKind::Eof 
            && self.current_token.kind != TokenKind::Semi
            && precedence < Precedence::from_token_kind(&self.current_token.kind)
        {
            left = self.parse_infix(left)?;
        }

        left = self.parse_posifix(left)?;
        
        Some(left)
    }

    fn parse_posifix(&mut self, left: Expr) -> Option<Expr> {
        match self.current_token.kind {
            TokenKind::PlusPlus | TokenKind::MinusMinus => {
                let op = self.current_token.kind.clone();
                let op_span = self.current_token.span;
                let left_span = left.span();
                self.advance();
                Some(Expr::Posifix { op: op, expr: Box::new(left), span: left_span.extend_to(op_span) })
            }
            _ => Some(left)
        }
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        match &self.current_token.kind {
            TokenKind::Literal { kind, suffix } => {
                if let LiteralKind::Char { terminated } = kind {
                    if !terminated {
                        self.diagnostics.push(unterminated_char(self.current_token.span).build());
                        return None;
                    }
                }
                
                if let LiteralKind::Str { terminated } = kind {
                    if !terminated {
                        self.diagnostics.push(unterminated_string(self.current_token.span).build());
                        return None;
                    }
                }
                
                let value = intern_global(self.current_token.text);
                let suffix_id = *suffix;
                
                let expr = Expr::Literal {
                    kind: kind.clone(),
                    value,
                    suffix: suffix_id,
                    span: self.current_token.span,
                };
                
                self.advance();
                Some(expr)
            }
            TokenKind::Ident => {
                let name = intern_global(self.current_token.text);
                let span = self.current_token.span;
                
                self.advance();
                Some(Expr::Ident { name, span })
            }
            TokenKind::OpenParen => {
                let start_span = self.current_token.span;
                self.advance();
                
                // 检查空括号：() 是空元组
                if self.current_token.kind == TokenKind::CloseParen {
                    let close_span = self.current_token.span;
                    self.advance();
                    return Some(Expr::Tuple {
                        elements: Vec::new(),
                        span: start_span.extend_to(close_span),
                    });
                }

                // 解析第一个表达式
                let first_expr = self.parse_expression()?;

                // 检查是否有逗号 - 如果有逗号就是元组
                if self.eat(TokenKind::Comma) {
                    let mut elements = vec![first_expr];

                    // 继续解析元组的其他元素
                    while self.current_token.kind != TokenKind::CloseParen 
                        && self.current_token.kind != TokenKind::Eof 
                    {
                        elements.push(self.parse_expression()?);

                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }

                    let close_paren = self.expect(
                        TokenKind::CloseParen, 
                        unclosed_delimiter("括号", self.current_token.span).build()
                    )?;

                    let span = start_span.extend_to(close_paren.span);
                    Some(Expr::Tuple { elements, span })
                } else {
                    // 没有逗号，就是分组表达式
                    let close_paren = self.expect(
                        TokenKind::CloseParen, 
                        unclosed_delimiter("括号", self.current_token.span).build()
                    )?;

                    let span = start_span.extend_to(close_paren.span);
                    Some(Expr::Grouped {
                        expr: Box::new(first_expr),
                        span,
                    })
                }
            }
            TokenKind::Bang | TokenKind::Minus => {
                let op = self.current_token.kind.clone();
                let start_span = self.current_token.span;
                
                self.advance();
                
                let expr = self.parse_expression_with_precedence(Precedence::Unary)?;
                
                let span = start_span.extend_to(expr.span());
                Some(Expr::Unary {
                    op: op,
                    operand: Box::new(expr),
                    span,
                })
            }
            TokenKind::If => self.parse_if_expression(),
            TokenKind::While => self.parse_while_expression(),
            TokenKind::For => self.parse_for_expression(),
            TokenKind::OpenBrace => self.parse_block_expression(),
            TokenKind::Loop => self.parse_loop_expression(),
            TokenKind::True => {
                let span = self.current_token.span;
                self.advance();
                Some(Expr::Bool { value: true, span: span })
            }
            TokenKind::False => {
                let span = self.current_token.span;
                self.advance();
                Some(Expr::Bool { value: false, span: span })
            }

            _ => {
                self.diagnostics.push(error("期待表达式")
                    .with_help("添加一个表达式在此处")
                    .with_span(self.current_token.span)
                    .build());
                None
            }
        }
    }

    fn parse_infix(&mut self, left: Expr) -> Option<Expr> {
        // dbg!(self.current_token.kind);
        match self.current_token.kind {
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Remainder
            | TokenKind::EqEq
            | TokenKind::NotEq
            | TokenKind::Lt
            | TokenKind::LtEq
            | TokenKind::Gt
            | TokenKind::GtEq
            | TokenKind::And
            | TokenKind::Or => self.parse_binary_expression(left),
            
            TokenKind::PlusEq
            | TokenKind::MinusEq
            | TokenKind::RemainderEq
            | TokenKind::StarEq
            | TokenKind::SlashEq
            | TokenKind::Eq => self.parse_assignment_expression(left),

            TokenKind::Dot => self.parse_field_access_expression(left),

            TokenKind::PathAccess => self.parse_path_access_expression(left),

            TokenKind::OpenParen => self.parse_call_exprssion(left),

            TokenKind::To => self.parse_to_expression(left),

            TokenKind::OpenBracket => self.parse_index_expression(left),
            
            _ => Some(left), // 不是中缀运算符，直接返回左表达式
        }
    }

    fn parse_index_expression(&mut self, indexed: Expr) -> Option<Expr> {
        self.expect(
            TokenKind::OpenBracket, 
            error("期待`[`")
                .with_span(self.current_token.span)
                .build()
            )?;

        let index = self.parse_expression()?;

        let close_bracket_span = self.expect(
            TokenKind::CloseBracket, 
            error("期待`[`")
                .with_span(self.current_token.span)
                .build()
            )?.span;

        let span = indexed.span().extend_to(close_bracket_span);

        Some(Expr::Index { indexed: Box::new(indexed), index: Box::new(index), span: span })
    }

    fn parse_to_expression(&mut self, start: Expr) -> Option<Expr> {
        self.advance();

        let end = self.parse_expression()?;
        let span = start.span().extend_to(end.span());
        Some(Expr::To { 
            strat: Box::new(start), 
            end: Box::new(end),
            span: span
        })
    }

    fn parse_loop_expression(&mut self) -> Option<Expr> {
        let span = self.current_token.span;
        self.advance();

        let body = self.parse_block()?;
        let body_span = body.span;

        Some(Expr::Loop { body: body, span: span.extend_to(body_span) })
    }

    fn parse_field_access_expression(&mut self, left: Expr) -> Option<Expr> {
        let span = self.current_token.span;
        self.advance();

        let name = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;

        Some(Expr::FieldAccess {
            base: Box::new(left),
            name: intern_global(name.text),
            span: span.extend_to(name.span)
        })
    }

    fn parse_path_access_expression(&mut self, left: Expr) -> Option<Expr> {
        let span = self.current_token.span;
        self.advance();

        while self.eat(TokenKind::PathAccess) {
            self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;
        }

        let name = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;

        Some(Expr::FieldAccess {
            base: Box::new(left),
            name: intern_global(name.text),
            span: span.extend_to(name.span)
        })
    }

    fn parse_call_exprssion(&mut self, callee: Expr) -> Option<Expr> {
        let span = self.current_token.span;
        self.advance();

        let mut arguments: Vec<Expr> = Vec::new();
        while self.current_token.kind != TokenKind::CloseParen && self.current_token.kind != TokenKind::Eof {
            arguments.push(self.parse_expression()?);

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let close = self.expect(TokenKind::CloseParen, unclosed_delimiter("括号", self.current_token.span).build())?.span;

        Some(Expr::Call { callee: Box::new(callee), args: arguments, span: span.extend_to(close) })
    }

    fn parse_binary_expression(&mut self, left: Expr) -> Option<Expr> {
        let op = self.current_token.kind.clone();
        let precedence = Precedence::from_token_kind(&op);
        
        self.advance();
        
        let right = self.parse_expression_with_precedence(precedence)?;
        
        let span = left.span().extend_to(right.span());
        Some(Expr::Binary {
            left: Box::new(left),
            op: op,
            right: Box::new(right),
            span,
        })
    }

    fn parse_assignment_expression(&mut self, left: Expr) -> Option<Expr> {
        // 检查赋值目标是否有效
        if !matches!(left, Expr::Ident { .. } | Expr::Index { .. }) {
            self.diagnostics.push(error("非法赋值对象")
                .with_span(self.current_token.span)
                .build());

            return None;
        }
        
        let op = match self.current_token.kind {
            TokenKind::PlusEq => AssignOp::Add,
            TokenKind::MinusEq => AssignOp::Subtract,
            TokenKind::StarEq => AssignOp::Multiply,
            TokenKind::SlashEq => AssignOp::Divide,
            TokenKind::RemainderEq => AssignOp::Remainder,
            TokenKind::Eq => AssignOp::Simple,
            _ => unreachable!()
        };
        let start_span = left.span();
        
        self.advance();
        
        let value = self.parse_expression_with_precedence(Precedence::Assignment)?;
        
        let span = start_span.extend_to(value.span());
        Some(Expr::Assignment {
            target: Box::new(left),
            op: op,
            value: Box::new(value),
            span,
        })
    }

    fn parse_if_expression(&mut self) -> Option<Expr> {
        let start_span = self.current_token.span;
        self.advance(); // 消耗 'if'
        
        let condition = self.parse_expression()?;
        
        let then_branch = self.parse_block()?;
        
        let else_branch = if self.current_token.kind == TokenKind::Else {
            self.advance(); // 消耗 'else'
            if self.current_token.kind ==TokenKind::If || self.current_token.kind == TokenKind::OpenBrace {
                Some(Box::new(self.parse_expression()?))
            } else {
                self.diagnostics.push(unclosed_delimiter("期待大括号", self.current_token.span).build());
                return None;
            }
        } else {
            None
        };
        
        let mut span = start_span.extend_to(then_branch.span);
        if let Some(else_branch) = &else_branch {
            span = span.extend_to(else_branch.span());
        }
        
        Some(Expr::If {
            condition: Box::new(condition),
            then_branch: then_branch,
            else_branch,
            span,
        })
    }

    fn parse_while_expression(&mut self) -> Option<Expr> {
        let start_span = self.current_token.span;
        self.advance(); // 消耗 'while'
        
        let condition = self.parse_expression()?;
        
        let body = self.parse_block()?;
        
        let span = start_span.extend_to(body.span);
        Some(Expr::While {
            condition: Box::new(condition),
            body: body,
            span,
        })
    }

    fn parse_for_expression(&mut self) -> Option<Expr> {
        let start_span = self.current_token.span;
        self.advance(); // 消耗 'for'
        
        // 解析迭代变量
        let variable = match self.parse_expression()? {
            Expr::Ident { name, span } => (name, span),
            _ => {
                self.diagnostics.push(self.expect_identifier_error().build());
                return None;
            }
        };
        
        // 检查 'in' 关键字
        self.expect(TokenKind::In, error("期待关键字`in")
            .with_span(self.current_token.span)
            .build())?;
        
        // 解析生成器表达式
        let generator = self.parse_expression()?;
        
        // 解析循环体
        let body = self.parse_block()?;
        
        let span = start_span.extend_to(body.span);
        Some(Expr::For {
            variable: Box::new(Expr::Ident {
                name: variable.0,
                span: variable.1,
            }),
            generator: Box::new(generator),
            body: body,
            span,
        })
    }

    fn parse_block_expression(&mut self) -> Option<Expr> {
        let block = self.parse_block()?;

        Some(Expr::Block { block: block })
    }

    fn parse_block(&mut self) -> Option<Block> {
        let start_span = self.current_token.span;
        
        // 期待开大括号 - 如果失败直接返回，因为这是块的基本结构
        let open_brace = self.expect(TokenKind::OpenBrace, error("期待大括号")
            .with_span(self.current_token.span)
            .build())?;

        let mut statements = Vec::new();
        let mut tail: Option<Box<Expr>> = None;

        // 解析块内容，容忍错误并继续
        while self.current_token.kind != TokenKind::CloseBrace && self.current_token.kind != TokenKind::Eof {
            let stmt_start = self.current_token.span;
            
            // 尝试解析语句，如果失败则记录错误并恢复
            if let Some(stmt) = self.parse_stmt() {
                // 检查是否是尾表达式
                if let Stmt::Expr { expr } = stmt {
                    if self.current_token.kind == TokenKind::CloseBrace {
                        tail = Some(expr);
                        break;
                    } else {
                        statements.push(Stmt::Expr { expr });
                    }
                } else {
                    statements.push(stmt);
                }
            }
            
            // 防止无限循环：如果解析失败但位置没变，强制前进
            if self.current_token.span == stmt_start {
                self.advance();
            }
            
            // 额外安全检查
            if self.current_token.kind == TokenKind::Eof {
                break;
            }
        }

        // 处理闭大括号
        let close_span = self.expect(TokenKind::CloseBrace, unclosed_delimiter("大括号", self.current_token.span).build())?.span;

        Some(Block {
            stmts: statements,
            tail,
            span: open_brace.span.extend_to(close_span),
        })
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        let start_span = self.current_token.span;
        
        let stmt_result = match self.current_token.kind {
            TokenKind::Let => self.parse_let_statement()
                .and_then(|stmt| {
                    // 期待分号
                    self.expect(TokenKind::Semi, self.expect_semi_error().build())?;
                    Some(stmt)
                }),
            TokenKind::Return => self.parse_return_statement()
                .and_then(|stmt| {
                    self.expect(TokenKind::Semi, self.expect_semi_error().build())?;
                    Some(stmt)
                }),
            TokenKind::Break => self.parse_break_statement()
                .and_then(|stmt| {
                    self.expect(TokenKind::Semi, self.expect_semi_error().build())?;
                    Some(stmt)
                }),
            TokenKind::Continue => self.parse_continue_statement()
                .and_then(|stmt| {
                    self.expect(TokenKind::Semi, self.expect_semi_error().build())?;
                    Some(stmt)
                }),
            TokenKind::While |
            TokenKind::For |
            TokenKind::If => {
                // 尝试解析为表达式语句
                let expr = self.parse_expression()?;
                let stmt = Stmt::Expr { expr: Box::new(expr) };
                
                self.eat(TokenKind::Semi);
                
                Some(stmt)
            }
            _ => {
                // 尝试解析为表达式语句
                let expr = self.parse_expression()?;
                let stmt = Stmt::Expr { expr: Box::new(expr) };
                
                // 如果不是尾表达式，需要分号
                if self.current_token.kind != TokenKind::CloseBrace {
                    self.expect(TokenKind::Semi, self.expect_semi_error().build())?;
                }
                
                Some(stmt)
            }
        };

        match stmt_result {
            Some(stmt) => Some(stmt),
            None => {
                self.sync_to_stmt();
                
                None
            }
        }
    }

    fn parse_continue_statement(&mut self) -> Option<Stmt> {
        let span = self.current_token.span;
        self.advance();

        Some(Stmt::Continue { span: span })
    }

    fn parse_break_statement(&mut self) -> Option<Stmt> {
        let mut span = self.current_token.span;
        self.advance();

        let mut value = None;
        if self.current_token.kind != TokenKind::Semi {
            value = Some(self.parse_expression()?);
            span = span.extend_to(value.clone().unwrap().span());
        }

        Some(Stmt::Break { value: value, span: span })
    }

    fn parse_let_statement(&mut self) -> Option<Stmt> {
        let mut span = self.current_token.span;
        self.advance();

        let mutable = self.eat(TokenKind::Mut);

        let name = self.expect(TokenKind::Ident, self.expect_identifier_error().build())?;

        let mut ty: Option<TypeAnnotation> = None;
        if self.eat(TokenKind::Colon) {
            ty = Some(self.parse_type()?);
            span = span.extend_to(ty.clone().unwrap().span());
        }

        let mut value: Option<Expr> = None;
        if self.eat(TokenKind::Eq) {
            value = Some(self.parse_expression()?);
            span = span.extend_to(value.clone().unwrap().span());
        }

        Some(Stmt::Let { mutable: mutable, name: intern_global(name.text), ty: ty, value: value, span: span })
    }

    fn parse_return_statement(&mut self) -> Option<Stmt> {
        let mut span = self.current_token.span;
        self.advance();

        let mut value: Option<Expr> = None;
        if self.current_token.kind != TokenKind::Semi {
            value = Some(self.parse_expression()?);
            span = span.extend_to(value.clone().unwrap().span());
        }

        Some(Stmt::Return { value: value, span: span })
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.current_token.kind == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    #[inline]
    fn expect_identifier_error(&self) -> DiagnosticBuilder {
        expected_identifier(self.current_token.text, self.current_token.span)
    }

    #[inline]
    fn expect_semi_error(&self) -> DiagnosticBuilder {
        error("期待冒号").with_span(self.current_token.span)
    }
}

pub fn parse(source_map: &SourceMap, file_id: FileId) -> (Crate, Vec<Diagnostic>) {
    let source_file = source_map.file(file_id).unwrap();
    let mut parser = match Parser::new(source_file,file_id) {
        Ok(parser) => parser,
        Err(err) => return (Crate::new(vec![]), vec![err])
    };
    parser.parse()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use litec_span::SourceMap;

    fn parse_test(source: &str) -> (Crate, Vec<Diagnostic>) {
        let mut source_map = SourceMap::new();
        let source_file_id = source_map.add_file("test.lc".to_string(), source.to_string(), Path::new("test.lc"));
        parse(&source_map, source_file_id)
    }

    fn assert_parse_success(source: &str) -> Crate {
        let mut source_map = SourceMap::new();
        let source_file_id = source_map.add_file("test.litec".to_string(), source.to_string(), &Path::new("test.litec"));
        let (krate, diagnostics) = parse(&source_map, source_file_id);

        if !diagnostics.is_empty() {
            for diagnostic in diagnostics {
                println!("{}", diagnostic.render(&source_map));
            }
            panic!();
        }

        krate
    }

    fn assert_parse_error(source: &str) -> Vec<Diagnostic> {
        let (_, diagnostics) = parse_test(source);
        if diagnostics.is_empty() { 
            panic!("预期解析失败，但成功了: {}", source); 
        }
        diagnostics
    }

    // 基本语法测试
    #[test]
    fn test_empty_crate() {
        let krate = assert_parse_success("");
        assert_eq!(krate.items.len(), 0);
    }

    #[test]
    fn test_function_declaration() {
        let source = r#"
            fn main() {
                let x = 42;
            }
        "#;
        let krate = assert_parse_success(source);
        assert_eq!(krate.items.len(), 1);
        
        if let Item::Function { name, params, body, .. } = &krate.items[0] {
            assert_eq!(*name, intern_global("main"));
            assert_eq!(params.len(), 0);
            assert_eq!(body.stmts.len(), 1);
        } else {
            panic!("预期是函数声明");
        }
    }

    #[test]
    fn test_function_with_params() {
        let source = r#"
            fn add(a: i32, b: i32) -> i32 {
                a + b
            }
        "#;
        let krate = assert_parse_success(source);
        
        if let Item::Function { name, params, return_type, .. } = &krate.items[0] {
            assert_eq!(*name, intern_global("add"));
            assert_eq!(params.len(), 2);
            assert!(return_type.is_some());
        } else {
            panic!("预期是函数声明");
        }
    }

    #[test]
    fn test_struct_declaration() {
        let source = r#"
            struct Point {
                x: i32,
                y: i32,
            }
        "#;
        let krate = assert_parse_success(source);
        
        if let Item::Struct { name, fields, .. } = &krate.items[0] {
            assert_eq!(*name, intern_global("Point"));
            assert_eq!(fields.len(), 2);
        } else {
            panic!("预期是结构体声明");
        }
    }

    #[test]
    fn test_visibility_modifiers() {
        let source = r#"
            pub fn public_function() {}
            priv fn private_function() {}
            
            pub struct PublicStruct {
                pub field: i32,
                priv private_field: i32,
            }
        "#;
        let krate = assert_parse_success(source);
        assert_eq!(krate.items.len(), 3);
    }

    // 表达式测试
    #[test]
    fn test_binary_expressions() {
        let sources = [
            "1 + 2",
            "a * b - c",
            "x == y && z != w",
            "a < b || c >= d",
        ];
        
        for source in sources {
            let expr_source = format!("fn test() {{ {}; }}", source);
            assert_parse_success(&expr_source);
        }
    }

    #[test]
    fn test_unary_expressions() {
        let sources = [
            "!true",
            "-42",
            "x++",
            "y--",
        ];
        
        for source in sources {
            let expr_source = format!("fn test() {{ {}; }}", source);
            assert_parse_success(&expr_source);
        }
    }

    #[test]
    fn test_literals() {
        let sources = [
            r#"42"#,
            r#"3.14"#,
            r#""hello""#,
            r#"'a'"#,
            r#"true"#,
            r#"false"#,
        ];
        
        for source in sources {
            let expr_source = format!("fn test() {{ {}; }}", source);
            assert_parse_success(&expr_source);
        }
    }

    // 控制流测试
    #[test]
    fn test_if_expression() {
        let source = r#"
            fn test(x: i32) {
                if x > 0 {
                    return 1;
                } else if x < 0 {
                    return -1;
                } else {
                    return 0;
                }
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn test_while_expression() {
        let source = r#"
            fn test() {
                let mut i = 0;
                while i < 10 {
                    i = i + 1;
                }
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn test_for_expression() {
        let source = r#"
            fn test() {
                for i in 1..10 {
                    println(i);
                }
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn test_loop_expression() {
        let source = r#"
            fn test() {
                loop {
                    if should_break() {
                        break;
                    }
                    continue;
                }
            }
        "#;
        assert_parse_success(source);
    }

    // 语句测试
    #[test]
    fn test_let_statement() {
        let source = r#"
            fn test() {
                let x = 42;
                let y: i32 = 100;
                let z;
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn test_return_statement() {
        let source = r#"
            fn test() {
                return;
                return 42;
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn test_break_continue() {
        let source = r#"
            fn test() {
                loop {
                    if condition {
                        break;
                    }
                    if other_condition {
                        continue;
                    }
                    break 42;
                }
            }
        "#;
        assert_parse_success(source);
    }

    // 属性测试
    #[test]
    fn test_attributes() {
        let source = r#"
            #[test]
            fn test_function() {}
            
            #[derive(Debug, Clone)]
            struct TestStruct {}
        "#;
        assert_parse_success(source);
    }

    // Use 语句测试
    #[test]
    fn test_use_statements() {
        let sources = [
            "use std::collections::HashMap;",
            "use std::io;",
            "use crate::module::Item as Alias;",
        ];
        
        for source in sources {
            let krate = assert_parse_success(source);
            assert_eq!(krate.items.len(), 1);
        }
    }

    // 错误恢复测试
    #[test]
    fn test_error_recovery() {
        let source = r#"
            fn missing_paren {
                let x = 42;
            }
            
            fn valid_function() {
                println("Hello");
            }
        "#;
        
        let diagnostics = assert_parse_error(source);
        assert!(!diagnostics.is_empty(), "应该产生诊断信息");
        
        // 检查是否成功恢复了并解析了第二个函数
        let (krate, _) = parse_test(source);
        // 错误恢复后应该至少解析出一个项目
        assert!(!krate.items.is_empty(), "错误恢复后应该解析出一些项目");
    }

    #[test]
    fn test_unterminated_string() {
        let source = r#"
            fn test() {
                let s = "unterminated string;
            }
        "#;
        let diagnostics = assert_parse_error(source);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_missing_semicolon() {
        let source = r#"
            fn test() {
                let x = 42
                let y = 100;
            }
        "#;
        let diagnostics = assert_parse_error(source);
        assert!(!diagnostics.is_empty());
    }

    // 复杂表达式测试
    #[test]
    fn test_nested_expressions() {
        let source = r#"
            fn complex() -> i32 {
                if (a + b) * c > d && e || f {
                    (x.call().field + y[z]) % 10
                } else {
                    -value.unwrap()
                }
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn test_method_chaining() {
        let source = r#"
            fn chain() {
                let result = obj.method().another().final_call();
                builder.set_a(1).set_b(2).build();
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn test_array_indexing() {
        let source = r#"
            fn indexing() {
                let x = arr[0];
                let y = matrix[i][j];
                arr[index] = value;
            }
        "#;
        assert_parse_success(source);
    }

    // 优先级测试
    #[test]
    fn test_operator_precedence() {
        let sources = [
            "a + b * c",      // 应该解析为 a + (b * c)
            "a * b + c",      // 应该解析为 (a * b) + c
            "!a && b",        // 应该解析为 (!a) && b
            "a == b || c",    // 应该解析为 (a == b) || c
        ];
        
        for source in sources {
            let expr_source = format!("fn test() {{ {}; }}", source);
            assert_parse_success(&expr_source);
        }
    }

    // 边界情况测试
    #[test]
    fn test_empty_block() {
        let source = r#"
            fn empty() {}
            fn with_empty_blocks() {
                if true {} else {}
                while false {}
                for i in 0..0 {}
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn test_comments_ignored() {
        let source = r#"
            // 单行注释
            fn commented() {
                // 函数内的注释
                let x = 42; // 行尾注释
                /* 块注释
                   多行内容
                */
            }
        "#;
        assert_parse_success(source);
    }

    // 综合测试
    #[test]
    fn test_complete_program() {
        let source = r#"
            use std::io;
            
            #[derive(Debug)]
            pub struct Calculator {
                value: i32,
            }

            pub fn main() {
                let mut calc = Calculator::new();
                let result = calc.add(10).add(20).get();
                
                if result > 0 {
                    println("Result: {}", result);
                } else {
                    println("Zero result");
                }
                
                for i in 1..5 {
                    println("Iteration: {}", i);
                }
            }
        "#;
        assert_parse_success(source);
    }

    #[test]
    fn print_crate() {
        let source = r#"
            /// 一个简单的计算器实现
            pub struct Calculator {
                value: i32,
            }

            impl Calculator {
                pub fn new() -> Self {
                    Calculator { value: 0 }
                }

                pub fn add(&mut self, x: i32) -> &mut Self {
                    self.value += x;
                    self
                }

                pub fn get(&self) -> i32 {
                    self.value
                }
            }

            /// 主函数
            pub fn main() {
                let mut calc = Calculator::new();
                let result = calc.add(10).add(20).get();

                if result > 0 {
                    println("正数结果: {}", result);
                } else if result < 0 {
                    println("负数结果: {}", result);
                } else {
                    println("零结果");
                }

                for i in 1..=5 {
                    println("迭代 {}: {}", i, i * 10);
                }

                let mut counter = 0;
                while counter < 3 {
                    println("计数器: {}", counter);
                    counter += 1;
                }
            }
        "#;

        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file("test.litec".to_string(), source.to_string(), &Path::new("test.litec"));

        let (krate, _) = parse(&source_map, file_id);

        println!("{:#?}", krate);
        assert!(true)
    }
}