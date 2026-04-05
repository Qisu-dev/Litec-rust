use std::ops::Bound;

use litec_ast::{
    ast::{
        DUMMY_NODE_ID, Expr, ExprKind, Ident, Lit, Mutability, Path, PathSegment, RangeLimits, StructExpr, StructExprField, UnOp
    },
    token::TokenKind,
    util::{
        accos_op::{AssocOp, Fixity},
        precedence::Precedence,
    },
};
use litec_error::error;
use litec_span::{intern_global, respan};

use crate::parser::{Parser, path::PathStyle};

impl<'a> Parser<'a> {
    pub(super) fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_expr_with_precedence(Bound::Unbounded)
    }

    fn parse_expr_with_precedence(&mut self, bound: Bound<Precedence>) -> Option<Expr> {
        self.skip_infix = false;
        self.parse_expr_with_precedence_inner(bound)
    }

    fn parse_expr_with_precedence_inner(&mut self, bound: Bound<Precedence>) -> Option<Expr> {
        let mut left = self.parse_prefix()?;

        while let Some(op) = self.peek_assoc_op() {
            if self.skip_infix {
                break;
            }

            let op_prec = op.value.precedence();
            if !self.allowed_by_bound(op_prec, bound) {
                break;
            }

            let next_bound = match op.value.fixity() {
                Fixity::Left => Bound::Excluded(op_prec),
                Fixity::Right => Bound::Included(op_prec),
                Fixity::None => Bound::Excluded(op_prec),
            };

            left = self.parse_infix(left, next_bound)?;

            if op.value.fixity() == Fixity::None {
                if let Some(next_op) = self.peek_assoc_op() {
                    if next_op.value.precedence() == op_prec {
                        self.error(error("不可结合运算符").with_span(next_op.span).build());
                        return None;
                    }
                }
            }

            left = self.parse_postfix(left)?;
        }

        Some(left)
    }

    /// 辅助函数：判断操作符优先级是否满足边界
    fn allowed_by_bound(&self, prec: Precedence, bound: Bound<Precedence>) -> bool {
        match bound {
            Bound::Included(p) => prec >= p,
            Bound::Excluded(p) => prec > p,
            Bound::Unbounded => true,
        }
    }

    fn parse_postfix(&mut self, mut lhs: Expr) -> Option<Expr> {
        loop {
            match self.current_token.kind {
                TokenKind::PlusPlus | TokenKind::MinusMinus => {
                    self.error(
                        error("不支持自增与自减")
                            .with_span(self.current_token.span)
                            .build(),
                    );
                    return None;
                }
                TokenKind::Dot => lhs = self.parse_field_access_expression(lhs)?,

                TokenKind::PathAccess => lhs = self.parse_path_access_expression(lhs)?,

                TokenKind::OpenParen => lhs = self.parse_call_exprssion(lhs)?,

                TokenKind::OpenBracket => lhs = self.parse_index_expression(lhs)?,

                TokenKind::OpenBrace if matches!(lhs.kind, ExprKind::Path { .. }) => {
                    if self.restrictions.no_struct_literal {
                        let snapshot = self.snapshot();

                        // 尝试解析 struct_init（但不消耗 token，只是验证）
                        if self.is_struct_init_start() {
                            self.restore(snapshot);
                            self.error(
                                error("有歧义的代码")
                                    .with_help("可以添加括号")
                                    .with_span(lhs.span)
                                    .build(),
                            );
                            return None;
                        }

                        self.restore(snapshot);
                        self.skip_infix = true;
                        return Some(lhs); // 不是 struct_init，安全返回
                    }

                    lhs = self.try_parse_struct_init(lhs)?;
                }
                _ => return Some(lhs),
            }
        }
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        let kind = self.current_token.kind;
        let lhs = match kind {
            TokenKind::Literal { kind, suffix } => {
                let span = self.current_token.span;

                let suffix_id = suffix;

                let expr = ExprKind::Literal(Lit {
                    kind: kind,
                    value: self.current_token.text,
                    suffix: suffix_id,
                });

                let span = span.extend_to(self.current_token.span);

                self.advance();
                Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: expr,
                    span: span,
                }
            }
            TokenKind::Ident if self.look_ahead(1, |tok| tok.kind == TokenKind::PathAccess) => {
                Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::Path(self.parse_path(PathStyle::Expr)?),
                    span: self.current_token.span,
                }
            }
            TokenKind::Ident => {
                let name = self.current_token.text;
                let span = self.current_token.span;
                self.advance();

                let ident = Ident {
                    text: name,
                    span: span,
                };

                Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::Path(ident.to_path()),
                    span,
                }
            }
            TokenKind::OpenParen => {
                let span = self.current_token.span;
                self.advance();

                // 检查空括号：() 是空元组
                if self.current_token.kind == TokenKind::CloseParen {
                    let close_span = self.current_token.span;
                    self.advance();
                    return Some(Expr {
                        node_id: DUMMY_NODE_ID,
                        kind: ExprKind::Unit,
                        span: span.extend_to(close_span),
                    });
                }

                // 解析第一个表达式
                let first_expr = self.parse_expr()?;

                // 检查是否有逗号 - 如果有逗号就是元组
                if self.eat(TokenKind::Comma) {
                    let mut elements = vec![first_expr];

                    // 继续解析元组的其他元素
                    while self.current_token.kind != TokenKind::CloseParen
                        && self.current_token.kind != TokenKind::Eof
                    {
                        elements.push(self.parse_expr()?);

                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }

                    let close_paren = self.expect(
                        TokenKind::CloseParen,
                        error("期待 `)`").with_span(self.current_token.span).build(),
                    )?;

                    let span = span.extend_to(close_paren.span);
                    Expr {
                        node_id: DUMMY_NODE_ID,
                        kind: ExprKind::Tuple(elements),
                        span,
                    }
                } else {
                    // 没有逗号，就是分组表达式
                    let close_paren = self.expect(
                        TokenKind::CloseParen,
                        error("期待 `)`").with_span(self.current_token.span).build(),
                    )?;

                    let span = span.extend_to(close_paren.span);
                    Expr {
                        node_id: DUMMY_NODE_ID,
                        kind: ExprKind::Grouped(Box::new(first_expr)),
                        span,
                    }
                }
            }
            TokenKind::Bang | TokenKind::Minus | TokenKind::Mul => {
                let start_span = self.current_token.span;
                let op = match self.current_token.kind {
                    TokenKind::Bang => UnOp::Not,
                    TokenKind::Minus => UnOp::Neg,
                    TokenKind::Mul => UnOp::Deref,
                    _ => unreachable!(),
                };

                self.advance();

                let expr = self.parse_expr_with_precedence(Bound::Excluded(Precedence::Prefix))?;
                let span = start_span.extend_to(expr.span);

                Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::Unary(op, Box::new(expr)),
                    span,
                }
            }
            TokenKind::If => self.parse_if_expression()?,
            TokenKind::While => self.parse_while_expression()?,
            TokenKind::For => self.parse_for_expression()?,
            TokenKind::OpenBrace => self.parse_block_expression()?,
            TokenKind::Loop => self.parse_loop_expression()?,
            TokenKind::True => {
                let span = self.current_token.span;
                self.advance();
                Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::Bool(true),
                    span,
                }
            }
            TokenKind::False => {
                let span = self.current_token.span;
                self.advance();
                Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::Bool(false),
                    span,
                }
            }
            TokenKind::BitAnd => {
                let span = self.current_token.span;
                self.advance();
                let expr = self.parse_expr()?;
                let span = span.extend_to(expr.span);
                Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::AddressOf(Box::new(expr)),
                    span,
                }
            }
            _ => {
                self.error(
                    error("期待表达式")
                        .with_help("添加一个表达式在此处")
                        .with_span(self.current_token.span)
                        .build(),
                );
                return None;
            }
        };

        let expr = self.parse_postfix(lhs)?;
        Some(expr)
    }

    fn parse_infix(&mut self, lhs: Expr, next_bound: Bound<Precedence>) -> Option<Expr> {
        let op = self.peek_assoc_op()?;
        match op.value {
            AssocOp::Binary(_) => self.parse_binary_expression(lhs, next_bound),

            AssocOp::Assign | AssocOp::AssignOp(_) => self.parse_assignment_expression(lhs),

            AssocOp::Cast => self.parse_as_expression(lhs),

            AssocOp::Range(limit) => self.parse_range_expression(lhs, limit),
        }
    }

    fn parse_as_expression(&mut self, lhs: Expr) -> Option<Expr> {
        self.advance();
        let target_type = self.parse_type()?;
        let span = lhs.span.extend_to(target_type.span);
        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Cast(Box::new(lhs), Box::new(target_type)),
            span,
        })
    }

    fn is_struct_init_start(&mut self) -> bool {
        let snapshot = self.snapshot();

        self.advance(); // 消耗 {

        // 空结构体 {}
        if self.current_token.kind == TokenKind::CloseBrace {
            self.restore(snapshot);
            return true;
        }

        // 必须是 Ident + :
        let result = if self.current_token.kind == TokenKind::Ident {
            self.advance();
            self.current_token.kind == TokenKind::Colon
        } else {
            false
        };

        self.restore(snapshot);
        result
    }

    fn extract_struct_path(&self, lhs: &Expr) -> Option<Path> {
        match lhs.kind {
            ExprKind::Path(ref path) => Some(path.clone()),
            _ => None,
        }
    }
    fn parse_struct_field(&mut self) -> Option<StructExprField> {
        let ident = self.parse_ident()?;
        let is_shorthand = !self.eat(TokenKind::Colon);

        let (value, span) = if is_shorthand {
            let expr = Expr {
                node_id: DUMMY_NODE_ID,
                kind: ExprKind::Path(ident.to_path()),
                span: ident.span,
            };

            (expr, ident.span)
        } else {
            let expr = self.parse_expr()?;
            let expr_span = expr.span;
            (expr, ident.span.extend_to(expr_span))
        };

        Some(StructExprField {
            name: ident,
            value,
            is_shorthand,
            span,
        })
    }

    fn can_start_struct_init(&mut self) -> bool {
        matches!(
            self.look_ahead(1, |tok| tok.kind),
            TokenKind::Ident | TokenKind::CloseBrace
        )
    }

    fn parse_struct_fields(&mut self) -> Option<Vec<StructExprField>> {
        let mut fields = Vec::new();
        loop {
            if self.current_token.kind == TokenKind::CloseBrace {
                break;
            }
            if self.current_token.kind != TokenKind::Ident {
                // 非标识符，提前退出，让上层处理回退
                return None;
            }
            fields.push(self.parse_struct_field()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        Some(fields)
    }

    fn try_parse_struct_init(&mut self, lhs: Expr) -> Option<Expr> {
        let path = self.extract_struct_path(&lhs)?;

        self.try_parse(|parser| {
            if !parser.can_start_struct_init() {
                return Some(lhs);
            }

            parser.advance(); // 消耗 '{'

            let fields = parser.parse_struct_fields()?;

            let close_span = parser
                .expect(
                    TokenKind::CloseBrace,
                    error("期待 `}}`")
                        .with_span(parser.current_token.span)
                        .build(),
                )?
                .span;
            let span = lhs.span.extend_to(close_span);

            // 如果是 if 表达式的一部分，回退
            if parser.current_token.kind == TokenKind::Else {
                return Some(lhs);
            }

            Some(Expr {
                node_id: DUMMY_NODE_ID,
                kind: ExprKind::StructExpr(StructExpr {
                    node_id: DUMMY_NODE_ID,
                    path,
                    fields,
                }),
                span,
            })
        })
    }

    fn parse_index_expression(&mut self, indexed: Expr) -> Option<Expr> {
        self.expect(
            TokenKind::OpenBracket,
            error("期待 `[`").with_span(self.current_token.span).build(),
        )?;

        let index = self.parse_expr()?;

        let close_bracket_span = self
            .expect(
                TokenKind::CloseBracket,
                error("期待 `[`").with_span(self.current_token.span).build(),
            )?
            .span;

        let span = indexed.span.extend_to(close_bracket_span);

        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Index(Box::new(indexed), Box::new(index)),
            span: span,
        })
    }

    fn parse_range_expression(&mut self, lhs: Expr, limit: RangeLimits) -> Option<Expr> {
        self.advance(); // 消耗 `..` 或 `..=`

        let rhs = self.parse_expr()?;

        let span = lhs.span.extend_to(rhs.span);

        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Range(Box::new(lhs), Box::new(rhs), limit),
            span: span,
        })
    }

    fn parse_loop_expression(&mut self) -> Option<Expr> {
        let span = self.current_token.span;
        self.advance();

        let body = self.parse_block()?;
        let span = span.extend_to(body.span);

        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Loop(Box::new(body)),
            span: span,
        })
    }

    fn parse_field_access_expression(&mut self, lhs: Expr) -> Option<Expr> {
        let span = self.current_token.span;
        self.advance();

        let name = self.parse_ident()?;
        let span = span.extend_to(name.span);

        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Field(Box::new(lhs), name),
            span: span,
        })
    }

    fn parse_path_access_expression(&mut self, lhs: Expr) -> Option<Expr> {
        let lhs_path = match lhs.kind {
            ExprKind::Path(path) => path,
            _ => {
                self.error(error("左边应该是标识符").with_span(lhs.span).build());
                return None;
            }
        };

        let mut path = self.parse_path(PathStyle::Expr)?;
        path.segments.splice(0..0, lhs_path.segments);
        let span = lhs.span.extend_to(path.span);

        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Path(path),
            span: span,
        })
    }

    fn parse_call_exprssion(&mut self, callee: Expr) -> Option<Expr> {
        let span = self.current_token.span;
        self.advance();

        let mut arguments: Vec<Expr> = Vec::new();
        while self.current_token.kind != TokenKind::CloseParen
            && self.current_token.kind != TokenKind::Eof
        {
            arguments.push(self.parse_expr()?);

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let span = span.extend_to(
            self.expect(
                TokenKind::CloseParen,
                error("期待 `)`").with_span(self.current_token.span).build(),
            )?
            .span,
        );

        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Call(Box::new(callee), arguments),
            span: span,
        })
    }

    fn parse_binary_expression(
        &mut self,
        lhs: Expr,
        next_bound: Bound<Precedence>,
    ) -> Option<Expr> {
        let op = self.peek_assoc_op()?;
        let op = match op.value {
            AssocOp::Binary(bin_op) => respan(op.span, bin_op),
            _ => unreachable!(),
        };

        self.advance();

        let right = self.parse_expr_with_precedence(next_bound)?;

        let span = lhs.span.extend_to(right.span);
        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Binary(Box::new(lhs), op, Box::new(right)),
            span: span,
        })
    }

    fn parse_assignment_expression(&mut self, lhs: Expr) -> Option<Expr> {
        let op = self.peek_assoc_op()?;
        let start_span = lhs.span;

        self.advance();

        let value = self.parse_expr_with_precedence(Bound::Included(Precedence::Assign))?;

        let span = start_span.extend_to(value.span);

        match op.value {
            AssocOp::Assign => Some(Expr {
                node_id: DUMMY_NODE_ID,
                kind: ExprKind::Assignment(Box::new(lhs), Box::new(value)),
                span: span,
            }),
            AssocOp::AssignOp(assign_op) => Some(Expr {
                node_id: DUMMY_NODE_ID,
                kind: ExprKind::AssignmentWithOp(
                    Box::new(lhs),
                    respan(op.span, assign_op),
                    Box::new(value),
                ),
                span: span,
            }),
            _ => unreachable!(),
        }
    }

    fn parse_if_expression(&mut self) -> Option<Expr> {
        let mut span = self.current_token.span;
        self.advance(); // 消耗 'if'
        let condition = self.parse_expr()?;

        let then_branch = self.parse_block()?;
        span = span.extend_to(then_branch.span);

        let else_branch = if self.current_token.kind == TokenKind::Else {
            self.advance(); // 消耗 'else'
            if self.current_token.kind == TokenKind::If
                || self.current_token.kind == TokenKind::OpenBrace
            {
                let expr = self.parse_expr()?;
                span = span.extend_to(expr.span);
                Some(Box::new(expr))
            } else {
                self.error(
                    error("期待 `if` 或 `{`")
                        .with_span(self.current_token.span)
                        .build(),
                );
                return None;
            }
        } else {
            None
        };

        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::If(Box::new(condition), then_branch, else_branch),
            span: span,
        })
    }

    fn parse_while_expression(&mut self) -> Option<Expr> {
        let start_span = self.current_token.span;
        self.advance(); // 消耗 'while'

        let condition = self.parse_expr()?;

        let body = self.parse_block()?;

        let span = start_span.extend_to(body.span);
        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::While(Box::new(condition), Box::new(body)),
            span: span,
        })
    }

    fn parse_for_expression(&mut self) -> Option<Expr> {
        let start_span = self.current_token.span;
        self.advance(); // 消耗 'for'

        let mutability = if self.eat(TokenKind::Mut) {
            Mutability::Mutable
        } else {
            Mutability::Immutable
        };

        // 解析迭代变量
        let variable = self.parse_ident()?;

        // 检查 'in' 关键字
        self.expect(
            TokenKind::In,
            error("期待 `in`")
                .with_span(self.current_token.span)
                .build(),
        )?;

        // 解析生成器表达式
        let generator = self.parse_expr()?;

        // 解析循环体
        let body = self.parse_block()?;

        let span = start_span.extend_to(body.span);
        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::For {
                mutability,
                variable,
                iter: Box::new(generator),
                body: Box::new(body),
            },
            span: span,
        })
    }

    fn parse_block_expression(&mut self) -> Option<Expr> {
        let block = self.parse_block()?;
        let span = block.span;
        Some(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Block(Box::new(block)),
            span: span,
        })
    }
}
