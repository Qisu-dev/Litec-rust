mod expr;
mod item;
mod path;
mod stmt;
mod ty;

use crate::lexer::{Lexer, LexerSnapshot};
use litec_ast::{
    ast::{Crate, DUMMY_NODE_ID, Ident, StrLit},
    token::{LiteralKind, Token, TokenKind},
    util::accos_op::AssocOp,
};
use litec_error::{Diagnostic, DiagnosticBuilder, error};
use litec_session::Session;
use litec_span::{FileId, Location, Span, Spanned, respan};

#[derive(Debug, Clone, Copy)]
struct Restrictions {
    no_struct_literal: bool,
}

pub struct Parser<'src> {
    session: &'src Session,
    file_id: FileId,

    lexer: Lexer<'src>,
    current_token: Token,
    last_token_end_span: Span,

    generic_nesting: u8,
    pending_token: Option<Token>,

    restrictions: Restrictions,
    skip_infix: bool,
}

pub struct ParserSnapshot {
    lexer_snaphot: LexerSnapshot,
    last_token_end: Span,
    current_token: Token,
    generic_nesting: u8,
    pending_token: Option<Token>,
    diagnostics_len: usize,
}

impl<'src> Parser<'src> {
    pub fn new(session: &'src Session, file_id: FileId) -> Self {
        let mut lexer = Lexer::new(session, file_id);
        let current_token = loop {
            let token = lexer.advance_token();

            match token {
                Ok(token) => break token,
                Err(err) => {
                    session.report(err);
                }
            }
        };

        Self {
            session,
            file_id,
            lexer,
            current_token: current_token,
            last_token_end_span: Span::default(),
            generic_nesting: 0,
            pending_token: None,
            restrictions: Restrictions {
                no_struct_literal: false,
            },
            skip_infix: false,
        }
    }

    fn advance(&mut self) {
        self.last_token_end_span = self.current_token.span;
        if let Some(pending_token) = self.pending_token.take() {
            self.current_token = pending_token;
            return;
        }
        loop {
            match self.lexer.advance_token() {
                Ok(token) => {
                    if self.generic_nesting > 0 && token.kind == TokenKind::Shr {
                        // 将 >> 拆分为两个独立的 >
                        let (first_span, second_span) = self.split_gtgt_span(&token.span);

                        let first_gt = Token {
                            kind: TokenKind::Gt,
                            text: ">".into(),
                            span: first_span,
                        };
                        let second_gt = Token {
                            kind: TokenKind::Gt,
                            text: ">".into(),
                            span: second_span,
                        };

                        // 注意顺序：先 push 第二个 >，然后设置第一个 > 为当前
                        self.pending_token = Some(second_gt);
                        self.current_token = first_gt;
                    } else {
                        self.current_token = token;
                    }
                    return;
                }
                Err(err) => {
                    self.error(err);
                }
            }
        }
    }

    fn split_gtgt_span(&self, span: &Span) -> (Span, Span) {
        let mid = Location {
            line: span.start.line,
            column: span.start.column + 1,
            offset: span.start.offset + 1,
        };
        let first_span = Span::new(span.start, mid, span.file);
        let second_span = Span::new(mid, span.end, span.file);
        (first_span, second_span)
    }

