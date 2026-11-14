//! Extraction options and result type aliases.

use std::num::NonZeroUsize;

use crate::document::PDFDocument;
use crate::layout::LAParams;

use super::plan::{ExecutionPlan, PageSet};

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

/// Which pages one stream reads, inside the selection its options already make. It can only
/// narrow that selection, never widen it. Build it from `default()` with the setters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CursorSelection {
    /// Zero-indexed page numbers to keep. None keeps every page. Pages that do not exist are
    /// ignored, and an empty list keeps none.
    pub pages: Option<Vec<usize>>,

    /// How many of the remaining pages to pass over first, counted in the narrowed order, so a
    /// first read of `max_pages(n)` and a second of `skip(n)` together read each page once.
    pub skip: usize,

    /// The most pages to read after that. None means no limit.
    pub max_pages: Option<NonZeroUsize>,
}

impl CursorSelection {
    /// Keep only these zero-indexed pages; missing ones are ignored and repeats count once.
    #[must_use]
    pub fn pages(mut self, pages: Vec<usize>) -> Self {
        self.pages = Some(pages);
        self
    }

    /// Pass over this many of the kept pages first; past the end, no page is left.
    #[must_use]
    pub const fn skip(mut self, skip: usize) -> Self {
        self.skip = skip;
        self
    }

    /// Read at most this many pages after the skip.
    #[must_use]
    pub const fn max_pages(mut self, max_pages: NonZeroUsize) -> Self {
        self.max_pages = Some(max_pages);
        self
    }
}

impl ExtractOptions {
    /// These options narrowed to `selection`: the plan order of `doc` under these options, kept
    /// to the selection's pages, past its `skip`, cut to its `max_pages`, in document order. An
    /// empty selection returns the options unchanged; any other one returns its order as an
    /// explicit `page_numbers` (empty when no page is left) with no `maxpages`.
    #[must_use]
    pub fn narrowed(&self, doc: &PDFDocument, selection: &CursorSelection) -> Self {
        if *selection == CursorSelection::default() {
            return self.clone();
        }
        let page_count = doc.page_tree_len();
        let plan = ExecutionPlan::new(page_count, self.page_numbers.as_deref(), self.maxpages);
        let wanted = selection
            .pages
            .as_deref()
            .map(|pages| PageSet::new(page_count, pages));
        let limit = selection.max_pages.map_or(usize::MAX, NonZeroUsize::get);
        let order = plan
            .order
            .into_iter()
            .filter(|&page_idx| wanted.as_ref().is_none_or(|set| set.contains(page_idx)))
            .skip(selection.skip)
            .take(limit)
            .collect();
        Self {
            page_numbers: Some(order),
            maxpages: 0,
            ..self.clone()
        }
    }
}

pub type Cell = Option<String>;
pub type Row = Vec<Cell>;
pub type Table = Vec<Row>;
pub type PageTables = Vec<Table>;
pub type DocumentTables = Vec<PageTables>;
