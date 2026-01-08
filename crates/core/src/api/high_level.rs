//! High-level text extraction API - port of pdfminer.six high_level.py
//!
//! Provides the main public API for PDF text extraction:
//! - `extract_text()` - Extract all text from a PDF as a String
//! - `extract_text_to_fp()` - Extract text to a writer
//! - `extract_pages()` - Iterator over analyzed pages

use std::io::Write;

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::api::stream::{PageStream, extract_pages_stream_from_doc};
use crate::arena::PageArena;
use crate::converter::{PDFPageAggregator, TextConverter};
use crate::error::{PdfError, Result};
use crate::layout::{LAParams, LTPage};
use crate::pdfdocument::PDFDocument;
use crate::pdfinterp::{PDFPageInterpreter, PDFResourceManager};
use crate::pdfpage::PDFPage;
use crate::table::{PageGeometry, TableSettings, extract_tables_from_ltpage};

use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy)]
struct ThreadRecord {
    id: std::thread::ThreadId,
    in_pool: bool,
}

static THREAD_LOG: OnceLock<Mutex<Vec<ThreadRecord>>> = OnceLock::new();

fn record_thread() {
    let log = THREAD_LOG.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut guard) = log.lock() {
        let in_pool = rayon::current_thread_index().is_some();
        guard.push(ThreadRecord {
            id: std::thread::current().id(),
            in_pool,
        });
    }
}

fn take_thread_log() -> Vec<ThreadRecord> {
    let log = THREAD_LOG.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = log.lock().unwrap();
    std::mem::take(&mut *guard)
}

pub fn clear_thread_log() {
    let log = THREAD_LOG.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = log.lock().unwrap();
    guard.clear();
}

pub fn take_thread_log_len() -> usize {
    take_thread_log().len()
}

pub(crate) fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

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
}

pub type Cell = Option<String>;
pub type Row = Vec<Cell>;
pub type Table = Vec<Row>;
pub type PageTables = Vec<Table>;

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            password: String::new(),
            page_numbers: None,
            maxpages: 0,
            caching: true,
            laparams: None,
        }
    }
}

/// Parse and return the text contained in PDF data.
///
/// This is the main text extraction function.
///
/// # Arguments
/// * `pdf_data` - PDF file contents as bytes
/// * `options` - Extraction options (None for defaults)
///
/// # Returns
/// A string containing all extracted text.
///
/// # Example
/// ```ignore
/// use bolivar_core::high_level::{extract_text, ExtractOptions};
///
/// let pdf_bytes = std::fs::read("document.pdf")?;
/// let text = extract_text(&pdf_bytes, None)?;
/// println!("{}", text);
/// ```
pub fn extract_text(pdf_data: &[u8], options: Option<ExtractOptions>) -> Result<String> {
    let options = options.unwrap_or_default();
    let doc = PDFDocument::new(pdf_data, &options.password)?;
    extract_text_with_document(&doc, options)
}

/// Extract text from an already-parsed PDFDocument.
pub fn extract_text_with_document(doc: &PDFDocument, options: ExtractOptions) -> Result<String> {
    let options = options;

    // Use LAParams or create default
    let laparams = options.laparams.clone().unwrap_or_default();

    // Create output buffer
    let mut output = Vec::new();

    extract_text_to_fp_from_doc_inner(
        doc,
        &mut output,
        options.page_numbers.as_deref(),
        options.maxpages,
        options.caching,
        Some(&laparams),
    )?;

    String::from_utf8(output).map_err(|e| PdfError::DecodeError(e.to_string()))
}

