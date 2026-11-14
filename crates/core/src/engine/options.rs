//! Extraction options and result type aliases.

use std::num::NonZeroUsize;

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

    /// How many pages a stream may have started or finished but not yet handed out, ahead of
    /// its reader. It also caps the stream's parallelism at `min(workers, read_ahead)`, and the
    /// finished pages waiting behind a slow one at `read_ahead - 1`. None uses
    /// [`DEFAULT_STREAM_WINDOW_CAPACITY`](crate::engine::stream::DEFAULT_STREAM_WINDOW_CAPACITY).
    /// Streams only: whole-document calls such as `extract_text` ignore it.
    pub read_ahead: Option<NonZeroUsize>,
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
            read_ahead: None,
        }
    }
}

pub type Cell = Option<String>;
pub type Row = Vec<Cell>;
pub type Table = Vec<Row>;
pub type PageTables = Vec<Table>;
pub type DocumentTables = Vec<PageTables>;
