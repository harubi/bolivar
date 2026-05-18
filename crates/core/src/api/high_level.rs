//! High-level text extraction API - port of pdfminer.six high_level.py
//!
//! Provides the main public API for PDF text extraction:
//! - `extract_text()` - Extract all text from a PDF as a String
//! - `extract_text_to_fp()` - Extract text to a writer
//! - `extract_pages_stream()` - Stream of analyzed pages from PDF bytes

use std::io::Write;

use crate::api::pipeline::Stream;
use crate::api::stream::extract_pages_stream_from_doc;
use crate::converter::{PDFPageAggregator, PDFTableCollector, TextConverter};
use crate::document::catalog::DEFAULT_CACHE_CAPACITY;
use crate::document::{PDFDocument, PDFPage};
use crate::error::{PdfError, Result};
use crate::image::ImageWriter;
use crate::interp::{PDFPageInterpreter, PDFResourceManager};
use crate::layout::{LAParams, LTPage};

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
#[derive(Clone, Copy)]
struct ThreadRecord {
    id: std::thread::ThreadId,
    in_pool: bool,
}

#[cfg(test)]
static THREAD_LOG: OnceLock<Mutex<Vec<ThreadRecord>>> = OnceLock::new();

#[cfg(test)]
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

#[cfg(not(test))]
fn record_thread() {}

#[cfg(test)]
fn take_thread_log() -> Vec<ThreadRecord> {
    let log = THREAD_LOG.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = log.lock().unwrap();
    std::mem::take(&mut *guard)
}

#[cfg(test)]
pub fn clear_thread_log() {
    let log = THREAD_LOG.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = log.lock().unwrap();
    guard.clear();
}

#[cfg(test)]
pub fn take_thread_log_len() -> usize {
    take_thread_log().len()
}

fn cache_capacity(caching: bool) -> usize {
    if caching { DEFAULT_CACHE_CAPACITY } else { 0 }
}

const TABLE_COLLECTOR_NO_RESULT: &str = "table collector produced no result";

/// Standard finisher for `PDFPageAggregator`-backed `process_page` calls: clones the result `LTPage`.
pub fn aggregator_result(agg: &mut PDFPageAggregator<'_>) -> Result<LTPage> {
    Ok(agg.get_result().clone())
}

/// Standard finisher for `PDFTableCollector`-backed `process_page` calls: takes the arena page or errors.
pub fn collector_result<'a>(
    collector: &mut PDFTableCollector<'a>,
) -> Result<crate::arena::types::ArenaPage<'a>> {
    collector
        .take_result()
        .ok_or_else(|| PdfError::DecodeError(TABLE_COLLECTOR_NO_RESULT.to_string()))
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

    /// Additional rotation to apply when interpreting pages.
    pub rotation: i64,
}

pub type Cell = Option<String>;
pub type Row = Vec<Cell>;
pub type Table = Vec<Row>;
pub type PageTables = Vec<Table>;
pub type DocumentTables = Vec<PageTables>;

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            password: String::new(),
            page_numbers: None,
            maxpages: 0,
            caching: true,
            laparams: None,
            rotation: 0,
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
    let doc =
        PDFDocument::new_with_cache(pdf_data, &options.password, cache_capacity(options.caching))?;
    extract_text_with_document(&doc, options)
}

/// Extract text from an already-parsed PDFDocument.
pub fn extract_text_with_document(doc: &PDFDocument, options: ExtractOptions) -> Result<String> {
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
        options.rotation,
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
        options.rotation,
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
    rotation: i64,
) -> Result<()> {
    // Validate PDF header
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    // Parse PDF document
    let doc = PDFDocument::new_with_cache(pdf_data, password, cache_capacity(caching))?;
    extract_text_to_fp_from_doc_inner(
        &doc,
        writer,
        page_numbers,
        maxpages,
        caching,
        laparams,
        rotation,
    )
}

