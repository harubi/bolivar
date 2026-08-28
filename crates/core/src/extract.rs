//! High-level text extraction API - port of pdfminer.six high_level.py
//!
//! Public extraction entry points. Each function is a thin call into the engine.
//!
//! # Call graph
//!
//! Public entry points (callers should use these):
//! - [`extract_text`] — PDF bytes → `String`
//! - [`extract_text_with_document`] — pre-parsed `PDFDocument` → `String`
//! - [`extract_text_to_fp`] — PDF bytes → writer
//! - [`extract_text_stream_from_doc_with_geometries`] — arena-backed text stream
//! - [`extract_pages_stream`] / [`extract_pages_stream_from_doc`] — analyzed pages
//! - [`extract_tables_stream_from_doc`] family — table streams
//! - [`extract_words_stream_from_doc_with_geometries`] — arena-backed word stream
//!
//! Internal helpers (file-private):
//! - `extract_text_to_fp_impl` — bytes path: validates header, opens doc, delegates
//! - `extract_text_to_fp_from_doc_impl` — drives a `TextConverter` over pages
//!
//! `extract_text` and `extract_text_to_fp` both lower into `*_from_doc_impl`,
//! so the actual rendering logic lives in exactly one place.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;

use crate::device::TextConverter;
use crate::device::{PDFPageAggregator, PDFTableCollector};
use crate::document::PDFDocument;
use crate::document::catalog::DEFAULT_CACHE_CAPACITY;
// Re-export engine items via `extract` so callers can reach `ExtractOptions`,
// `process_page`, `aggregator_result`, etc. through the canonical extract path.
pub use crate::engine::{
    ExecutionPlan, ExtractOptions, PageTables, Stream, aggregator_result, collector_result,
    no_precheck, no_precheck_cancellable, process_page, process_page_with_cancellation, run_batch,
    run_stream, run_stream_cancellable, validate_geometry_count,
};
use crate::error::{PdfError, Result};
use crate::image::ImageWriter;
use crate::interp::PDFResourceManager;
use crate::layout::{LAParams, LTPage};
use crate::table::probe::{page_has_edges_with_cancellation, should_probe_tables};
use crate::table::{
    PageGeometry, TableMetadata, TableSettings, TextSettings, WordObj,
    collect_table_objects_from_arena, extract_tables_from_objects_with_cancellation,
    extract_tables_with_metadata_from_objects_with_cancellation,
    extract_text_from_objects_borrowed, extract_words_from_objects_borrowed,
};

fn cache_capacity(caching: bool) -> usize {
    if caching { DEFAULT_CACHE_CAPACITY } else { 0 }
}

fn index_geometries(
    page_count: usize,
    order: &[usize],
    geometries: Vec<PageGeometry>,
) -> Result<Arc<[Option<PageGeometry>]>> {
    validate_geometry_count(order, geometries.len())?;

    let mut by_page: Vec<Option<PageGeometry>> =
        std::iter::repeat_with(|| None).take(page_count).collect();
    for (page_idx, geometry) in order.iter().copied().zip(geometries) {
        by_page[page_idx] = Some(geometry);
    }
    Ok(by_page.into())
}