/// Parse text from PDF data and write to a writer.
///
/// # Arguments
/// * `pdf_data` - PDF file contents as bytes
/// * `writer` - Output writer for extracted text
/// * `options` - Extraction options (None for defaults)
///
/// # Example
/// ```ignore
/// use bolivar_core::high_level::{extract_text_to_fp, ExtractOptions};
/// use std::fs::File;
///
/// let pdf_bytes = std::fs::read("document.pdf")?;
/// let mut output = File::create("output.txt")?;
/// extract_text_to_fp(&pdf_bytes, &mut output, None)?;
/// ```
pub fn extract_text_to_fp<W: Write>(
    pdf_data: &[u8],
    writer: &mut W,
    options: Option<ExtractOptions>,
) -> Result<()> {
    let options = options.unwrap_or_default();

    let laparams = options.laparams.as_ref();

    extract_text_to_fp_inner(
        pdf_data,
        writer,
        &options.password,
        options.page_numbers.as_deref(),
        options.maxpages,
        options.caching,
        laparams,
    )
}

/// Inner implementation of extract_text_to_fp.
fn extract_text_to_fp_inner<W: Write>(
    pdf_data: &[u8],
    writer: &mut W,
    password: &str,
    page_numbers: Option<&[usize]>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&LAParams>,
) -> Result<()> {
    // Validate PDF header
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    // Parse PDF document
    let doc = PDFDocument::new(pdf_data, password)?;
    extract_text_to_fp_from_doc_inner(&doc, writer, page_numbers, maxpages, caching, laparams)
}

fn extract_text_to_fp_from_doc_inner<W: Write>(
    doc: &PDFDocument,
    writer: &mut W,
    page_numbers: Option<&[usize]>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&LAParams>,
) -> Result<()> {
    // Get LAParams (use default if not provided)
    let default_laparams = LAParams::default();
    let laparams = laparams.unwrap_or(&default_laparams);

    // Create text converter
    let mut converter = TextConverter::new(writer, "utf-8", 1, Some(laparams.clone()), false);

    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let mut selected_pages: Vec<(usize, PDFPage)> = Vec::new();
    let mut page_count = 0;
    for (page_idx, page_result) in PDFPage::create_pages(doc).enumerate() {
        if let Some(nums) = page_numbers
            && !nums.contains(&page_idx)
        {
            continue;
        }

        if maxpages > 0 && page_count >= maxpages {
            break;
        }

        let page = page_result?;
        selected_pages.push((page_idx, page));
        page_count += 1;
    }

    let mut results: Vec<(usize, Result<LTPage>)> = pool.install(|| {
        selected_pages
            .into_par_iter()
            .map_init(PageArena::new, |arena, (page_idx, page)| {
                arena.reset();
                let mut rsrcmgr = PDFResourceManager::with_caching(caching);
                let mut aggregator =
                    PDFPageAggregator::new(Some(laparams.clone()), page_idx as i32 + 1, arena);
                let ltpage = process_page(&page, &mut aggregator, &mut rsrcmgr, doc);
                (page_idx, ltpage)
            })
            .collect()
    });

    results.sort_by_key(|(page_idx, _)| *page_idx);
    for (_, result) in results {
        let ltpage = result?;
        converter.receive_layout(ltpage);
    }

    Ok(())
}

/// Process a PDF page and return its layout.
///
/// Uses PDFPageInterpreter to execute the page's content stream,
/// which populates the device (aggregator) with layout items.
pub(crate) fn process_page(
    page: &PDFPage,
    aggregator: &mut PDFPageAggregator<'_>,
    rsrcmgr: &mut PDFResourceManager,
    doc: &PDFDocument,
) -> Result<LTPage> {
    record_thread();

    // Create interpreter with resource manager and aggregator as device
    let mut interpreter = PDFPageInterpreter::new(rsrcmgr, aggregator);

    // Process page - this executes the content stream and populates the device
    interpreter.process_page(page, Some(doc));

    // Get the analyzed result from aggregator
    Ok(aggregator.get_result().clone())
}

fn page_geometry_from_ltpage(page: &LTPage) -> PageGeometry {
    let bbox = page.bbox();
    PageGeometry {
        page_bbox: bbox,
        mediabox: bbox,
        initial_doctop: 0.0,
        force_crop: false,
    }
}

