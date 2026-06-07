use litec_ast::{
    ast::{DUMMY_NODE_ID, Stmt, StmtKind},
    token::TokenKind,
};
use litec_error::PResult;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_stmt(&mut self) -> PResult<Stmt> {
        let span = self.current_token.span;
        let stmt_kind = self.parse_stmt_kind()?;
        let span = span.extend_to(self.last_token_end_span);

        Ok(Stmt {
            node_id: DUMMY_NODE_ID,
            kind: stmt_kind,
            span: span,
        })
    }

    fn parse_stmt_kind(&mut self) -> PResult<StmtKind> {
        match self.current_token.kind {
            TokenKind::Let => self.parse_let_statement(),
            TokenKind::Defer => self.parse_defer_statement(),
            _ => {
                let expr = self.parse_expr()?;
                if self.eat(TokenKind::Semi) {
                    Ok(StmtKind::Semi(Box::new(expr)))
                } else {
                    Ok(StmtKind::Expr(Box::new(expr)))
                }
            }
        }
    }

    fn parse_let_statement(&mut self) -> PResult<StmtKind> {
        self.advance(); // 消耗 `let`

        let name = self.parse_pat()?;

        let ty = if self.eat(TokenKind::Colon) {
            Some(Box::new(self.parse_ty()?))
        } else {
            None
        };

        let value = if self.eat(TokenKind::Assign) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        self.expect(TokenKind::Semi, self.expect_semi_error())?;

        Ok(StmtKind::Let(name, ty, value))
    }

    fn parse_defer_statement(&mut self) -> PResult<StmtKind> {
        self.advance();

        let expr = self.parse_expr()?;

        if expr.kind.expr_requires_semi_to_be_stmt() {
            self.expect(TokenKind::Semi, self.expect_semi_error())?;
        }

        Ok(StmtKind::Defer(Box::new(expr)))
    }
}
