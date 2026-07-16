use litec_ast::{
    ast::{
        Attr, Block, DUMMY_NODE_ID, Extern, ExternItem, ExternItemKind, Field, Fn, FnRetTy, FnSig,
        Generics, Ident, Impl, ImplItem, ImplItemKind, Inline, Item, ItemKind, Mutability, Param,
        ParamKind, StructKind, TraitItem, TraitItemKind, TypeAlias, UseTree, UseTreeKind, Variant,
        VariantData, VariantField, Visibility,
    },
    token::TokenKind,
};
use litec_error::{PResult, error};
use litec_span::intern_global;

use crate::parser::{Parser, parse, path::PathStyle};

#[derive(Debug, Clone, Copy)]
pub struct FnContext {
    pub allow_variadic: bool,
    pub allow_generics: bool,
}

impl FnContext {
    /// 普通自由函数
    pub const FREE: Self = Self {
        allow_variadic: false,
        allow_generics: true,
    };

    /// extern 块内的函数
    pub const EXTERN_ITEM: Self = Self {
        allow_variadic: true,
        allow_generics: false,
    };

    /// extern "ABI" fn foo() { } 形式的函数
    pub const _EXTERN_FN: Self = Self {
        allow_variadic: false,
        allow_generics: true,
    };

    pub const TRAIT_FN: Self = Self {
        allow_generics: true,
        allow_variadic: false,
    };
}

impl<'a> Parser<'a> {
    fn parse_item_common(&mut self) -> PResult<(Option<Attr>, Visibility)> {
        let attr = self.parse_attribute()?;
        let vis = if self.eat(TokenKind::Pub) {
            Visibility::Public
        } else if self.eat(TokenKind::Priv) {
            Visibility::Inherited
        } else {
            Visibility::Inherited
        };
        Ok((attr, vis))
    }

    pub(super) fn parse_item(&mut self) -> PResult<Item> {
        let span = self.current_token.span;
        let (attr, vis) = self.parse_item_common()?;

        let kind = match self.current_token.kind {
            TokenKind::Fn => self.parse_fn_item()?,
            TokenKind::Struct => self.parse_struct_item()?,
            TokenKind::Use => self.parse_use_item()?,
            TokenKind::Mod => self.parse_module_item()?,
            TokenKind::Extern => self.parse_extern()?,
            TokenKind::Type => ItemKind::TypeAlias(self.parse_type_alias()?),
            TokenKind::Impl => self.parse_impl()?,
            TokenKind::Trait => self.parse_trait()?,
            TokenKind::Enum => self.parse_enum()?,
            _ => {
                return Err(self.error(error("期待一个`item`").with_span(self.current_token.span)));
            }
        };

        let span = span.extend_to(self.last_token_end_span);

        Ok(Item {
            attr,
            node_id: DUMMY_NODE_ID,
            visibility: vis,
            span,
            kind,
        })
    }

    fn parse_enum(&mut self) -> PResult<ItemKind> {
        self.advance();

        let ident = self.parse_ident()?;

        let generics = self.parse_generics()?;

        self.expect(TokenKind::OpenBrace, self.span_error("期待 `{`"))?;

        let mut variants = Vec::new();

        while self.current_token.kind != TokenKind::Eof
            && self.current_token.kind != TokenKind::CloseBrace
        {
            variants.push(self.parse_variant()?);
            self.eat(TokenKind::Comma);
        }

        self.expect(TokenKind::CloseBrace, self.span_error("期待 `}`"))?;

        Ok(ItemKind::Enum(ident, generics, variants))
    }

    fn parse_struct_kind(&mut self) -> PResult<StructKind> {
        match self.current_token.kind {
            TokenKind::Semi => {
                self.advance();

                Ok(StructKind::Unit)
            }
            TokenKind::OpenParen => {
                self.advance();

                let mut tys = Vec::new();

                while self.current_token.kind != TokenKind::Eof
                    && self.current_token.kind != TokenKind::CloseParen
                {
                    tys.push(self.parse_ty()?);
                    self.eat(TokenKind::Comma);
                }

                self.expect(TokenKind::CloseParen, self.span_error("期待 `)`"))?;

                self.expect(TokenKind::Semi, self.span_error("期待 `;`"))?;

                Ok(StructKind::Tuple(tys))
            }
            TokenKind::OpenBrace => {
                self.advance();

                let mut fields = Vec::new();
                while self.current_token.kind != TokenKind::Eof
                    && self.current_token.kind != TokenKind::CloseBrace
                {
                    fields.push(self.parse_field()?);
                    self.eat(TokenKind::Comma);
                }

                self.expect(TokenKind::CloseBrace, self.span_error("期待 `}`"))?;

                Ok(StructKind::Struct(fields))
            }
            _ => Err(self.error(self.span_error("未知内容"))),
        }
    }