    #[inline]
    fn expect(&mut self, kind: TokenKind, err: Diagnostic) -> Option<Token> {
        if self.current_token.kind == kind {
            let token = self.current_token.clone();
            self.advance();
            Some(token)
        } else {
            self.session.report(err);
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
            if self.is_item_start() {
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
            TokenKind::Fn,
            TokenKind::Eof,
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

    fn is_item_start(&self) -> bool {
        matches!(
            self.current_token.kind,
            TokenKind::Fn
                | TokenKind::Struct
                | TokenKind::Use
                | TokenKind::Pub
                | TokenKind::Priv
                | TokenKind::Hash
        )
    }

    fn snapshot(&self) -> ParserSnapshot {
        ParserSnapshot {
            lexer_snaphot: self.lexer.snapshot(),
            last_token_end: self.last_token_end_span,
            current_token: self.current_token.clone(),
            generic_nesting: self.generic_nesting,
            pending_token: self.pending_token.clone(),
            diagnostics_len: self.session.diagnostics().len(),
        }
    }

    fn restore(&mut self, snapshot: ParserSnapshot) {
        self.lexer.restore(snapshot.lexer_snaphot);
        self.last_token_end_span = snapshot.last_token_end;
        self.current_token = snapshot.current_token;
        self.generic_nesting = snapshot.generic_nesting;
        self.pending_token = snapshot.pending_token;
        self.session
            .diagnostics
            .borrow_mut()
            .truncate(snapshot.diagnostics_len);
    }

    fn try_parse<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let snapshot = self.snapshot();
        let result = f(self);
        if result.is_none() {
            self.restore(snapshot);
        }
        result
    }

    pub fn parse(mut self) -> Crate {
        let mut items = Vec::new();

        while self.current_token.kind != TokenKind::Eof {
            match self.parse_item() {
                Some(stmt) => items.push(stmt),
                None => {
                    self.sync_to_item();
                }
            }
        }

        Crate {
            node_id: DUMMY_NODE_ID,
            items,
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.current_token.kind == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    fn look_ahead<R>(&mut self, dist: usize, looker: impl FnOnce(&Token) -> R) -> R {
        let snapshot = self.snapshot();
        for _ in 0..dist {
            self.advance();
        }
        let result = looker(&self.current_token);
        self.restore(snapshot);
        result
    }

    fn peek_assoc_op(&mut self) -> Option<Spanned<AssocOp>> {
        match AssocOp::from_token(&self.current_token) {
            Some(op) => Some(respan(self.current_token.span, op)),
            None => {
                return None;
            }
        }
    }

    fn error(&mut self, error: Diagnostic) {
        self.session.report(error);
    }

    #[inline]
    fn parse_ident(&mut self) -> Option<Ident> {
        let token = self.expect(
            TokenKind::Ident,
            error("期待标识符")
                .with_span(self.current_token.span)
                .build(),
        )?;
        Some(Ident {
            text: token.text.into(),
            span: token.span,
        })
    }

    #[inline]
    fn parse_str_lit(&mut self) -> Option<StrLit> {
        let token = self.expect(
            TokenKind::Literal {
                kind: LiteralKind::Str,
                suffix: None,
            },
            error("期待字符串字面量")
                .with_span(self.current_token.span)
                .build(),
        )?;

        Some(StrLit {
            text: token.text,
            span: token.span,
        })
    }

    #[inline]
    fn expect_semi_error(&self) -> DiagnosticBuilder {
        error("期待 `;`").with_span(self.current_token.span)
    }

    #[inline]
    fn check(&self, kind: TokenKind) -> bool {
        self.current_token.kind == kind
    }
}

pub fn parse(session: &Session, file_id: FileId) -> Crate {
    let parser = Parser::new(session, file_id);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use core::panic;
    use std::path::Path;

    use super::*;
    use litec_ast::{
        ast::{
            BinOpKind, Expr, ExprKind, Fn, FnRetTy, ItemKind, Mutability, RangeLimits, StmtKind,
            TyKind, UnOp,
        },
        token::LiteralKind,
    };
    use litec_span::{SourceMap, intern_global};

    /// 辅助函数：将源代码解析为 AST 和诊断
    fn parse_str(src: &str) -> (Crate, Vec<Diagnostic>) {
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "test.lt".to_string(),
            src.to_string(),
            &Path::new("test.lt"),
        );
        let session = Session::new(source_map);
        let krate = parse(&session, file_id);
        for diagnostic in session.diagnostics.borrow().iter() {
            println!(
                "{}",
                diagnostic.clone().render(&session.source_map.borrow())
            );
        }
        (krate, session.diagnostics.take())
    }

    // 辅助：从 AST 中获取第一个 item 的函数签名（如果有）
    fn get_first_fn(krate: &Crate) -> Option<&Fn> {
        krate.items.first().and_then(|item| match &item.kind {
            ItemKind::Fn(f) => Some(f),
            _ => None,
        })
    }

    // 辅助：从函数体中获取第一个表达式（假设函数体只有一个块，且块尾表达式为所需）
    fn get_fn_body_expr(f: &Fn) -> Option<&Expr> {
        f.body.as_ref().and_then(|block| block.tail.as_deref())
    }

    // ========== 基本项解析 ==========

    #[test]
    fn test_empty_crate() {
        let (krate, diags) = parse_str("");
        assert!(diags.is_empty());
        assert!(krate.items.is_empty());
    }

    #[test]
    fn test_simple_function() {
        let src = "fn foo() {}";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(krate.items.len(), 1);
        let f = get_first_fn(&krate).expect("不是函数");
        assert_eq!(f.sig.name.text, intern_global("foo"));
        assert!(f.sig.params.is_empty());
        assert!(f.body.is_some());
    }

    #[test]
    fn test_function_with_params() {
        let src = "fn foo(x: i32, y: i32) -> i32 { x + y }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        assert_eq!(f.sig.params.len(), 2);
        assert_eq!(f.sig.params[0].name.text, intern_global("x"));
        assert_eq!(f.sig.params[1].name.text, intern_global("y"));
        match &f.sig.return_type {
            FnRetTy::Ty(ty) => match &ty.kind {
                TyKind::Path { path } => {
                    assert_eq!(path.segments.len(), 1);
                    assert_eq!(path.segments[0].name.text, intern_global("i32"));
                }
                _ => panic!("期望路径类型"),
            },
            _ => panic!("期望显式返回类型"),
        }
    }

    #[test]
    fn test_function_with_generics() {
        let src = "fn foo<T>(x: T) -> T { x }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        assert_eq!(f.sig.generics.params.len(), 1);
        assert_eq!(f.sig.generics.params[0].name.text, intern_global("T"));
    }

    #[test]
    fn test_struct() {
        let src = "struct Foo { x: i32, y: i32 }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(krate.items.len(), 1);
        let item = &krate.items[0];
        match &item.kind {
            ItemKind::Struct(ident, generics, fields) => {
                assert_eq!(ident.text, intern_global("Foo"));
                assert!(generics.params.is_empty());
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name.text, intern_global("x"));
                assert_eq!(fields[1].name.text, intern_global("y"));
            }
            _ => panic!("期望结构体"),
        }
    }

    #[test]
    fn test_use_statement() {
        let src = "use foo::bar;";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(krate.items.len(), 1);
        let item = &krate.items[0];
        match &item.kind {
            ItemKind::Use(tree) => {
                assert_eq!(tree.prefix.segments.len(), 2);
                assert_eq!(tree.prefix.segments[0].name.text, intern_global("foo"));
                assert_eq!(tree.prefix.segments[1].name.text, intern_global("bar"));
            }
            _ => panic!("期望 use"),
        }
    }

    // ========== 表达式解析 ==========

    #[test]
    fn test_literal_expr() {
        let src = "fn f() { 42 }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Literal(lit) => {
                assert_eq!(lit.kind, LiteralKind::Integer);
                assert_eq!(lit.value, intern_global("42"))
            }
            _ => panic!("期望字面量"),
        }
    }

