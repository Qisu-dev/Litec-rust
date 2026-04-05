use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet, renderer::DecorStyle};
use litec_span::{SourceMap, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextLines {
    Compact,  // 上下各1行
    Standard, // 上下各3行
    Full,     // 无限制
    Limited(usize),
}

impl Default for ContextLines {
    fn default() -> Self {
        ContextLines::Standard
    }
}

impl ContextLines {
    fn context_size(&self) -> usize {
        match self {
            ContextLines::Compact => 1,
            ContextLines::Standard => 3,
            ContextLines::Full => usize::MAX,
            ContextLines::Limited(n) => *n,
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
    pub fn to_annotate_level(&'_ self) -> Level<'_> {
        match self {
            DiagnosticLevel::Error => Level::ERROR,
            DiagnosticLevel::Warning => Level::WARNING,
            DiagnosticLevel::Note => Level::NOTE,
            DiagnosticLevel::Help => Level::HELP,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct SlicedSource {
    pub content: String,
    pub start_line: usize,
    pub base_offset: usize,
    pub path: String,
}

/// 折叠策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldEmptyLines {
    /// 不折叠，保留所有空行
    Keep,
    /// 折叠连续空行，保留指定数量的空行作为上下文
    Fold {
        /// 保留的错误行附近的空行数（单侧）
        context_empty: usize,
        /// 折叠后的标记（默认 "..."）
        marker: &'static str,
    },
    /// 删除所有空行
    Remove,
}

impl Default for FoldEmptyLines {
    fn default() -> Self {
        // 默认：保留错误行上下各1个空行，其他折叠
        FoldEmptyLines::Fold {
            context_empty: 1,
            marker: "...",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub code: Option<String>,
    pub span: Span,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
    pub context_lines: ContextLines,
    pub fold_empty: FoldEmptyLines, // 新增：空行折叠策略
    sliced_sources: Vec<SlicedSource>,
}

impl Diagnostic {
    pub fn new(level: DiagnosticLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            code: None,
            span: Span::default(),
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
            context_lines: ContextLines::Compact,
            fold_empty: FoldEmptyLines::default(),
            sliced_sources: Vec::new(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(DiagnosticLevel::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(DiagnosticLevel::Warning, message)
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    pub fn with_context_lines(mut self, context: ContextLines) -> Self {
        self.context_lines = context;
        self
    }

    /// 设置空行折叠策略
    pub fn with_fold_empty(mut self, fold: FoldEmptyLines) -> Self {
        self.fold_empty = fold;
        self
    }

    /// 预提取源代码切片
    fn prepare(&mut self, source_map: &SourceMap) -> bool {
        self.sliced_sources.clear();

        let Some(source_file) = source_map.file(self.span.file) else {
            return false;
        };
        let Some(path_str) = source_file.path.to_str() else {
            return false;
        };

        let start_lc = source_file.offset_to_linecol(self.span.start.offset);
        let end_lc = source_file.offset_to_linecol(self.span.end.offset);

        let error_start_line = start_lc.line;
        let error_end_line = end_lc.line;

        let total_lines = if source_file.line_breaks.len() > 1 {
            source_file.line_breaks.len() - 1
        } else {
            1
        };

        let context = self.context_lines.context_size();
        let display_start = error_start_line.saturating_sub(context);
        let display_end = (error_end_line + context).min(total_lines.saturating_sub(1));

        // 根据折叠策略提取源代码
        let main_sliced = match self.fold_empty {
            FoldEmptyLines::Keep => {
                extract_lines_preserve_empty(&source_file.source, display_start, display_end)
            }
            FoldEmptyLines::Fold {
                context_empty,
                marker,
            } => extract_lines_fold_empty(
                &source_file.source,
                display_start,
                display_end,
                error_start_line,
                error_end_line,
                context_empty,
                marker,
            ),
            FoldEmptyLines::Remove => {
                extract_lines_remove_empty(&source_file.source, display_start, display_end)
            }
        };

        let main_base_offset = source_file.line_breaks[display_start];

        self.sliced_sources.push(SlicedSource {
            content: main_sliced,
            start_line: display_start,
            base_offset: main_base_offset,
            path: path_str.to_string(),
        });

        // 处理跨文件的 labels
        for label in &self.labels {
            if label.span.file != self.span.file {
                if let Some(label_file) = source_map.file(label.span.file) {
                    if let Some(label_path) = label_file.path.to_str() {
                        let label_start = label_file.offset_to_linecol(label.span.start.offset);
                        let label_end = label_file.offset_to_linecol(label.span.end.offset);

                        let label_display_start = label_start.line.saturating_sub(context);
                        let label_display_end = (label_end.line + context)
                            .min(label_file.line_breaks.len().saturating_sub(2));

                        let label_slice = match self.fold_empty {
                            FoldEmptyLines::Keep => extract_lines_preserve_empty(
                                &label_file.source,
                                label_display_start,
                                label_display_end,
                            ),
                            FoldEmptyLines::Fold {
                                context_empty,
                                marker,
                            } => extract_lines_fold_empty(
                                &label_file.source,
                                label_display_start,
                                label_display_end,
                                label_start.line,
                                label_end.line,
                                context_empty,
                                marker,
                            ),
                            FoldEmptyLines::Remove => extract_lines_remove_empty(
                                &label_file.source,
                                label_display_start,
                                label_display_end,
                            ),
                        };

                        let label_base = label_file.line_breaks[label_display_start];

                        self.sliced_sources.push(SlicedSource {
                            content: label_slice,
                            start_line: label_display_start,
                            base_offset: label_base,
                            path: label_path.to_string(),
                        });
                    }
                }
            }
        }

        true
    }

    /// 构建 Report
    fn build_report(&self) -> Option<Vec<Group<'_>>> {
        if self.sliced_sources.is_empty() {
            return None;
        }

        let main_source = &self.sliced_sources[0];

        let adjusted_start = self
            .span
            .start
            .offset
            .saturating_sub(main_source.base_offset);
        let adjusted_end = self.span.end.offset.saturating_sub(main_source.base_offset);

        let content_len = main_source.content.len();
        let adjusted_start = adjusted_start.min(content_len);
        let adjusted_end = adjusted_end.min(content_len);

        let (adjusted_start, adjusted_end) = if adjusted_start > adjusted_end {
            (adjusted_end, adjusted_start)
        } else {
            (adjusted_start, adjusted_end)
        };

        let mut main_title = self.level.to_annotate_level().primary_title(&self.message);

        if let Some(code) = &self.code {
            main_title = main_title.id(code);
        }

        let line_start_display = main_source.start_line;

        let main_snippet = Snippet::source(&main_source.content)
            .path(&main_source.path)
            .line_start(line_start_display + 1)
            .fold(false)
            .annotation(
                AnnotationKind::Primary
                    .span(adjusted_start..adjusted_end)
                    .label(&self.message),
            );

        let mut main_group = Group::with_title(main_title).element(main_snippet);

        for (i, help) in self.help.iter().enumerate() {
            let msg = if self.help.len() > 1 {
                format!("help {}: {}", i + 1, help)
            } else {
                format!("help: {}", help)
            };
            main_group = main_group.element(Level::HELP.message(msg));
        }

        for (i, note) in self.notes.iter().enumerate() {
            let msg = if self.notes.len() > 1 {
                format!("note {}: {}", i + 1, note)
            } else {
                format!("note: {}", note)
            };
            main_group = main_group.element(Level::NOTE.message(msg));
        }

        let mut groups = vec![main_group];
        let mut source_idx = 1;

        for label in &self.labels {
            if label.span.file != self.span.file {
                if source_idx < self.sliced_sources.len() {
                    let label_source = &self.sliced_sources[source_idx];
                    let label_adjusted_start = label
                        .span
                        .start
                        .offset
                        .saturating_sub(label_source.base_offset);
                    let label_adjusted_end = label
                        .span
                        .end
                        .offset
                        .saturating_sub(label_source.base_offset);

                    let content_len = label_source.content.len();
                    let label_adjusted_start = label_adjusted_start.min(content_len);
                    let label_adjusted_end = label_adjusted_end.min(content_len);

                    let label_title = Level::NOTE.secondary_title(&label.message);
                    let label_snippet = Snippet::source(&label_source.content)
                        .path(&label_source.path)
                        .line_start(label_source.start_line)
                        .annotation(
                            AnnotationKind::Context
                                .span(label_adjusted_start..label_adjusted_end)
                                .label(&label.message),
                        );

                    let label_group = Group::with_title(label_title).element(label_snippet);
                    groups.push(label_group);
                    source_idx += 1;
                }
            } else {
                let label_adjusted_start = label
                    .span
                    .start
                    .offset
                    .saturating_sub(main_source.base_offset);
                let label_adjusted_end = label
                    .span
                    .end
                    .offset
                    .saturating_sub(main_source.base_offset);

                let content_len = main_source.content.len();
                let label_adjusted_start = label_adjusted_start.min(content_len);
                let label_adjusted_end = label_adjusted_end.min(content_len);

                let label_title = Level::NOTE.secondary_title(&label.message);
                let label_snippet = Snippet::source(&main_source.content)
                    .path(&main_source.path)
                    .line_start(main_source.start_line)
                    .annotation(
                        AnnotationKind::Context
                            .span(label_adjusted_start..label_adjusted_end)
                            .label(&label.message),
                    );

                let label_group = Group::with_title(label_title).element(label_snippet);
                groups.push(label_group);
            }
        }

        Some(groups)
    }

    pub fn render(mut self, source_map: &SourceMap) -> String {
        if !self.prepare(source_map) {
            return self.fallback_render();
        }

        match self.build_report() {
            Some(groups) => {
                let renderer = Renderer::styled()
                    .anonymized_line_numbers(false)
                    .decor_style(DecorStyle::Unicode);
                renderer.render(&groups)
            }
            None => self.fallback_render(),
        }
    }

    pub fn render_to_string(&self, source_map: &SourceMap) -> String {
        self.clone().render(source_map)
    }

    fn fallback_render(&self) -> String {
        let level_str = match self.level {
            DiagnosticLevel::Error => "error",
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Note => "note",
            DiagnosticLevel::Help => "help",
        };
        let code_str = self
            .code
            .as_ref()
            .map(|c| format!("[{}] ", c))
            .unwrap_or_default();
        format!("{}: {}{}", level_str, code_str, self.message)
    }
}

/// 保留所有空行（原始实现）
fn extract_lines_preserve_empty(source: &str, start_line: usize, end_line: usize) -> String {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let end_line = end_line.min(lines.len().saturating_sub(1));

    if start_line >= lines.len() {
        return String::new();
    }

    lines[start_line..=end_line].concat()
}

/// 折叠空行：保留错误行附近的空行，其他空行替换为 "..."
/// - 保留错误行上下 `context_empty` 行的空行
/// - 连续空行超过限制时，折叠为 "..."
fn extract_lines_fold_empty(
    source: &str,
    start_line: usize,
    end_line: usize,
    error_start_line: usize,
    error_end_line: usize,
    context_empty: usize,
    marker: &str,
) -> String {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let end_line = end_line.min(lines.len().saturating_sub(1));

    if start_line >= lines.len() {
        return String::new();
    }

    let mut result = String::new();
    let mut consecutive_empty = 0;
    let mut last_was_folded = false;

    for (idx, line) in lines[start_line..=end_line].iter().enumerate() {
        let absolute_line = start_line + idx;
        let is_empty = line.trim().is_empty();

        // 判断是否在错误行的上下文范围内
        let in_error_context = absolute_line + context_empty >= error_start_line
            && absolute_line <= error_end_line + context_empty;

        if is_empty {
            if in_error_context {
                // 在错误上下文内，保留空行
                result.push_str(line);
                consecutive_empty = 0;
                last_was_folded = false;
            } else {
                // 不在错误上下文内，计数连续空行
                consecutive_empty += 1;

                // 第一个空行显示折叠标记
                if consecutive_empty == 1 && !last_was_folded {
                    result.push_str(marker);
                    result.push('\n');
                    last_was_folded = true;
                }
                // 后续空行完全跳过（不添加到结果）
            }
        } else {
            // 非空行，重置计数
            consecutive_empty = 0;
            last_was_folded = false;
            result.push_str(line);
        }
    }

    result
}

/// 删除所有空行
fn extract_lines_remove_empty(source: &str, start_line: usize, end_line: usize) -> String {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let end_line = end_line.min(lines.len().saturating_sub(1));

    if start_line >= lines.len() {
        return String::new();
    }

    lines[start_line..=end_line]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .concat()
}

/// 便捷构建器
pub struct DiagnosticBuilder(Diagnostic);

impl DiagnosticBuilder {
    pub fn new(level: DiagnosticLevel, message: impl Into<String>) -> Self {
        Self(Diagnostic::new(level, message))
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

    pub fn with_context_lines(mut self, context: ContextLines) -> Self {
        self.0.context_lines = context;
        self
    }

    pub fn with_fold_empty(mut self, fold: FoldEmptyLines) -> Self {
        self.0.fold_empty = fold;
        self
    }

    pub fn build(self) -> Diagnostic {
        self.0
    }

    pub fn render(self, source_map: &SourceMap) -> String {
        self.0.render(source_map)
    }
}

impl From<DiagnosticBuilder> for Diagnostic {
    fn from(value: DiagnosticBuilder) -> Self {
        value.build()
    }
}

// 便捷函数
pub fn error(message: impl Into<String>) -> DiagnosticBuilder {
    DiagnosticBuilder::error(message)
}

pub fn warning(message: impl Into<String>) -> DiagnosticBuilder {
    DiagnosticBuilder::warning(message)
}