/// Iterator over analyzed pages.
///
/// Yields LTPage objects for each page in the PDF.
///
/// Note: Due to lifetime constraints with the underlying PDF document,
/// this iterator pre-processes pages into a vector. For very large PDFs,
/// consider using extract_text_to_fp with streaming output instead.
pub struct PageIterator {
    /// Pre-processed pages
    pages: std::vec::IntoIter<Result<LTPage>>,
}

impl Iterator for PageIterator {
    type Item = Result<LTPage>;

    fn next(&mut self) -> Option<Self::Item> {
        self.pages.next()
    }
}

impl PageIterator {
    /// Returns the number of remaining pages.
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Returns true if there are no more pages.
    pub fn is_empty(&self) -> bool {
        self.pages.len() == 0
    }
}

/// Extract and stream LTPage objects from PDF data in order.
///
/// Returns a PageStream that yields ordered LTPage results.
pub fn extract_pages_stream(
    pdf_data: &[u8],
    options: Option<ExtractOptions>,
) -> Result<PageStream> {
    let mut options = options.unwrap_or_default();
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    let doc = PDFDocument::new(pdf_data, &options.password)?;
    extract_pages_stream_from_doc(Arc::new(doc), options)
}

/// Extract and yield LTPage objects from PDF data.
///
/// Returns an iterator over analyzed pages.
///
/// # Arguments
/// * `pdf_data` - PDF file contents as bytes
/// * `options` - Extraction options (None for defaults)
///
/// # Example
/// ```ignore
/// use bolivar_core::high_level::{extract_pages, ExtractOptions};
/// use bolivar_core::layout::LTTextBox;
///
/// let pdf_bytes = std::fs::read("document.pdf")?;
/// for page_result in extract_pages(&pdf_bytes, None)? {
///     let page = page_result?;
///     for item in page.iter() {
///         // Process layout items
///     }
/// }
/// ```
pub fn extract_pages(pdf_data: &[u8], options: Option<ExtractOptions>) -> Result<PageIterator> {
    let mut options = options.unwrap_or_default();
    // Match pdfminer.high_level.extract_pages default behavior.
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    // Validate PDF header
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    let stream = extract_pages_stream(pdf_data, Some(options))?;
    let pages: Vec<Result<LTPage>> = stream.collect();

    Ok(PageIterator {
        pages: pages.into_iter(),
    })
}

/// Extract LTPage objects from an already-parsed PDFDocument.
pub fn extract_pages_with_document(
    doc: &PDFDocument,
    options: ExtractOptions,
) -> Result<Vec<LTPage>> {
    extract_pages_stream(doc.bytes(), Some(options))?.collect()
}

/// Extract tables from an already-parsed PDFDocument.
pub fn extract_tables_with_document(
    doc: &PDFDocument,
    options: ExtractOptions,
    settings: &TableSettings,
) -> Result<Vec<Vec<Vec<Vec<Option<String>>>>>> {
    let mut options = options;
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    let pages = extract_pages_with_document(doc, options.clone())?;
    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let mut results: Vec<(usize, Vec<Vec<Vec<Option<String>>>>)> = pool.install(|| {
        pages
            .into_par_iter()
            .enumerate()
            .map(|(idx, page)| {
                let geom = page_geometry_from_ltpage(&page);
                (idx, extract_tables_from_ltpage(&page, &geom, settings))
            })
            .collect()
    });

    results.sort_by_key(|(idx, _)| *idx);
    Ok(results.into_iter().map(|(_, tables)| tables).collect())
}

