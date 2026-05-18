//! Streaming extraction primitives.

use std::sync::Arc;

use crate::api::pipeline::{
    ExecutionPlan, Stream, no_precheck, run_stream, validate_geometry_count,
};
use crate::converter::{PDFPageAggregator, PDFTableCollector};
use crate::document::PDFDocument;
use crate::error::{PdfError, Result};
use crate::interp::PDFResourceManager;
use crate::layout::{LAParams, LTPage};
use crate::table::edge_probe::{page_has_edges, should_skip_tables};
use crate::table::{
    PageGeometry, TableMetadata, TableSettings, TextSettings, WordObj,
    collect_table_objects_from_arena, extract_tables_from_objects,
    extract_tables_with_metadata_from_objects, extract_text_from_objects,
    extract_words_from_objects,
};

use super::high_level::{
    ExtractOptions, PageTables, aggregator_result, collector_result, process_page,
};

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
fn record_stream_usage() {
    if STREAM_USAGE_ENABLED.load(Ordering::Relaxed) {
        STREAM_USAGE.fetch_add(1, Ordering::Relaxed);
    }
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

pub fn extract_pages_stream_from_doc(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
) -> Result<Stream<LTPage>> {
    #[cfg(test)]
    record_stream_usage();

    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }
    let laparams = options.laparams.clone();
    let caching = options.caching;
    let rotation = options.rotation;

    run_stream(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck::<LTPage>,
        move |arena, page_idx, page, doc| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut aggregator =
                PDFPageAggregator::new(laparams.clone(), page_idx as i32 + 1, arena);
            process_page(
                page,
                &mut aggregator,
                &mut rsrcmgr,
                rotation,
                doc,
                aggregator_result,
            )
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
    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    validate_geometry_count(&plan.order, geometries.len())?;
    extract_tables_stream_from_doc_with_geometries_internal(
        doc,
        options,
        settings,
        Some(Arc::new(geometries)),
    )
}

/// Stream per-page table metadata for selected pages using arena-backed collection.
///
/// Runs a single arena walk per page and produces `Vec<TableMetadata>` directly,
/// avoiding the double-layout-walk path that consumed `LTPage` objects.
pub fn extract_tables_metadata_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    settings: TableSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Stream<Vec<TableMetadata>>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    validate_geometry_count(&plan.order, geometries.len())?;

    let order_index: std::collections::HashMap<usize, usize> = plan
        .order
        .iter()
        .enumerate()
        .map(|(i, &p)| (p, i))
        .collect();
    let laparams = options.laparams.clone();
    let caching = options.caching;
    let geoms = Arc::new(geometries);

    run_stream(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck::<Vec<TableMetadata>>,
        move |arena, page_idx, page, doc| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector =
                PDFTableCollector::new(laparams.clone(), page_idx as i32 + 1, arena);
            let page_arena = process_page(page, &mut collector, &mut rsrcmgr, 0, doc, |c| {
                collector_result(c)
            })?;
            let arena_lookup = collector.arena_lookup();
            let selected_idx = *order_index
                .get(&page_idx)
                .ok_or_else(|| PdfError::DecodeError("page not in plan".to_string()))?;
            let geom = &geoms[selected_idx];
            let (chars, edges) = collect_table_objects_from_arena(&page_arena, geom);
            Ok(extract_tables_with_metadata_from_objects(
                chars,
                edges,
                geom,
                &settings,
                arena_lookup,
            ))
        },
    )
}

/// Stream per-page text for selected pages using arena-backed collection.
pub fn extract_text_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    settings: TextSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Stream<String>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    validate_geometry_count(&plan.order, geometries.len())?;

    let order_index: std::collections::HashMap<usize, usize> = plan
        .order
        .iter()
        .enumerate()
        .map(|(i, &p)| (p, i))
        .collect();
    let laparams = options.laparams.clone();
    let caching = options.caching;
    let geoms = Arc::new(geometries);

    run_stream(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck::<String>,
        move |arena, page_idx, page, doc| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector =
                PDFTableCollector::new(laparams.clone(), page_idx as i32 + 1, arena);
            let page_arena = process_page(page, &mut collector, &mut rsrcmgr, 0, doc, |c| {
                collector_result(c)
            })?;
            let arena_lookup = collector.arena_lookup();
            let selected_idx = *order_index
                .get(&page_idx)
                .ok_or_else(|| PdfError::DecodeError("page not in plan".to_string()))?;
            let geom = &geoms[selected_idx];
            let (chars, _edges) = collect_table_objects_from_arena(&page_arena, geom);
            Ok(extract_text_from_objects(
                chars,
                settings.clone(),
                arena_lookup,
            ))
        },
    )
}

