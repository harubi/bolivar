//! Layout analysis module for PDF text extraction.
//!
//! - `types` - Layout types (LTPage, LTChar, LTTextLine, LTTextBox, etc.)
//! - `params` - Layout analysis parameters (LAParams)
//! - `analysis` - Grouping and clustering algorithms
//! - `table` - Table extraction functionality

pub mod analysis;
pub mod arena;
pub mod params;
pub mod table;
pub mod types;

// Re-export all public types for convenience
pub use analysis::*;
pub use params::*;
pub use table::*;
pub use types::*;
