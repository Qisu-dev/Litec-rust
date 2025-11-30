use std::borrow::Cow;
use annotate_snippets::{renderer::DecorStyle, AnnotationKind, Group, Level, Renderer, Snippet};
use litec_span::{SourceMap, Span};

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub code: Option<String>,
    pub span: Span,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
}

impl Diagnostic {
    pub fn to_group<'a>(&'a self, source_map: &'a SourceMap) -> Option<Group<'a>> {
        let source_file = source_map.file(self.span.file)?;

        let (slice_source, line_start) = self.extract_source_lines(source_file, self.span.start.line, self.span.end.line)?;

        let relative_start = self.calculate_relative_offset(source_file, self.span.start.line, self.span.start.offset);
        let relative_end = self.calculate_relative_offset(source_file, self.span.start.line, self.span.end.offset);

        // 创建主要的代码片段
        let snippet = Snippet::source(slice_source)
            .path(source_file.path.to_str()?)
            .line_start(line_start + 1)
            .annotation(
                AnnotationKind::Primary
                    .span(relative_start..relative_end)
                    .label(&self.message)
            );
        
        // 创建主要的 Group
        let mut group = self.level.to_annotate_level().primary_title(&self.message);
        
        if let Some(code) = &self.code {
            group = group.id(code);
        }

        // 添加主要的代码片段
        let mut group = group.element(snippet);

         // 添加帮助信息 - 改进：提供更结构化的格式
        for (i, help) in self.help.iter().enumerate() {
            let formatted_help = if self.help.len() > 1 {
                format!("帮助 {}: {}", i + 1, help)
            } else {
                help.clone()
            };
            
            let message = Level::HELP.message(formatted_help);
            group = group.element(message);
        }

        // 添加备注信息 - 改进：提供更结构化的格式
        for (i, note) in self.notes.iter().enumerate() {
            let formatted_note = if self.notes.len() > 1 {
                format!("备注 {}: {}", i + 1, note)
            } else {
                note.clone()
            };
            
            let message = Level::NOTE.message(formatted_note);
            group = group.element(message);
        }

        // 添加标签信息
        for label in &self.labels {
            if let Some(label_source_file) = source_map.file(label.span.file) {
                if let Some((label_slice_source, label_line_start)) = self.extract_source_lines(label_source_file, label.span.start.line, label.span.end.line) {
                    let label_relative_start = self.calculate_relative_offset(label_source_file, label.span.start.line, label.span.start.offset);
                    let label_relative_end = self.calculate_relative_offset(label_source_file, label.span.start.line, label.span.end.offset);

                    let label_snippet = Snippet::source(label_slice_source)
                        .path(label_source_file.path.to_str()?)
                        .line_start(label_line_start + 1)
                        .annotation(
                            AnnotationKind::Context
                                .span(label_relative_start..label_relative_end)
                                .label(&label.message)
                        );

                    group = group.element(label_snippet);
                }
            }
        }

        Some(group)
    }
    
    pub fn render(&self, source_map: &litec_span::SourceMap) -> String {
        match self.to_group(source_map) {
            Some(group) => {
                let renderer = Renderer::styled()
                    .anonymized_line_numbers(false)
                    .decor_style(DecorStyle::Unicode);

                renderer.render(&[group])
            }
            None => {
                // 回退到简单的错误消息
                let level_str = match self.level {
                    DiagnosticLevel::Error => "error",
                    DiagnosticLevel::Warning => "warning", 
                    DiagnosticLevel::Note => "note",
                    DiagnosticLevel::Help => "help",
                };

                let code_str = self.code.as_ref()
                    .map(|code| format!("[{}] ", code))
                    .unwrap_or_default();

                format!("{}: {}{}", level_str, code_str, self.message)
            }
        }
    }
    
    fn extract_source_lines(&self, source_file: &litec_span::SourceFile, start_line: usize, end_line: usize) -> Option<(Cow<'static, str>, usize)> {
        let mut lines = Vec::new();
        
        // 确保行号在有效范围内
        let max_line = source_file.line_breaks.len().saturating_sub(2);
        let start_line = start_line.min(max_line);
        let end_line = end_line.min(max_line);
        
        // 如果起始行大于结束行，交换它们
        let (start_line, end_line) = if start_line > end_line {
            (end_line, start_line)
        } else {
            (start_line, end_line)
        };

        for line_num in start_line..=end_line {
            if line_num >= source_file.line_breaks.len().saturating_sub(1) {
                break;
            }

            if let Ok(line_text) = std::panic::catch_unwind(|| source_file.line_text(line_num)) {
                lines.push(line_text);
            }
        }

        if lines.is_empty() {
            None
        } else {
            Some((Cow::Owned(lines.join("\n")), start_line))
        }
    }

