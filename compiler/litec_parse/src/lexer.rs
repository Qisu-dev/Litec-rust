use std::cell::Ref;
use std::sync::Arc;

use litec_ast::token::LiteralKind::*;
use litec_ast::token::TokenKind::*;
use litec_ast::token::{Token, TokenKind};
use litec_error::{Diagnostic, error};
use litec_session::Session;
use litec_span::{FileId, SourceFile, SourceMap, Span, intern_global};
use unicode_properties::UnicodeEmoji;

/// Lexer 结果类型
pub type LexResult<T> = Result<T, Diagnostic>;

#[derive(Debug)]
pub struct Lexer<'src> {
    session: &'src Session, // 源字符串切片
    file_id: FileId,
    position: usize,            // 当前字节位置
    current_token_start: usize, // 当前token的起始字节位置
}

#[derive(Debug)]
pub struct LexerSnapshot {
    position: usize,
    current_token_start: usize,
}

pub fn is_id_start(c: char) -> bool {
    c == '_' || unicode_xid::UnicodeXID::is_xid_start(c)
}

pub fn is_id_continue(c: char) -> bool {
    unicode_xid::UnicodeXID::is_xid_continue(c)
}

impl<'src> Lexer<'src> {
    pub fn new(session: &'src Session, file_id: FileId) -> Self {
        Lexer {
            session,
            file_id,
            position: 0,
            current_token_start: 0,
        }
    }

    pub(crate) fn snapshot(&self) -> LexerSnapshot {
        LexerSnapshot {
            position: self.position,
            current_token_start: self.current_token_start,
        }
    }

    pub(crate) fn restore(&mut self, snapshot: LexerSnapshot) {
        self.position = snapshot.position;
        self.current_token_start = snapshot.current_token_start;
    }

    fn is_eof(&self) -> bool {
        self.position >= self.source_file().len()
    }

    fn current_char(&self) -> Option<char> {
        self.source_file().source[self.position..].chars().next()
    }

    fn peek_char(&self, n: usize) -> Option<char> {
        self.source_file().source[self.position..].chars().nth(n)
    }