/// Extract tables from an already-parsed PDFDocument using per-page geometry.
pub fn extract_tables_with_document_geometries(
    doc: &PDFDocument,
    options: ExtractOptions,
    settings: &TableSettings,
    geometries: &[PageGeometry],
) -> Result<Vec<Vec<Vec<Vec<Option<String>>>>>> {
    let mut options = options;
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    let pages = extract_pages_with_document(doc, options.clone())?;
    if geometries.len() != pages.len() {
        return Err(PdfError::DecodeError(format!(
            "geometry count mismatch: expected {}, got {}",
            pages.len(),
            geometries.len()
        )));
    }
    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let mut results: Vec<(usize, Vec<Vec<Vec<Option<String>>>>)> = pool.install(|| {
        pages
            .into_par_iter()
            .enumerate()
            .map(|(idx, page)| {
                let geom = &geometries[idx];
                (idx, extract_tables_from_ltpage(&page, geom, settings))
            })
            .collect()
    });

    results.sort_by_key(|(idx, _)| *idx);
    Ok(results.into_iter().map(|(_, tables)| tables).collect())
}

/// Extract tables for specific pages with per-page geometry in input order.
pub fn extract_tables_for_pages(
    doc: &PDFDocument,
    page_numbers: &[usize],
    geometries: &[PageGeometry],
    options: ExtractOptions,
    settings: &TableSettings,
) -> Result<Vec<PageTables>> {
    if page_numbers.len() != geometries.len() {
        return Err(PdfError::InvalidArgument(format!(
            "geometry count mismatch: expected {}, got {}",
            page_numbers.len(),
            geometries.len()
        )));
    }

    let mut options = options;
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    let laparams = options.laparams.clone();

    let mut wanted: std::collections::HashMap<usize, (usize, PageGeometry)> =
        std::collections::HashMap::new();
    for (pos, page_idx) in page_numbers.iter().enumerate() {
        wanted.insert(*page_idx, (pos, geometries[pos].clone()));
    }

    let mut pending: Vec<(usize, usize, PDFPage, PageGeometry)> = Vec::new();
    for (page_idx, page_result) in PDFPage::create_pages(doc).enumerate() {
        let Some((requested_pos, geom)) = wanted.get(&page_idx) else {
            continue;
        };
        let page = page_result?;
        pending.push((*requested_pos, page_idx, page, geom.clone()));
    }

    if pending.len() != page_numbers.len() {
        return Err(PdfError::InvalidArgument(
            "page number out of range".to_string(),
        ));
    }

    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let mut results: Vec<Result<(usize, PageTables)>> = pool.install(|| {
        pending
            .into_par_iter()
            .map_init(
                PageArena::new,
                |arena, (requested_pos, page_idx, page, geom)| {
                    arena.reset();
                    let mut rsrcmgr = PDFResourceManager::with_caching(options.caching);
                    let mut aggregator =
                        PDFPageAggregator::new(laparams.clone(), page_idx as i32 + 1, arena);
                    let ltpage = process_page(&page, &mut aggregator, &mut rsrcmgr, doc)?;
                    Ok((
                        requested_pos,
                        extract_tables_from_ltpage(&ltpage, &geom, settings),
                    ))
                },
            )
            .collect()
    });

    let mut ordered = Vec::with_capacity(results.len());
    for result in results.drain(..) {
        ordered.push(result?);
    }
    ordered.sort_by_key(|(pos, _)| *pos);
    Ok(ordered.into_iter().map(|(_, tables)| tables).collect())
}