/// Stream per-page words for selected pages using arena-backed collection.
pub fn extract_words_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    settings: TextSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Stream<Vec<WordObj>>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    validate_geometry_count(&plan.order, geometries.len())?;

    let order_index: std::collections::HashMap<usize, usize> = plan
        .order
        .iter()
        .enumerate()
        .map(|(i, &p)| (p, i))
        .collect();
    let laparams = options.laparams.clone();
    let caching = options.caching;
    let geoms = Arc::new(geometries);

    run_stream(
        doc,
        options.page_numbers,
        options.maxpages,
        no_precheck::<Vec<WordObj>>,
        move |arena, page_idx, page, doc| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector =
                PDFTableCollector::new(laparams.clone(), page_idx as i32 + 1, arena);
            let page_arena = process_page(page, &mut collector, &mut rsrcmgr, 0, doc, |c| {
                collector_result(c)
            })?;
            let arena_lookup = collector.arena_lookup();
            let selected_idx = *order_index
                .get(&page_idx)
                .ok_or_else(|| PdfError::DecodeError("page not in plan".to_string()))?;
            let geom = &geoms[selected_idx];
            let (chars, _edges) = collect_table_objects_from_arena(&page_arena, geom);
            Ok(extract_words_from_objects(
                chars,
                settings.clone(),
                arena_lookup,
            ))
        },
    )
}

fn extract_tables_stream_from_doc_with_geometries_internal(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    settings: TableSettings,
    geometries: Option<Arc<Vec<PageGeometry>>>,
) -> Result<Stream<PageTables>> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    let plan = ExecutionPlan::new(
        doc.page_index().len(),
        options.page_numbers.as_deref(),
        options.maxpages,
    );
    let order_index: std::collections::HashMap<usize, usize> = plan
        .order
        .iter()
        .enumerate()
        .map(|(i, &p)| (p, i))
        .collect();
    let laparams = options.laparams.clone();
    let caching = options.caching;
    let settings_for_pre = settings.clone();
    let settings_for_run = settings;
    let geoms = geometries;

    run_stream(
        doc,
        options.page_numbers.clone(),
        options.maxpages,
        move |_page_idx, page, doc| {
            // Cheap edge probe before running the full interpreter — preserves the
            // original skip path so text-only PDFs don't pay table-collector cost.
            let has_edges = page_has_edges(page, doc, caching)?;
            if should_skip_tables(&settings_for_pre, has_edges) {
                Ok(Some(Vec::new()))
            } else {
                Ok(None)
            }
        },
        move |arena, page_idx, page, doc| {
            let mut rsrcmgr = PDFResourceManager::with_caching(caching);
            let mut collector =
                PDFTableCollector::new(laparams.clone(), page_idx as i32 + 1, arena);
            let page_arena = process_page(page, &mut collector, &mut rsrcmgr, 0, doc, |c| {
                collector_result(c)
            })?;
            let arena_lookup = collector.arena_lookup();
            let selected_idx = *order_index
                .get(&page_idx)
                .ok_or_else(|| PdfError::DecodeError("page not in plan".to_string()))?;
            let geom = match geoms.as_ref() {
                Some(g) => g[selected_idx].clone(),
                None => PageGeometry {
                    page_bbox: page_arena.bbox,
                    mediabox: page_arena.bbox,
                    initial_doctop: 0.0,
                    force_crop: false,
                },
            };
            let (chars, edges) = collect_table_objects_from_arena(&page_arena, &geom);
            Ok(extract_tables_from_objects(
                chars,
                edges,
                &geom,
                &settings_for_run,
                arena_lookup,
            ))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::pipeline::{
        reset_stream_worker_lifecycle_counters, set_stream_worker_lifecycle_enabled,
        stream_worker_lifecycle_counts, stream_worker_lifecycle_test_guard,
    };

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

        push_obj(
            &mut out,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
            &mut offsets,
        );

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
        let out = super::extract_tables_stream_from_doc_with_settings(doc, options, settings)
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
    fn table_stream_early_drop_releases_real_workers() {
        let _guard = stream_worker_lifecycle_test_guard();

        let pdf = build_minimal_pdf_with_pages(64);
        let doc = Arc::new(PDFDocument::new(pdf, "").unwrap());

        set_stream_worker_lifecycle_enabled(true);
        reset_stream_worker_lifecycle_counters();

        let baseline = stream_worker_lifecycle_counts().2;
        let stream_count = 16usize;
        let mut streams = Vec::with_capacity(stream_count);

        for i in 0..stream_count {
            let mut settings = TableSettings::default();
            let i_f = i as f64;
            settings.snap_x_tolerance = 2.0 + i_f * 0.10;
            settings.snap_y_tolerance = 2.2 + i_f * 0.10;
            settings.join_x_tolerance = 1.5 + i_f * 0.07;
            settings.join_y_tolerance = 1.7 + i_f * 0.07;
            settings.edge_min_length = 3.0 + i_f * 0.15;
            settings.edge_min_length_prefilter = 1.0 + i_f * 0.05;
            settings.intersection_x_tolerance = 2.5 + i_f * 0.09;
            settings.intersection_y_tolerance = 2.7 + i_f * 0.09;

            let mut stream = extract_tables_stream_from_doc_with_settings(
                Arc::clone(&doc),
                ExtractOptions::default(),
                settings,
            )
            .unwrap();
            let _ = stream.next();
            streams.push(stream);
        }

        drop(streams);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let (_, _, active) = stream_worker_lifecycle_counts();
            if active == baseline || std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let (started, exited, active) = stream_worker_lifecycle_counts();
        set_stream_worker_lifecycle_enabled(false);

        assert!(
            started >= stream_count,
            "expected at least {stream_count} workers, got {started}"
        );
        assert_eq!(active, baseline);
        assert_eq!(started, exited);
    }
}
