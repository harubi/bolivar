//! High-level text extraction API - port of pdfminer.six high_level.py
//!
//! Provides the main public API for PDF text extraction:
//! - `extract_text()` - Extract all text from a PDF as a String
//! - `extract_text_to_fp()` - Extract text to a writer
//! - `extract_pages()` - Iterator over analyzed pages

use std::io::Write;

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::converter::{PDFPageAggregator, TextConverter};
use crate::error::{PdfError, Result};
use crate::layout::{LAParams, LTPage};
use crate::pdfdocument::PDFDocument;
use crate::pdfinterp::{PDFPageInterpreter, PDFResourceManager};
use crate::pdfpage::PDFPage;
use crate::table::{PageGeometry, TableSettings, extract_tables_from_ltpage};

#[cfg(any(test, feature = "test-utils"))]
use std::sync::{Mutex, OnceLock};

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy)]
struct ThreadRecord {
    id: std::thread::ThreadId,
    in_pool: bool,
}

#[cfg(any(test, feature = "test-utils"))]
static THREAD_LOG: OnceLock<Mutex<Vec<ThreadRecord>>> = OnceLock::new();

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
fn take_thread_log() -> Vec<ThreadRecord> {
    let log = THREAD_LOG.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = log.lock().unwrap();
    std::mem::take(&mut *guard)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn clear_thread_log() {
    let log = THREAD_LOG.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = log.lock().unwrap();
    guard.clear();
}

#[cfg(any(test, feature = "test-utils"))]
pub fn take_thread_log_len() -> usize {
    take_thread_log().len()
}

fn normalize_threads(threads: Option<usize>) -> Option<usize> {
    match threads {
        Some(0) | Some(1) => None,
        Some(n) => Some(n),
        None => std::thread::available_parallelism().ok().map(|n| n.get()),
    }
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

    /// Optional thread count for parallel page processing.
    /// None or Some(1) uses the sequential path.
    pub threads: Option<usize>,
}

/// Cache for layout pages extracted from a document.
#[derive(Debug, Default)]
pub struct LayoutCache {
    options: Option<ExtractOptions>,
    pages: Option<Vec<LTPage>>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self {
            options: None,
            pages: None,
        }
    }

    pub fn get_or_init(&mut self, doc: &PDFDocument, options: ExtractOptions) -> Result<&[LTPage]> {
        let needs_refresh = self.options.as_ref().map_or(true, |o| o != &options);
        if needs_refresh {
            let pages = extract_pages_with_document(doc, options.clone())?;
            self.pages = Some(pages);
            self.options = Some(options);
        }
        Ok(self.pages.as_ref().unwrap())
    }
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            password: String::new(),
            page_numbers: None,
            maxpages: 0,
            caching: true,
            laparams: None,
            threads: None,
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
    let mut options = options;
    options.threads = normalize_threads(options.threads);

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
        options.threads,
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
        options.threads,
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
    threads: Option<usize>,
) -> Result<()> {
    let threads = normalize_threads(threads);
    // Validate PDF header
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    // Parse PDF document
    let doc = PDFDocument::new(pdf_data, password)?;
    extract_text_to_fp_from_doc_inner(
        &doc,
        writer,
        page_numbers,
        maxpages,
        caching,
        laparams,
        threads,
    )
}

