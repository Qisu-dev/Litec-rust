use litec_error::{Diag, ErrorGuaranteed, diag_ctxt::DiagCtxt};
use litec_span::SourceMap;
use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};

#[derive(Debug)]
pub struct Session {
    source_map: Rc<RefCell<SourceMap>>,
    diag_ctxt: DiagCtxt,
}

impl Session {
    pub fn new(source_map: SourceMap) -> Self {
        let source_map = Rc::new(RefCell::new(source_map));
        let diag_ctxt = DiagCtxt::new(source_map.clone());
        Self {
            source_map,
            diag_ctxt,
        }
    }

    pub fn report(&self, diag: Diag) -> Option<ErrorGuaranteed> {
        self.diag_ctxt.emit(diag)
    }

    pub fn report_err(&self, diag: Diag) -> ErrorGuaranteed {
        self.diag_ctxt.emit_err(diag)
    }

    pub fn diag_ctxt(&self) -> &DiagCtxt {
        &self.diag_ctxt
    }

    /// 获取源代码映射（只读）
    pub fn source_map(&self) -> Ref<'_, SourceMap> {
        self.source_map.borrow()
    }

    pub fn mut_source_map(&self) -> RefMut<'_, SourceMap> {
        self.source_map.borrow_mut()
    }

    pub fn error_count(&self) -> usize {
        self.diag_ctxt.diags_count()
    }
}