fn geometry_for_page(
    geometries: &[Option<PageGeometry>],
    page_idx: usize,
) -> Result<&PageGeometry> {
    geometries
        .get(page_idx)
        .and_then(Option::as_ref)
        .ok_or_else(|| PdfError::DecodeError("page geometry not in plan".to_string()))
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
/// use bolivar_core::extract::extract_text;
/// use bolivar_core::engine::ExtractOptions;
///
/// let pdf_bytes = std::fs::read("document.pdf")?;
/// let text = extract_text(&pdf_bytes, None)?;
/// println!("{}", text);
/// ```
#[hotpath::measure]
pub fn extract_text(pdf_data: &[u8], options: Option<ExtractOptions>) -> Result<String> {
    let options = options.unwrap_or_default();
    let doc =
        PDFDocument::new_with_cache(pdf_data, &options.password, cache_capacity(options.caching))?;
    extract_text_with_document(&doc, options)
}

/// Extract text from an already-parsed PDFDocument.
pub fn extract_text_with_document(doc: &PDFDocument, options: ExtractOptions) -> Result<String> {
    // Create output buffer
    let mut output = Vec::new();

    extract_text_to_fp_from_doc_impl(doc, &mut output, &options)?;

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
/// use bolivar_core::extract::extract_text_to_fp;
/// use bolivar_core::engine::ExtractOptions;
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

    // Validate PDF header
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    // Parse PDF document
    let doc =
        PDFDocument::new_with_cache(pdf_data, &options.password, cache_capacity(options.caching))?;
    extract_text_to_fp_from_doc_impl(&doc, writer, &options)
}

#[hotpath::measure]
fn extract_text_to_fp_from_doc_impl<W: Write>(
    doc: &PDFDocument,
    writer: &mut W,
    options: &ExtractOptions,
) -> Result<()> {
    // Get LAParams (use default if not provided)
    let laparams = options.laparams.unwrap_or_default();

    // Create text converter
    let mut converter = TextConverter::new(writer, "utf-8", 1, Some(laparams), false);

    let results = run_batch(
        doc,
        options.page_numbers.as_deref(),
        options.maxpages,
        |arena, page_idx, page, doc| {
            let mut rsrcmgr = PDFResourceManager::with_caching(options.caching);
            let mut aggregator = PDFPageAggregator::new(Some(laparams), page_idx as i32 + 1, arena);
            let mut ltpage = process_page(
                page,
                &mut aggregator,
                &mut rsrcmgr,
                options.rotation,
                doc,
                aggregator_result,
            )?;
            if options.bidi {
                ltpage.set_bidi(true);
            }
            Ok(ltpage)
        },
    )?;

    for (_, ltpage) in results {
        converter.receive_layout(ltpage);
    }

    Ok(())
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
#[hotpath::measure]
pub fn extract_pages_with_images_with_document(
    doc: Arc<PDFDocument>,
    options: ExtractOptions,
    output_dir: &str,
) -> Result<Vec<LTPage>> {
    let laparams = options.laparams.unwrap_or_default();
    let caching = options.caching;
    let rotation = options.rotation;
    let bidi = options.bidi;
    let output_dir = output_dir.to_string();

    let stream = run_stream_cancellable(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck_cancellable::<LTPage>,
        move |arena, page_idx, page, doc, cancellation| {
            cancellation.check()?;
            let writer = ImageWriter::for_page(&output_dir, page_idx)?;
            let writer = Rc::new(RefCell::new(writer));
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut aggregator = PDFPageAggregator::new_with_imagewriter(
                Some(laparams),
                page_idx as i32 + 1,
                Some(writer),
                arena,
            );
            let mut ltpage = process_page_with_cancellation(
                page,
                &mut aggregator,
                &mut rsrcmgr,
                rotation,
                doc,
                cancellation,
                aggregator_result,
            )?;
            if bidi {
                ltpage.set_bidi(true);
            }
            Ok(ltpage)
        },
    )?;

    stream
        .map(|r| r.map(|(_, page)| page))
        .collect::<Result<Vec<_>>>()
}

pub fn extract_pages_stream_from_doc(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
) -> Result<Stream<LTPage>> {
    #[cfg(test)]
    record_stream_usage();

    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    let laparams = options.laparams;
    let caching = options.caching;
    let rotation = options.rotation;
    let bidi = options.bidi;

    run_stream_cancellable(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck_cancellable::<LTPage>,
        move |arena, page_idx, page, doc, cancellation| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut aggregator = PDFPageAggregator::new(laparams, page_idx as i32 + 1, arena);
            let mut ltpage = process_page_with_cancellation(
                page,
                &mut aggregator,
                &mut rsrcmgr,
                rotation,
                doc,
                cancellation,
                aggregator_result,
            )?;
            if bidi {
                ltpage.set_bidi(true);
            }
            Ok(ltpage)
        },
    )
}

pub fn extract_tables_stream_from_doc(
    doc: Arc<PDFDocument>,
    options: ExtractOptions,
) -> Result<Stream<PageTables>> {
    extract_tables_stream_from_doc_with_geometries_internal(
        doc,
        options,
        TableSettings::default(),
        None,
    )
}

pub fn extract_tables_stream_from_doc_with_settings(
    doc: Arc<PDFDocument>,
    options: ExtractOptions,
    settings: TableSettings,
) -> Result<Stream<PageTables>> {
    extract_tables_stream_from_doc_with_geometries_internal(doc, options, settings, None)
}

pub fn extract_tables_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    options: ExtractOptions,
    settings: TableSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Stream<PageTables>> {
    extract_tables_stream_from_doc_with_geometries_internal(
        doc,
        options,
        settings,
        Some(geometries),
    )
}

/// Stream per-page table metadata for selected pages using arena-backed collection.
///
/// Runs a single arena walk per page and produces `Vec<TableMetadata>` directly,
/// avoiding the double-layout-walk path that consumed `LTPage` objects.
pub fn extract_tables_metadata_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    mut settings: TableSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Stream<Vec<TableMetadata>>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    settings.text_settings.bidi |= options.bidi;

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    let geoms = index_geometries(doc.page_index().len(), &plan.order, geometries)?;
    let laparams = options.laparams;
    let caching = options.caching;

    run_stream_cancellable(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck_cancellable::<Vec<TableMetadata>>,
        move |arena, page_idx, page, doc, cancellation| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector = PDFTableCollector::new(laparams, page_idx as i32 + 1, arena);
            let page_arena = process_page_with_cancellation(
                page,
                &mut collector,
                &mut rsrcmgr,
                0,
                doc,
                cancellation,
                collector_result,
            )?;
            let arena_lookup = collector.arena_lookup();
            let geom = geometry_for_page(&geoms, page_idx)?;
            let (chars, edges) = collect_table_objects_from_arena(&page_arena, geom);
            extract_tables_with_metadata_from_objects_with_cancellation(
                chars,
                edges,
                geom,
                &settings,
                arena_lookup,
                cancellation,
            )
        },
    )
}

