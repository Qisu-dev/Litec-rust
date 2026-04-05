use litec_ast::ast;
use litec_ast::{
    token::Base,
};
use litec_error::{Diagnostic, error};
use litec_hir::hir::{self, FloatKind, IntKind, LiteralIntKind, LiteralValue, UIntKind};
use litec_span::{Span, StringId, get_global_string};

pub struct Lower {
    pub diagnostics: Vec<Diagnostic>,
}

impl Lower {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn lower(mut self, ast: ast::Crate) -> (hir::Crate, Vec<Diagnostic>) {
        let hir = self.lower_crate(ast);
        (hir, self.diagnostics)
    }

    fn lower_crate(&mut self, ast: ast::Crate) -> hir::Crate {
        todo!()
    }

    fn lower_item(&mut self, ast: ast::Item) -> VResult<hir::Item> {
        todo!()
    }

    fn lower_int_literal_value(
        &mut self,
        base: Base,
        value: StringId,
        suffix: Option<StringId>,
        span: Span,
    ) -> Option<LiteralValue> {
        let s = get_global_string(value).unwrap();
        let radix = base as u32;
        let s = match suffix {
            Some(suffix_id) => &s[0..s.len() - get_global_string(suffix_id).unwrap().len()],
            None => s.as_ref(),
        };
        // 检查是否有负号
        let (is_negative, num_str) = if s.starts_with('-') {
            (true, &s[1..])
        } else {
            (false, s.as_ref())
        };

        // 解析绝对值为 u64
        let abs_value = match u64::from_str_radix(num_str, radix) {
            Ok(v) => v,
            Err(e) => {
                self.diagnostics.push(
                    error("数字解析失败")
                        .with_help(e.to_string())
                        .with_span(span)
                        .build(),
                );
                return None;
            }
        };

        // 转换为补码形式：负数取反加一，正数不变
        let bits = if is_negative {
            abs_value.wrapping_neg() // 等价于 (!abs_value).wrapping_add(1)
        } else {
            abs_value
        };

        // 根据后缀确定类型（只用于范围检查，不影响 bits 的存储）
        let kind = if let Some(suffix_id) = suffix {
            let suffix_str = get_global_string(suffix_id).unwrap();
            match suffix_str.as_ref() {
                "i8" => {
                    // 检查范围：-128 ~ 127
                    if is_negative && abs_value > 128 || !is_negative && abs_value > 127 {
                        self.diagnostics
                            .push(error("超出 i8 范围").with_span(span).build());
                        return None;
                    }
                    LiteralIntKind::Signed(IntKind::I8)
                }
                "i16" => {
                    if is_negative && abs_value > 32768 || !is_negative && abs_value > 32767 {
                        self.diagnostics
                            .push(error("超出 i16 范围").with_span(span).build());
                        return None;
                    }
                    LiteralIntKind::Signed(IntKind::I16)
                }
                "i32" => {
                    if is_negative && abs_value > 2147483648
                        || !is_negative && abs_value > 2147483647
                    {
                        self.diagnostics
                            .push(error("超出 i32 范围").with_span(span).build());
                        return None;
                    }
                    LiteralIntKind::Signed(IntKind::I32)
                }
                "i64" => {
                    if is_negative && abs_value > 9223372036854775807 {
                        self.diagnostics
                            .push(error("超出 i64 范围").with_span(span).build());
                        return None;
                    }
                    LiteralIntKind::Signed(IntKind::I64)
                }
                "isize" => {
                    if cfg!(target_pointer_width = "32") {
                        if is_negative && abs_value > 2147483648
                            || !is_negative && abs_value > 2147483647
                        {
                            self.diagnostics
                                .push(error("超出 isize 范围").with_span(span).build());
                            return None;
                        }
                        LiteralIntKind::Signed(IntKind::Isize)
                    } else {
                        if is_negative && abs_value > i64::MAX as u64 {
                            self.diagnostics
                                .push(error("超出 isize 范围").with_span(span).build());
                            return None;
                        }
                        LiteralIntKind::Signed(IntKind::Isize)
                    }
                }
                "u8" | "u16" | "u32" | "u64" | "usize" => {
                    if is_negative {
                        self.diagnostics
                            .push(error("无符号类型不能有负数").with_span(span).build());
                        return None;
                    }
                    match suffix_str.as_ref() {
                        "u8" => {
                            if abs_value > u8::MAX as u64 {
                                self.diagnostics
                                    .push(error("超出 u8 范围").with_span(span).build());
                                return None;
                            }
                            LiteralIntKind::Unsigned(UIntKind::U8)
                        }
                        "u16" => {
                            if abs_value > u16::MAX as u64 {
                                self.diagnostics
                                    .push(error("超出 u16 范围").with_span(span).build());
                                return None;
                            }
                            LiteralIntKind::Unsigned(UIntKind::U16)
                        }
                        "u32" => {
                            if abs_value > u32::MAX as u64 {
                                self.diagnostics
                                    .push(error("超出 u32 范围").with_span(span).build());
                                return None;
                            }
                            LiteralIntKind::Unsigned(UIntKind::U32)
                        }
                        "u64" => {
                            if abs_value > u64::MAX as u64 {
                                self.diagnostics
                                    .push(error("超出 u64 范围").with_span(span).build());
                                return None;
                            }
                            LiteralIntKind::Unsigned(UIntKind::U64)
                        }
                        "usize" => {
                            if cfg!(target_pointer_width = "32") {
                                if abs_value > u32::MAX as u64 {
                                    self.diagnostics
                                        .push(error("超出 usize 范围").with_span(span).build());
                                    return None;
                                }
                                LiteralIntKind::Unsigned(UIntKind::Usize)
                            } else {
                                if abs_value > u64::MAX {
                                    self.diagnostics
                                        .push(error("超出 usize 范围").with_span(span).build());
                                    return None;
                                }
                                LiteralIntKind::Unsigned(UIntKind::Usize)
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                _ => {
                    self.diagnostics
                        .push(error("未知数字后缀").with_span(span).build());
                    return None;
                }
            }
        } else {
            if is_negative && abs_value > 2147483648 || !is_negative && abs_value > 2147483647 {
                self.diagnostics
                    .push(error("超出 i32 范围").with_span(span).build());
                return None;
            }
            LiteralIntKind::Signed(IntKind::I32)
        };

        Some(LiteralValue::Int { value: bits, kind })
    }

    fn lower_float_literal_value(
        &mut self,
        value: StringId,
        suffix: Option<StringId>,
        span: Span,
    ) -> Option<LiteralValue> {
        let s = get_global_string(value).unwrap();
        // 解析浮点数值
        let parsed_value = match s.parse::<f64>() {
            Ok(value) => value,
            Err(e) => {
                self.diagnostics.push(
                    error("数字解析失败")
                        .with_help(e.to_string())
                        .with_span(span)
                        .build(),
                );
                return None;
            }
        };

        // 处理后缀并创建相应的 LitFloatValue
        let float_kind = if let Some(suffix_id) = suffix {
            let suffix_str = get_global_string(suffix_id).unwrap();
            match suffix_str.as_ref() {
                "f32" => {
                    if parsed_value < f32::MIN as f64 || parsed_value > f32::MAX as f64 {
                        self.diagnostics
                            .push(error("超出u32数字范围").with_span(span).build());
                    }
                    FloatKind::F32
                }
                "f64" => {
                    if parsed_value < f64::MIN || parsed_value > f64::MAX {
                        self.diagnostics
                            .push(error("超出f64数字范围").with_span(span).build());
                    }
                    FloatKind::F64
                }
                _ => {
                    self.diagnostics
                        .push(error("未知数字后缀").with_span(span).build());
                    return None;
                }
            }
        } else {
            // 无后缀的情况，使用 F32
            if parsed_value < f32::MIN as f64 || parsed_value > f32::MAX as f64 {
                self.diagnostics
                    .push(error("超出f32数字范围").with_span(span).build());
            }
            FloatKind::F32
        };

        Some(LiteralValue::Float {
            value: parsed_value,
            kind: float_kind,
        })
    }
}
fn unescape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('0') => result.push('\0'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('\'') => result.push('\''),
                Some('x') => {
                    // \xNN 十六进制转义
                    let mut hex = String::new();
                    if let Some(h1) = chars.next() {
                        hex.push(h1);
                    }
                    if let Some(h2) = chars.next() {
                        hex.push(h2);
                    }
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    }
                }
                Some('u') => {
                    // \u{NNNN} Unicode 转义
                    if chars.next() == Some('{') {
                        let mut hex = String::new();
                        while let Some(h) = chars.next() {
                            if h == '}' {
                                break;
                            }
                            hex.push(h);
                        }
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(c) = char::from_u32(code) {
                                result.push(c);
                            }
                        }
                    }
                }
                Some(c) => {
                    // 未知转义，保留原样或报错
                    result.push('\\');
                    result.push(c);
                }
                None => break,
            }
        } else {
            result.push(c);
        }
    }

    result
}

pub fn lower(ast: ast::Crate) -> (hir::Crate, Vec<Diagnostic>) {
    let mut lower = Lower::new(ast);
    lower.lower()
}
