//! Extraction options and result type aliases.

use crate::layout::LAParams;

/// Options for text extraction.
///
/// Port of the various optional parameters from pdfminer.six high_level functions.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractOptions {
    /// Password for encrypted PDFs.
    pub password: String,

    /// Zero-indexed page numbers to extract. None means all pages.
    pub page_numbers: Option<Vec<usize>>,

    /// Maximum number of pages to extract. 0 means no limit.
    pub maxpages: usize,

    /// Whether to cache resources (fonts, images).
    pub caching: bool,

    /// Layout analysis parameters. None uses default LAParams.
    pub laparams: Option<LAParams>,

    /// Additional rotation to apply when interpreting pages.
    pub rotation: i64,

    /// Use ICU to reconstruct visual bidirectional text into logical order.
    pub bidi: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            password: String::new(),
            page_numbers: None,
            maxpages: 0,
            caching: true,
            laparams: None,
            rotation: 0,
            bidi: false,
        }
    }
}

pub type Cell = Option<String>;
pub type Row = Vec<Cell>;
pub type Table = Vec<Row>;
pub type PageTables = Vec<Table>;
pub type DocumentTables = Vec<PageTables>;
