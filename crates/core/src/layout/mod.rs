//! Layout analysis module for PDF text extraction.
//!
//! This module contains:
//! - Layout analysis types (LTPage, LTChar, LTTextLine, LTTextBox, etc.)
//! - Layout analysis parameters (LAParams)
//! - Grouping and clustering algorithms
//! - Table extraction functionality

pub mod analysis;
pub mod elements;
pub mod params;
pub mod table;

// Re-export params
pub use params::*;

// Re-export element types for backwards compatibility
pub use elements::*;

// Re-export analysis types and functions
pub use analysis::*;

// Re-export table types for backwards compatibility
pub use table::*;
