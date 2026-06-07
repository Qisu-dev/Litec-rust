mod expr;
mod item;
mod pat;
mod path;
mod stmt;
mod ty;

use crate::lexer::{Lexer, LexerSnapshot};
use litec_ast::{
    ast::{Crate, DUMMY_NODE_ID, Ident, StrLit},
    token::{LiteralKind, Token, TokenKind},
    util::accos_op::AssocOp,
};
use litec_error::{Diag, ErrorGuaranteed, PResult, error};
use litec_session::Session;
use litec_span::{FileId, Location, Span, Spanned, respan};

pub struct Parser<'src> {
    session: &'src Session,
    file_id: FileId,

    lexer: Lexer<'src>,
    current_token: Token,
    last_token_end_span: Span,

    generic_nesting: u8,
    pending_token: Option<Token>,

    skip_infix: bool,
}

pub struct ParserSnapshot {
    lexer_snaphot: LexerSnapshot,
    last_token_end: Span,
    current_token: Token,
    generic_nesting: u8,
    pending_token: Option<Token>,
    diag_len: usize,
}

impl<'src> Parser<'src> {
    pub fn new(session: &'src Session, file_id: FileId) -> Self {
        let mut lexer = Lexer::new(session, file_id);
        let current_token = loop {
            let token = lexer.advance_token();

            match token {
                Ok(token) => break token,
                Err(_) => {}
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
                Err(_) => {}
            }
        }
    }

    fn split_gtgt_span(&self, span: &Span) -> (Span, Span) {
        let mid = Location {
            line: span.lo.line,
            column: span.lo.column + 1,
            offset: span.lo.offset + 1,
        };
        let first_span = Span::new(span.lo, mid, span.file);
        let second_span = Span::new(mid, span.hi, span.file);
        (first_span, second_span)
    }

    #[inline]
    fn expect(&mut self, kind: TokenKind, err: Diag) -> PResult<Token> {
        if self.current_token.kind == kind {
            let token = self.current_token.clone();
            self.advance();
            Ok(token)
        } else {
            Err(self.session.report_err(err))
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
            diag_len: self.session.diag_ctxt().diags_count(),
        }
    }

    fn restore(&mut self, snapshot: ParserSnapshot) {
        self.lexer.restore(snapshot.lexer_snaphot);
        self.last_token_end_span = snapshot.last_token_end;
        self.current_token = snapshot.current_token;
        self.generic_nesting = snapshot.generic_nesting;
        self.pending_token = snapshot.pending_token;
        self.session.diag_ctxt().truncate(snapshot.diag_len);
    }

    pub fn parse(mut self) -> Crate {
        let mut items = Vec::new();

        while self.current_token.kind != TokenKind::Eof {
            match self.parse_item() {
                Ok(stmt) => items.push(stmt),
                Err(_) => {
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

    fn error(&mut self, error: Diag) -> ErrorGuaranteed {
        self.session.report_err(error)
    }

    fn span_error(&self, str: impl Into<String>) -> Diag {
        error(str.into()).with_span(self.current_token.span)
    }

    #[inline]
    fn parse_ident(&mut self) -> PResult<Ident> {
        let token = self.expect(
            TokenKind::Ident,
            error("期待标识符").with_span(self.current_token.span),
        )?;
        Ok(Ident {
            text: token.text.into(),
            span: token.span,
        })
    }

    #[inline]
    fn parse_str_lit(&mut self) -> PResult<StrLit> {
        let token = self.expect(
            TokenKind::Literal {
                kind: LiteralKind::Str,
                suffix: None,
            },
            error("期待字符串字面量").with_span(self.current_token.span),
        )?;

        Ok(StrLit {
            text: token.text,
            span: token.span,
        })
    }

    #[inline]
    fn expect_semi_error(&self) -> Diag {
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
    use litec_ast::ast::{ExprKind, Fn, ItemKind, Mutability, PatKind, StmtKind};
    use litec_span::{SourceMap, intern_global};

    /// 辅助函数：将源代码解析为 AST 和诊断
    fn parse_str(src: &str) -> (Crate, Vec<Diag>) {
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "test.lt".to_string(),
            src.to_string(),
            &Path::new("test.lt"),
        );
        let session = Session::new(source_map);
        let krate = parse(&session, file_id);
        session.diag_ctxt().clone().flush();
        (krate, session.diag_ctxt().take_diags())
    }

    // 辅助：从 AST 中获取第一个 item 的函数签名（如果有）
    fn get_first_fn(krate: &Crate) -> Option<&Fn> {
        krate.items.first().and_then(|item| match &item.kind {
            ItemKind::Fn(f) => Some(f),
            _ => None,
        })
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
            StmtKind::Let(pat, ty, value) => {
                let name = match &pat.kind {
                    PatKind::Ident(binding_mode, ident) => {
                        match binding_mode {
                            Mutability::Immutable => {}
                            _ => panic!(),
                        }
                        ident
                    }
                    _ => panic!("{:#?}", pat),
                };
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
            StmtKind::Semi(expr) => {
                if let ExprKind::Return(value) = &expr.kind {
                    assert!(value.is_some());
                } else {
                    panic!()
                }
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
        let _body = f.body.as_ref().expect("无函数体");
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
            
            }

            impl Bar for Foo {
                type A = Foo;
                pub fn new() -> Foo {
                    Foo {}
                }
            }
        "#;
        let (_, diags) = parse_str(src);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_pattern() {
        let src = r#"
            fn main() {
                let (a, b) = (1, 2);
                let mut a = 1;
                let (mut a, mut b) = (1, 2);
            }
        "#;
        let (_, diags) = parse_str(src);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_match() {
        let src = r#"
            struct Foo {
                a: i32,
                b: i32,
            }

            fn main() {
                let foo = Foo {
                    a: 10,
                    b: 10,
                };
                match foo {
                    Foo { a, b } => {
                        print(a);
                        print(b);
                    }
                }
            }
        "#;
        let (_, diags) = parse_str(src);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_enum() {
        let src = r#"
            pub enum Result<T, E> {
                Ok(T),
                Err(E),
            }

            pub enum Foo {
                Bar,
                Baz {
                    result: Result<i32, i64>
                }
            }

            pub fn main() {
                let foo = Foo::Baz {
                    result: Result::Ok(1)
                };

                match foo {
                    Foo::Bar => {},
                    Foo::Baz { result } => {
                        println(result);
                    }
                }
            }
        "#;
        let (_, diags) = parse_str(src);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_defer() {
        let src = r#"
            extern "C" {
                fn malloc(size: usize) -> *mut ();
            }

            pub fn main() {
                let ptr = malloc(sizeof::<T>());
                defer free(ptr);
            }
        "#;
        let (_, diags) = parse_str(src);
        assert!(diags.is_empty());
    }
}
