use litec_ast::{
    ast::{
        Bounds, DUMMY_NODE_ID, Generic, GenericArg, GenericArgs, Generics, Mutability, Ty, TyKind,
    },
    token::TokenKind,
};
use litec_error::{PResult, error};

use crate::parser::{Parser, path::PathStyle};

impl<'a> Parser<'a> {
    pub(super) fn parse_ty(&mut self) -> PResult<Ty> {
        let start_span = self.current_token.span;

        let kind = self.parse_ty_kind()?;
        let span = start_span.extend_to(self.last_token_end_span);
        Ok(Ty {
            node_id: DUMMY_NODE_ID,
            kind,
            span,
        })
    }

    fn parse_ty_kind(&mut self) -> PResult<TyKind> {
        match self.current_token.kind {
            TokenKind::Ident => {
                let path = self.parse_path(PathStyle::Type)?;

                Ok(TyKind::Path { path })
            }
            TokenKind::OpenParen => {
                // 处理元组类型：(T, U, V)
                self.parse_tuple_type()
            }
            TokenKind::Mul => {
                // 处理指针类型：*const T, *mut T
                self.parse_pointer_type()
            }
            TokenKind::SelfUpper => {
                self.advance();
                Ok(TyKind::SelfTy)
            }
            _ => {
                return Err(self.error(error("期待类型").with_span(self.current_token.span)));
            }
        }
    }

    pub(super) fn parse_generic_args(&mut self) -> PResult<GenericArgs> {
        let lt_span = self
            .expect(
                TokenKind::Lt,
                error("期待 `<`").with_span(self.current_token.span),
            )?
            .span;

        let mut args = Vec::new();

        // 解析逗号分隔的类型参数
        loop {
            // 如果已经到达结束符，跳出循环
            if self.current_token.kind == TokenKind::Gt {
                break;
            }

            args.push(GenericArg::Type(self.parse_ty()?));

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
            return Err(self.error(
                error("期待 `>` 来结束泛型参数")
                    .with_span(self.current_token.span)
                    .with_label(lt_span, "对应的 `<` 在这里"),
            ));
        };

        Ok(GenericArgs { args, span: span })
    }

    fn parse_tuple_type(&mut self) -> PResult<TyKind> {
        self.advance(); // 消耗 '('

        // 检查空元组：()
        if self.current_token.kind == TokenKind::CloseParen {
            self.advance();
            return Ok(TyKind::Unit);
        }

        let mut elements = Vec::new();

        // 解析元组元素
        loop {
            elements.push(self.parse_ty()?);
            if self.eat(TokenKind::Comma) {
                // 继续解析下一个元素
                continue;
            } else {
                break;
            }
        }

        self.expect(
            TokenKind::CloseParen,
            error("期待 `)`").with_span(self.current_token.span),
        )?;

        Ok(TyKind::Tuple { elems: elements })
    }

    fn parse_pointer_type(&mut self) -> PResult<TyKind> {
        // 检查指针类型：*const, *mut
        let mutability = if self.eat(TokenKind::Mul) {
            if self.eat(TokenKind::Mut) {
                Mutability::Mutable
            } else {
                Mutability::Immutable
            }
        } else {
            return Err(self.error(error("期待 `*`").with_span(self.current_token.span)));
        };

        // 解析指向的类型
        let target_type = self.parse_ty()?;

        Ok(TyKind::Ptr {
            mutability,
            ty: Box::new(target_type),
        })
    }

    pub(super) fn parse_generics(&mut self) -> PResult<Generics> {
        let span = self.current_token.span;
        if !self.eat(TokenKind::Lt) {
            return Ok(Generics::empty());
        }
        self.generic_nesting += 1;
        let mut generic_params = Vec::new();

        while self.current_token.kind != TokenKind::Gt {
            let generic = self.parse_generic()?;
            generic_params.push(generic);
            self.eat(TokenKind::Comma);
        }
        self.generic_nesting -= 1;

        let end_span = self
            .expect(
                TokenKind::Gt,
                error("期待 `>`").with_span(self.current_token.span),
            )?
            .span;
        Ok(Generics {
            node_id: DUMMY_NODE_ID,
            params: generic_params,
            span: span.extend_to(end_span),
        })
    }

    fn parse_generic(&mut self) -> PResult<Generic> {
        let span = self.current_token.span;
        let name = self.parse_ident()?;
        let bounds = self.parse_bounds()?;
        let span = span.extend_to(name.span);
        Ok(Generic {
            node_id: DUMMY_NODE_ID,
            name,
            bounds,
            span,
        })
    }

    fn parse_bounds(&mut self) -> PResult<Option<Bounds>> {
        let span = self.current_token.span;
        // 检查是否以冒号开头
        if !self.eat(TokenKind::Colon) {
            return Ok(None);
        }

        let mut bounds = Vec::new();
        loop {
            // 解析一个 trait 路径
            let path = self.parse_path(PathStyle::Type)?;
            bounds.push(path);

            // 检查是否有 + 继续
            if self.eat(TokenKind::Plus) {
                continue;
            } else {
                break;
            }
        }

        let span = span.extend_to(self.last_token_end_span); // 或当前位置

        Ok(Some(Bounds {
            node_id: DUMMY_NODE_ID,
            bounds,
            span: span,
        }))
    }
}
