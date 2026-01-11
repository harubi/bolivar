//! Output Converters for PDF Content - port of pdfminer.six converter.py
//!
//! Provides converters for transforming PDF layout content into various output formats:
//! - PDFLayoutAnalyzer: Base device that creates layout objects from PDF content
//! - PDFPageAggregator: Collects analyzed pages for later retrieval
//! - TextConverter: Plain text output
//! - HTMLConverter: HTML output with positioning
//! - HOCRConverter: hOCR format output
//! - XMLConverter: XML output with full structure

mod base;
mod html;
mod text;
mod xml;

// Re-export all public items for backwards compatibility
pub use base::{
    LTContainer, PDFConverter, PDFEdgeProbe, PDFLayoutAnalyzer, PDFPageAggregator, PathOp,
};
pub use html::{HOCRConverter, HTMLConverter};
pub use text::TextConverter;
pub use xml::XMLConverter;
