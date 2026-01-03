//! PDF content stream interpretation and device output.
//!
//! This module contains:
//! - `interpreter`: PDF content stream parser and page interpreter
//! - `device`: Output device traits and implementations

pub mod device;
pub mod interpreter;

// Re-export main types for convenience
pub use device::{
    PDFDevice, PDFFontLike, PDFStackT, PDFStackValue, PDFTextDevice, PDFTextSeq, PDFTextSeqItem,
    PathSegment, TagExtractor,
};
pub use interpreter::{
    ContentToken, FontId, PDFContentParser, PDFPageInterpreter, PDFResourceManager,
};
