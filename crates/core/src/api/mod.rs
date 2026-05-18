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
pub mod pipeline;
pub mod stream;

// Re-export for convenience
pub use high_level::{
    ExtractOptions, extract_pages_stream, extract_pages_with_images_with_document, extract_text,
    extract_text_to_fp, extract_text_with_document,
};
pub use pipeline::{Stream, no_precheck, run_batch, run_stream};
pub use stream::{
    extract_pages_stream_from_doc, extract_tables_stream_from_doc,
    extract_tables_stream_from_doc_with_geometries, extract_tables_stream_from_doc_with_settings,
    extract_text_stream_from_doc_with_geometries, extract_words_stream_from_doc_with_geometries,
};
