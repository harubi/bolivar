//! PDF content stream interpretation and device output.
//!
//! This module contains:
//! - `interpreter`: PDF content stream parser and page interpreter
//! - `types`: helper types shared between the interpreter and the `PDFDevice` trait
//! - `extractor`: `TagExtractor` device (interpreter-side device impl)
//! - `ops`: Operator implementations by category

pub mod extractor;
pub mod interpreter;
pub mod ops;
pub mod types;

// Re-export main types for convenience.
//
// `PDFDevice` and `PDFTextDevice` themselves now live at `crate::device`; they
// are re-exported here to keep `bolivar_core::interp::PDFDevice` working for
// external callers until D3 migrates them.
pub use crate::device::{PDFDevice, PDFTextDevice};
pub use extractor::TagExtractor;
pub use interpreter::{
    ContentToken, FontId, PDFContentParser, PDFPageInterpreter, PDFResourceManager,
};
pub use types::{PDFFontLike, PDFStackT, PDFStackValue, PDFTextSeq, PDFTextSeqItem, PathSegment};
