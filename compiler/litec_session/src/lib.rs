use std::cell::{Ref, RefCell};

use litec_error::Diagnostic;
use litec_span::SourceMap;

/// 编译会话
#[derive(Debug, Clone)]
pub struct Session {
    /// 源文件映射
    pub source_map: RefCell<SourceMap>,
    pub diagnostics: RefCell<Vec<Diagnostic>>,
}

impl Session {
    pub fn new(source_map: SourceMap) -> Self {
        Self {
            source_map: source_map.into(),
            diagnostics: Vec::new().into(),
        }
    }

    /// 报告一个诊断
    pub fn report(&self, diagnostic: Diagnostic) {
        self.diagnostics.borrow_mut().push(diagnostic.clone());
    }

    /// 获取所有诊断（用于最后输出统计）
    pub fn diagnostics(&self) -> Ref<'_, Vec<Diagnostic>> {
        self.diagnostics.borrow()
    }

    pub fn print_diagnotics(&self) {
        for diagnostic in self.diagnostics.borrow().clone() {
            println!("{}", diagnostic.render(&self.source_map.borrow()));
        }
    }

    pub fn borrow_source_map(&self) -> Ref<'_, SourceMap> {
        self.source_map.borrow()
    }
}
