use litec_span::SourceMap;
use std::{
    cell::RefCell,
    rc::Rc,
};

use crate::{Diag, DiagLevel, ErrorGuaranteed};

#[derive(Debug, Clone)]
pub struct DiagCtxt {
    source_map: Rc<RefCell<SourceMap>>,
    diags: RefCell<Vec<Diag>>,
}

impl DiagCtxt {
    pub fn new(source_map: Rc<RefCell<SourceMap>>) -> Self {
        Self {
            source_map,
            diags: RefCell::new(Vec::new()),
        }
    }

    pub fn emit(&self, diag: Diag) -> Option<ErrorGuaranteed> {
        let level = diag.level;
        self.diags.borrow_mut().push(diag);
        if level == DiagLevel::Error {
            Some(ErrorGuaranteed::new())
        } else {
            None
        }
    }

    /// 发射一个错误诊断，直接返回 ErrorGuaranteed
    pub fn emit_err(&self, diag: Diag) -> ErrorGuaranteed {
        debug_assert!(
            diag.level == DiagLevel::Error,
            "emit_err called with non-error diagnostic"
        );
        self.emit(diag).unwrap() // 因为检查了级别，unwrap 安全
    }

    pub fn diags_count(&self) -> usize {
        self.diags.borrow().len()
    }

    pub fn take_diags(&self) -> Vec<Diag> {
        self.diags.take()
    }

    pub fn truncate(&self, idx: usize) {
        self.diags.borrow_mut().truncate(idx);
    }

    pub fn flush(&self) {
        let diags = self.diags.borrow_mut();
        let source_map = self.source_map.borrow();
        for diag in diags.clone().into_iter() {
            eprintln!("{}", diag.render(&source_map));
        }
    }
}

impl Drop for DiagCtxt {
    fn drop(&mut self) {
        let source_map = self.source_map.borrow();
        for diag in self.diags.take() {
            eprintln!("{}", diag.render(&source_map))
        }
    }
}
