//! uniffi-exported bolivar surface for JVM/Swift/Clojure bindings.

// UniFFI emits its metadata as a generated constant array.
#![allow(clippy::large_const_arrays)]

mod cursor;
mod document;
mod error;
mod extract;
mod metadata;
mod types;

pub use cursor::{NativePageTableRowsCursor, NativeTableCursor};
pub use document::NativePdfDocument;
pub use error::BolivarError;
pub use extract::{quick_extract_text, quick_extract_text_from_bytes};
pub use types::{
    BoundingBox, ExtractOptions, LayoutChar, LayoutLine, LayoutPage, LayoutParams, LayoutTextBox,
    MetadataEntry, PageSummary, PageTableRows, PdfPermissions, PdfVersion, RawCharacter,
    RawDocument, RawDocumentMetadata, RawPage, RawPageBoxes, RawTable, RawTableBoundingBox,
    RawTableCell, RawTextBox, RawTextLine, Table, TableCell, TableOptions,
};

pub fn bolivar_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

uniffi::include_scaffolding!("bolivar");
