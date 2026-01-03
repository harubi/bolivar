//! Font handling modules for PDF text extraction.
//!
//! This module contains font-related functionality including:
//! - CMap (character mapping) support
//! - Encoding databases
//! - Font metrics for Standard 14 fonts
//! - TrueType font parsing
//! - Latin encoding tables

pub mod cmap;
pub mod encoding;
pub mod latin_enc;
pub mod metrics;
pub mod pdffont;
pub mod truetype;

// Re-export main types for convenience
pub use cmap::{CMap, CMapBase, CMapDB, IdentityCMap, UnicodeMap};
pub use encoding::EncodingDB;
pub use metrics::FONT_METRICS;
pub use pdffont::{PDFCIDFont, PDFFont};
pub use truetype::TrueTypeFont;
