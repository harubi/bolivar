//! High-level API module for PDF text extraction.
//!
//! This module provides the main public API for PDF text extraction.
//!
//! # Example
//!
//! ```ignore
//! use bolivar_core::api::{extract_text, ExtractOptions};
//!
//! let pdf_bytes = std::fs::read("document.pdf")?;
//! let text = extract_text(&pdf_bytes, None)?;
//! ```

pub mod high_level;
pub mod stream;

// Re-export for convenience
pub use high_level::{
    ExtractOptions, PageIterator, extract_pages, extract_pages_stream, extract_pages_with_document,
    extract_text, extract_text_to_fp, extract_text_with_document,
};
pub use stream::PageStream;
