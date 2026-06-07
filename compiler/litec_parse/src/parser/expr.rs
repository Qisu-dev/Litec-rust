use std::ops::Bound;

use crate::parser::{Parser, path::PathStyle};
use litec_ast::{
    ast::{
        Arm, DUMMY_NODE_ID, Expr, ExprKind, Ident, Lit, Path, RangeLimits, StructExpr,
        StructExprField, UnOp,
    },
    token::{LiteralKind, Token, TokenKind},
    util::{
        accos_op::{AssocOp, Fixity},
        precedence::Precedence,
    },
};
use litec_error::{PResult, error};
use litec_span::{StringId, respan};

#[derive(Debug)]
pub(super) enum DestructuredFloat {
    /// 纯指数形式，如 `1e2`
    Single(StringId),
    /// 以点号结尾，如 `1.`
    TrailingDot(StringId), // 整数部分
    /// 普通小数，如 `1.2` 或带指数 `1.2e3`
    MiddleDot(StringId, StringId), // (整数部分, 小数部分)
    /// 非法
    Error,
}

impl<'a> Parser<'a> {
    pub(super) fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_expr_with_precedence(Bound::Unbounded)
    }

    fn parse_expr_with_precedence(&mut self, bound: Bound<Precedence>) -> PResult<Expr> {
        self.skip_infix = false;
        self.parse_expr_with_precedence_inner(bound)
    }

    fn parse_expr_with_precedence_inner(&mut self, bound: Bound<Precedence>) -> PResult<Expr> {
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
                        self.error(error("不可结合运算符").with_span(next_op.span));
                    }
                }
            }

            left = self.parse_postfix(left)?;
        }

        Ok(left)
    }

    /// 辅助函数：判断操作符优先级是否满足边界
    fn allowed_by_bound(&self, prec: Precedence, bound: Bound<Precedence>) -> bool {
        match bound {
            Bound::Included(p) => prec >= p,
            Bound::Excluded(p) => prec > p,
            Bound::Unbounded => true,
        }
    }

    fn parse_postfix(&mut self, mut lhs: Expr) -> PResult<Expr> {
        loop {
            match self.current_token.kind {
                TokenKind::PlusPlus | TokenKind::MinusMinus => {
                    return Err(
                        self.error(error("不支持自增与自减").with_span(self.current_token.span))
                    );
                }
                TokenKind::Dot => lhs = self.parse_field_access_expression(lhs)?,

                TokenKind::PathAccess => lhs = self.parse_path_access_expression(lhs)?,

                TokenKind::OpenParen => lhs = self.parse_call_exprssion(lhs)?,

                TokenKind::OpenBracket => lhs = self.parse_index_expression(lhs)?,

                TokenKind::OpenBrace if matches!(lhs.kind, ExprKind::Path { .. }) => {
                    match self.try_parse_struct_init(lhs) {
                        Ok(expr) => lhs = expr,
                        Err(lhs) => {
                            return Ok(lhs);
                        }
                    }
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn parse_prefix(&mut self) -> PResult<Expr> {
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
            TokenKind::SelfUpper => {
                let span = self.current_token.span;
                let text = self.current_token.text;
                self.advance();

                let ident = Ident { text, span };

                Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::Path(ident.to_path()),
                    span,
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
                    return Ok(Expr {
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
                        error("期待 `)`").with_span(self.current_token.span),
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
                        error("期待 `)`").with_span(self.current_token.span),
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
            TokenKind::Match => self.parse_match_expression()?,
            TokenKind::Return => self.parse_return_expression()?,
            TokenKind::Continue => self.parse_continue_statement()?,
            TokenKind::Break => self.parse_break_statement()?,
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
                return Err(self.error(
                    error("期待表达式")
                        .with_help("添加一个表达式在此处")
                        .with_span(self.current_token.span),
                ));
            }
        };

        let expr = self.parse_postfix(lhs)?;
        Ok(expr)
    }

    fn parse_infix(&mut self, lhs: Expr, next_bound: Bound<Precedence>) -> PResult<Expr> {
        let op = match self.peek_assoc_op() {
            Some(op) => op,
            None => {
                return Err(self.error(error("期待assoc op").with_span(self.current_token.span)));
            }
        };
        match op.value {
            AssocOp::Binary(_) => self.parse_binary_expression(lhs, next_bound),

            AssocOp::Assign | AssocOp::AssignOp(_) => self.parse_assignment_expression(lhs),

            AssocOp::Cast => self.parse_as_expression(lhs),

            AssocOp::Range(limit) => self.parse_range_expression(lhs, limit),
        }
    }

    fn parse_match_expression(&mut self) -> PResult<Expr> {
        self.advance(); // 度过match

        let expr = self.parse_expr()?;

        self.expect(TokenKind::OpenBrace, self.span_error("期待 `{`"))?;

        let mut arms = Vec::new();

        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            arms.push(self.parse_arm()?);
        }

        let span = expr.span.extend_to(
            self.expect(TokenKind::CloseBrace, self.span_error("期待 `}`"))?
                .span,
        );

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Match(Box::new(expr), arms),
            span: span,
        })
    }

    fn parse_arm(&mut self) -> PResult<Arm> {
        let pattern = self.parse_pat()?;

        let guard = if self.eat(TokenKind::If) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        self.expect(TokenKind::FatArrow, self.span_error("期待 `=>`"))?;

        let body = if self.check(TokenKind::OpenBrace) {
            let body = self.parse_expr()?;
            self.eat(TokenKind::Comma);
            body
        } else {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::Comma, self.span_error("期待 `,`"))?;
            expr
        };
        let span = pattern.span.extend_to(body.span);

        Ok(Arm {
            pat: pattern,
            guard,
            body: Box::new(body),
            span,
            node_id: DUMMY_NODE_ID,
        })
    }

    fn parse_as_expression(&mut self, lhs: Expr) -> PResult<Expr> {
        self.advance();
        let target_type = self.parse_ty()?;
        let span = lhs.span.extend_to(target_type.span);
        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Cast(Box::new(lhs), Box::new(target_type)),
            span,
        })
    }

    fn extract_struct_path(&mut self, lhs: &Expr) -> Option<Path> {
        match lhs.kind {
            ExprKind::Path(ref path) => Some(path.clone()),
            _ => None,
        }
    }

    fn parse_struct_field(&mut self) -> PResult<StructExprField> {
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

        Ok(StructExprField {
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

    fn parse_struct_fields(&mut self) -> PResult<Vec<StructExprField>> {
        let mut fields = Vec::new();
        loop {
            if self.current_token.kind == TokenKind::CloseBrace {
                break;
            }
            if self.current_token.kind != TokenKind::Ident {
                // 非标识符，提前退出，让上层处理回退
                return Err(self.error(error("期待标识符").with_span(self.current_token.span)));
            }
            fields.push(self.parse_struct_field()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        Ok(fields)
    }

    fn try_parse_struct_init(&mut self, lhs: Expr) -> Result<Expr, Expr> {
        let path = match self.extract_struct_path(&lhs) {
            Some(path) => path,
            None => return Err(lhs),
        };

        let snapshot = self.snapshot(); // 保存状态

        if !self.can_start_struct_init() {
            // 尚未消耗 token，直接返回
            return Err(lhs);
        }

        self.advance(); // 消耗 '{'

        // 解析字段，如果失败则回滚
        let fields = match self.parse_struct_fields() {
            Ok(fields) => fields,
            Err(_) => {
                self.restore(snapshot);
                return Err(lhs);
            }
        };

        let close_span = match self.expect(
            TokenKind::CloseBrace,
            error("期待 `}`").with_span(self.current_token.span),
        ) {
            Ok(t) => t.span,
            Err(_) => {
                self.restore(snapshot);
                return Err(lhs);
            }
        };

        if self.current_token.kind == TokenKind::Else {
            self.restore(snapshot);
            return Err(lhs);
        }

        let span = lhs.span.extend_to(close_span);
        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::StructExpr(StructExpr {
                node_id: DUMMY_NODE_ID,
                path,
                fields,
            }),
            span,
        })
    }

    fn parse_index_expression(&mut self, indexed: Expr) -> PResult<Expr> {
        self.expect(
            TokenKind::OpenBracket,
            error("期待 `[`").with_span(self.current_token.span),
        )?;

        let index = self.parse_expr()?;

        let close_bracket_span = self
            .expect(
                TokenKind::CloseBracket,
                error("期待 `[`").with_span(self.current_token.span),
            )?
            .span;

        let span = indexed.span.extend_to(close_bracket_span);

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Index(Box::new(indexed), Box::new(index)),
            span: span,
        })
    }

    fn parse_range_expression(&mut self, lhs: Expr, limit: RangeLimits) -> PResult<Expr> {
        self.advance(); // 消耗 `..` 或 `..=`

        let rhs = self.parse_expr()?;

        let span = lhs.span.extend_to(rhs.span);

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Range(Box::new(lhs), Box::new(rhs), limit),
            span: span,
        })
    }

    fn parse_loop_expression(&mut self) -> PResult<Expr> {
        let span = self.current_token.span;
        self.advance();

        let body = self.parse_block()?;
        let span = span.extend_to(body.span);

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Loop(Box::new(body)),
            span: span,
        })
    }

    fn parse_field_access_expression(&mut self, mut lhs: Expr) -> PResult<Expr> {
        self.advance();

        match self.current_token.kind {
            TokenKind::Ident => {
                let name = self.parse_ident()?;
                let span = lhs.span.extend_to(name.span);

                Ok(Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::Field(Box::new(lhs), name),
                    span: span,
                })
            }
            TokenKind::Literal {
                kind: LiteralKind::Integer,
                suffix,
            } => {
                if suffix.is_some() {
                    return Err(self.error(self.span_error(format!("禁止字段访问使用后缀"))));
                }
                let span = lhs.span.extend_to(self.current_token.span);
                let result = Ok(Expr {
                    node_id: DUMMY_NODE_ID,
                    kind: ExprKind::Field(
                        Box::new(lhs),
                        Ident {
                            text: self.current_token.text,
                            span: self.current_token.span,
                        },
                    ),
                    span,
                });
                self.advance();
                result
            }
            TokenKind::Literal {
                kind: LiteralKind::Float,
                suffix,
            } => {
                if suffix.is_some() {
                    return Err(self.error(self.span_error(format!("禁止字段访问使用后缀"))));
                }

                match self.break_up_float(self.current_token.text) {
                    DestructuredFloat::Single(_sym) => {
                        // 例如 1e2，在字段访问中无意义，直接报错
                        return Err(self.error(self.span_error("在后缀中不允许浮点数")));
                    }
                    DestructuredFloat::TrailingDot(sym) => {
                        let span = lhs.span.extend_to(self.current_token.span);
                        lhs = Expr {
                            node_id: DUMMY_NODE_ID,
                            kind: ExprKind::Field(
                                Box::new(lhs),
                                Ident {
                                    text: sym,
                                    span: self.current_token.span,
                                },
                            ),
                            span,
                        };
                        self.current_token =
                            Token::new(TokenKind::Dot, self.current_token.span, ".".into());
                    }
                    DestructuredFloat::MiddleDot(sym1, sym2) => {
                        if sym2.to_string().contains('e') || sym2.to_string().contains('E') {
                            return Err(self.error(self.span_error("字段访问不允许指数")));
                        }
                        let span = lhs.span.extend_to(self.current_token.span);
                        lhs = Expr {
                            node_id: DUMMY_NODE_ID,
                            kind: ExprKind::Field(Box::new(lhs), Ident { text: sym1, span }),
                            span,
                        };
                        lhs = Expr {
                            node_id: DUMMY_NODE_ID,
                            kind: ExprKind::Field(Box::new(lhs), Ident { text: sym2, span }),
                            span,
                        };
                    }
                    DestructuredFloat::Error => {
                        return Err(self.error(self.span_error("非法浮点数不可以做字段访问")));
                    }
                }

                Ok(lhs)
            }
            _ => Err(self.error(self.span_error("未知的token"))),
        }
    }

    fn break_up_float(&self, name: StringId) -> DestructuredFloat {
        let s = name.to_string();
        if let Some(dot_pos) = s.find('.') {
            let int_part = &s[..dot_pos];
            let rest = &s[dot_pos + 1..];
            if rest.is_empty() {
                DestructuredFloat::TrailingDot(int_part.into())
            } else {
                DestructuredFloat::MiddleDot(int_part.into(), rest.into())
            }
        } else {
            if s.find("e").is_some() || s.find("E").is_some() {
                DestructuredFloat::Single(name)
            } else {
                DestructuredFloat::Error
            }
        }
    }

    fn parse_path_access_expression(&mut self, lhs: Expr) -> PResult<Expr> {
        let lhs_path = match lhs.kind {
            ExprKind::Path(path) => path,
            _ => {
                return Err(self.error(error("左边应该是标识符").with_span(lhs.span)));
            }
        };

        let mut path = self.parse_path(PathStyle::Expr)?;
        path.segments.splice(0..0, lhs_path.segments);
        let span = lhs.span.extend_to(path.span);

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Path(path),
            span: span,
        })
    }

    fn parse_call_exprssion(&mut self, callee: Expr) -> PResult<Expr> {
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
                error("期待 `)`").with_span(self.current_token.span),
            )?
            .span,
        );

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Call(Box::new(callee), arguments),
            span: span,
        })
    }

    fn parse_binary_expression(
        &mut self,
        lhs: Expr,
        next_bound: Bound<Precedence>,
    ) -> PResult<Expr> {
        let op = match self.peek_assoc_op() {
            Some(op) => op,
            None => {
                return Err(self.error(error("期待 assoc op").with_span(self.current_token.span)));
            }
        };
        let op = match op.value {
            AssocOp::Binary(bin_op) => respan(op.span, bin_op),
            _ => unreachable!(),
        };

        self.advance();

        let right = self.parse_expr_with_precedence(next_bound)?;

        let span = lhs.span.extend_to(right.span);
        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Binary(Box::new(lhs), op, Box::new(right)),
            span: span,
        })
    }

    fn parse_assignment_expression(&mut self, lhs: Expr) -> PResult<Expr> {
        let op = match self.peek_assoc_op() {
            Some(op) => op,
            None => {
                return Err(self.error(error("期待 assoc op").with_span(self.current_token.span)));
            }
        };
        let start_span = lhs.span;

        self.advance();

        let value = self.parse_expr_with_precedence(Bound::Included(Precedence::Assign))?;

        let span = start_span.extend_to(value.span);

        match op.value {
            AssocOp::Assign => Ok(Expr {
                node_id: DUMMY_NODE_ID,
                kind: ExprKind::Assignment(Box::new(lhs), Box::new(value)),
                span: span,
            }),
            AssocOp::AssignOp(assign_op) => Ok(Expr {
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

    fn parse_if_expression(&mut self) -> PResult<Expr> {
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
                return Err(
                    self.error(error("期待 `if` 或 `{`").with_span(self.current_token.span))
                );
            }
        } else {
            None
        };

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::If(Box::new(condition), then_branch, else_branch),
            span: span,
        })
    }

    fn parse_while_expression(&mut self) -> PResult<Expr> {
        let start_span = self.current_token.span;
        self.advance(); // 消耗 'while'

        let condition = self.parse_expr()?;

        let body = self.parse_block()?;

        let span = start_span.extend_to(body.span);
        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::While(Box::new(condition), Box::new(body)),
            span: span,
        })
    }

    fn parse_for_expression(&mut self) -> PResult<Expr> {
        let start_span = self.current_token.span;
        self.advance(); // 消耗 'for'

        // 解析迭代变量
        let variable = self.parse_pat()?;

        // 检查 'in' 关键字
        self.expect(
            TokenKind::In,
            error("期待 `in`").with_span(self.current_token.span),
        )?;

        // 解析生成器表达式
        let generator = self.parse_expr()?;

        // 解析循环体
        let body = self.parse_block()?;

        let span = start_span.extend_to(body.span);
        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::For {
                variable,
                iter: Box::new(generator),
                body: Box::new(body),
            },
            span: span,
        })
    }

    fn parse_block_expression(&mut self) -> PResult<Expr> {
        let block = self.parse_block()?;
        let span = block.span;
        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Block(Box::new(block)),
            span: span,
        })
    }

    fn parse_return_expression(&mut self) -> PResult<Expr> {
        let mut span = self.current_token.span;
        self.advance();

        let value = if self.current_token.kind != TokenKind::Semi {
            let expr = Box::new(self.parse_expr()?);
            span = span.extend_to(expr.span);
            Some(expr)
        } else {
            None
        };

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Return(value),
            span,
        })
    }

    fn parse_continue_statement(&mut self) -> PResult<Expr> {
        let span = self.current_token.span;
        self.advance();

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Continue,
            span,
        })
    }

    fn parse_break_statement(&mut self) -> PResult<Expr> {
        let mut span = self.current_token.span;
        self.advance();

        let value = if self.current_token.kind != TokenKind::Semi {
            let expr = Box::new(self.parse_expr()?);
            span = span.extend_to(expr.span);
            Some(expr)
        } else {
            None
        };

        Ok(Expr {
            node_id: DUMMY_NODE_ID,
            kind: ExprKind::Break(value),
            span,
        })
    }
}
