//! PDF Document module - document structure, pages, and security.
//!
//! This module contains:
//! - `catalog` - PDF document parsing, xref tables, object resolution (PDFDocument)
//! - `page` - PDF page iteration and attributes (PDFPage)
//! - `security` - PDF encryption/decryption handlers
//! - `saslprep` - RFC 4013 SASLprep for password normalization

pub mod catalog;
pub mod page;
pub mod saslprep;
pub mod security;

// Re-export main types for convenience
pub use catalog::{PDFDocument, PageLabels, PdfBytes};
pub use page::{PDFPage, PageIterator};
pub use saslprep::saslprep;
pub use security::{
    PASSWORD_PADDING, PDFSecurityHandler, PDFStandardSecurityHandlerV2,
    PDFStandardSecurityHandlerV4, PDFStandardSecurityHandlerV5, create_security_handler,
};