fn extract_text_to_fp_from_doc_inner<W: Write>(
    doc: &PDFDocument,
    writer: &mut W,
    page_numbers: Option<&[usize]>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&LAParams>,
    rotation: i64,
) -> Result<()> {
    // Get LAParams (use default if not provided)
    let default_laparams = LAParams::default();
    let laparams = laparams.unwrap_or(&default_laparams).clone();

    // Create text converter
    let mut converter = TextConverter::new(writer, "utf-8", 1, Some(laparams.clone()), false);

    let results = crate::api::pipeline::run_batch(
        doc,
        page_numbers,
        maxpages,
        |arena, page_idx, page, doc| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut aggregator =
                PDFPageAggregator::new(Some(laparams.clone()), page_idx as i32 + 1, arena);
            process_page(
                page,
                &mut aggregator,
                &mut rsrcmgr,
                rotation,
                doc,
                aggregator_result,
            )
        },
    )?;

    for (_, ltpage) in results {
        converter.receive_layout(ltpage);
    }

    Ok(())
}

/// Run a page through the interpreter against `device`, applying `rotation` if non-zero,
/// then extract a result via `finish`.
///
/// Generic over any `D: PDFDevice` so the same per-page processing path serves both
/// layout aggregation (`PDFPageAggregator` -> `LTPage`) and arena collection
/// (`PDFTableCollector` -> `ArenaPage`); helpers `aggregator_result` and
/// `collector_result` cover the two standard finisher shapes.
pub fn process_page<D, R>(
    page: &PDFPage,
    device: &mut D,
    rsrcmgr: &mut PDFResourceManager,
    rotation: i64,
    doc: &PDFDocument,
    finish: impl FnOnce(&mut D) -> Result<R>,
) -> Result<R>
where
    D: crate::interp::device::PDFDevice,
{
    record_thread();

    let rotated_page;
    let page = if rotation.rem_euclid(360) == 0 {
        page
    } else {
        rotated_page = page.with_extra_rotation(rotation);
        &rotated_page
    };

    let mut interpreter = PDFPageInterpreter::new(rsrcmgr, device);
    interpreter.process_page(page, Some(doc))?;
    finish(device)
}

/// Extract and stream LTPage objects from PDF data in order.
///
/// Returns a `Stream<LTPage>` that yields ordered `(page_idx, LTPage)` results.
pub fn extract_pages_stream(
    pdf_data: &[u8],
    options: Option<ExtractOptions>,
) -> Result<Stream<LTPage>> {
    let mut options = options.unwrap_or_default();
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    let doc =
        PDFDocument::new_with_cache(pdf_data, &options.password, cache_capacity(options.caching))?;
    extract_pages_stream_from_doc(Arc::new(doc), options)
}

/// Extract LTPage objects from an already-parsed PDFDocument while exporting images.
///
/// Pages are processed in parallel. Each worker constructs its own `ImageWriter`
/// (so the non-`Send` `Rc<RefCell<_>>` never crosses thread boundaries) and the
/// writer is scoped to the page index, producing deterministic `page-XXXX-…`
/// filenames regardless of cross-thread scheduling.
pub fn extract_pages_with_images_with_document(
    doc: Arc<PDFDocument>,
    options: ExtractOptions,
    output_dir: &str,
) -> Result<Vec<LTPage>> {
    let laparams = options.laparams.clone().unwrap_or_default();
    let caching = options.caching;
    let rotation = options.rotation;
    let output_dir = output_dir.to_string();

    let stream = crate::api::pipeline::run_stream(
        doc,
        options.page_numbers,
        options.maxpages,
        crate::api::pipeline::no_precheck::<LTPage>,
        move |arena, page_idx, page, doc| {
            let writer = ImageWriter::for_page(&output_dir, page_idx)?;
            let writer = Rc::new(RefCell::new(writer));
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut aggregator = PDFPageAggregator::new_with_imagewriter(
                Some(laparams.clone()),
                page_idx as i32 + 1,
                Some(writer),
                arena,
            );
            process_page(
                page,
                &mut aggregator,
                &mut rsrcmgr,
                rotation,
                doc,
                aggregator_result,
            )
        },
    )?;

    stream
        .map(|r| r.map(|(_, page)| page))
        .collect::<Result<Vec<_>>>()
}

