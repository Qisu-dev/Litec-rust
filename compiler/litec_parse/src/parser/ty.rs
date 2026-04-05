use litec_ast::{
    ast::{
        DUMMY_NODE_ID, GenericArg, GenericArgs, GenericParam, GenericParams, Mutability, Ty, TyKind,
    },
    token::TokenKind,
};
use litec_error::error;

use crate::parser::{Parser, path::PathStyle};

impl<'a> Parser<'a> {
    pub(super) fn parse_type(&mut self) -> Option<Ty> {
        let start_span = self.current_token.span;

        let kind = self.parse_type_kind()?;
        let span = start_span.extend_to(self.last_token_end_span);
        Some(Ty {
            node_id: DUMMY_NODE_ID,
            kind,
            span,
        })
    }

    fn parse_type_kind(&mut self) -> Option<TyKind> {
        match self.current_token.kind {
            TokenKind::Ident => {
                let path = self.parse_path(PathStyle::Type)?;

                Some(TyKind::Path { path })
            }
            TokenKind::OpenParen => {
                // 处理元组类型：(T, U, V)
                self.parse_tuple_type()
            }
            TokenKind::Mul => {
                // 处理指针类型：*const T, *mut T
                self.parse_pointer_type()
            }
            TokenKind::BitAnd => {
                // 处理引用类型：&T, &mut T
                self.parse_reference_type()
            }
            // 可以添加更多类型解析逻辑
            _ => {
                self.error(error("期待类型").with_span(self.current_token.span).build());
                None
            }
        }
    }

    pub(super) fn parse_generic_args(&mut self) -> Option<GenericArgs> {
        let lt_span = self
            .expect(
                TokenKind::Lt,
                error("期待 `<`").with_span(self.current_token.span).build(),
            )?
            .span;

        let mut args = Vec::new();

        // 解析逗号分隔的类型参数
        loop {
            // 如果已经到达结束符，跳出循环
            if self.current_token.kind == TokenKind::Gt {
                break;
            }

            args.push(GenericArg::Type(self.parse_type()?));

            // 检查是否有更多参数
            if self.eat(TokenKind::Comma) {
                // 允许尾随逗号：<T, U,>
                if self.current_token.kind == TokenKind::Gt {
                    break;
                }
                continue;
            } else {
                break;
            }
        }

        // 处理结束的 >
        let span = if self.current_token.kind == TokenKind::Gt {
            let span = self.current_token.span;
            self.advance();
            span.extend_to(lt_span)
        } else {
            self.error(
                error("期待 `>` 来结束泛型参数")
                    .with_span(self.current_token.span)
                    .with_label(lt_span, "对应的 `<` 在这里")
                    .build(),
            );
            return None;
        };

        Some(GenericArgs { args, span: span })
    }

    fn parse_tuple_type(&mut self) -> Option<TyKind> {
        self.advance(); // 消耗 '('

        // 检查空元组：()
        if self.current_token.kind == TokenKind::CloseParen {
            self.advance();
            return Some(TyKind::Unit);
        }

        let mut elements = Vec::new();

        // 解析元组元素
        loop {
            elements.push(self.parse_type()?);
            if self.eat(TokenKind::Comma) {
                // 继续解析下一个元素
                continue;
            } else {
                break;
            }
        }

        self.expect(
            TokenKind::CloseParen,
            error("期待 `)`").with_span(self.current_token.span).build(),
        )?;

        Some(TyKind::Tuple { elems: elements })
    }

    fn parse_pointer_type(&mut self) -> Option<TyKind> {
        // 检查指针类型：*const, *mut
        let mutability = if self.eat(TokenKind::Mul) {
            if self.eat(TokenKind::Const) {
                Mutability::Immutable
            } else if self.eat(TokenKind::Mut) {
                Mutability::Mutable
            } else {
                Mutability::Immutable
            }
        } else {
            self.error(error("期待 `*`").with_span(self.current_token.span).build());
            return None;
        };

        // 解析指向的类型
        let target_type = self.parse_type()?;

        Some(TyKind::Ptr {
            mutability,
            ty: Box::new(target_type),
        })
    }

    fn parse_reference_type(&mut self) -> Option<TyKind> {
        // 消耗 &
        self.advance();

        // 检查是否可变引用：&mut
        let mutability = if self.eat(TokenKind::Mut) {
            Mutability::Mutable
        } else {
            Mutability::Immutable
        };

        // 解析引用的类型
        let target_type = self.parse_type()?;

        Some(TyKind::Ref {
            mutability,
            ty: Box::new(target_type),
        })
    }

    pub(super) fn parse_generic_params(&mut self) -> Option<GenericParams> {
        let span = self.current_token.span;
        if !self.eat(TokenKind::Lt) {
            return Some(GenericParams::empty());
        }
        self.generic_nesting += 1;
        let mut generic_params = Vec::new();

        while self.current_token.kind != TokenKind::Gt {
            let generic = self.parse_generic_param()?;
            generic_params.push(generic);
            self.eat(TokenKind::Comma);
        }
        self.generic_nesting -= 1;

        let end_span = self
            .expect(
                TokenKind::Gt,
                error("期待 `>`").with_span(self.current_token.span).build(),
            )?
            .span;
        Some(GenericParams {
            node_id: DUMMY_NODE_ID,
            params: generic_params,
            span: span.extend_to(end_span),
        })
    }

    fn parse_generic_param(&mut self) -> Option<GenericParam> {
        let span = self.current_token.span;
        let name = self.parse_ident()?;
        let span = span.extend_to(name.span);
        Some(GenericParam {
            node_id: DUMMY_NODE_ID,
            name,
            span,
        })
    }
}