    fn source_file(&self) -> Ref<'_, SourceFile> {
        Ref::map(self.session.source_map.borrow(), |sm| {
            sm.file(self.file_id).unwrap()
        })
    }

    pub fn advance(&mut self, n: usize) {
        for _ in 0..n {
            if let Some(c) = self.current_char() {
                self.position += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn advance_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        while let Some(c) = self.current_char() {
            if predicate(c) {
                self.position += c.len_utf8(); // 直接增加字符的字节长度
            } else {
                break;
            }
        }
    }

    fn start_token(&mut self) {
        self.current_token_start = self.position;
    }

    fn create_token(&self, kind: TokenKind) -> Token {
        let text = intern_global(
            &self.source_file().source_text()[self.current_token_start..self.position],
        );
        Token {
            kind,
            text,
            span: self.make_span(),
        }
    }

    fn make_span(&self) -> Span {
        Span {
            start: self
                .source_file()
                .location_from_offset(self.current_token_start),
            end: self.source_file().location_from_offset(self.position),
            file: self.file_id,
        }
    }

    pub fn advance_token(&mut self) -> LexResult<Token> {
        // 跳过空白字符
        self.skip_whitespace();

        // 开始新token
        self.start_token();

        if self.is_eof() {
            return Ok(self.create_token(Eof));
        }

        let Some(first_char) = self.current_char() else {
            return Ok(self.create_token(Eof));
        };

        let token_kind = match first_char {
            ';' => {
                self.advance(1);
                Semi
            }
            ',' => {
                self.advance(1);
                Comma
            }
            '.' => self.lex_dot(),
            '(' => {
                self.advance(1);
                OpenParen
            }
            ')' => {
                self.advance(1);
                CloseParen
            }
            '{' => {
                self.advance(1);
                OpenBrace
            }
            '}' => {
                self.advance(1);
                CloseBrace
            }
            '[' => {
                self.advance(1);
                OpenBracket
            }
            ']' => {
                self.advance(1);
                CloseBracket
            }
            '@' => {
                self.advance(1);
                At
            }
            '#' => {
                self.advance(1);
                Hash
            }
            '~' => {
                self.advance(1);
                Tilde
            }
            '?' => {
                self.advance(1);
                Question
            }
            ':' => self.lex_colon(),
            '$' => {
                self.advance(1);
                Dollar
            }
            '=' => self.lex_equals(),
            '!' => self.lex_bang(),
            '<' => self.lex_lt(),
            '>' => self.lex_gt(),
            '-' => self.lex_minus(),
            '&' => self.lex_and(),
            '|' => self.lex_or(),
            '+' => self.lex_plus(),
            '*' => self.lex_star(),
            '/' => match self.lex_slash() {
                Some(kind) => kind,
                None => return self.advance_token(),
            },
            '^' => {
                self.advance(1);
                match self.current_char() {
                    Some('=') => {
                        self.advance(1); // 消费 '='
                        BitXorEq
                    }
                    _ => BitXor,
                }
            }
            '%' => self.lex_percent(),

            '\'' => self.lex_char()?,
            '"' => self.lex_string()?,

            number @ '0'..='9' => self.lex_number(number)?,

            ident if is_id_start(ident) => self.lex_identifier(),

            c if !c.is_ascii() && c.is_emoji_char() => {
                let span = self.make_span();
                return Err(error(format!("非法字符 `{}`", c)).with_span(span).build());
            }

            c => {
                let span = self.make_span();
                return Err(error(format!("非法字符 `{}`", c)).with_span(span).build());
            }
        };

        Ok(self.create_token(token_kind))
    }

    fn lex_colon(&mut self) -> TokenKind {
        self.advance(1);
        match self.current_char() {
            Some(':') => {
                self.advance(1);
                PathAccess
            }
            _ => Colon,
        }
    }

    fn lex_dot(&mut self) -> TokenKind {
        self.advance(1);
        match self.current_char() {
            Some('.') => {
                self.advance(1);
                match self.current_char() {
                    Some('=') => {
                        self.advance(1);
                        ToEq
                    }
                    Some('.') => {
                        self.advance(1);
                        Ellipsis
                    }
                    _ => To,
                }
            }
            _ => Dot,
        }
    }

    fn lex_percent(&mut self) -> TokenKind {
        self.advance(1); // 消费 '%'
        if self.current_char() == Some('=') {
            self.advance(1); // 消费 '='
            RemainderEq
        } else {
            Remainder
        }
    }

    fn lex_plus(&mut self) -> TokenKind {
        self.advance(1); // 消费第一个 '+'
        match self.current_char() {
            Some('+') => {
                self.advance(1); // 消费第二个 '+'
                PlusPlus
            }
            Some('=') => {
                self.advance(1); // 消费 '='
                PlusEq
            }
            _ => Add,
        }
    }

    fn lex_minus(&mut self) -> TokenKind {
        self.advance(1); // 消费第一个 '-'
        match self.current_char() {
            Some('-') => {
                self.advance(1); // 消费第二个 '-'
                MinusMinus
            }
            Some('>') => {
                self.advance(1);
                Arrow
            }
            Some('=') => {
                self.advance(1); // 消费 '='
                MinusEq
            }
            _ => Minus,
        }
    }

    fn lex_equals(&mut self) -> TokenKind {
        self.advance(1); // 消费第一个 '='
        match self.current_char() {
            Some('=') => {
                self.advance(1);
                EqEq
            }
            Some('>') => {
                self.advance(1);
                FatArrow
            }
            _ => Assign,
        }
    }

    fn lex_bang(&mut self) -> TokenKind {
        self.advance(1); // 消费 '!'
        if self.current_char() == Some('=') {
            self.advance(1); // 消费 '='
            NotEq
        } else {
            Bang
        }
    }

    fn lex_lt(&mut self) -> TokenKind {
        self.advance(1); // 消费 '<'
        match self.current_char() {
            Some('=') => {
                self.advance(1);
                Le
            }
            Some('<') => {
                self.advance(1);
                match self.current_char() {
                    Some('=') => {
                        self.advance(1);
                        ShlEq
                    }
                    _ => Shl,
                }
            }
            _ => Lt,
        }
    }

    fn lex_gt(&mut self) -> TokenKind {
        self.advance(1); // 消费 '>'
        match self.current_char() {
            Some('=') => {
                self.advance(1);
                Ge
            }
            Some('>') => {
                self.advance(1);
                match self.current_char() {
                    Some('=') => {
                        self.advance(1);
                        ShrEq
                    }
                    _ => Shr,
                }
            }
            _ => Gt,
        }
    }

    fn lex_and(&mut self) -> TokenKind {
        self.advance(1); // 消费 '&'
        if self.current_char() == Some('&') {
            self.advance(1); // 消费第二个 '&'
            And
        } else if self.current_char() == Some('=') {
            self.advance(1); // 消费 '='
            BitAndEq
        } else {
            BitAnd
        }
    }

    fn lex_or(&mut self) -> TokenKind {
        self.advance(1); // 消费 '|'
        if self.current_char() == Some('|') {
            self.advance(1); // 消费第二个 '|'
            Or
        } else if self.current_char() == Some('=') {
            self.advance(1); // 消费 '='
            BitOrEq
        } else {
            BitOr
        }
    }

    fn lex_slash(&mut self) -> Option<TokenKind> {
        self.advance(1); // 消费 '/'
        match self.current_char() {
            Some('/') => {
                self.advance(1); // 消费第二个 '/'
                self.skip_line_comment();
                None
            }
            Some('*') => {
                self.advance(1); // 消费 '*'
                self.skip_block_comment();
                None
            }
            Some('=') => {
                self.advance(1);
                Some(DivEq)
            }
            _ => Some(Div),
        }
    }

    fn lex_star(&mut self) -> TokenKind {
        self.advance(1);
        match self.current_char() {
            Some('=') => {
                self.advance(1);
                MulEq
            }
            _ => Mul,
        }
    }

    fn lex_char(&mut self) -> LexResult<TokenKind> {
        self.advance(1); // 消费开头的单引号
        let terminated = self.parse_single_quoted_string();

        if !terminated {
            let span: Span = self.make_span();
            return Err(error("未关闭的字符字面量").with_span(span).build());
        }

        // 检查是否有后缀
        let suffix = if self.current_char().map_or(false, is_id_start) {
            let suffix_start = self.position;
            self.eat_suffix();
            let suffix_value = &self.source_file().source[suffix_start..self.position];
            Some(intern_global(suffix_value))
        } else {
            None
        };

        Ok(Literal { kind: Char, suffix })
    }

    fn lex_string(&mut self) -> LexResult<TokenKind> {
        self.advance(1); // 消费开头的双引号
        let terminated = self.parse_double_quoted_string();

        if !terminated {
            let span = self.make_span();
            return Err(error("未闭合的字符串").with_span(span).build());
        }

        // 检查是否有后缀
        let suffix = if self.current_char().map_or(false, is_id_start) {
            let suffix_start = self.position;
            self.eat_suffix();
            let suffix_value = &self.source_file().source[suffix_start..self.position];
            Some(intern_global(suffix_value))
        } else {
            None
        };

        Ok(Literal { kind: Str, suffix })
    }

    fn parse_single_quoted_string(&mut self) -> bool {
        loop {
            match self.current_char() {
                Some('\'') => {
                    self.advance(1); // 消费结尾的单引号
                    return true;
                }
                Some('\\') => {
                    self.advance(1); // 消费反斜杠
                    if self.current_char().is_some() {
                        self.advance(1); // 消费转义字符
                    }
                }
                None => break, // 到达文件末尾
                _ => {
                    self.advance(1); // 消费普通字符
                }
            }
        }
        false
    }

    fn parse_double_quoted_string(&mut self) -> bool {
        loop {
            match self.current_char() {
                Some('"') => {
                    self.advance(1); // 消费结尾的双引号
                    return true;
                }
                Some('\\') => {
                    self.advance(1); // 消费反斜杠
                    // 处理转义序列
                    if let Some(c) = self.current_char() {
                        if c == '\\' || c == '"' {
                            self.advance(1); // 消费转义字符
                        }
                    }
                }
                None => break, // 到达文件末尾
                _ => {
                    self.advance(1); // 消费普通字符
                }
            }
        }
        false
    }

    fn lex_number(&mut self, first_digit: char) -> LexResult<TokenKind> {
        let mut empty_int = false;

        // 消费第一个数字
        self.advance(1);

        if first_digit == '0' {
            match self.current_char() {
                Some('b') => {
                    self.advance(1); // 消费 'b'
                    empty_int = !self.eat_digits(|c| matches!(c, '0'..='1'));
                }
                Some('o') => {
                    self.advance(1); // 消费 'o'
                    empty_int = !self.eat_digits(|c| matches!(c, '0'..='7'));
                }
                Some('x') => {
                    self.advance(1); // 消费 'x'
                    empty_int = !self.eat_digits(|c| c.is_ascii_hexdigit());
                }
                Some('0'..='9') | Some('_') => {
                    self.eat_digits(|c| c.is_ascii_digit() || c == '_');
                }
                _ => {
                    // 纯0，后面没有其他字符
                }
            }
        } else {
            self.eat_digits(|c| c.is_ascii_digit() || c == '_');
        }

        // 检查是否有小数点或指数（浮点数）
        let mut empty_exponent = false;
        let mut is_float = false;

        if self.current_char() == Some('.') {
            let next_char = self.peek_char(1);

            // 检查是否是范围运算符 (如 1..2)
            if next_char == Some('.') {
                // 这是范围运算符，不是小数点，所以不处理
            }
            // 检查是否是方法调用 (如 1.foo())
            else if next_char.map_or(false, |c| c.is_alphabetic() || c == '_') {
                // 这是方法调用，不是小数点，所以不处理
            }
            // 正常的小数点
            else if next_char != Some('.') && !next_char.map_or(false, is_id_start) {
                self.advance(1); // 消费小数点
                is_float = true;

                if self.current_char().map_or(false, |c| c.is_ascii_digit()) {
                    self.eat_digits(|c| c.is_ascii_digit() || c == '_');

                    if matches!(self.current_char(), Some('e') | Some('E')) {
                        self.advance(1); // 消费 'e' 或 'E'
                        empty_exponent = !self.eat_float_exponent();
                    }
                }
            }
        }

        // 检查指数（对于整数）
        if !is_float && matches!(self.current_char(), Some('e') | Some('E')) {
            self.advance(1); // 消费 'e' 或 'E'
            empty_exponent = !self.eat_float_exponent();
            is_float = true;
        }

        // 检查错误情况
        if !is_float && empty_int {
            let span = self.make_span();
            return Err(error("未关闭的整数").with_span(span).build());
        }

        if is_float && empty_exponent {
            let span = self.make_span();
            return Err(error("未关闭的浮点数").with_span(span).build());
        }

        // 解析后缀
        let suffix = if self.current_char().map_or(false, is_id_start) {
            let suffix_start = self.position;
            self.eat_suffix();
            let suffix_value = &self.source_file().source[suffix_start..self.position];
            Some(intern_global(suffix_value))
        } else {
            None
        };

        Ok(if is_float {
            Literal {
                kind: Float,
                suffix,
            }
        } else {
            Literal {
                kind: Integer,
                suffix,
            }
        })
    }

    fn eat_float_exponent(&mut self) -> bool {
        if matches!(self.current_char(), Some('+') | Some('-')) {
            self.advance(1); // 消费符号
        }
        self.eat_digits(|c| c.is_ascii_digit() || c == '_')
    }

    fn eat_digits(&mut self, mut predicate: impl FnMut(char) -> bool) -> bool {
        let mut has_digits = false;

        while let Some(c) = self.current_char() {
            if predicate(c) {
                has_digits = true;
                self.advance(1);
            } else {
                break;
            }
        }

        has_digits
    }

    fn eat_suffix(&mut self) {
        self.advance_while(is_id_continue);
    }

    fn lex_identifier(&mut self) -> TokenKind {
        // 消费第一个字符（已经是标识符起始字符）
        self.advance(1);

        // 消费剩余的标识符字符
        self.advance_while(is_id_continue);

        // 获取标识符文本
        let ident_text = &self.source_file().source[self.current_token_start..self.position];

        // 检查是否为关键字
        match ident_text {
            "fn" => Fn,
            "let" => Let,
            "if" => If,
            "else" => Else,
            "while" => While,
            "for" => For,
            "return" => Return,
            "true" => True,
            "false" => False,
            "in" => In,
            "struct" => Struct,
            "loop" => Loop,
            "break" => Break,
            "continue" => Continue,
            "pub" => Pub,
            "priv" => Priv,
            "use" => Use,
            "as" => As,
            "extern" => Extern,
            "mut" => Mut,
            "const" => Const,
            "mod" => Mod,
            "super" => Super,
            "crate" => Crate,
            "self" => SelfLower,
            "Self" => SelfUpper,
            "trait" => Trait,
            "type" => Type,
            "impl" => Impl,
            _ => Ident,
        }
    }

    fn skip_whitespace(&mut self) {
        self.advance_while(|c| c.is_whitespace());
    }

    fn skip_line_comment(&mut self) {
        self.advance_while(|c| c != '\n');
    }

    fn skip_block_comment(&mut self) {
        let mut depth = 1;
        while depth > 0 {
            match (self.current_char(), self.peek_char(1)) {
                (Some('/'), Some('*')) => {
                    self.advance(2); // 消费 '/*'
                    depth += 1;
                }
                (Some('*'), Some('/')) => {
                    self.advance(2); // 消费 '*/'
                    depth -= 1;
                }
                (Some(_), _) => {
                    self.advance(1); // 消费普通字符
                }
                (None, _) => break, // 到达文件末尾
            }
        }
    }
}