    fn calculate_relative_offset(&self, source_file: &litec_span::SourceFile, line_number: usize, offset: usize) -> usize {
        if line_number < source_file.line_breaks.len() {
            let line_start_offset = source_file.line_breaks[line_number];
            if offset >= line_start_offset {
                offset.saturating_sub(line_start_offset)
            } else {
                0
            }
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
}

impl DiagnosticLevel {
    pub fn to_annotate_level<'a>(&self) -> Level<'a> {
        match self {
            DiagnosticLevel::Error => Level::ERROR,
            DiagnosticLevel::Warning => Level::WARNING,
            DiagnosticLevel::Note => Level::NOTE,
            DiagnosticLevel::Help => Level::HELP,
        }
    }
}

impl<'a> From<DiagnosticLevel> for Level<'a> {
    fn from(level: DiagnosticLevel) -> Self {
        level.to_annotate_level()
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Debug)]
pub struct DiagnosticBuilder(pub Diagnostic);

impl DiagnosticBuilder {
    pub fn new(level: DiagnosticLevel, message: impl Into<String>) -> Self {
        Self(Diagnostic {
            level,
            message: message.into(),
            code: None,
            span: Span::default(),
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        })
    }

    pub fn render(&self, source_map: &SourceMap) -> String {
        self.0.render(source_map)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(DiagnosticLevel::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(DiagnosticLevel::Warning, message)
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.0.code = Some(code.into());
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.0.span = span;
        self
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.0.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.0.notes.push(note.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.0.help.push(help.into());
        self
    }

    pub fn build(self) -> Diagnostic {
        self.0
    }
}

// 为方便使用，提供全局函数
pub fn error(message: impl Into<String>) -> DiagnosticBuilder {
    DiagnosticBuilder::error(message)
}

pub fn warning(message: impl Into<String>) -> DiagnosticBuilder {
    DiagnosticBuilder::warning(message)
}

// 国际化支持 - 预留接口
pub mod messages {
    // 目前先用中文，后续可以替换为国际化键值
    pub const UNTERMINATED_CHAR: &str = "未终止的字符字面量";
    pub const UNTERMINATED_STRING: &str = "未终止的字符串字面量";
    pub const UNEXPECTED_TOKEN: &str = "意外的标记";
    pub const EXPECTED_IDENTIFIER: &str = "期望标识符";
    pub const UNCLOSED_DELIMITER: &str = "未关闭的";
    
    #[inline]
    pub fn invalid_char(c: char) -> String {
        format!("非法字符 `{}`",c)
    }
}

// 便捷的错误创建函数
pub fn unterminated_char(span: Span) -> DiagnosticBuilder {
    error(messages::UNTERMINATED_CHAR)
        .with_span(span)
        .with_help("字符字面量应以单引号结束")
}

pub fn unterminated_string(span: Span) -> DiagnosticBuilder {
    error(messages::UNTERMINATED_STRING)
        .with_span(span)
        .with_help("字符串字面量应以双引号结束")
}

pub fn invalid_character(c: char, span: Span) -> DiagnosticBuilder {
    error(messages::invalid_char(c))
        .with_span(span)
        .with_help("此字符在当前上下文中不允许使用")
}

pub fn unexpected_token(expected: &str, found: &str, span: Span) -> DiagnosticBuilder {
    error(messages::UNEXPECTED_TOKEN)
        .with_span(span)
        .with_label(span, format!("期望: {}, 找到: {}", expected, found))
}

pub fn expected_identifier(found: &str, span: Span) -> DiagnosticBuilder {
    error(messages::EXPECTED_IDENTIFIER)
        .with_span(span)
        .with_label(span, format!("找到: {}", found))
}

pub fn unclosed_delimiter(delimiter: &str, span: Span) -> DiagnosticBuilder {
    error(format!("{}{}", messages::UNCLOSED_DELIMITER, delimiter))
        .with_span(span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::path::Path;

    // 创建测试用的源代码映射
    fn create_test_source_map() -> Arc<litec_span::SourceMap> {
        let mut source_map = litec_span::SourceMap::new();
        let source_code = r#"fn main() {
    let x = 5;
    let y = "hello;
    let z = @a;
}"#;
        
        source_map.add_file(
            "test.rs".to_string(),
            source_code.to_string(),
            Path::new("test.rs"),
        );
        
        Arc::new(source_map)
    }

    // 使用绝对偏移量创建测试用的 Span
    fn create_span_from_offsets(source_map: &litec_span::SourceMap, start_offset: usize, end_offset: usize) -> Span {
        let file_id = litec_span::FileId(0);
        let source_file = source_map.file(file_id).unwrap();
        
        // 计算对应的行和列
        let start_loc = source_file.location_from_offset(start_offset);
        let end_loc = source_file.location_from_offset(end_offset);
        
        Span::new(start_loc, end_loc, file_id)
    }

    #[test]
    fn test_basic_error_creation() {
        let source_map = create_test_source_map();
        
        // 创建一个简单的错误 - 指向 "hello 的位置 (绝对偏移量 32-38)
        let span = create_span_from_offsets(&source_map, 32, 38);
        let diagnostic = unterminated_string(span).build();
        
        // 检查诊断的基本属性
        assert_eq!(diagnostic.level, DiagnosticLevel::Error);
        assert_eq!(diagnostic.message, messages::UNTERMINATED_STRING);
        assert_eq!(diagnostic.help, vec!["字符串字面量应以双引号结束"]);
    }

    #[test]
    fn test_diagnostic_rendering() {
        let source_map = create_test_source_map();
        
        // 创建一个错误并渲染 - 指向 "hello 的位置
        let span = create_span_from_offsets(&source_map, 32, 38);
        let diagnostic = unterminated_string(span).build();
        let output = diagnostic.render(&source_map);
        
        // 检查输出包含预期的内容
        assert!(output.contains("未终止的字符串字面量"));
        assert!(output.contains("字符串字面量应以双引号结束"));
        assert!(output.contains("test.rs"));
    }

    #[test]
    fn test_diagnostic_with_code() {
        let source_map = create_test_source_map();
        
        let span = create_span_from_offsets(&source_map, 32, 38);
        let diagnostic = unterminated_string(span)
            .with_code("E123")
            .build();
        
        let output = diagnostic.render(&source_map);
        
        // 检查错误代码出现在输出中
        assert!(output.contains("E123"));
    }

    #[test]
    fn test_diagnostic_with_labels() {
        let source_map = create_test_source_map();
        
        let span = create_span_from_offsets(&source_map, 32, 38);
        let diagnostic = error("测试错误")
            .with_span(span)
            .with_label(span, "这是标签")
            .build();
        
        let output = diagnostic.render(&source_map);
        
        // 检查标签出现在输出中
        assert!(output.contains("测试错误"));
        assert!(output.contains("这是标签"));
    }

    #[test]
    fn test_diagnostic_with_notes_and_help() {
        let source_map = create_test_source_map();
        
        let span = create_span_from_offsets(&source_map, 32, 38);
        let diagnostic = error("测试错误")
            .with_span(span)
            .with_note("这是一个备注")
            .with_help("这是一个帮助信息")
            .build();
        
        let output = diagnostic.render(&source_map);
        
        // 检查备注和帮助信息出现在输出中
        assert!(output.contains("测试错误"));
        assert!(output.contains("这是一个备注"));
        assert!(output.contains("这是一个帮助信息"));
    }

    #[test]
    fn test_multiple_labels() {
        let source_map = create_test_source_map();
        
        let span1 = create_span_from_offsets(&source_map, 3, 7);   // 指向 main
        let span2 = create_span_from_offsets(&source_map, 32, 38); // 指向 "hello
        
        let diagnostic = error("多个标签测试")
            .with_span(span1)
            .with_label(span1, "第一个标签")
            .with_label(span2, "第二个标签")
            .build();
        
        let output = diagnostic.render(&source_map);
        
        // 检查两个标签都出现在输出中
        assert!(output.contains("多个标签测试"));
        assert!(output.contains("第一个标签"));
        assert!(output.contains("第二个标签"));
    }

    #[test]
    fn test_warning_level() {
        let source_map = create_test_source_map();
        
        let span = create_span_from_offsets(&source_map, 3, 7);
        let diagnostic = warning("这是一个警告")
            .with_span(span)
            .build();
        
        let output = diagnostic.render(&source_map);
        
        // 检查警告级别正确显示
        assert!(output.contains("warning") || output.contains("警告"));
        assert!(output.contains("这是一个警告"));
    }

    #[test]
    fn test_invalid_character_diagnostic() {
        let source_map = create_test_source_map();
        
        let span = create_span_from_offsets(&source_map, 45, 46); // 指向 'a
        let diagnostic = invalid_character('@', span).build();
        
        let output = diagnostic.render(&source_map);
        
        // 检查无效字符错误正确显示
        assert!(output.contains("非法字符"));
        assert!(output.contains("@"));
    }

    #[test]
    fn test_unexpected_token_diagnostic() {
        let source_map = create_test_source_map();
        
        let span = create_span_from_offsets(&source_map, 3, 7);
        let diagnostic = unexpected_token("identifier", "keyword", span).build();
        
        let output = diagnostic.render(&source_map);
        
        // 检查意外的标记错误正确显示
        assert!(output.contains("意外的标记"));
        assert!(output.contains("期望: identifier"));
        assert!(output.contains("找到: keyword"));
    }

    #[test]
    fn test_empty_diagnostic() {
        let source_map = create_test_source_map();
        
        // 创建一个没有源代码位置的诊断
        let diagnostic = error("没有位置信息的错误").build();
        
        let output = diagnostic.render(&source_map);
        
        // 检查即使没有位置信息也能渲染
        assert!(output.contains("没有位置信息的错误"));
        assert!(output.contains("error"));
    }

    #[test]
    fn test_diagnostic_builder_chain() {
        let source_map = create_test_source_map();
        
        let span = create_span_from_offsets(&source_map, 32, 38);
        
        // 测试构建器链式调用
        let diagnostic = error("链式调用测试")
            .with_code("E999")
            .with_span(span)
            .with_label(span, "链式标签")
            .with_note("链式备注")
            .with_help("链式帮助")
            .build();
        
        // 检查所有属性都正确设置
        assert_eq!(diagnostic.message, "链式调用测试");
        assert_eq!(diagnostic.code, Some("E999".to_string()));
        assert_eq!(diagnostic.labels.len(), 1);
        assert_eq!(diagnostic.notes.len(), 1);
        assert_eq!(diagnostic.help.len(), 1);
        
        let output = diagnostic.render(&source_map);
        assert!(output.contains("链式调用测试"));
        assert!(output.contains("E999"));
        assert!(output.contains("链式标签"));
        assert!(output.contains("链式备注"));
        assert!(output.contains("链式帮助"));
    }

    #[test]
    fn test_convenience_functions() {
        let source_map = create_test_source_map();
        
        // 测试各种便捷函数
        let span = create_span_from_offsets(&source_map, 45, 46);
        
        let diagnostic1 = unterminated_char(span).build();
        let diagnostic2 = expected_identifier("found_keyword", span).build();
        let diagnostic3 = unclosed_delimiter("分隔符", span).build();
        
        // 检查便捷函数创建的诊断具有正确的属性
        assert_eq!(diagnostic1.message, messages::UNTERMINATED_CHAR);
        assert_eq!(diagnostic2.message, messages::EXPECTED_IDENTIFIER);
        assert!(diagnostic3.message.contains("未关闭的分隔符"));
        
        // 检查渲染没有 panic
        let _ = diagnostic1.render(&source_map);
        let _ = diagnostic2.render(&source_map);
        let _ = diagnostic3.render(&source_map);
    }

    #[test]
    fn demonstrate_diagnostics() {
        use std::path::Path;

        // 创建测试源代码
        let source_code = r#"fn main() {
        let x = 'a;
        let a = @a;
        let y = "hello;
        let z = 42;

        if x == 10 {
            println!("x is 10");
        }

        let result = calculate(x, y);
    }"#;

        let mut source_map = litec_span::SourceMap::new();
        let file_id = source_map.add_file(
            "demo.rs".to_string(),
            source_code.to_string(),
            Path::new("demo.rs"),
        );

        // 分析源代码，确定关键位置的绝对偏移量
        let source_file = source_map.file(file_id).unwrap();
        let source_text = &source_file.source;
        
        // 找到关键位置的绝对偏移量
        let find_offset = |text: &str| -> Option<usize> {
            source_text.find(text)
        };

        println!("{}", "=".repeat(80));
        println!("🚀 诊断系统演示 (使用绝对偏移量)");
        println!("{}", "=".repeat(80));

        // 1. 未终止字符字面量 - 指向 'a; 的位置
        println!("\n📝 1. 未终止字符字面量:");
        if let Some(start) = find_offset("'a;") {
            let char_span = create_span_from_offsets(&source_map, start, start + 3);
            let diagnostic = unterminated_char(char_span).build();
            println!("{}", diagnostic.render(&source_map));
        }

        // 2. 未终止字符串字面量 - 指向 "hello; 的位置
        println!("\n📝 2. 未终止字符串字面量:");
        if let Some(start) = find_offset("\"hello;") {
            let string_span = create_span_from_offsets(&source_map, start, start + 7);
            let diagnostic = unterminated_string(string_span).build();
            println!("{}", diagnostic.render(&source_map));
        }

        // 3. 无效字符
        println!("\n📝 3. 无效字符:");
        if let Some(start) = find_offset("@a;") {
            let invalid_span = create_span_from_offsets(&source_map, start, start + 1);
            let diagnostic = invalid_character('@', invalid_span).build();
            println!("{}", diagnostic.render(&source_map));
        }

        // 4. 意外的标记 - 指向 if 的位置
        println!("\n📝 4. 意外的标记:");
        if let Some(start) = find_offset("if x") {
            let token_span = create_span_from_offsets(&source_map, start, start + 2);
            let diagnostic = unexpected_token("expression", "keyword", token_span).build();
            println!("{}", diagnostic.render(&source_map));
        }

        // 5. 期望标识符 - 指向 calculate 的位置
        println!("\n📝 5. 期望标识符:");
        if let Some(start) = find_offset("calculate") {
            let ident_span = create_span_from_offsets(&source_map, start, start + 9);
            let diagnostic = expected_identifier("function", ident_span).build();
            println!("{}", diagnostic.render(&source_map));
        }

        // 6. 未关闭分隔符
        println!("\n📝 6. 未关闭分隔符:");
        if let Some(start) = find_offset("'a;") {
            let delimiter_span = create_span_from_offsets(&source_map, start, start + 1);
            let diagnostic = unclosed_delimiter("单引号", delimiter_span).build();
            println!("{}", diagnostic.render(&source_map));
        }

        // 7. 复杂的错误，包含多个标签、备注和帮助
        println!("\n📝 7. 复杂错误（多标签、备注、帮助）:");
        if let Some(main_start) = find_offset("calculate") {
            let main_span = create_span_from_offsets(&source_map, main_start, main_start + 9);
            
            if let Some(arg1_start) = find_offset("x, y") {
                let arg1_span = create_span_from_offsets(&source_map, arg1_start, arg1_start + 1);
                let arg2_span = create_span_from_offsets(&source_map, arg1_start + 3, arg1_start + 4);

                let diagnostic = error("类型不匹配")
                    .with_code("E0308")
                    .with_span(main_span)
                    .with_label(arg1_span, "期望 i32，找到 char")
                    .with_label(arg2_span, "期望 &str，找到 String")
                    .with_note("函数签名: fn calculate(a: i32, b: &str) -> bool")
                    .with_help("考虑使用 x.to_string() 转换")
                    .with_help("或者修改函数签名以接受这些类型")
                    .build();

                println!("{}", diagnostic.render(&source_map));
            }
        }

        // 8. 警告示例 - 指向 42 的位置
        println!("\n📝 8. 警告示例:");
        if let Some(start) = find_offset("42") {
            let warn_span = create_span_from_offsets(&source_map, start, start + 2);
            let diagnostic = warning("未使用的变量")
                .with_code("W001")
                .with_span(warn_span)
                .with_help("考虑使用这个变量或添加 #[allow(unused)]")
                .build();

            println!("{}", diagnostic.render(&source_map));
        }

        println!("\n{}", "=".repeat(80));
        println!("🎉 演示结束！");
        println!("{}", "=".repeat(80));

        assert!(true);
    }

    // 添加边界测试
    #[test]
    fn test_edge_cases() {
        let source_map = create_test_source_map();
        let source_file = source_map.file(litec_span::FileId(0)).unwrap();
        let source_len = source_file.source.len();
        
        // 测试超出边界的偏移量
        let span = create_span_from_offsets(&source_map, source_len - 1, source_len);
        let diagnostic = error("边界测试").with_span(span).build();
        
        // 应该不会 panic
        let output = diagnostic.render(&source_map);
        assert!(output.contains("边界测试"));
        
        // 测试空范围
        let empty_span = create_span_from_offsets(&source_map, 10, 10);
        let diagnostic = error("空范围测试").with_span(empty_span).build();
        let output = diagnostic.render(&source_map);
        assert!(output.contains("空范围测试"));
    }
}