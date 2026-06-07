use crate::parser::{Parser, path::PathStyle};
use litec_ast::{
    ast::{DUMMY_NODE_ID, Lit, Mutability, Pat, PatKind, Path, StructFieldPat},
    token::TokenKind,
};
use litec_error::PResult;
use litec_span::intern_global;

impl<'a> Parser<'a> {
    // 公共入口：解析一个完整的模式（支持 `|`）
    pub fn parse_pat(&mut self) -> PResult<Pat> {
        let start = self.current_token.span;
        let first = self.parse_pat_primary()?;
        if !self.check(TokenKind::BitOr) {
            return Ok(first);
        }
        let mut pats = vec![first];
        while self.eat(TokenKind::BitOr) {
            pats.push(self.parse_pat_primary()?);
        }
        let span = start.extend_to(pats.last().unwrap().span);
        Ok(Pat {
            node_id: DUMMY_NODE_ID,
            kind: PatKind::Or(pats),
            span,
        })
    }

    // 解析不含顶层 `|` 的模式
    fn parse_pat_primary(&mut self) -> PResult<Pat> {
        let span = self.current_token.span;
        let kind = match self.current_token.kind {
            TokenKind::Ident if self.current_token.text == intern_global("_") => {
                self.advance();
                PatKind::Wild
            }
            TokenKind::Mut => {
                self.advance();
                let ident = self.parse_ident()?;
                PatKind::Ident(Mutability::Mutable, ident)
            }
            TokenKind::Ident => {
                // 可能为普通标识符绑定或路径模式
                self.parse_ident_or_path()?
            }
            TokenKind::OpenParen => {
                self.advance();
                let mut pats = Vec::new();
                while self.current_token.kind != TokenKind::CloseParen
                    && self.current_token.kind != TokenKind::Eof
                {
                    pats.push(self.parse_pat()?); // 内部支持 `|`
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::CloseParen, self.span_error("期待 `)`"))?;
                PatKind::Tuple(pats)
            }
            TokenKind::OpenBrace => self.parse_struct_pat()?,
            TokenKind::Literal { kind, suffix } => {
                let text = self.current_token.text;
                self.advance();
                PatKind::Lit(Lit {
                    kind,
                    value: text,
                    suffix,
                })
            }
            _ => return Err(self.error(self.span_error("期待 pattern"))),
        };
        Ok(Pat {
            node_id: DUMMY_NODE_ID,
            kind,
            span: span.extend_to(self.last_token_end_span),
        })
    }

    // 解析标识符开头的模式（可能是绑定或路径），不包含 `mut` 前缀
    fn parse_ident_or_path(&mut self) -> PResult<PatKind> {
        let snapshot = self.snapshot();
        // 尝试解析路径（支持泛型参数）
        let path = match self.parse_path(PathStyle::Type) {
            Ok(p) => p,
            Err(_) => {
                self.restore(snapshot);
                let ident = self.parse_ident()?;
                return Ok(PatKind::Ident(Mutability::Immutable, ident));
            }
        };
        // 根据后续 token 决定模式类型
        if self.check(TokenKind::OpenBrace) {
            self.parse_struct_pat_with_path(path)
        } else if self.check(TokenKind::OpenParen) {
            let inner = self.parse_pat()?;
            Ok(PatKind::Enum(path, Some(Box::new(inner))))
        } else {
            match &path.segments.as_slice() {
                [seg] => Ok(PatKind::Ident(Mutability::Immutable, seg.name)),
                _ => Ok(PatKind::Enum(path, None)),
            }
        }
    }

    // 结构体模式解析（路径已确定）
    fn parse_struct_pat_with_path(&mut self, path: Path) -> PResult<PatKind> {
        self.expect(TokenKind::OpenBrace, self.span_error("期待 `{`"))?;
        let mut fields = Vec::new();
        let mut has_rest = false;
        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            if self.eat(TokenKind::PathAccess) {
                has_rest = true;
                break;
            }
            let field_span = self.current_token.span;
            let field_name = self.parse_ident()?;
            let pat = if self.eat(TokenKind::Colon) {
                self.parse_pat()?
            } else {
                Pat {
                    node_id: DUMMY_NODE_ID,
                    kind: PatKind::Ident(Mutability::Immutable, field_name),
                    span: field_span,
                }
            };
            let span = field_span.extend_to(pat.span);
            fields.push(StructFieldPat {
                name: field_name,
                pat,
                span: field_span.extend_to(span),
                node_id: DUMMY_NODE_ID,
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::CloseBrace, self.span_error("期待 `}`"))?;
        Ok(PatKind::Struct(path, fields, has_rest))
    }

    // 辅助：解析结构体模式（先解析路径）
    fn parse_struct_pat(&mut self) -> PResult<PatKind> {
        let path = self.parse_path(PathStyle::Type)?;
        self.parse_struct_pat_with_path(path)
    }
}