/// Stream analyzed layout and table metadata from one interpreter pass per page.
pub fn extract_layout_tables_metadata_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    mut settings: TableSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Stream<(LTPage, Vec<TableMetadata>)>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    settings.text_settings.bidi |= options.bidi;

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    let geoms = index_geometries(doc.page_index().len(), &plan.order, geometries)?;
    let laparams = options.laparams;
    let caching = options.caching;
    let rotation = options.rotation;
    let bidi = options.bidi;

    run_stream_cancellable(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck_cancellable::<(LTPage, Vec<TableMetadata>)>,
        move |arena, page_idx, page, doc, cancellation| {
            #[cfg(test)]
            record_combined_pass();

            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector = PDFTableCollector::new(laparams, page_idx as i32 + 1, arena);
            let page_arena = process_page_with_cancellation(
                page,
                &mut collector,
                &mut rsrcmgr,
                rotation,
                doc,
                cancellation,
                collector_result,
            )?;
            let arena_lookup = collector.arena_lookup();
            let geom = geometry_for_page(&geoms, page_idx)?;
            let (chars, edges) = collect_table_objects_from_arena(&page_arena, geom);
            let tables = extract_tables_with_metadata_from_objects_with_cancellation(
                chars,
                edges,
                geom,
                &settings,
                arena_lookup,
                cancellation,
            )?;
            cancellation.check()?;
            let mut layout_page = page_arena.materialize(arena_lookup);
            if let Some(laparams) = laparams {
                layout_page.analyze(&laparams);
            }
            cancellation.check()?;
            if bidi {
                layout_page.set_bidi(true);
            }
            Ok((layout_page, tables))
        },
    )
}

/// Stream per-page text for selected pages using arena-backed collection.
pub fn extract_text_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    mut settings: TextSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Stream<String>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    settings.bidi |= options.bidi;

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    let geoms = index_geometries(doc.page_index().len(), &plan.order, geometries)?;
    let laparams = options.laparams;
    let caching = options.caching;

    run_stream_cancellable(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck_cancellable::<String>,
        move |arena, page_idx, page, doc, cancellation| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector = PDFTableCollector::new(laparams, page_idx as i32 + 1, arena);
            let page_arena = process_page_with_cancellation(
                page,
                &mut collector,
                &mut rsrcmgr,
                0,
                doc,
                cancellation,
                collector_result,
            )?;
            let arena_lookup = collector.arena_lookup();
            let geom = geometry_for_page(&geoms, page_idx)?;
            let (chars, _edges) = collect_table_objects_from_arena(&page_arena, geom);
            cancellation.check()?;
            let text = extract_text_from_objects_borrowed(chars, &settings, arena_lookup);
            cancellation.check()?;
            Ok(text)
        },
    )
}