    fn parse_variant(&mut self) -> PResult<Variant> {
        let ident = self.parse_ident()?;

        if self.eat(TokenKind::OpenParen) {
            let mut types = Vec::new();

            while self.current_token.kind != TokenKind::Eof
                && self.current_token.kind != TokenKind::CloseParen
            {
                types.push(self.parse_ty()?);
                self.eat(TokenKind::Comma);
            }

            let span = self
                .expect(TokenKind::CloseParen, self.span_error("期待 `)`"))?
                .span;
            let span = ident.span.extend_to(span);
            Ok(Variant {
                ident,
                data: VariantData::Tuple(types),
                span,
                node_id: DUMMY_NODE_ID,
            })
        } else if self.eat(TokenKind::OpenBrace) {
            let mut fields = Vec::new();

            while self.current_token.kind != TokenKind::Eof
                && self.current_token.kind != TokenKind::CloseBrace
            {
                fields.push(self.parse_variant_field()?);
                self.eat(TokenKind::Comma);
            }

            let span = self
                .expect(TokenKind::CloseBrace, self.span_error("期待 `}`"))?
                .span;

            let span = ident.span.extend_to(span);

            Ok(Variant {
                ident,
                data: VariantData::Struct(fields),
                span,
                node_id: DUMMY_NODE_ID,
            })
        } else {
            let span = ident.span;
            Ok(Variant {
                ident,
                data: VariantData::Unit,
                span,
                node_id: DUMMY_NODE_ID,
            })
        }
    }

    fn parse_variant_field(&mut self) -> PResult<VariantField> {
        let ident = self.parse_ident()?;

        self.expect(TokenKind::Colon, self.span_error("期待 `:`"))?;

        let ty = self.parse_ty()?;

        let span = ident.span.extend_to(ty.span);

        Ok(VariantField {
            name: ident,
            ty,
            span,
            node_id: DUMMY_NODE_ID,
        })
    }

    fn parse_extern(&mut self) -> PResult<ItemKind> {
        self.advance(); // 消耗 `extern`

        // 解析 ABI 类型（可选）
        let abi = if matches!(self.current_token.kind, TokenKind::Literal { .. }) {
            let abi_token = self.current_token.clone();
            self.advance();
            Some(Ident {
                text: intern_global(
                    &abi_token.text.to_string()[1..abi_token.text.to_string().len() - 1],
                ),
                span: abi_token.span,
            })
        } else {
            if !self.check(TokenKind::OpenBrace) {
                return Err(self.error(error("期待ABI类型").with_span(self.current_token.span)));
            }
            None
        };

        // 期待开大括号
        self.expect(
            TokenKind::OpenBrace,
            error("期待 `{`").with_span(self.current_token.span),
        )?;

        // 解析外部函数列表
        let mut items = Vec::new();
        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            items.push(self.parse_extern_item()?);
        }

        // 期待闭大括号
        self.expect(
            TokenKind::CloseBrace,
            error("期待 `}`").with_span(self.current_token.span),
        )?;