#[cfg(test)]
mod tests {
    use super::{
        ExtractOptions, extract_pages, extract_tables_for_pages, extract_tables_with_document,
        extract_tables_with_document_geometries,
    };
    use crate::pdfdocument::PDFDocument;
    use crate::table::{PageGeometry, TableSettings};
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    static THREAD_LOG_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn thread_log_guard() -> std::sync::MutexGuard<'static, ()> {
        THREAD_LOG_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("thread log test lock")
    }

    fn build_minimal_pdf_with_pages(page_count: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");

        let mut offsets: Vec<usize> = Vec::new();
        let push_obj = |buf: &mut Vec<u8>, obj: String, offsets: &mut Vec<usize>| {
            offsets.push(buf.len());
            buf.extend_from_slice(obj.as_bytes());
        };

        // 1: Catalog
        push_obj(
            &mut out,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
            &mut offsets,
        );

        // 2: Pages
        let kids: String = (0..page_count)
            .map(|i| format!("{} 0 R", 3 + i))
            .collect::<Vec<_>>()
            .join(" ");
        push_obj(
            &mut out,
            format!(
                "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
                kids, page_count
            ),
            &mut offsets,
        );

        // Page objects and their content streams
        for i in 0..page_count {
            let page_id = 3 + i;
            let contents_id = 3 + page_count + i;
            push_obj(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents {} 0 R >>\nendobj\n",
                    page_id, contents_id
                ),
                &mut offsets,
            );
        }

        for i in 0..page_count {
            let contents_id = 3 + page_count + i;
            push_obj(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n",
                    contents_id
                ),
                &mut offsets,
            );
        }

        let xref_pos = out.len();
        let obj_count = offsets.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", obj_count + 1).as_bytes(),
        );
        for offset in offsets {
            out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }
        out.extend_from_slice(b"trailer\n<< /Size ");
        out.extend_from_slice((obj_count + 1).to_string().as_bytes());
        out.extend_from_slice(b" /Root 1 0 R >>\nstartxref\n");
        out.extend_from_slice(xref_pos.to_string().as_bytes());
        out.extend_from_slice(b"\n%%EOF");

        out
    }

    #[test]
    fn test_extract_pages_uses_rayon_pool() {
        let _guard = thread_log_guard();
        let pdf_data = build_minimal_pdf_with_pages(4);

        super::clear_thread_log();

        let options = ExtractOptions::default();
        let pages: Vec<_> = extract_pages(&pdf_data, Some(options))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(pages.len(), 4);

        let records = super::take_thread_log();
        let used_pool = records.iter().any(|record| record.in_pool);
        assert!(used_pool, "expected rayon pool to be used");

        let unique: HashSet<_> = records.iter().map(|record| record.id).collect();
        assert!(!unique.is_empty(), "expected at least one recorded thread");
    }

    #[test]
    fn test_extract_pages_uses_stream_path() {
        let pdf_data = build_minimal_pdf_with_pages(2);
        crate::api::stream::take_stream_usage();

        let options = ExtractOptions::default();
        let pages: Vec<_> = extract_pages(&pdf_data, Some(options))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(pages.len(), 2);
        assert_eq!(crate::api::stream::take_stream_usage(), 1);
    }

    #[test]
    fn test_extract_tables_with_document_parallel_ordered() {
        let pdf_data = build_minimal_pdf_with_pages(3);
        let doc = PDFDocument::new(&pdf_data, "").unwrap();
        let options = ExtractOptions::default();
        let settings = TableSettings::default();

        let tables = extract_tables_with_document(&doc, options, &settings).unwrap();
        assert_eq!(tables.len(), 3);
    }

    #[test]
    fn test_extract_tables_with_document_geometries_for_pages_preserves_order() {
        let pdf_data = build_minimal_pdf_with_pages(3);
        let doc = PDFDocument::new(&pdf_data, "").unwrap();
        let settings = TableSettings::default();
        let options = ExtractOptions::default();

        let geom0 = PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let geom2 = PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 400.0,
            force_crop: false,
        };

        let page_numbers = vec![2, 0];
        let geoms = vec![geom2, geom0];

        let tables =
            extract_tables_for_pages(&doc, &page_numbers, &geoms, options, &settings).unwrap();

        assert_eq!(tables.len(), 2);
    }

    #[test]
    fn test_extract_tables_with_document_geometries_length_mismatch() {
        let pdf_data = build_minimal_pdf_with_pages(2);
        let doc = PDFDocument::new(&pdf_data, "").unwrap();
        let options = ExtractOptions::default();
        let settings = TableSettings::default();
        let geom = PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };

        let err =
            extract_tables_with_document_geometries(&doc, options, &settings, &[geom]).unwrap_err();
        assert!(err.to_string().contains("geometry"));
    }
}
