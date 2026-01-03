//! High-level API module for PDF text extraction.
//!
//! This module provides the main public API for PDF text extraction.
//!
//! # Builder API
//!
//! For a fluent API, use [`ExtractorBuilder`]:
//!
//! ```ignore
//! use bolivar_core::api::ExtractorBuilder;
//!
//! let text = ExtractorBuilder::new("document.pdf")
//!     .password("secret")
//!     .pages(0..5)
//!     .parallel(4)
//!     .extract_text()?;
//! ```
//!
//! # Direct API
//!
//! For direct function calls, use [`extract_text`] and [`extract_pages`]:
//!
//! ```ignore
//! use bolivar_core::api::{extract_text, ExtractOptions};
//!
//! let pdf_bytes = std::fs::read("document.pdf")?;
//! let text = extract_text(&pdf_bytes, None)?;
//! ```

pub mod builder;
pub mod high_level;

// Re-export builder types
pub use builder::{ExtractorBuilder, ExtractorBuilderFromBytes};

// Re-export for convenience
pub use high_level::{
    ExtractOptions, PageIterator, extract_pages, extract_pages_with_document, extract_text,
    extract_text_to_fp, extract_text_with_document,
};
