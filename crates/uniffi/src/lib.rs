//! uniffi-exported bolivar surface for JVM/Swift/Clojure bindings.

mod document;
mod error;
mod extract;
mod types;

pub use document::NativePdfDocument;
pub use error::BolivarError;
pub use extract::{quick_extract_text, quick_extract_text_from_bytes};
pub use types::{
    BoundingBox, ExtractOptions, LayoutChar, LayoutLine, LayoutPage, LayoutParams, LayoutTextBox,
    PageSummary, PageTableRows, Table, TableCell, TableOptions,
};

uniffi::include_scaffolding!("bolivar");