        Ok(ItemKind::Extern(Extern {
            node_id: DUMMY_NODE_ID,
            abi: abi,
            items: items,
        }))
    }

    fn parse_type_alias(&mut self) -> PResult<TypeAlias> {
        self.advance(); // 度过type
        let ident = self.parse_ident()?;
        let generics = self.parse_generics()?;
        self.expect(
            TokenKind::Assign,
            error("期待 `=`").with_span(self.current_token.span),
        )?;
        let ty = self.parse_ty()?;
        self.expect(TokenKind::Semi, self.expect_semi_error())?;

        Ok(TypeAlias {
            node_id: DUMMY_NODE_ID,
            name: ident,
            generics,
            ty,
        })
    }

    fn parse_trait(&mut self) -> PResult<ItemKind> {
        self.advance(); // 度过 trait

        let name = self.parse_ident()?;

        let generics = self.parse_generics()?;

        self.expect(TokenKind::OpenBrace, self.span_error("期待 `{`"))?;

        let mut items = Vec::new();

        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            items.push(self.parse_trait_item()?);
        }

        self.expect(TokenKind::CloseBrace, self.span_error("期待 `}`"))?;

        Ok(ItemKind::Trait(name, generics, items))
    }

    fn parse_trait_item(&mut self) -> PResult<TraitItem> {
        let span = self.current_token.span;
        let (attr, vis) = self.parse_item_common()?;
        let kind = match self.current_token.kind {
            TokenKind::Fn => {
                let sig = self.parse_fn_sig(FnContext::TRAIT_FN)?;
                self.expect(TokenKind::Semi, self.span_error("期待 `;`"))?;
                TraitItemKind::Fn(sig)
            }
            _ => {
                return Err(self.error(self.span_error("未知 token")));
            }
        };

        let span = span.extend_to(self.last_token_end_span);

        Ok(TraitItem {
            node_id: DUMMY_NODE_ID,
            attr: attr,
            kind: kind,
            span: span,
            visibility: vis,
        })
    }

    fn parse_impl(&mut self) -> PResult<ItemKind> {
        self.advance(); // 度过impl
        let generics = self.parse_generics()?;
        let snapshot = self.snapshot();

        let ty = self.parse_ty()?;
        let (of_trait, self_ty) = if self.eat(TokenKind::For) {
            self.restore(snapshot);
            let of_trait = self.parse_path(PathStyle::Type)?;
            self.eat(TokenKind::For);
            let self_ty = self.parse_ty()?;
            (Some(of_trait), self_ty)
        } else {
            (None, ty)
        };

        self.expect(
            TokenKind::OpenBrace,
            error("期待 `{`").with_span(self.current_token.span),
        )?;

        let mut items = Vec::new();

        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            let impl_item = self.parse_impl_item()?;

            items.push(impl_item);
        }

        self.expect(
            TokenKind::CloseBrace,
            error("期待 `}`").with_span(self.current_token.span),
        )?;

        Ok(ItemKind::Impl(Impl {
            node_id: DUMMY_NODE_ID,
            generics,
            of_trait,
            self_ty: Box::new(self_ty),
            items,
        }))
    }

    fn parse_impl_item(&mut self) -> PResult<ImplItem> {
        let item = self.parse_item()?;
        let impl_kind = match item.kind {
            ItemKind::Fn(fn_) => ImplItemKind::Fn(fn_),
            _ => {
                return Err(self.error(error("impl内部仅能有函数与类型别名").with_span(item.span)));
            }
        };
        Ok(ImplItem {
            node_id: DUMMY_NODE_ID,
            attr: item.attr,
            visibility: item.visibility,
            span: item.span,
            kind: impl_kind,
        })
    }

    fn parse_extern_item(&mut self) -> PResult<ExternItem> {
        let span = self.current_token.span;
        let (attr, vis) = self.parse_item_common()?;

        let kind = match self.current_token.kind {
            TokenKind::Fn => {
                let sig = self.parse_fn_sig(FnContext::EXTERN_ITEM)?;
                self.expect(TokenKind::Semi, self.expect_semi_error())?;
                ExternItemKind::Fn(Fn {
                    node_id: DUMMY_NODE_ID,
                    sig,
                    body: None,
                })
            }
            _ => {
                return Err(
                    self.error(error("期待一个`extern item`").with_span(self.current_token.span))
                );
            }
        };
        let span = span.extend_to(self.last_token_end_span);

        Ok(ExternItem {
            attr,
            node_id: DUMMY_NODE_ID,
            visibility: vis,
            kind: kind,
            span: span,
        })
    }

    fn parse_module_item(&mut self) -> PResult<ItemKind> {
        // 消耗 `mod`
        self.advance();

        let name = self.parse_ident()?;

        let inline = self.parse_module_inline(name)?;

        Ok(ItemKind::Module(name, inline))
    }

    fn parse_module_inline(&mut self, module_name: Ident) -> PResult<Inline> {
        if self.eat(TokenKind::OpenBrace) {
            let mut items = Vec::new();
            loop {
                if self.current_token.kind == TokenKind::CloseBrace {
                    self.advance();
                    break;
                } else {
                    items.push(self.parse_item()?);
                }
            }
            Ok(Inline::Inline(items))
        } else {
            let current_file = self
                .session
                .mut_source_map()
                .file(self.file_id)
                .unwrap()
                .path
                .clone();
            let dir = current_file.parent().ok_or(
                self.error(error("当前文件没有父文件").with_span(self.current_token.span)),
            )?;
            let path = dir.join(format!("{}.lt", module_name.text.to_string()));

            if path.exists() {
                return Err(self.error(
                    error(format!("不存在的文件 `{}`", module_name.text.to_string()))
                        .with_span(module_name.span),
                ));
            }

            let file_id = match self.session.source_map().path_to_id(&path) {
                Some(file_id) => *file_id,
                None => {
                    let context = match std::fs::read_to_string(path.clone()) {
                        Ok(context) => context,
                        Err(err) => {
                            return Err(self.error(
                                error(format!("读取文件错误 `{}`", err.to_string()))
                                    .with_span(module_name.span),
                            ));
                        }
                    };
                    self.session.mut_source_map().add_file(
                        path.file_name().unwrap().to_string_lossy().to_string(),
                        context,
                        &path,
                    )
                }
            };

            let krate = parse(self.session, file_id);

            Ok(Inline::External(krate.items))
        }
    }

    fn parse_use_item(&mut self) -> PResult<ItemKind> {
        self.advance(); // 消耗 `use`

        let use_tree = self.parse_use_tree()?;

        self.expect(TokenKind::Semi, self.expect_semi_error())?;

        Ok(ItemKind::Use(use_tree))
    }

    fn parse_use_tree(&mut self) -> PResult<UseTree> {
        let start_span = self.current_token.span;
        let prefix = self.parse_path(PathStyle::Mod)?;

        let (use_tree_kind, span) = if self.eat(TokenKind::OpenBrace) {
            let mut items = Vec::new();
            while self.current_token.kind != TokenKind::CloseBrace
                && self.current_token.kind != TokenKind::Eof
            {
                items.push(self.parse_use_tree()?);

                // 可选逗号
                if self.current_token.kind == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            let close_brace_span = self
                .expect(
                    TokenKind::CloseBrace,
                    error("期待 `}`").with_span(self.current_token.span),
                )?
                .span
                .extend_to(start_span);
            (
                UseTreeKind::Nested(items, close_brace_span),
                start_span.extend_to(close_brace_span),
            )
        } else if self.eat(TokenKind::As) {
            let ident = self.parse_ident()?;
            let span = start_span.extend_to(ident.span);
            (UseTreeKind::Simple(Some(ident)), span)
        } else if self.current_token.kind == TokenKind::Mul {
            let span = start_span.extend_to(self.current_token.span);
            self.advance();
            (UseTreeKind::Glob, span)
        } else {
            (UseTreeKind::Simple(None), start_span.extend_to(prefix.span))
        };

        Ok(UseTree {
            node_id: DUMMY_NODE_ID,
            prefix,
            kind: use_tree_kind,
            span: span,
        })
    }

    fn parse_struct_item(&mut self) -> PResult<ItemKind> {
        self.advance();

        let name = self.parse_ident()?;

        let generics = self.parse_generics()?;

        let struct_kind = self.parse_struct_kind()?;

        Ok(ItemKind::Struct(name, generics, struct_kind))
    }

    fn parse_field(&mut self) -> PResult<Field> {
        let span = self.current_token.span;
        let vis = match self.current_token.kind {
            TokenKind::Pub => {
                self.advance();
                Visibility::Public
            }
            TokenKind::Priv => {
                self.advance();
                Visibility::Inherited
            }
            _ => Visibility::Inherited,
        };

        let name = self.parse_ident()?;

        self.expect(
            TokenKind::Colon,
            error("期待 `:`").with_span(self.current_token.span),
        )?;

        let ty = self.parse_ty()?;
        let span = span.extend_to(ty.span);

        Ok(Field {
            node_id: DUMMY_NODE_ID,
            name: name,
            ty: ty,
            visibility: vis,
            span: span,
        })
    }

    fn parse_fn_item(&mut self) -> PResult<ItemKind> {
        let sig = self.parse_fn_sig(FnContext::FREE)?;

        let block = self.parse_block()?;

        Ok(ItemKind::Fn(Fn {
            node_id: DUMMY_NODE_ID,
            sig: sig,
            body: Some(block),
        }))
    }

    fn parse_param(&mut self) -> PResult<Param> {
        let start = self.current_token.span;
        if self.eat(TokenKind::Mul) {
            let mutability = if self.eat(TokenKind::Mut) {
                Mutability::Mutable
            } else {
                Mutability::Immutable
            };

            let self_ = self.expect(TokenKind::SelfLower, self.span_error("期待 `self`"))?;
            let span = start.extend_to(self_.span);

            return Ok(Param {
                node_id: DUMMY_NODE_ID,
                kind: ParamKind::SelfPtr(mutability),
                span: span,
            });
        }

        if self.eat(TokenKind::SelfLower) {
            let span = start.extend_to(self.last_token_end_span);
            return Ok(Param {
                node_id: DUMMY_NODE_ID,
                kind: ParamKind::SelfValue(Mutability::Immutable),
                span,
            });
        } else if self.check(TokenKind::Mut)
            && self.look_ahead(1, |token| token.kind == TokenKind::SelfLower)
        {
            self.advance();
            let span = start.extend_to(self.current_token.span);
            self.advance();
            return Ok(Param {
                node_id: DUMMY_NODE_ID,
                kind: ParamKind::SelfValue(Mutability::Mutable),
                span,
            });
        }
        let pat = self.parse_pat()?;
        self.expect(TokenKind::Colon, self.span_error("expected `:`"))?;
        let ty = self.parse_ty()?;
        let span = start.extend_to(ty.span);
        Ok(Param {
            node_id: DUMMY_NODE_ID,
            kind: ParamKind::Normal(pat, ty),
            span: span,
        })
    }

    /// 解析开始的位置:
    /// fn foo() -> i32 { ... }
    /// ^             ^
    ///            结束的地方
    fn parse_fn_sig(&mut self, ctxt: FnContext) -> PResult<FnSig> {
        self.advance(); // 消耗 `fn`
        let name = self.parse_ident()?;

        let generics = if ctxt.allow_generics {
            self.parse_generics()? // 解析可能存在的泛型参数，如果没有则返回空
        } else {
            // 不允许泛型：检查是否出现了 `<`
            if self.current_token.kind == TokenKind::Lt {
                return Err(
                    self.error(error("不允许此处使用泛型").with_span(self.current_token.span))
                );
            }
            Generics::empty()
        };

        self.expect(
            TokenKind::OpenParen,
            error("期待 `(`").with_span(self.current_token.span),
        )?;

        let mut params = Vec::new();

        let is_variadic = loop {
            if self.check(TokenKind::Ellipsis) {
                if ctxt.allow_variadic {
                    self.advance();
                    break true;
                } else {
                    return Err(self.error(
                        error("不允许此处使用可变参数").with_span(self.current_token.span),
                    ));
                }
            }
            if self.check(TokenKind::CloseParen) {
                break false;
            }

            params.push(self.parse_param()?);

            self.eat(TokenKind::Comma);
        };

        self.expect(
            TokenKind::CloseParen,
            error("期待 `)`").with_span(self.current_token.span),
        )?;

        let return_ty = if self.eat(TokenKind::Arrow) {
            FnRetTy::Ty(self.parse_ty()?)
        } else {
            FnRetTy::Default(self.current_token.span)
        };
        Ok(FnSig {
            generics: generics,
            name,
            params,
            return_type: return_ty,
            is_variadic,
        })
    }

    pub(super) fn parse_block(&mut self) -> PResult<Block> {
        // 期待开大括号 - 如果失败直接返回，因为这是块的基本结构
        let open_brace = self
            .expect(
                TokenKind::OpenBrace,
                error("期待 `{`").with_span(self.current_token.span),
            )?
            .span;

        let mut statements = Vec::new();

        // 解析块内容，容忍错误并继续
        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            let stmt_start = self.current_token.span;

            if let Ok(stmt) = self.parse_stmt() {
                statements.push(stmt);
            } else {
                self.sync_to_stmt();
            }

            if self.current_token.span == stmt_start {
                self.advance();
            }
        }

        // 处理闭大括号
        let close_span = self
            .expect(
                TokenKind::CloseBrace,
                error("期待 `}`").with_span(self.current_token.span),
            )?
            .span;

        Ok(Block {
            node_id: DUMMY_NODE_ID,
            stmts: statements,
            span: open_brace.extend_to(close_span),
        })
    }

    fn parse_attribute(&mut self) -> PResult<Option<Attr>> {
        if !self.eat(TokenKind::Hash) {
            return Ok(None);
        }
        let span = self
            .expect(
                TokenKind::OpenBracket,
                error("期待 `[`").with_span(self.current_token.span),
            )?
            .span;
        let path = self.parse_path(PathStyle::Attr)?;
        let arg = if self.eat(TokenKind::Assign) {
            Some(self.parse_str_lit()?)
        } else {
            None
        };

        let span = span.extend_to(
            self.expect(
                TokenKind::CloseBracket,
                error("期待 `]`").with_span(self.current_token.span),
            )?
            .span,
        );
        Ok(Some(Attr {
            path: path,
            arg: arg,
            span,
        }))
    }
}