/// Stream per-page words for selected pages using arena-backed collection.
pub fn extract_words_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    mut settings: TextSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Stream<Vec<WordObj>>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    settings.bidi |= options.bidi;

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    let geoms = index_geometries(doc.page_index().len(), &plan.order, geometries)?;
    let laparams = options.laparams;
    let caching = options.caching;

    run_stream_cancellable(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck_cancellable::<Vec<WordObj>>,
        move |arena, page_idx, page, doc, cancellation| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector = PDFTableCollector::new(laparams, page_idx as i32 + 1, arena);
            let page_arena = process_page_with_cancellation(
                page,
                &mut collector,
                &mut rsrcmgr,
                0,
                doc,
                cancellation,
                collector_result,
            )?;
            let arena_lookup = collector.arena_lookup();
            let geom = geometry_for_page(&geoms, page_idx)?;
            let (chars, _edges) = collect_table_objects_from_arena(&page_arena, geom);
            cancellation.check()?;
            let words = extract_words_from_objects_borrowed(chars, &settings, arena_lookup);
            cancellation.check()?;
            Ok(words)
        },
    )
}

fn extract_tables_stream_from_doc_with_geometries_internal(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    mut settings: TableSettings,
    geometries: Option<Vec<PageGeometry>>,
) -> Result<Stream<PageTables>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    settings.text_settings.bidi |= options.bidi;

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    let geometries = match geometries {
        Some(geometries) => Some(index_geometries(
            doc.page_index().len(),
            &plan.order,
            geometries,
        )?),
        None => None,
    };
    let laparams = options.laparams;
    let caching = options.caching;
    let settings_for_pre = settings.clone();
    let settings_for_run = settings;
    let geoms = geometries;

    run_stream_cancellable(
        doc,
        options.page_numbers.clone(),
        options.maxpages,
        move |_page_idx, page, doc, cancellation| {
            if !should_probe_tables(&settings_for_pre) {
                return Ok(None);
            }

            // Cheap edge probe before running the full interpreter — preserves the
            // original skip path so text-only PDFs don't pay table-collector cost.
            let has_edges = page_has_edges_with_cancellation(page, doc, caching, cancellation)?;
            if has_edges {
                Ok(None)
            } else {
                Ok(Some(Vec::new()))
            }
        },
        move |arena, page_idx, page, doc, cancellation| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector = PDFTableCollector::new(laparams, page_idx as i32 + 1, arena);
            let page_arena = process_page_with_cancellation(
                page,
                &mut collector,
                &mut rsrcmgr,
                0,
                doc,
                cancellation,
                collector_result,
            )?;
            let arena_lookup = collector.arena_lookup();
            let default_geometry;
            let geom = match geoms.as_ref() {
                Some(g) => geometry_for_page(g, page_idx)?,
                None => {
                    default_geometry = PageGeometry {
                        page_bbox: page_arena.bbox,
                        mediabox: page_arena.bbox,
                        initial_doctop: 0.0,
                        force_crop: false,
                    };
                    &default_geometry
                }
            };
            let (chars, edges) = collect_table_objects_from_arena(&page_arena, geom);
            extract_tables_from_objects_with_cancellation(
                chars,
                edges,
                geom,
                &settings_for_run,
                arena_lookup,
                cancellation,
            )
        },
    )
}

#[cfg(test)]
use std::sync::atomic::Ordering;

#[cfg(test)]
static STREAM_USAGE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static STREAM_USAGE_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static STREAM_USAGE_TEST_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();
#[cfg(test)]
static COMBINED_PASS_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static COMBINED_PASS_COUNT_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static COMBINED_PASS_TEST_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();