    #[test]
    fn test_binary_expr() {
        let src = "fn f() { 1 + 2 }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Binary(lhs, op, rhs) => {
                assert_eq!(op.value, BinOpKind::Add);
                match lhs.kind {
                    ExprKind::Literal(lit) => {
                        assert_eq!(lit.kind, LiteralKind::Integer);
                        assert_eq!(lit.value, intern_global("1"));
                    }
                    _ => panic!("左侧应为字面量"),
                }
                match rhs.kind {
                    ExprKind::Literal(lit) => {
                        assert_eq!(lit.kind, LiteralKind::Integer);
                        assert_eq!(lit.value, intern_global("2"));
                    }
                    _ => panic!("右侧应为字面量"),
                }
            }
            _ => panic!("期望二元表达式"),
        }
    }

    #[test]
    fn test_precedence() {
        let src = "fn f() { 1 + 2 * 3 }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        // 应该解析为 1 + (2 * 3)
        match &expr.kind {
            ExprKind::Binary(lhs, op, rhs) => {
                assert_eq!(op.value, BinOpKind::Add);
                // lhs 应该是 1
                match &lhs.kind {
                    ExprKind::Literal(lit) => assert_eq!(lit.kind, LiteralKind::Integer),
                    _ => panic!("左侧应为字面量"),
                }
                // rhs 应该是乘法
                match &rhs.kind {
                    ExprKind::Binary(lhs2, op2, rhs2) => {
                        assert_eq!(op2.value, BinOpKind::Mul);
                        match lhs2.kind {
                            ExprKind::Literal(lit) => {
                                assert_eq!(lit.kind, LiteralKind::Integer);
                                assert_eq!(lit.value, intern_global("2"));
                            }
                            _ => panic!("左侧应为字面量"),
                        }
                        match rhs2.kind {
                            ExprKind::Literal(lit) => {
                                assert_eq!(lit.kind, LiteralKind::Integer);
                                assert_eq!(lit.value, intern_global("3"));
                            }
                            _ => panic!("右侧应为字面量"),
                        }
                    }
                    _ => panic!("右侧应为乘法"),
                }
            }
            _ => panic!("期望加法在最外层"),
        }
    }

    #[test]
    fn test_unary_expr() {
        let src = "fn f() { -x }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Unary(op, operand) => {
                assert_eq!(*op, UnOp::Neg);
                match &operand.kind {
                    ExprKind::Path(path) => {
                        assert_eq!(path.segments[0].name.text, intern_global("x"))
                    }
                    _ => panic!("操作数应为标识符"),
                }
            }
            _ => panic!("期望一元表达式"),
        }
    }

    #[test]
    fn test_if_expr() {
        let src = "fn f() { if x > 0 { 1 } else { 2 } }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::If(cond, then, else_) => {
                // 条件应为 x > 0
                match &cond.kind {
                    ExprKind::Binary(lhs, op, rhs) => {
                        assert_eq!(op.value, BinOpKind::Gt);
                        match &lhs.kind {
                            ExprKind::Path(path) => {
                                assert_eq!(path.segments[0].name.text, intern_global("x"))
                            }
                            _ => panic!("左侧应为标识符"),
                        }
                        match rhs.kind {
                            ExprKind::Literal(lit) => {
                                assert_eq!(lit.kind, LiteralKind::Integer);
                                assert_eq!(lit.value, intern_global("0"));
                            }
                            _ => panic!("右侧应为字面量"),
                        }
                    }
                    _ => panic!("条件应为比较"),
                }
                // then 分支块应包含字面量 1 作为尾表达式
                if let Some(tail) = &then.tail {
                    match &tail.kind {
                        ExprKind::Literal(lit) => assert_eq!(lit.kind, LiteralKind::Integer),
                        _ => panic!("then 分支尾应为字面量"),
                    }
                }
                // else 分支
                if let Some(else_expr) = else_ {
                    match &else_expr.kind {
                        ExprKind::Block(block) => {
                            if let Some(tail) = &block.tail {
                                match &tail.kind {
                                    ExprKind::Literal(lit) => {
                                        assert_eq!(lit.kind, LiteralKind::Integer)
                                    }
                                    _ => panic!("else 分支尾应为字面量"),
                                }
                            }
                        }
                        _ => panic!("else 应为块"),
                    }
                }
            }
            _ => panic!("期望 if 表达式"),
        }
    }

    #[test]
    fn test_while_loop() {
        let src = "fn f() { while x < 10 { x += 1; } }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::While(cond, body) => {
                match &cond.kind {
                    ExprKind::Binary(_, op, _) => assert_eq!(op.value, BinOpKind::Lt),
                    _ => panic!("条件应为比较"),
                }
                // 检查循环体是否包含语句
                assert!(!body.stmts.is_empty());
            }
            _ => panic!("期望 while 循环"),
        }
    }

    #[test]
    fn test_for_loop() {
        let src = "fn f() { for i in 0..10 { } }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::For { variable, iter, .. } => {
                assert_eq!(variable.text, intern_global("i"));
                match &iter.kind {
                    ExprKind::Range(start, end, RangeLimits::HalfOpen) => {
                        match start.kind {
                            ExprKind::Literal(lit) => {
                                assert_eq!(lit.kind, LiteralKind::Integer);
                                assert_eq!(lit.value, intern_global("0"));
                                assert_eq!(lit.suffix, None);
                            }
                            _ => panic!("期待字面量"),
                        }
                        match end.kind {
                            ExprKind::Literal(lit) => {
                                assert_eq!(lit.kind, LiteralKind::Integer);
                                assert_eq!(lit.value, intern_global("10"));
                                assert_eq!(lit.suffix, None);
                            }
                            _ => panic!("期待字面量"),
                        }
                    }
                    _ => panic!("期望范围表达式"),
                }
            }
            _ => panic!("期望 for 循环"),
        }
    }

    #[test]
    fn test_block_expr() {
        let src = "fn f() { { let x = 1; x } }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let outer_expr = get_fn_body_expr(f).expect("没有外部表达式");
        match &outer_expr.kind {
            ExprKind::Block(block) => {
                assert_eq!(block.stmts.len(), 1);
                assert!(block.tail.is_some());
                match &block.tail.as_ref().unwrap().kind {
                    ExprKind::Path(path) => {
                        assert_eq!(path.segments[0].name.text, intern_global("x"))
                    }
                    _ => panic!("尾表达式应为标识符"),
                }
            }
            _ => panic!("期望块表达式"),
        }
    }

    #[test]
    fn test_path_access() {
        let src = "fn f() { foo::bar }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Path(path) => {
                assert_eq!(path.segments.len(), 2);
                assert_eq!(path.segments[0].name.text, intern_global("foo"));
                assert_eq!(path.segments[1].name.text, intern_global("bar"));
            }
            _ => panic!("期望路径访问"),
        }
    }

    #[test]
    fn test_field_access() {
        let src = "fn f() { x.y }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Field(base, name) => {
                match &base.kind {
                    ExprKind::Path(path) => {
                        assert_eq!(path.segments[0].name.text, intern_global("x"))
                    }
                    _ => panic!("基表达式应为标识符"),
                }
                assert_eq!(name.text, intern_global("y"));
            }
            _ => panic!("期望字段访问"),
        }
    }

    #[test]
    fn test_call_expr() {
        let src = "fn f() { foo(1, 2) }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Call(callee, args) => {
                match &callee.kind {
                    ExprKind::Path(path) => {
                        assert_eq!(path.segments[0].name.text, intern_global("foo"))
                    }
                    _ => panic!("callee 应为标识符"),
                }
                assert_eq!(args.len(), 2);
            }
            _ => panic!("期望调用"),
        }
    }

    #[test]
    fn test_index_expr() {
        let src = "fn f() { arr[0] }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Index(base, index) => {
                match &base.kind {
                    ExprKind::Path(path) => {
                        assert_eq!(path.segments[0].name.text, intern_global("arr"))
                    }
                    _ => panic!("基表达式应为标识符"),
                }
                // 索引应为字面量
                match &index.kind {
                    ExprKind::Literal(lit) => assert_eq!(lit.kind, LiteralKind::Integer),
                    _ => panic!("索引应为整数"),
                }
            }
            _ => panic!("期望索引表达式"),
        }
    }

    #[test]
    fn test_cast_expr() {
        let src = "fn f() { x as i32 }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Cast(expr, ty) => {
                match &expr.kind {
                    ExprKind::Path(path) => {
                        assert_eq!(path.segments[0].name.text, intern_global("x"))
                    }
                    _ => panic!("左操作数应为标识符"),
                }
                match &ty.kind {
                    TyKind::Path { path } => {
                        assert_eq!(path.segments[0].name.text, intern_global("i32"));
                    }
                    _ => panic!("目标类型应为路径"),
                }
            }
            _ => panic!("期望类型转换"),
        }
    }

    #[test]
    fn test_range_expr() {
        let src = "fn f() { 0..10 }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let expr = get_fn_body_expr(f).expect("没有表达式");
        match &expr.kind {
            ExprKind::Range(_, _, RangeLimits::HalfOpen) => {}
            _ => panic!("期望范围表达式"),
        }
    }

    #[test]
    fn test_let_statement() {
        let src = "fn f() { let x = 42; }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let body = f.body.as_ref().expect("无函数体");
        assert_eq!(body.stmts.len(), 1);
        let stmt = &body.stmts[0];
        match &stmt.kind {
            StmtKind::Let(mutable, name, ty, value) => {
                assert_eq!(*mutable, Mutability::Immutable);
                assert_eq!(name.text, intern_global("x"));
                assert!(ty.is_none());
                assert!(value.is_some());
            }
            _ => panic!("期望 let 语句"),
        }
    }

    #[test]
    fn test_return_statement() {
        let src = "fn f() { return 42; }";
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        let body = f.body.as_ref().expect("无函数体");
        assert_eq!(body.stmts.len(), 1);
        let stmt = &body.stmts[0];
        match &stmt.kind {
            StmtKind::Return(value) => {
                assert!(value.is_some());
            }
            _ => panic!("期望 return 语句"),
        }
    }

    // ========== 错误恢复测试 ==========

    #[test]
    fn test_missing_semi() {
        let src = "fn f() { let x = 5 }"; // 缺少分号
        let (krate, diags) = parse_str(src);
        assert!(!diags.is_empty());
        // 虽然错误，但解析器应继续并返回 AST
        let f = get_first_fn(&krate).expect("不是函数");
        let body = f.body.as_ref().expect("无函数体");
    }

    #[test]
    fn test_unclosed_delimiter() {
        let src = "fn f() { let x = 5; ";
        let (_, diags) = parse_str(src);
        assert!(!diags.is_empty());
        // 应该有未闭合大括号的错误
    }

    #[test]
    fn test_invalid_expression() {
        let src = "fn f() { 1 + }";
        let (_, diags) = parse_str(src);
        assert!(!diags.is_empty());
    }

    #[test]
    fn test_complex_function() {
        let src = r#"
            #[inline = "always"]
            fn factorial(n: i32) -> i32 {
                if n <= 1 {
                    1
                } else {
                    n * factorial(n - 1)
                }
            }
        "#;
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
        let f = get_first_fn(&krate).expect("不是函数");
        assert_eq!(f.sig.name.text, intern_global("factorial"));
        assert_eq!(f.sig.params.len(), 1);
        // 可以进一步检查函数体
    }

    #[test]
    fn test_impl_struct() {
        let src = r#"
            struct Foo {}
            trait Bar {
                type A;
            }

            impl Bar for Foo {
                type A = Foo;
                pub fn new() -> Foo {
                    Foo {}
                }
            }
        "#;
        let (krate, diags) = parse_str(src);
        assert!(diags.is_empty());
    }
}
