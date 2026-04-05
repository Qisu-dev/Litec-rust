use std::path::PathBuf;

use litec_ast::{
    ast::{
        Attr, Block, DUMMY_NODE_ID, Extern, ExternItem, ExternItemKind, Field, Fn, FnRetTy, FnSig,
        Ident, Impl, ImplItem, ImplItemKind, Inline, Item, ItemKind, Param, StmtKind, Ty, TyKind,
        TypeAlias, UseTree, UseTreeKind, Visibility,
    },
    token::TokenKind,
};
use litec_error::error;
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
    pub const EXTERN_FN: Self = Self {
        allow_variadic: false,
        allow_generics: true,
    };

    pub const TRAIT_FN: Self = Self {
        allow_generics: true,
        allow_variadic: false,
    };
}

impl<'a> Parser<'a> {
    fn parse_item_common(&mut self) -> Option<(Option<Attr>, Visibility)> {
        let attr = self.parse_attribute();
        let vis = if self.eat(TokenKind::Pub) {
            Visibility::Public
        } else if self.eat(TokenKind::Priv) {
            Visibility::Inherited
        } else {
            Visibility::Inherited
        };
        Some((attr, vis))
    }

    pub(super) fn parse_item(&mut self) -> Option<Item> {
        let span = self.current_token.span;
        let (attr, vis) = self.parse_item_common()?;

        let kind = match self.current_token.kind {
            TokenKind::Fn => self.parse_fn_item()?,
            TokenKind::Struct => self.parse_struct_item()?,
            TokenKind::Use => self.parse_use_item()?,
            TokenKind::Mod => self.parse_module_item()?,
            TokenKind::Extern => self.parse_extern()?,
            TokenKind::Type => self.parse_type_alias()?,
            TokenKind::Impl => self.parse_impl()?,
            TokenKind::Trait => self.parse_trait()?,
            _ => {
                self.error(
                    error("期待一个`item`")
                        .with_span(self.current_token.span)
                        .build(),
                );
                return None;
            }
        };

        let span = span.extend_to(self.last_token_end_span);

        Some(Item {
            attr,
            node_id: DUMMY_NODE_ID,
            visibility: vis,
            span,
            kind,
        })
    }