#[cfg(test)]
fn record_stream_usage() {
    if STREAM_USAGE_ENABLED.load(Ordering::Relaxed) {
        STREAM_USAGE.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
fn record_combined_pass() {
    if COMBINED_PASS_COUNT_ENABLED.load(Ordering::Relaxed) {
        COMBINED_PASS_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
fn take_combined_pass_count() -> usize {
    COMBINED_PASS_COUNT.swap(0, Ordering::Relaxed)
}

#[cfg(test)]
fn set_combined_pass_count_enabled(enabled: bool) {
    COMBINED_PASS_COUNT_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
fn combined_pass_test_guard() -> std::sync::MutexGuard<'static, ()> {
    COMBINED_PASS_TEST_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .expect("combined pass test lock")
}

#[cfg(test)]
pub(crate) fn take_stream_usage() -> usize {
    STREAM_USAGE.swap(0, Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn set_stream_usage_enabled(enabled: bool) {
    STREAM_USAGE_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn stream_usage_test_guard() -> std::sync::MutexGuard<'static, ()> {
    STREAM_USAGE_TEST_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .expect("stream usage test lock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::processor;
    use crate::table::{TableProbePolicy, TableSettings};
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    static THREAD_LOG_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn thread_log_guard() -> std::sync::MutexGuard<'static, ()> {
        THREAD_LOG_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("thread log test lock")
    }

    fn full_page_geometries(page_count: usize) -> Vec<PageGeometry> {
        let geom = PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        vec![geom; page_count]
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

        processor::clear_thread_log();

        let options = ExtractOptions::default();
        let pages: Vec<_> = extract_pages_stream(&pdf_data, Some(options))
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(pages.len(), 4);

        let records = processor::take_thread_log();
        let used_pool = records.iter().any(|record| record.in_pool);
        assert!(used_pool, "expected rayon pool to be used");

        let unique: HashSet<_> = records.iter().map(|record| record.id).collect();
        assert!(!unique.is_empty(), "expected at least one recorded thread");
    }

    #[test]
    fn test_extract_pages_uses_stream_path() {
        let _guard = stream_usage_test_guard();
        let pdf_data = build_minimal_pdf_with_pages(2);
        set_stream_usage_enabled(true);
        take_stream_usage();

        let options = ExtractOptions::default();
        let pages: Vec<_> = extract_pages_stream(&pdf_data, Some(options))
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(pages.len(), 2);
        let usage = take_stream_usage();
        set_stream_usage_enabled(false);
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

    fn table_probe_calls(settings: TableSettings) -> usize {
        let pdf_data = build_minimal_pdf_with_pages(1);
        let doc = Arc::new(PDFDocument::new(&pdf_data, "").unwrap());
        let options = ExtractOptions::default();

        crate::layout::table::probe::take_probe_calls();
        let out: Vec<PageTables> =
            extract_tables_stream_from_doc_with_settings(Arc::clone(&doc), options, settings)
                .unwrap()
                .map(|r| r.map(|(_, t)| t))
                .collect::<Result<Vec<_>>>()
                .unwrap();
        assert_eq!(out.len(), 1);
        crate::layout::table::probe::take_probe_calls()
    }

    #[test]
    fn table_probe_policy_always_runs_probe() {
        let settings = TableSettings {
            probe_policy: TableProbePolicy::Always,
            ..Default::default()
        };

        assert!(table_probe_calls(settings) > 0);
    }

    #[test]
    fn table_probe_policy_never_bypasses_probe() {
        let settings = TableSettings {
            probe_policy: TableProbePolicy::Never,
            ..Default::default()
        };

        assert_eq!(table_probe_calls(settings), 0);
    }

    #[test]
    fn table_probe_policy_auto_bypasses_probe_for_text_strategy() {
        let settings = TableSettings {
            vertical_strategy: crate::table::TableStrategy::Text,
            ..Default::default()
        };

        assert_eq!(table_probe_calls(settings), 0);
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

    #[test]
    fn combined_layout_and_tables_use_one_interpreter_pass() {
        use crate::layout::{LTItem, LTTextBox, TextBoxType};

        let _guard = combined_pass_test_guard();
        let pdf_data = build_table_pdf_with_text();
        let doc = Arc::new(PDFDocument::new(&pdf_data, "").unwrap());
        let options = ExtractOptions::default();
        let settings = TableSettings::default();

        set_combined_pass_count_enabled(true);
        take_combined_pass_count();
        let mut out = extract_layout_tables_metadata_stream_from_doc_with_geometries(
            Arc::clone(&doc),
            options,
            settings,
            full_page_geometries(1),
        )
        .unwrap()
        .collect::<Result<Vec<_>>>()
        .unwrap();

        assert_eq!(take_combined_pass_count(), 1);
        set_combined_pass_count_enabled(false);
        let (_, (page, tables)) = out.pop().unwrap();
        let page_has_text = page.iter().any(|item| match item {
            LTItem::TextBox(TextBoxType::Horizontal(text_box)) => {
                text_box.get_text().contains("Total")
            }
            LTItem::TextBox(TextBoxType::Vertical(text_box)) => {
                text_box.get_text().contains("Total")
            }
            _ => false,
        });
        let table_has_text = tables
            .iter()
            .flat_map(|table| &table.cells)
            .any(|cell| cell.text == "Total");
        assert!(page_has_text);
        assert!(table_has_text);
    }

    #[test]
    fn test_page_stream_only_creates_requested_pages() {
        // Hold the stream-usage guard: this test calls `extract_pages_stream_from_doc`
        // which bumps the global STREAM_USAGE counter when the flag is enabled by a
        // peer test, and we don't want to race with those checks.
        let _guard = stream_usage_test_guard();
        let pdf = build_minimal_pdf_with_pages(5);
        let doc = Arc::new(PDFDocument::new(pdf, "").unwrap());
        crate::document::page::reset_page_create_count(doc.as_ref());

        let options = ExtractOptions {
            page_numbers: Some(vec![2]),
            ..ExtractOptions::default()
        };

        let stream = extract_pages_stream_from_doc(Arc::clone(&doc), options).unwrap();
        let _ = stream.collect::<Result<Vec<_>>>().unwrap();

        let created = crate::document::page::take_page_create_count(doc.as_ref());
        assert_eq!(created, 1);
    }

    #[test]
    fn test_tables_stream_uses_geometries_len_mismatch() {
        let pdf = build_minimal_pdf_with_pages(2);
        let doc = PDFDocument::new(pdf, "").unwrap();
        let options = ExtractOptions::default();
        let settings = TableSettings::default();
        let geoms = full_page_geometries(1);

        let err =
            extract_tables_stream_from_doc_with_geometries(doc.into(), options, settings, geoms);
        assert!(err.is_err());
        if let Err(err) = err {
            assert!(err.to_string().contains("geometry count"));
        }
    }

    #[test]
    fn tables_stream_with_settings_smoke() {
        let pdf = build_minimal_pdf_with_pages(1);
        let doc = Arc::new(PDFDocument::new(pdf, "").unwrap());
        let options = ExtractOptions::default();
        let settings = TableSettings::default();
        let out = extract_tables_stream_from_doc_with_settings(doc, options, settings)
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn text_pages_with_geometries_avoid_ltpage_stream_usage() {
        let _guard = stream_usage_test_guard();
        let pdf = build_minimal_pdf_with_pages(2);
        let doc = Arc::new(PDFDocument::new(pdf, "").unwrap());
        let options = ExtractOptions::default();
        let geoms = full_page_geometries(2);

        set_stream_usage_enabled(true);
        take_stream_usage();
        let out = extract_text_stream_from_doc_with_geometries(
            Arc::clone(&doc),
            options,
            crate::table::TextSettings::default(),
            geoms,
        )
        .unwrap()
        .collect::<Result<Vec<_>>>()
        .unwrap();
        let usage = take_stream_usage();
        set_stream_usage_enabled(false);

        assert_eq!(out.len(), 2);
        assert_eq!(usage, 0);
    }

    #[test]
    fn words_pages_with_geometries_avoid_ltpage_stream_usage() {
        let _guard = stream_usage_test_guard();
        let pdf = build_minimal_pdf_with_pages(2);
        let doc = Arc::new(PDFDocument::new(pdf, "").unwrap());
        let options = ExtractOptions::default();
        let geoms = full_page_geometries(2);

        set_stream_usage_enabled(true);
        take_stream_usage();
        let out = extract_words_stream_from_doc_with_geometries(
            Arc::clone(&doc),
            options,
            crate::table::TextSettings::default(),
            geoms,
        )
        .unwrap()
        .collect::<Result<Vec<_>>>()
        .unwrap();
        let usage = take_stream_usage();
        set_stream_usage_enabled(false);

        assert_eq!(out.len(), 2);
        assert_eq!(usage, 0);
    }

    #[test]
    fn table_stream_early_drop_does_not_wait_for_workers() {
        let pdf = build_minimal_pdf_with_pages(64);
        let doc = Arc::new(PDFDocument::new(pdf, "").unwrap());
        let stream = extract_tables_stream_from_doc_with_settings(
            Arc::clone(&doc),
            ExtractOptions::default(),
            TableSettings::default(),
        )
        .unwrap();

        let start = std::time::Instant::now();
        drop(stream);
        assert!(start.elapsed() < std::time::Duration::from_millis(500));
    }
}
