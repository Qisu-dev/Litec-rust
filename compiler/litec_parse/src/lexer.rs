use litec_ast::token::{Base, Token, TokenKind};
use litec_ast::token::TokenKind::*;
use litec_ast::token::LiteralKind::*;
use litec_span::{intern_global, FileId, SourceFile, SourceMap, Span};
use litec_error::{error, invalid_character, unterminated_char, unterminated_string, Diagnostic};
use unicode_properties::UnicodeEmoji;

/// Lexer 结果类型
pub type LexResult<T> = Result<T, Diagnostic>;

#[derive(Debug)]
pub struct Lexer<'src> {
    source: &'src SourceFile,           // 源字符串切片
    file_id: FileId,
    position: usize,             // 当前字节位置
    current_token_start: usize,  // 当前token的起始字节位置
}

#[derive(Debug)]
pub struct LexerSnapshot {
    position: usize,
    current_token_start: usize
}

pub fn is_id_start(c: char) -> bool {
    c == '_' || unicode_xid::UnicodeXID::is_xid_start(c)
}

pub fn is_id_continue(c: char) -> bool {
    unicode_xid::UnicodeXID::is_xid_continue(c)
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src SourceFile, file_id: FileId) -> Self {
        Lexer {
            source,
            file_id,
            position: 0,
            current_token_start: 0,
        }
    }

    pub fn snapshot(&self) -> LexerSnapshot {
        LexerSnapshot {
            position: self.position,
            current_token_start: self.current_token_start
        }
    }

    pub fn restore(&mut self, snapshot: LexerSnapshot) {
        self.position = snapshot.position;
        self.current_token_start = snapshot.current_token_start;
    }

    fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }

    fn current_char(&self) -> Option<char> {
        self.source.source[self.position..].chars().next()
    }

    fn peek_char(&self, n: usize) -> Option<char> {
        self.source.source[self.position..].chars().nth(n)
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
                self.position += c.len_utf8();  // 直接增加字符的字节长度
            } else {
                break;
            }
        }
    }

    fn start_token(&mut self) {
        self.current_token_start = self.position;
    }

    fn create_token(&self, kind: TokenKind) -> Token<'src> {
        let text = &self.source.source_text()[self.current_token_start..self.position];
        Token {
            kind,
            text,
            span: self.make_span()
        }
    }

    fn make_span(&self) -> Span {
        Span {
            start: self.source.location_from_offset(self.current_token_start),
            end: self.source.location_from_offset(self.position),
            file: self.file_id
        }
    }

    pub fn advance_token(&mut self) -> LexResult<Token<'src>> {
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
            ';' => { self.advance(1); Semi },
            ',' => { self.advance(1); Comma },
            '.' => self.lex_dot(),
            '(' => { self.advance(1); OpenParen },
            ')' => { self.advance(1); CloseParen },
            '{' => { self.advance(1); OpenBrace },
            '}' => { self.advance(1); CloseBrace },
            '[' => { self.advance(1); OpenBracket },
            ']' => { self.advance(1); CloseBracket },
            '@' => { self.advance(1); At },
            '#' => { self.advance(1); Hash },
            '~' => { self.advance(1); Tilde },
            '?' => { self.advance(1); Question },
            ':' => self.lex_colon(),
            '$' => { self.advance(1); Dollar },
            '=' => self.lex_equals(),
            '!' => self.lex_bang(),
            '<' => self.lex_lt(),
            '>' => self.lex_gt(),
            '-' => self.lex_minus(),
            '&' => self.lex_and(),
            '|' => self.lex_or(),
            '+' => self.lex_plus(),
            '*' => self.lex_star(),
            '/' => {
                match self.lex_slash() {
                    Some(kind) => kind,
                    None => return self.advance_token(),
                }
            },
            '^' => { self.advance(1); BitXor },
            '%' => self.lex_percent(),

            '\'' => self.lex_char()?,
            '"' => self.lex_string()?,

            number @ '0'..='9' => self.lex_number(number)?,

            ident if is_id_start(ident) => self.lex_identifier(),

            c if !c.is_ascii() && c.is_emoji_char() => {
                let span = self.make_span();
                return Err(invalid_character(
                    c,
                    span,
                ).build());
            }

            c => {
                let span = self.make_span();
                return Err(invalid_character(
                    c,
                    span,
                ).build());
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
            _ => Colon
        }
    }

    fn lex_dot(&mut self) -> TokenKind {
        self.advance(1);
        match self.current_char() {
            Some('.') => {
                self.advance(1);
                match self.current_char() {
                    Some('.') => {
                        self.advance(1);
                        ToEq
                    }
                    _ => To
                }
            }
            _ => Dot
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
            _ => {
                Assign
            }
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
                Shl
            }
            _ => Lt
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
                Shr
            }
            _ => Gt
        }
    }

    fn lex_and(&mut self) -> TokenKind {
        self.advance(1); // 消费 '&'
        if self.current_char() == Some('&') {
            self.advance(1); // 消费第二个 '&'
            And
        } else {
            BitAnd
        }
    }

    fn lex_or(&mut self) -> TokenKind {
        self.advance(1); // 消费 '|'
        if self.current_char() == Some('|') {
            self.advance(1); // 消费第二个 '|'
            Or
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
            _ => Mul
        }
    }

    fn lex_char(&mut self) -> LexResult<TokenKind> {
        self.advance(1); // 消费开头的单引号
        let terminated = self.parse_single_quoted_string();
        
        if !terminated {
            let span: Span = self.make_span();
            return Err(unterminated_char(span).build());
        }
        
        // 检查是否有后缀
        let suffix = if self.current_char().map_or(false, is_id_start) {
            let suffix_start = self.position;
            self.eat_suffix();
            let suffix_value = &self.source.source[suffix_start..self.position];
            Some(intern_global(suffix_value))
        } else {
            None
        };

        Ok(Literal {
            kind: Char { terminated },
            suffix,
        })
    }

    fn lex_string(&mut self) -> LexResult<TokenKind> {
        self.advance(1); // 消费开头的双引号
        let terminated = self.parse_double_quoted_string();
        
        if !terminated {
            let span = self.make_span();
            return Err(unterminated_string(span).build());
        }
        
        // 检查是否有后缀
        let suffix = if self.current_char().map_or(false, is_id_start) {
            let suffix_start = self.position;
            self.eat_suffix();
            let suffix_value = &self.source.source[suffix_start..self.position];
            Some(intern_global(suffix_value))
        } else {
            None
        };

        Ok(Literal {
            kind: Str { terminated },
            suffix,
        })
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
        let mut base = Base::Decimal;
        let mut empty_int = false;
        
        // 消费第一个数字
        self.advance(1);
        
        if first_digit == '0' {
            match self.current_char() {
                Some('b') => {
                    self.advance(1); // 消费 'b'
                    base = Base::Binary;
                    empty_int = !self.eat_digits(|c| matches!(c, '0'..='1'));
                }
                Some('o') => {
                    self.advance(1); // 消费 'o'
                    base = Base::Octal;
                    empty_int = !self.eat_digits(|c| matches!(c, '0'..='7'));
                }
                Some('x') => {
                    self.advance(1); // 消费 'x'
                    base = Base::Hexadecimal;
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
            return Err(error("未关闭的整数")
                        .with_span(span)
                    .build());
        }
        
        if is_float && empty_exponent {
            let span = self.make_span();
            return Err(error("未关闭的浮点数")
                        .with_span(span)
                        .build());
        }
        
        // 解析后缀
        let suffix = if self.current_char().map_or(false, is_id_start) {
            let suffix_start = self.position;
            self.eat_suffix();
            let suffix_value = &self.source.source[suffix_start..self.position];
            Some(intern_global(suffix_value))
        } else {
            None
        };

        Ok(if is_float {
            Literal {
                kind: Float { base },
                suffix,
            }
        } else {
            Literal {
                kind: Int { base },
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
        let ident_text = &self.source.source[self.current_token_start..self.position];
        
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

pub fn tokenize<'a>(source_map: &'a SourceMap, file_id: FileId)  -> Vec<Token<'a>> {
    let mut lexer = Lexer::new(source_map.file(file_id).unwrap(), file_id);

    let mut tokens = Vec::new();

    loop {
        match lexer.advance_token() {
            Ok(token) =>{ 
                let kind = token.kind.clone();
                tokens.push(token);

                if kind == TokenKind::Eof {
                    break;
                }
            },
            Err(e) => {
                eprintln!("{}", e.render(source_map));
            } 
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use litec_span::SourceMap;

    // 创建测试用的 SourceFile
    fn create_test_source() -> (SourceMap, FileId) {
        let mut source_map = SourceMap::new();
        let source_code = r#"fn main() {
    let x = 5;
    let y = "hello";
    let z = 'a';
    let result = calculate(42);
}"#;
        
        let file_id = source_map.add_file(
            "test.rs".to_string(),
            source_code.to_string(),
            Path::new("test.rs"),
        );
        
        (source_map, file_id)
    }

    // 创建包含错误的测试源代码
    fn create_error_test_source() -> (SourceMap, FileId) {
        let mut source_map = SourceMap::new();
        let source_code = r#"fn main() {
    let x = 'a;
    let y = "hello;
    let z = @;
}"#;
        
        let file_id = source_map.add_file(
            "error_test.rs".to_string(),
            source_code.to_string(),
            Path::new("error_test.rs"),
        );
        
        (source_map, file_id)
    }

    #[test]
    fn test_lexer_creation() {
        let (source_map, file_id) = create_test_source();
        let source_file = source_map.file(file_id).unwrap();
        let lexer = Lexer::new(source_file, file_id);
        
        assert_eq!(lexer.position, 0);
        assert_eq!(lexer.current_token_start, 0);
    }

    #[test]
    fn test_basic_tokens() {
        let (source_map, file_id) = create_test_source();
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        // 测试识别 fn 关键字
        let token = lexer.advance_token().unwrap();
        assert_eq!(token.kind, TokenKind::Fn);
        assert_eq!(token.text, "fn");
        
        // 测试识别标识符 main
        let token = lexer.advance_token().unwrap();
        assert_eq!(token.kind, TokenKind::Ident);
        assert_eq!(token.text, "main");
        
        // 测试识别括号
        let token = lexer.advance_token().unwrap();
        assert_eq!(token.kind, TokenKind::OpenParen);
        assert_eq!(token.text, "(");
        
        let token = lexer.advance_token().unwrap();
        assert_eq!(token.kind, TokenKind::CloseParen);
        assert_eq!(token.text, ")");
    }

    #[test]
    fn test_keywords() {
        let source_code = "fn let if else while for return true false in struct loop break continue pub priv use as";
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "keywords.rs".to_string(),
            source_code.to_string(),
            Path::new("keywords.rs"),
        );
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        let expected_keywords = vec![
            TokenKind::Fn, TokenKind::Let, TokenKind::If, TokenKind::Else,
            TokenKind::While, TokenKind::For, TokenKind::Return, TokenKind::True,
            TokenKind::False, TokenKind::In, TokenKind::Struct, TokenKind::Loop,
            TokenKind::Break, TokenKind::Continue, TokenKind::Pub, TokenKind::Priv,
            TokenKind::Use, TokenKind::As,
        ];
        
        for expected in expected_keywords {
            let token = lexer.advance_token().unwrap();
            assert_eq!(token.kind, expected);
        }
    }

    #[test]
    fn test_numbers() {
        let source_code = "123 0x1F 0b1010 0o755 3.14 1e10 1.5e-3 0 0.0";
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "numbers.rs".to_string(),
            source_code.to_string(),
            Path::new("numbers.rs"),
        );
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        // 测试十进制整数
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Int { base: Base::Decimal }, .. }));
        assert_eq!(token.text, "123");
        
        // 测试十六进制
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Int { base: Base::Hexadecimal }, .. }));
        assert_eq!(token.text, "0x1F");
        
        // 测试二进制
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Int { base: Base::Binary }, .. }));
        assert_eq!(token.text, "0b1010");
        
        // 测试八进制
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Int { base: Base::Octal }, .. }));
        assert_eq!(token.text, "0o755");
        
        // 测试浮点数
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Float { .. }, .. }));
        assert_eq!(token.text, "3.14");
    }

    #[test]
    fn test_string_and_char_literals() {
        let source_code = r#""hello" 'a' "escaped\"string" '\\'"#;
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "literals.rs".to_string(),
            source_code.to_string(),
            Path::new("literals.rs"),
        );
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        // 测试字符串字面量
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Str { terminated: true }, .. }));
        assert_eq!(token.text, "\"hello\"");
        
        // 测试字符字面量
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Char { terminated: true }, .. }));
        assert_eq!(token.text, "'a'");
        
        // 测试转义字符串
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Str { terminated: true }, .. }));
        assert_eq!(token.text, "\"escaped\\\"string\"");
        
        // 测试转义字符
        let token = lexer.advance_token().unwrap();
        assert!(matches!(token.kind, TokenKind::Literal { kind: litec_ast::token::LiteralKind::Char { terminated: true }, .. }));
        assert_eq!(token.text, "'\\\\'");
    }

    #[test]
    fn test_operators() {
        let source_code = "+ ++ += - -- -= -> = == => ! != < <= > >= & && | || * *= / /= % %= . .. : ::";
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "operators.rs".to_string(),
            source_code.to_string(),
            Path::new("operators.rs"),
        );
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        let expected_operators = vec![
            TokenKind::Add, TokenKind::PlusPlus, TokenKind::PlusEq,
            TokenKind::Minus, TokenKind::MinusMinus, TokenKind::MinusEq, TokenKind::Arrow,
            TokenKind::Assign, TokenKind::EqEq, TokenKind::FatArrow,
            TokenKind::Bang, TokenKind::NotEq,
            TokenKind::Lt, TokenKind::Le, TokenKind::Gt, TokenKind::Ge,
            TokenKind::BitAnd, TokenKind::And, TokenKind::BitOr, TokenKind::Or,
            TokenKind::Mul, TokenKind::MulEq, TokenKind::Div, TokenKind::DivEq,
            TokenKind::Remainder, TokenKind::RemainderEq,
            TokenKind::Dot, TokenKind::To, TokenKind::Colon, TokenKind::PathAccess,
        ];
        
        for expected in expected_operators {
            let token = lexer.advance_token().unwrap();
            assert_eq!(token.kind, expected, "Failed for operator: {:?}", expected);
        }
    }

    #[test]
    fn test_comments() {
        let source_code = r#"// 这是单行注释
/* 这是
   多行注释 */
fn main() {}"#;
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "comments.rs".to_string(),
            source_code.to_string(),
            Path::new("comments.rs"),
        );
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        // 然后应该是 fn 关键字
        let token = lexer.advance_token().unwrap();
        assert_eq!(token.kind, TokenKind::Fn);
    }

    #[test]
    fn test_error_recovery() {
        let (source_map, file_id) = create_error_test_source();
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        let mut tokens = Vec::new();
        let mut errors = Vec::new();
        
        loop {
            match lexer.advance_token() {
                Ok(token) => {
                    tokens.push(token.clone());
                    if token.kind == TokenKind::Eof {
                        break;
                    }
                }
                Err(error) => {
                    errors.push(error);
                    // 错误恢复：跳过当前字符
                    if !lexer.is_eof() {
                        lexer.advance(1);
                    }
                }
            }
        }
        
        // 应该检测到错误
        assert!(!errors.is_empty(), "应该检测到词法错误");
        
        // 应该仍然能够解析一些有效的 token
        assert!(!tokens.is_empty(), "应该解析出一些有效的 token");
        
        // 检查是否包含预期的 token
        let has_fn = tokens.iter().any(|t| t.kind == TokenKind::Fn);
        let has_let = tokens.iter().any(|t| t.kind == TokenKind::Let);
        assert!(has_fn || has_let, "应该解析出一些关键字");
    }

    #[test]
    fn test_span_correctness() {
        let (source_map, file_id) = create_test_source();
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        let token = lexer.advance_token().unwrap(); // fn
        
        // 检查 Span 是否正确设置
        assert_eq!(token.span.file, file_id);
        assert!(token.span.start.offset < token.span.end.offset);
        assert_eq!(token.text, "fn");
        
        // 检查位置信息
        let start_line = token.span.start.line;
        let start_column = token.span.start.column;
        let end_line = token.span.end.line;
        let end_column = token.span.end.column;
        
        // fn 应该在文件的开始位置
        assert_eq!(start_line, 0);
        assert_eq!(start_column, 0);
        assert_eq!(end_line, 0);
        assert_eq!(end_column, 2);
    }

    #[test]
    fn test_tokenize_function() {
        let (source_map, file_id) = create_test_source();
        let tokens = tokenize(&source_map, file_id);
        
        // 检查 token 数量
        assert!(!tokens.is_empty(), "应该解析出 token");
        
        // 检查是否以 Eof 结束
        let last_token = tokens.last().unwrap();
        assert_eq!(last_token.kind, TokenKind::Eof);
        
        // 检查包含预期的 token 类型
        let token_kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert!(token_kinds.contains(&TokenKind::Fn));
        assert!(token_kinds.contains(&TokenKind::Ident));
        assert!(token_kinds.contains(&TokenKind::OpenParen));
        assert!(token_kinds.contains(&TokenKind::CloseParen));
        assert!(token_kinds.contains(&TokenKind::OpenBrace));
        assert!(token_kinds.contains(&TokenKind::CloseBrace));
        assert!(token_kinds.contains(&TokenKind::Let));
        assert!(token_kinds.contains(&TokenKind::Literal { kind: litec_ast::token::LiteralKind::Int { base: Base::Decimal }, suffix: None }));
    }

    #[test]
    fn test_snapshot_restore() {
        let (source_map, file_id) = create_test_source();
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        // 读取前两个 token
        lexer.advance_token().unwrap();
        
        // 创建快照
        let snapshot = lexer.snapshot();

        let token2 = lexer.advance_token().unwrap();
        
        lexer.advance_token().unwrap();
        lexer.advance_token().unwrap();
        
        // 恢复到快照
        lexer.restore(snapshot);
        
        // 现在读取的 token 应该和之前读取的第二个 token 后的状态一致
        let token_after_restore = lexer.advance_token().unwrap();
        assert_eq!(token_after_restore.kind, token2.kind);
        assert_eq!(token_after_restore.text, token2.text);
    }

    #[test]
    fn test_unicode_identifiers() {
        let source_code = "fn mαin() { let 变量 = 42; }";
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(
            "unicode.rs".to_string(),
            source_code.to_string(),
            Path::new("unicode.rs"),
        );
        let source_file = source_map.file(file_id).unwrap();
        let mut lexer = Lexer::new(source_file, file_id);
        
        // 应该能正常解析 Unicode 标识符
        let token = lexer.advance_token().unwrap(); // fn
        assert_eq!(token.kind, TokenKind::Fn);
        
        let token = lexer.advance_token().unwrap(); // mαin
        assert_eq!(token.kind, TokenKind::Ident);
        assert_eq!(token.text, "mαin");
        
        let token = lexer.advance_token().unwrap(); // (
        assert_eq!(token.kind, TokenKind::OpenParen);
        
        let token = lexer.advance_token().unwrap(); // )
        assert_eq!(token.kind, TokenKind::CloseParen);
        
        let token = lexer.advance_token().unwrap(); // {
        assert_eq!(token.kind, TokenKind::OpenBrace);
        
        let token = lexer.advance_token().unwrap(); // let
        assert_eq!(token.kind, TokenKind::Let);
        
        let token = lexer.advance_token().unwrap(); // 变量
        assert_eq!(token.kind, TokenKind::Ident);
        assert_eq!(token.text, "变量");
    }
}