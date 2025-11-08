use litec_span::Span;
use litec_typed_hir::{ty::Ty, DefKind};
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum TypeError {
    #[error("undefine variable name {name} at {span:?}")]
    UndefineSymbol {
        name: String,
        span: Span
    },
    
    #[error("exprected type but found variable, name {name} at {span:?}")]
    ExpectedTypeButFoundVariable {
        name: String,
        span: Span
    },

    #[error("exprected type but found function, name {name} at {span:?}")]
    ExpectedTypeButFoundFunction {
        name: String,
        span: Span
    },

    #[error("redefine variable name {name} at {span:?}")]
    RedefineVariable {
        name: String,
        span: Span
    },

    #[error("type mismatch {t1:?} {t2:?}")]
    TypeMismatch {
        t1: Ty,
        t2: Ty,
    },

    #[error("binary operand type error {ty:?} at {span:?}")]
    BinaryOperandError {
        ty: Ty,
        span: Span
    },

    #[error("unknow type as type, kind {kind:?} at {span:?}")]
    UnknowTypeAsType {
        kind: DefKind,
        span: Span
    },

    #[error("expected bool, but found {left:?} and {right:?}")]
    ExpectedBoolButFoundTwo {
        left: Ty,
        right: Ty
    },
    #[error("expected bool, but found {operand_ty:?}")]
    ExpectedBoolButFound {
        operand_ty: Ty
    },

    #[error("expected function but found {ty:?} at {span:?}")]
    ExpectedFunctionButFound {
        ty: Ty,
        span: Span
    },

    #[error("arguments length is not equal, expected {expected_length} but found {really_length}")]
    ArgumentLengthNotEqual {
        expected_length: usize,
        really_length: usize
    },

    #[error("undefine field {base}::{field}")]
    UndefineField {
        base: String,
        field: String
    },

    #[error("undefine field {path}")]
    UndefinePath {
        path: String
    },

    #[error("{error}")]
    Error {
        error: litec_error::Error
    }
}