    fn parse_extern(&mut self) -> Option<ItemKind> {
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
                self.error(
                    error("期待ABI类型")
                        .with_span(self.current_token.span)
                        .build(),
                );
                return None;
            }
            None
        };

        // 期待开大括号
        self.expect(
            TokenKind::OpenBrace,
            error("期待 `{`").with_span(self.current_token.span).build(),
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
            error("期待 `}`").with_span(self.current_token.span).build(),
        )?;

        Some(ItemKind::Extern(Extern {
            node_id: DUMMY_NODE_ID,
            abi: abi,
            items: items,
        }))
    }

    fn parse_type_alias(&mut self) -> Option<ItemKind> {
        self.advance(); // 度过type
        let ident = self.parse_ident()?;
        let generics = self.parse_generic_params()?;
        self.expect(
            TokenKind::Assign,
            error("期待 `=`").with_span(self.current_token.span).build(),
        )?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Semi, self.expect_semi_error().build())?;

        Some(ItemKind::TypeAlias(TypeAlias {
            node_id: DUMMY_NODE_ID,
            ident,
            generics,
            ty,
        }))
    }

    fn parse_trait(&mut self) -> Option<ItemKind> {
        self.advance(); // 度过 trait

        let name = self.parse_ident()?;

        todo!();
    }

    fn parse_impl(&mut self) -> Option<ItemKind> {
        self.advance(); // 度过impl
        let generics = self.parse_generic_params()?;
        let snapshot = self.snapshot();

        let ty = self.parse_type()?;
        let (of_trait, self_ty) = if self.eat(TokenKind::For) {
            self.restore(snapshot);
            let of_trait = self.parse_path(PathStyle::Type)?;
            self.eat(TokenKind::For);
            let self_ty = self.parse_type()?;
            (Some(of_trait), self_ty)
        } else {
            (None, ty)
        };

        self.expect(
            TokenKind::OpenBrace,
            error("期待 `{`").with_span(self.current_token.span).build(),
        )?;

        let mut items = Vec::new();

        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            let impl_item = self.parse_impl_item()?;

            if of_trait.is_some() {
                items.push(impl_item);
            } else {
                if let ImplItemKind::Type(_) = impl_item.kind {
                    self.error(
                        error("在固定实现内不可以有类型别名")
                            .with_span(impl_item.span)
                            .build(),
                    );
                    return None;
                }
                items.push(impl_item);
            }
        }

        self.expect(
            TokenKind::CloseBrace,
            error("期待 `}`").with_span(self.current_token.span).build(),
        )?;

        Some(ItemKind::Impl(Impl {
            node_id: DUMMY_NODE_ID,
            generics,
            of_trait,
            self_ty: Box::new(self_ty),
            items,
        }))
    }

    fn parse_impl_item(&mut self) -> Option<ImplItem> {
        let item = self.parse_item()?;
        let impl_kind = match item.kind {
            ItemKind::Fn(fn_) => ImplItemKind::Fn(fn_),
            ItemKind::TypeAlias(type_alias) => ImplItemKind::Type(type_alias),
            _ => {
                self.error(
                    error("impl内部仅能有函数与类型别名")
                        .with_span(item.span)
                        .build(),
                );
                return None;
            }
        };
        Some(ImplItem {
            node_id: DUMMY_NODE_ID,
            attr: item.attr,
            visibility: item.visibility,
            span: item.span,
            kind: impl_kind,
        })
    }

    fn parse_extern_item(&mut self) -> Option<ExternItem> {
        let span = self.current_token.span;
        let (attr, vis) = self.parse_item_common()?;

        let kind = match self.current_token.kind {
            TokenKind::Fn => {
                let sig = self.parse_fn_sig(FnContext::EXTERN_ITEM)?;
                self.expect(TokenKind::Semi, self.expect_semi_error().build())?;
                ExternItemKind::Fn(Fn {
                    node_id: DUMMY_NODE_ID,
                    sig,
                    body: None,
                })
            }
            _ => {
                self.error(
                    error("期待一个`extern item`")
                        .with_span(self.current_token.span)
                        .build(),
                );
                return None;
            }
        };
        let span = span.extend_to(self.last_token_end_span);

        Some(ExternItem {
            attr,
            node_id: DUMMY_NODE_ID,
            visibility: vis,
            kind: kind,
            span: span,
        })
    }

    fn parse_module_item(&mut self) -> Option<ItemKind> {
        // 消耗 `mod`
        self.advance();

        let name = self.parse_ident()?;

        let inline = self.parse_module_inline(name)?;

        Some(ItemKind::Module(name, inline))
    }

    fn parse_module_inline(&mut self, module_name: Ident) -> Option<Inline> {
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
            Some(Inline::Inline(items))
        } else {
            let current_file = self
                .session
                .source_map
                .borrow()
                .file(self.file_id)?
                .path
                .clone();
            let dir = current_file.parent()?;
            let path = dir.join(format!("{}.lt", module_name.text.to_string()));

            if path.exists() {
                self.error(
                    error(format!("不存在的文件 `{}`", module_name.text.to_string()))
                        .with_span(module_name.span)
                        .into(),
                );
                return None;
            }

            let file_id = match self.session.borrow_source_map().path_to_id(&path) {
                Some(file_id) => *file_id,
                None => {
                    let context = match std::fs::read_to_string(path.clone()) {
                        Ok(context) => context,
                        Err(err) => {
                            self.error(
                                error(format!("读取文件错误 `{}`", err.to_string()))
                                    .with_span(module_name.span)
                                    .build(),
                            );
                            return None;
                        }
                    };
                    self.session.source_map.borrow_mut().add_file(
                        path.file_name().unwrap().to_string_lossy().to_string(),
                        context,
                        &path,
                    )
                }
            };

            let krate = parse(self.session, file_id);

            Some(Inline::External(krate.items))
        }
    }

    fn parse_use_item(&mut self) -> Option<ItemKind> {
        self.advance(); // 消耗 `use`

        let use_tree = self.parse_use_tree()?;

        self.expect(TokenKind::Semi, self.expect_semi_error().build())?;

        Some(ItemKind::Use(use_tree))
    }

    fn parse_use_tree(&mut self) -> Option<UseTree> {
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
                    error("期待 `}`").with_span(self.current_token.span).build(),
                )?
                .span
                .extend_to(start_span);
            (
                UseTreeKind::Nested(items, close_brace_span),
                start_span.extend_to(close_brace_span),
            )
        } else if self.current_token.kind == TokenKind::PathAccess {
            let ident = self.parse_ident()?;
            let (rename, span) = if self.eat(TokenKind::As) {
                let rename = self.parse_ident()?;
                let span = start_span.extend_to(rename.span);
                (Some(rename), span)
            } else {
                (None, start_span.extend_to(ident.span))
            };
            (UseTreeKind::Simple(rename), span)
        } else if self.current_token.kind == TokenKind::Mul {
            let span = start_span.extend_to(self.current_token.span);
            self.advance();
            (UseTreeKind::Glob, span)
        } else {
            (UseTreeKind::Simple(None), start_span.extend_to(prefix.span))
        };

        Some(UseTree {
            node_id: DUMMY_NODE_ID,
            prefix,
            kind: use_tree_kind,
            span: span,
        })
    }

    fn parse_struct_item(&mut self) -> Option<ItemKind> {
        self.advance();

        let name = self.parse_ident()?;
        let generics = self.parse_generic_params()?;

        self.expect(
            TokenKind::OpenBrace,
            error("期待大括号")
                .with_span(self.current_token.span)
                .build(),
        )?;

        let mut fields = Vec::new();
        let mut index = 0;
        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            fields.push(self.parse_field(index)?);

            self.eat(TokenKind::Comma);
            index += 1;
        }

        self.expect(
            TokenKind::CloseBrace,
            error("期待 `}`").with_span(self.current_token.span).build(),
        )?;

        Some(ItemKind::Struct(name, generics, fields))
    }

    fn parse_field(&mut self, index: u32) -> Option<Field> {
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
            error("期待 `:`").with_span(self.current_token.span).build(),
        )?;

        let ty = self.parse_type()?;
        let span = span.extend_to(ty.span);

        Some(Field {
            node_id: DUMMY_NODE_ID,
            name: name,
            ty: ty,
            visibility: vis,
            index: index,
            span: span,
        })
    }

    fn parse_fn_item(&mut self) -> Option<ItemKind> {
        let sig = self.parse_fn_sig(FnContext::FREE)?;

        let block = self.parse_block()?;

        Some(ItemKind::Fn(Fn {
            node_id: DUMMY_NODE_ID,
            sig: sig,
            body: Some(block),
        }))
    }

    fn parse_param(&mut self) -> Option<Param> {
        let name = self.parse_ident()?;
        self.expect(
            TokenKind::Colon,
            error("期待 `:`").with_span(self.current_token.span).build(),
        )?;
        let ty = self.parse_type()?;
        let span = name.span.extend_to(ty.span);

        Some(Param {
            node_id: DUMMY_NODE_ID,
            name: name,
            ty,
            span: span,
        })
    }

    /// 解析开始的位置:
    /// fn foo() -> i32 { ... }
    /// ^             ^
    ///            结束的地方
    fn parse_fn_sig(&mut self, ctxt: FnContext) -> Option<FnSig> {
        self.advance(); // 消耗 `fn`
        let name = self.parse_ident()?;

        let generics = if ctxt.allow_generics {
            self.parse_generic_params()?
        } else {
            self.error(
                error("不允许此处使用泛型")
                    .with_span(self.current_token.span)
                    .build(),
            );
            return None;
        };

        self.expect(
            TokenKind::OpenParen,
            error("期待 `(`").with_span(self.current_token.span).build(),
        )?;

        let mut params = Vec::new();

        let is_variadic = loop {
            if self.check(TokenKind::Ellipsis) {
                if ctxt.allow_variadic {
                    self.advance();
                    break true;
                } else {
                    self.error(
                        error("不允许此处使用可变参数")
                            .with_span(self.current_token.span)
                            .build(),
                    );
                    return None;
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
            error("期待 `)`").with_span(self.current_token.span).build(),
        )?;

        let return_ty = if self.eat(TokenKind::Arrow) {
            FnRetTy::Ty(self.parse_type()?)
        } else {
            FnRetTy::Default(self.current_token.span)
        };
        Some(FnSig {
            generics: generics,
            name,
            params,
            return_type: return_ty,
            is_variadic,
        })
    }

    pub(super) fn parse_block(&mut self) -> Option<Block> {
        // 期待开大括号 - 如果失败直接返回，因为这是块的基本结构
        let open_brace = self
            .expect(
                TokenKind::OpenBrace,
                error("期待 `{`").with_span(self.current_token.span).build(),
            )?
            .span;

        let mut statements = Vec::new();
        let mut tail = None;

        // 解析块内容，容忍错误并继续
        while self.current_token.kind != TokenKind::CloseBrace
            && self.current_token.kind != TokenKind::Eof
        {
            let stmt_start = self.current_token.span;

            // 尝试解析语句，如果失败则记录错误并恢复
            if let Some(stmt) = self.parse_stmt() {
                // 检查是否是尾表达式
                if self.current_token.kind == TokenKind::Semi {
                    self.advance(); // 消耗分号
                    statements.push(stmt);
                    continue;
                }
                if self.current_token.kind == TokenKind::CloseBrace {
                    match stmt.kind {
                        StmtKind::Expr(expr) => {
                            tail = Some(expr);
                            break;
                        }
                        _ => {}
                    }
                }
                self.error(
                    error(format!("未添加分号"))
                        .with_span(self.current_token.span)
                        .build(),
                );
            }

            // 防止无限循环：如果解析失败但位置没变，强制前进
            if self.current_token.span == stmt_start {
                self.advance();
            }

            // 额外安全检查
            if self.current_token.kind == TokenKind::Eof {
                break;
            }
        }

        // 处理闭大括号
        let close_span = self
            .expect(
                TokenKind::CloseBrace,
                error("期待 `}`").with_span(self.current_token.span).build(),
            )?
            .span;

        Some(Block {
            node_id: DUMMY_NODE_ID,
            stmts: statements,
            tail,
            span: open_brace.extend_to(close_span),
        })
    }

    fn parse_attribute(&mut self) -> Option<Attr> {
        if !self.eat(TokenKind::Hash) {
            return None;
        }
        let span = self
            .expect(
                TokenKind::OpenBracket,
                error("期待 `[`").with_span(self.current_token.span).build(),
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
                error("期待 `]`").with_span(self.current_token.span).build(),
            )?
            .span,
        );
        Some(Attr {
            path: path,
            arg: arg,
            span,
        })
    }
}
