use std::{
    cell::RefCell, ops::Range, path::Path, rc::Rc, sync::Once
};

use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringId(pub usize);

impl From::<&str> for StringId {
    fn from(value: &str) -> Self {
        intern_global(value)
    }
}

impl std::fmt::Display for StringId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", get_global_string(*self).unwrap())
    }
}

#[derive(Debug, Default)]
pub struct StringPool {
    pub strings: RefCell<Vec<Rc<str>>>,
    pub index_map: RefCell<FxHashMap<Rc<str>, StringId>>,
}

impl StringPool {
    /// 创建新的空字符串池
    pub fn new() -> Self {
        Self {
            strings: Default::default(),
            index_map: Default::default()
        }
    }
    
    /// 添加字符串到池中，返回其ID
    /// 如果字符串已存在，返回现有ID
    #[inline]
    pub fn intern(&self, s: &str) -> StringId {
        let s = Rc::from(s);
        // 检查是否已存在
        if let Some(&id) = self.index_map.borrow().get(&s) {
            return id;
        }
        
        // 创建新条目
        let id = StringId(self.strings.borrow().len());
        self.index_map.borrow_mut().insert(s.clone(), id);
        self.strings.borrow_mut().push(s);
        id
    }
    
    /// 根据ID获取字符串的只读引用
    #[inline]
    pub fn get(&self, id: StringId) -> Option<Rc<str>> {
        self.strings.borrow().get(id.0).map(|v| v).cloned()
    }
    
    /// 检查字符串是否已在池中
    #[inline]
    pub fn contains(&self, s: &str) -> bool {
        self.index_map.borrow().contains_key(s)
    }
    
    /// 获取池中字符串数量
    #[inline]
    pub fn len(&self) -> usize {
        self.strings.borrow().len()
    }
    
    /// 检查池是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// 全局字符串池

static INIT: Once = Once::new();
static mut GLOBAL_STRING_POOL: *mut StringPool = std::ptr::null_mut();

#[inline]
pub fn get_global_string_pool() -> &'static mut StringPool {
    unsafe {
        INIT.call_once(|| {
            let boxed = Box::new(StringPool::default());
            GLOBAL_STRING_POOL = Box::into_raw(boxed);
        });
        &mut *GLOBAL_STRING_POOL
    }
}

#[inline]
pub fn intern_global(s: &str) -> StringId {
    get_global_string_pool().intern(s)
}

#[inline]
pub fn get_global_string(id: StringId) -> Option<Rc<str>> {
    get_global_string_pool().get(id)
}

#[inline]
pub fn contains_global(s: &str) -> bool {
    get_global_string_pool().contains(s)
}

#[inline]
pub fn global_pool_len() -> usize {
    get_global_string_pool().len()
}

// 在 litec_span 中确保 Span 包含行列信息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Location,
    pub end: Location,
    pub file: FileId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct LineCol {
    pub line: usize,   // 0-based
    pub column: usize, // 0-based（字符索引）
}

impl Default for Span {
    fn default() -> Self {
        Self {
            start: Location { line: 0, column: 0, offset: 0 },
            end: Location { line: 0, column: 0, offset: 0 },
            file: FileId(0),
        }
    }
}

impl Span {
    pub fn new(start: Location, end: Location, file_id: FileId) -> Self {
        assert!(start <= end, "Span start must <= end");
        Self { start, end, file: file_id }
    }
    
    pub fn start(&self) -> Location {
        self.start
    }

    pub fn end(&self) -> Location {
        self.end
    }
    
    pub fn len(&self) -> usize {
        self.end.offset - self.start.offset
    }
    
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    
    pub fn extend_to(&self, other: Span) -> Self {
        assert!(self.file == other.file);
        let start = if self.start.offset > other.start.offset {
            self.start
        } else {
            other.start
        };
        let end = if self.end.offset > other.end.offset {
            self.end
        } else {
            other.end
        };
        Self::new(start, end, self.file)
    }
}

#[derive(Debug)]
pub struct SourceFile {
    pub name: String,
    pub source:  String,
    pub path: Box<Path>,
    pub line_breaks: Vec<usize>,
}

impl SourceFile {
    pub fn new(name: String, src: String, path: &Path) -> Self {
        let mut breaks = vec![0];
        for (i, &b) in src.as_bytes().iter().enumerate() {
            if b == b'\n' {
                breaks.push(i + 1);
            }
        }
        if breaks.last() != Some(&src.len()) {
            breaks.push(src.len());
        }
        Self { name, source: src, path: path.into(), line_breaks: breaks }
    }

    pub fn len(&self) -> usize {
        self.source.len()
    }

    /// 字节偏移 → LineCol
    pub fn offset_to_linecol(&self, offset: usize) -> LineCol {
        let line = match self.line_breaks.binary_search(&offset) {
            Ok(l)  => l,
            Err(l) => l.saturating_sub(1),
        };
        let col = offset - self.line_breaks[line];
        LineCol { line, column: col }
    }

    pub fn location_from_offset(&self, offset: usize) -> Location {
        let line_col = self.offset_to_linecol(offset);
        Location { 
            line: line_col.line, 
            column: line_col.column, 
            offset: offset 
        }
    }

    /// 一次返回 (start, end) 的 LineCol
    #[inline]
    pub fn line_col_range(&self, start_offset: usize, end_offset: usize) -> (LineCol, LineCol) {
        (
            self.offset_to_linecol(start_offset),
            self.offset_to_linecol(end_offset),
        )
    }

    /// 取第 line 行文本（不带换行符）
    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_breaks[line];
        let end   = self.line_breaks[line + 1];
        &self.source[start..end].trim_end_matches(&['\r', '\n'][..])
    }

    /// 获取指定行的字节范围
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let start = self.line_breaks[line];
        let end = if line + 1 < self.line_breaks.len() {
            self.line_breaks[line + 1]
        } else {
            self.source.len()
        };
        start..end
    }

    pub fn get_text_from_span(&self, span: Span) -> &str {
        &self.source[span.start.offset..span.end.offset]
    }

    /// 获取源文件文本
    pub fn source_text(&self) -> &str {
        &self.source
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub usize);

#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self {
            files: Vec::new()
        }
    }
    /// 加文件，返回 FileId
    pub fn add_file(&mut self, name: String, src: String, path: &Path) -> FileId {
        let id = FileId(self.files.len());
        self.files.push(SourceFile::new(name, src, path));
        id
    }

    /// 取文件
    pub fn file(&self, id: FileId) -> Option<&SourceFile> {
        if id.0 >= self.files.len() {
            None
        } else {
            Some(&self.files[id.0])
        }
    }

    /// 返回 (start_lc, end_lc)
    pub fn line_col(&self, span: Span) -> (LineCol, LineCol) {
        let f = self.file(span.file);
        f.unwrap().line_col_range(span.start.offset, span.end.offset)
    }

    /// 取一行文本
    pub fn line_text(&self, file: FileId, line: usize) -> &str {
        self.file(file).unwrap().line_text(line)
    }

    /// 获取源文件文本
    pub fn source_text(&self, file_id: FileId) -> &str {
        &self.files[file_id.0].source
    }

    /// 获取指定行的字节范围
    pub fn line_range(&self, file_id: FileId, line: usize) -> Range<usize> {
        self.files[file_id.0].line_range(line)
    }
}