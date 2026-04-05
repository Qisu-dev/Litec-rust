use litec_ast::{
    ast::{DUMMY_NODE_ID, GenericArgs, Ident, Path, PathSegment},
    token::TokenKind,
};
use litec_error::error;

use crate::parser::Parser;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathStyle {
    /// 例如 `a::<b>`
    Expr,
    /// 例如 `a::<b>` 或 `a<b>`
    Type,
    /// 例如 `a::b`
    Mod,
    /// 例如 `lang`
    Attr,
}

impl<'a> Parser<'a> {
    pub fn parse_path(&mut self, style: PathStyle) -> Option<Path> {
        let span = self.current_token.span;
        let mut segments = Vec::new();

        // 解析第一段
        segments.push(self.parse_path_segment(style)?);

        // 后续的 ::segment
        loop {
            if !self.eat(TokenKind::PathAccess) {
                break;
            }

            if style == PathStyle::Attr && self.check(TokenKind::Assign) {
                break;
            }

            if style == PathStyle::Mod
                && (self.check(TokenKind::OpenBrace) || self.check(TokenKind::Mul))
            {
                break;
            }

            segments.push(self.parse_path_segment(style)?);
        }

        // Mod 风格检查：不允许泛型
        if style == PathStyle::Mod {
            for segment in &segments {
                if segment.generic_args.is_some() {
                    self.error(
                        error("模块路径中不允许泛型参数")
                            .with_span(segment.span)
                            .with_help("移除 `<...>` 或使用类型路径")
                            .build(),
                    );
                }
            }
        }

        Some(Path {
            node_id: DUMMY_NODE_ID,
            segments,
            span: span.extend_to(self.last_token_end_span),
        })
    }

    /// 解析单个路径段：ident 或 ident::<args> 或 ident<args>
    fn parse_path_segment(&mut self, style: PathStyle) -> Option<PathSegment> {
        let name = match self.current_token.kind {
            TokenKind::Ident => Ident {
                text: self.current_token.text,
                span: self.current_token.span,
            },
            TokenKind::Super | TokenKind::Crate | TokenKind::SelfLower => {
                if style == PathStyle::Mod {
                    Ident {
                        text: self.current_token.text,
                        span: self.current_token.span,
                    }
                } else {
                    self.error(
                        error("在非路径中禁止使用 `super` `crate` `self`")
                            .with_span(self.current_token.span)
                            .build(),
                    );
                    return None;
                }
            }
            _ => return None,
        };
        self.advance();

        // 根据风格决定是否解析泛型
        let generic_args = self.parse_generic_args_if_allowed(style)?;

        let span = generic_args
            .as_ref()
            .map(|args| name.span.extend_to(args.span))
            .unwrap_or(name.span);

        Some(PathSegment {
            node_id: DUMMY_NODE_ID,
            name,
            span,
            generic_args,
        })
    }

    fn parse_generic_args_if_allowed(&mut self, style: PathStyle) -> Option<Option<GenericArgs>> {
        match style {
            PathStyle::Type => {
                // Type 风格：< 总是泛型
                if self.current_token.kind == TokenKind::Lt {
                    self.generic_nesting += 1;
                    let args = self.parse_generic_args()?;
                    self.generic_nesting -= 1;
                    Some(Some(args))
                } else {
                    Some(None)
                }
            }

            PathStyle::Expr => {
                if self.current_token.kind == TokenKind::PathAccess {
                    // 向前看一个 token，判断是否是 <
                    if self.look_ahead(1, |tok| tok.kind == TokenKind::Lt) {
                        // 确实是 turbofish，消费 :: 并解析泛型参数
                        self.advance(); // 消费 ::
                        self.generic_nesting += 1;
                        let args = self.parse_generic_args()?;
                        self.generic_nesting -= 1;
                        Some(Some(args))
                    } else {
                        // 普通路径分隔符，不解析泛型参数
                        Some(None)
                    }
                } else if self.current_token.kind == TokenKind::Lt {
                    // 没有 ::，< 视为比较运算符，不解析泛型
                    Some(None)
                } else {
                    Some(None)
                }
            }

            PathStyle::Mod | PathStyle::Attr => {
                // Mod, Attr 风格：不允许泛型，但继续解析（后续统一报错）
                if self.current_token.kind == TokenKind::Lt {
                    self.error(
                        error("路径不允许泛型")
                            .with_span(self.current_token.span)
                            .build(),
                    );
                    return None;
                } else {
                    Some(None)
                }
            }
        }
    }
}