#[cfg(test)]
mod tests {
    use super::{ExtractOptions, PageTables, extract_pages_stream};
    use crate::api::stream::{
        extract_tables_stream_from_doc_with_geometries,
        extract_tables_stream_from_doc_with_settings,
    };
    use crate::document::PDFDocument;
    use crate::error::Result;
    use crate::table::{PageGeometry, TableProbePolicy, TableSettings};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex, OnceLock};

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

    fn build_table_pdf_with_text() -> Vec<u8> {
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
        push_obj(
            &mut out,
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
            &mut offsets,
        );

        // 3: Page with font + contents
        push_obj(
            &mut out,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n".to_string(),
            &mut offsets,
        );

        let stream = "0 0 0 RG\n0 0 0 rg\n1 w\n0 0 100 50 re S\n50 0 m 50 50 l S\nBT /F1 12 Tf 10 20 Td (Total) Tj ET\n";
        push_obj(
            &mut out,
            format!(
                "4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
                stream.len(),
                stream
            ),
            &mut offsets,
        );

        // 5: Font
        push_obj(
            &mut out,
            "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string(),
            &mut offsets,
        );

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
        let pages: Vec<_> = extract_pages_stream(&pdf_data, Some(options))
            .unwrap()
            .collect::<Result<Vec<_>>>()
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
        let _guard = crate::api::stream::stream_usage_test_guard();
        let pdf_data = build_minimal_pdf_with_pages(2);
        crate::api::stream::set_stream_usage_enabled(true);
        crate::api::stream::take_stream_usage();

        let options = ExtractOptions::default();
        let pages: Vec<_> = extract_pages_stream(&pdf_data, Some(options))
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(pages.len(), 2);
        let usage = crate::api::stream::take_stream_usage();
        crate::api::stream::set_stream_usage_enabled(false);
        assert!(usage >= 1);
    }

    #[test]
    fn test_extract_tables_with_document_parallel_ordered() {
        let pdf_data = build_minimal_pdf_with_pages(3);
        let doc = Arc::new(PDFDocument::new(&pdf_data, "").unwrap());
        let options = ExtractOptions::default();
        let settings = TableSettings::default();

        let tables: Vec<PageTables> =
            extract_tables_stream_from_doc_with_settings(Arc::clone(&doc), options, settings)
                .unwrap()
                .map(|r| r.map(|(_, t)| t))
                .collect::<Result<Vec<_>>>()
                .unwrap();
        assert_eq!(tables.len(), 3);
    }

    #[test]
    fn test_extract_tables_with_document_geometries_length_mismatch() {
        let pdf_data = build_minimal_pdf_with_pages(2);
        let doc = Arc::new(PDFDocument::new(&pdf_data, "").unwrap());
        let options = ExtractOptions::default();
        let settings = TableSettings::default();
        let geom = PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };

        let err = match extract_tables_stream_from_doc_with_geometries(
            Arc::clone(&doc),
            options,
            settings,
            vec![geom],
        ) {
            Ok(_) => panic!("expected geometry count mismatch error"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("geometry"));
    }

    #[test]
    fn table_probe_policy_controls_skip() {
        let pdf_data = build_minimal_pdf_with_pages(1);
        let doc = Arc::new(PDFDocument::new(&pdf_data, "").unwrap());
        let options = ExtractOptions::default();
        let settings = TableSettings {
            probe_policy: TableProbePolicy::Always,
            ..Default::default()
        };

        crate::layout::table::edge_probe::take_probe_calls();
        let out: Vec<PageTables> =
            extract_tables_stream_from_doc_with_settings(Arc::clone(&doc), options, settings)
                .unwrap()
                .map(|r| r.map(|(_, t)| t))
                .collect::<Result<Vec<_>>>()
                .unwrap();
        assert_eq!(out.len(), 1);
        let calls = crate::layout::table::edge_probe::take_probe_calls();
        assert!(calls > 0);
    }

    #[test]
    fn table_text_output_matches_before() {
        let pdf_data = build_table_pdf_with_text();
        let doc = Arc::new(PDFDocument::new(&pdf_data, "").unwrap());
        let options = ExtractOptions::default();
        let settings = TableSettings::default();

        let out: Vec<PageTables> =
            extract_tables_stream_from_doc_with_settings(Arc::clone(&doc), options, settings)
                .unwrap()
                .map(|r| r.map(|(_, t)| t))
                .collect::<Result<Vec<_>>>()
                .unwrap();
        let found = out
            .iter()
            .flatten()
            .flatten()
            .flatten()
            .any(|c| c.as_deref() == Some("Total"));
        assert!(found, "tables: {:?}", out);
    }
}