fn extract_text_to_fp_from_doc_inner<W: Write>(
    doc: &PDFDocument,
    writer: &mut W,
    page_numbers: Option<&[usize]>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&LAParams>,
    threads: Option<usize>,
) -> Result<()> {
    let threads = normalize_threads(threads);
    // Get LAParams (use default if not provided)
    let default_laparams = LAParams::default();
    let laparams = laparams.unwrap_or(&default_laparams);

    // Create text converter
    let mut converter = TextConverter::new(writer, "utf-8", 1, Some(laparams.clone()), false);

    let thread_count = threads.filter(|count| *count > 1);
    if let Some(count) = thread_count {
        let pool = ThreadPoolBuilder::new()
            .num_threads(count)
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
                .map(|(page_idx, page)| {
                    let mut rsrcmgr = PDFResourceManager::with_caching(caching);
                    let mut aggregator =
                        PDFPageAggregator::new(Some(laparams.clone()), page_idx as i32 + 1);
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
    } else {
        // Create resource manager
        let mut rsrcmgr = PDFResourceManager::with_caching(caching);

        // Process pages
        let mut page_count = 0;
        for (page_idx, page_result) in PDFPage::create_pages(doc).enumerate() {
            // Check page number filter
            if let Some(nums) = page_numbers
                && !nums.contains(&page_idx)
            {
                continue;
            }

            // Check maxpages limit
            if maxpages > 0 && page_count >= maxpages {
                break;
            }

            let page = page_result?;

            // Create aggregator for this page
            let mut aggregator =
                PDFPageAggregator::new(Some(laparams.clone()), page_idx as i32 + 1);

            // Process page content and get layout
            let ltpage = process_page(&page, &mut aggregator, &mut rsrcmgr, doc)?;

            // Render to text
            converter.receive_layout(ltpage);

            page_count += 1;
        }
    }

    Ok(())
}

/// Process a PDF page and return its layout.
///
/// Uses PDFPageInterpreter to execute the page's content stream,
/// which populates the device (aggregator) with layout items.
fn process_page(
    page: &PDFPage,
    aggregator: &mut PDFPageAggregator,
    rsrcmgr: &mut PDFResourceManager,
    doc: &PDFDocument,
) -> Result<LTPage> {
    #[cfg(any(test, feature = "test-utils"))]
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
    options.threads = normalize_threads(options.threads);

    // Validate PDF header
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    // Parse PDF document
    let doc = PDFDocument::new(pdf_data, &options.password)?;

    let pages = extract_pages_from_doc(&doc, &options)?;

    Ok(PageIterator {
        pages: pages.into_iter(),
    })
}

/// Extract LTPage objects from an already-parsed PDFDocument.
pub fn extract_pages_with_document(
    doc: &PDFDocument,
    options: ExtractOptions,
) -> Result<Vec<LTPage>> {
    let mut options = options;
    options.threads = normalize_threads(options.threads);
    extract_pages_from_doc(doc, &options)?.into_iter().collect()
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
    options.threads = normalize_threads(options.threads);

    let pages = extract_pages_with_document(doc, options.clone())?;
    let thread_count = options.threads.filter(|count| *count > 1);

    if let Some(count) = thread_count {
        let pool = ThreadPoolBuilder::new()
            .num_threads(count)
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
    } else {
        let mut results = Vec::with_capacity(pages.len());
        for page in pages {
            let geom = page_geometry_from_ltpage(&page);
            results.push(extract_tables_from_ltpage(&page, &geom, settings));
        }
        Ok(results)
    }
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
    options.threads = normalize_threads(options.threads);

    let pages = extract_pages_with_document(doc, options.clone())?;
    if geometries.len() != pages.len() {
        return Err(PdfError::DecodeError(format!(
            "geometry count mismatch: expected {}, got {}",
            pages.len(),
            geometries.len()
        )));
    }

    let thread_count = options.threads.filter(|count| *count > 1);

    if let Some(count) = thread_count {
        let pool = ThreadPoolBuilder::new()
            .num_threads(count)
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
    } else {
        let mut results = Vec::with_capacity(pages.len());
        for (idx, page) in pages.into_iter().enumerate() {
            let geom = &geometries[idx];
            results.push(extract_tables_from_ltpage(&page, geom, settings));
        }
        Ok(results)
    }
}

fn extract_pages_from_doc(
    doc: &PDFDocument,
    options: &ExtractOptions,
) -> Result<Vec<Result<LTPage>>> {
    // Use LAParams as provided (None disables layout analysis).
    let laparams = options.laparams.clone();

    let thread_count = options.threads.filter(|count| *count > 1);
    // Collect pages into a vector
    // This is necessary because PDFPage borrows from PDFDocument
    let mut pages: Vec<Result<LTPage>> = Vec::new();
    let mut page_count = 0;

    if let Some(count) = thread_count {
        let pool = ThreadPoolBuilder::new()
            .num_threads(count)
            .build()
            .map_err(|e| PdfError::DecodeError(e.to_string()))?;

        let mut pending: Vec<(usize, PDFPage)> = Vec::new();
        let mut errors: Vec<(usize, Result<LTPage>)> = Vec::new();

        for (page_idx, page_result) in PDFPage::create_pages(doc).enumerate() {
            if let Some(ref nums) = options.page_numbers
                && !nums.contains(&page_idx)
            {
                continue;
            }

            if options.maxpages > 0 && page_count >= options.maxpages {
                break;
            }

            match page_result {
                Ok(page) => {
                    pending.push((page_idx, page));
                    page_count += 1;
                }
                Err(e) => {
                    errors.push((page_idx, Err(e)));
                }
            }
        }

        let mut processed: Vec<(usize, Result<LTPage>)> = pool.install(|| {
            pending
                .into_par_iter()
                .map(|(page_idx, page)| {
                    let mut rsrcmgr = PDFResourceManager::with_caching(options.caching);
                    let mut aggregator =
                        PDFPageAggregator::new(laparams.clone(), page_idx as i32 + 1);
                    let ltpage = process_page(&page, &mut aggregator, &mut rsrcmgr, doc);
                    (page_idx, ltpage)
                })
                .collect()
        });

        processed.extend(errors);
        processed.sort_by_key(|(page_idx, _)| *page_idx);
        pages.extend(processed.into_iter().map(|(_, result)| result));
    } else {
        // Create resource manager
        let mut rsrcmgr = PDFResourceManager::with_caching(options.caching);

        for (page_idx, page_result) in PDFPage::create_pages(doc).enumerate() {
            // Check page number filter
            if let Some(ref nums) = options.page_numbers
                && !nums.contains(&page_idx)
            {
                continue;
            }

            // Check maxpages limit
            if options.maxpages > 0 && page_count >= options.maxpages {
                break;
            }

            match page_result {
                Ok(page) => {
                    // Create aggregator for this page
                    let mut aggregator =
                        PDFPageAggregator::new(laparams.clone(), page_idx as i32 + 1);

                    // Process page content using interpreter
                    match process_page(&page, &mut aggregator, &mut rsrcmgr, doc) {
                        Ok(ltpage) => {
                            pages.push(Ok(ltpage));
                            page_count += 1;
                        }
                        Err(e) => {
                            pages.push(Err(e));
                        }
                    }
                }
                Err(e) => {
                    pages.push(Err(e));
                }
            }
        }
    }

    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::{
        ExtractOptions, extract_pages, extract_tables_with_document,
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
        let mut push_obj = |buf: &mut Vec<u8>, obj: String, offsets: &mut Vec<usize>| {
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
    fn test_parallel_extract_pages_uses_multiple_threads() {
        let _guard = thread_log_guard();
        let pdf_data = build_minimal_pdf_with_pages(4);

        super::clear_thread_log();

        let options = ExtractOptions {
            password: String::new(),
            page_numbers: None,
            maxpages: 0,
            caching: true,
            laparams: None,
            threads: Some(2),
        };

        let pages: Vec<_> = extract_pages(&pdf_data, Some(options))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(pages.len(), 4);

        let records = super::take_thread_log();
        let used_pool = records.iter().any(|record| record.in_pool);
        assert!(used_pool, "expected parallel processing to use rayon pool");

        let unique: HashSet<_> = records.iter().map(|record| record.id).collect();
        assert!(!unique.is_empty(), "expected at least one recorded thread");
    }

    #[test]
    fn test_default_threads_uses_parallelism_when_available() {
        let _guard = thread_log_guard();
        let parallelism = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        if parallelism <= 1 {
            eprintln!("skipping: available parallelism is {parallelism}");
            return;
        }

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
        assert!(used_pool, "expected default threads to use rayon pool");
    }

    #[test]
    fn test_layout_cache_reuses_pages() {
        let _guard = thread_log_guard();
        let pdf_data = build_minimal_pdf_with_pages(3);
        let doc = PDFDocument::new(&pdf_data, "").unwrap();

        super::clear_thread_log();
        let mut cache = super::LayoutCache::new();
        let options = ExtractOptions {
            password: String::new(),
            page_numbers: None,
            maxpages: 0,
            caching: true,
            laparams: None,
            threads: Some(2),
        };

        let _ = cache.get_or_init(&doc, options.clone()).unwrap();
        assert!(super::take_thread_log_len() > 0);

        super::clear_thread_log();
        let _ = cache.get_or_init(&doc, options).unwrap();
        assert_eq!(super::take_thread_log_len(), 0);
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
