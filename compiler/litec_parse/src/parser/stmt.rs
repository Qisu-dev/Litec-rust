use litec_ast::{
    ast::{DUMMY_NODE_ID, Mutability, Stmt, StmtKind},
    token::TokenKind,
};

use crate::parser::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_stmt(&mut self) -> Option<Stmt> {
        let span = self.current_token.span;
        let stmt_kind = self.parse_stmt_kind()?;
        let span = span.extend_to(self.last_token_end_span);

        Some(Stmt {
            node_id: DUMMY_NODE_ID,
            kind: stmt_kind,
            span: span,
        })
    }

    fn parse_stmt_kind(&mut self) -> Option<StmtKind> {
        let stmt_result = match self.current_token.kind {
            TokenKind::Let => self.parse_let_statement(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Break => self.parse_break_statement(),
            TokenKind::Continue => self.parse_continue_statement(),
            _ => {
                // 尝试解析为表达式语句
                let expr = self.parse_expr()?;
                let stmt = if self.current_token.kind == TokenKind::Semi {
                    StmtKind::Semi(Box::new(expr))
                } else {
                    StmtKind::Expr(Box::new(expr))
                };

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

    fn parse_continue_statement(&mut self) -> Option<StmtKind> {
        self.advance();

        Some(StmtKind::Continue)
    }

    fn parse_break_statement(&mut self) -> Option<StmtKind> {
        self.advance();

        let value = if self.current_token.kind != TokenKind::Semi {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        Some(StmtKind::Break(value))
    }

    fn parse_let_statement(&mut self) -> Option<StmtKind> {
        self.advance(); // 消耗 `let`

        let mutable = if self.eat(TokenKind::Mut) {
            Mutability::Mutable
        } else {
            Mutability::Immutable
        };

        let name = self.parse_ident()?;

        let ty = if self.eat(TokenKind::Colon) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        let value = if self.eat(TokenKind::Assign) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        Some(StmtKind::Let(mutable, name, ty, value))
    }

    fn parse_return_statement(&mut self) -> Option<StmtKind> {
        self.advance();

        let value = if self.current_token.kind != TokenKind::Semi {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        Some(StmtKind::Return(value))
    }
}
