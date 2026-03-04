//! Streaming extraction primitives.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread::JoinHandle;

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::arena::PageArena;
use crate::converter::{PDFPageAggregator, PDFTableCollector};
use crate::error::{PdfError, Result};
use crate::layout::{LAParams, LTPage};
use crate::pdfdocument::PDFDocument;
use crate::pdfinterp::PDFResourceManager;
use crate::pdfpage::PDFPage;
use crate::table::edge_probe::{page_has_edges, should_skip_tables};
use crate::table::{
    PageGeometry, TableSettings, TextSettings, WordObj, collect_table_objects_from_arena,
    extract_tables_from_objects, extract_text_from_objects, extract_words_from_objects,
};

use super::high_level::{
    ExtractOptions, PageTables, default_thread_count, process_page, process_page_arena,
};

pub const DEFAULT_STREAM_BUFFER_CAPACITY: usize = 50;

#[cfg(test)]
static STREAM_USAGE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static STREAM_USAGE_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static STREAM_USAGE_TEST_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();
#[cfg(test)]
static STREAM_WORKER_LIFECYCLE_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static STREAM_WORKERS_STARTED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static STREAM_WORKERS_EXITED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static STREAM_WORKERS_ACTIVE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static STREAM_WORKER_LIFECYCLE_TEST_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> =
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

#[cfg(test)]
pub(crate) fn set_stream_worker_lifecycle_enabled(enabled: bool) {
    STREAM_WORKER_LIFECYCLE_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn reset_stream_worker_lifecycle_counters() {
    STREAM_WORKERS_STARTED.store(0, Ordering::Relaxed);
    STREAM_WORKERS_EXITED.store(0, Ordering::Relaxed);
    STREAM_WORKERS_ACTIVE.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn stream_worker_lifecycle_counts() -> (usize, usize, usize) {
    (
        STREAM_WORKERS_STARTED.load(Ordering::Relaxed),
        STREAM_WORKERS_EXITED.load(Ordering::Relaxed),
        STREAM_WORKERS_ACTIVE.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
pub(crate) fn stream_worker_lifecycle_test_guard() -> std::sync::MutexGuard<'static, ()> {
    STREAM_WORKER_LIFECYCLE_TEST_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .expect("stream worker lifecycle test lock")
}

#[cfg(test)]
struct StreamWorkerLifecycleCounter {
    tracked: bool,
}

#[cfg(test)]
impl StreamWorkerLifecycleCounter {
    fn start() -> Self {
        if STREAM_WORKER_LIFECYCLE_ENABLED.load(Ordering::Relaxed) {
            STREAM_WORKERS_STARTED.fetch_add(1, Ordering::Relaxed);
            STREAM_WORKERS_ACTIVE.fetch_add(1, Ordering::Relaxed);
            Self { tracked: true }
        } else {
            Self { tracked: false }
        }
    }
}

#[cfg(test)]
impl Drop for StreamWorkerLifecycleCounter {
    fn drop(&mut self) {
        if self.tracked {
            STREAM_WORKERS_EXITED.fetch_add(1, Ordering::Relaxed);
            STREAM_WORKERS_ACTIVE.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

type StreamItem = (usize, Result<LTPage>);

fn build_page_order(doc: &PDFDocument, options: &ExtractOptions) -> Vec<usize> {
    let mut order = Vec::new();
    let page_count = doc.page_index().len();
    let mut selected = 0usize;

    for page_idx in 0..page_count {
        if let Some(ref nums) = options.page_numbers
            && !nums.contains(&page_idx)
        {
            continue;
        }
        if options.maxpages > 0 && selected >= options.maxpages {
            break;
        }
        order.push(page_idx);
        selected += 1;
    }

    order
}

pub struct PageStream {
    rx: Option<Receiver<StreamItem>>,
    order: Vec<usize>,
    next_pos: usize,
    buffer: BTreeMap<usize, Result<LTPage>>,
    done: bool,
    failed: bool,
    max_buffered: usize,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

pub struct TableStream {
    rx: Option<Receiver<(usize, Result<PageTables>)>>,
    order: Vec<usize>,
    next_pos: usize,
    buffer: BTreeMap<usize, Result<PageTables>>,
    done: bool,
    failed: bool,
    max_buffered: usize,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl TableStream {
    fn new(
        rx: Receiver<(usize, Result<PageTables>)>,
        order: Vec<usize>,
        cancel: Arc<AtomicBool>,
        worker: JoinHandle<()>,
    ) -> Self {
        Self {
            rx: Some(rx),
            order,
            next_pos: 0,
            buffer: BTreeMap::new(),
            done: false,
            failed: false,
            max_buffered: 0,
            cancel,
            worker: Some(worker),
        }
    }
}

impl Iterator for TableStream {
    type Item = Result<(usize, PageTables)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }

        loop {
            if self.next_pos >= self.order.len() {
                return None;
            }

            let expected = self.order[self.next_pos];
            if let Some(result) = self.buffer.remove(&expected) {
                self.next_pos += 1;
                if result.is_err() {
                    self.failed = true;
                    self.cancel.store(true, Ordering::Relaxed);
                }
                return Some(result.map(|tables| (expected, tables)));
            }

            if self.done {
                return None;
            }

            let recv_result = match self.rx.as_ref() {
                Some(rx) => rx.recv(),
                None => {
                    self.done = true;
                    return None;
                }
            };

            match recv_result {
                Ok((page_idx, result)) => {
                    self.buffer.insert(page_idx, result);
                    if self.buffer.len() > self.max_buffered {
                        self.max_buffered = self.buffer.len();
                    }
                }
                Err(_) => {
                    self.done = true;
                }
            }
        }
    }
}

impl Drop for TableStream {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.rx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl PageStream {
    fn new(
        rx: Receiver<StreamItem>,
        order: Vec<usize>,
        cancel: Arc<AtomicBool>,
        worker: JoinHandle<()>,
    ) -> Self {
        Self {
            rx: Some(rx),
            order,
            next_pos: 0,
            buffer: BTreeMap::new(),
            done: false,
            failed: false,
            max_buffered: 0,
            cancel,
            worker: Some(worker),
        }
    }

    pub const fn max_buffered(&self) -> usize {
        self.max_buffered
    }
}

impl Iterator for PageStream {
    type Item = Result<LTPage>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }

        loop {
            if self.next_pos >= self.order.len() {
                return None;
            }

            let expected = self.order[self.next_pos];
            if let Some(result) = self.buffer.remove(&expected) {
                self.next_pos += 1;
                if result.is_err() {
                    self.failed = true;
                    self.cancel.store(true, Ordering::Relaxed);
                }
                return Some(result);
            }

            if self.done {
                return None;
            }

            let recv_result = match self.rx.as_ref() {
                Some(rx) => rx.recv(),
                None => {
                    self.done = true;
                    return None;
                }
            };

            match recv_result {
                Ok((page_idx, result)) => {
                    self.buffer.insert(page_idx, result);
                    if self.buffer.len() > self.max_buffered {
                        self.max_buffered = self.buffer.len();
                    }
                }
                Err(_) => {
                    self.done = true;
                }
            }
        }
    }
}

impl Drop for PageStream {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.rx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn extract_pages_stream_from_doc(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
) -> Result<PageStream> {
    #[cfg(test)]
    record_stream_usage();

    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    let laparams = options.laparams.clone();
    let caching = options.caching;
    let order = build_page_order(doc.as_ref(), &options);
    let work_order = order.clone();

    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let (tx, rx) = sync_channel(DEFAULT_STREAM_BUFFER_CAPACITY);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);
    let doc_worker = Arc::clone(&doc);
    let next_index = Arc::new(AtomicUsize::new(0));
    let next_index_worker = Arc::clone(&next_index);

    let worker = std::thread::spawn(move || {
        #[cfg(test)]
        let _worker_lifecycle = StreamWorkerLifecycleCounter::start();

        pool.install(|| {
            (0..thread_count).into_par_iter().for_each(|_| {
                let mut arena = PageArena::new();
                loop {
                    if cancel_worker.load(Ordering::Relaxed) {
                        return;
                    }
                    let pos = next_index_worker.fetch_add(1, Ordering::Relaxed);
                    if pos >= work_order.len() {
                        break;
                    }
                    let page_idx = work_order[pos];
                    let page = match PDFPage::get_page_by_index(doc_worker.as_ref(), page_idx) {
                        Ok(page) => Arc::new(page),
                        Err(e) => {
                            if tx.send((page_idx, Err(e))).is_err() {
                                cancel_worker.store(true, Ordering::Relaxed);
                            }
                            continue;
                        }
                    };
                    doc_worker.cache_page(page_idx, Arc::clone(&page));

                    arena.reset();
                    let mut rsrcmgr = PDFResourceManager::with_caching(caching);
                    let mut aggregator =
                        PDFPageAggregator::new(laparams.clone(), page_idx as i32 + 1, &mut arena);
                    let ltpage =
                        process_page(&page, &mut aggregator, &mut rsrcmgr, doc_worker.as_ref());
                    if cancel_worker.load(Ordering::Relaxed) {
                        return;
                    }
                    if tx.send((page_idx, ltpage)).is_err() {
                        cancel_worker.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            });
        });
    });

    Ok(PageStream::new(rx, order, cancel, worker))
}

pub fn extract_tables_stream_from_doc(
    doc: Arc<PDFDocument>,
    options: ExtractOptions,
) -> Result<TableStream> {
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
) -> Result<TableStream> {
    extract_tables_stream_from_doc_with_geometries_internal(doc, options, settings, None)
}

pub fn extract_tables_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    options: ExtractOptions,
    settings: TableSettings,
    geometries: Vec<PageGeometry>,
) -> Result<TableStream> {
    let geom_count = doc.page_index().len();
    if geometries.len() != geom_count {
        return Err(PdfError::DecodeError(format!(
            "geometry count mismatch: expected {}, got {}",
            geom_count,
            geometries.len()
        )));
    }
    extract_tables_stream_from_doc_with_geometries_internal(
        doc,
        options,
        settings,
        Some(Arc::new(geometries)),
    )
}

/// Extract per-page text for selected pages using arena-backed collection.
pub fn extract_text_pages_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    settings: TextSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Vec<(usize, String)>> {
    let geom_count = doc.page_index().len();
    if geometries.len() != geom_count {
        return Err(PdfError::DecodeError(format!(
            "geometry count mismatch: expected {}, got {}",
            geom_count,
            geometries.len()
        )));
    }
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    let order = build_page_order(doc.as_ref(), &options);
    let laparams = options.laparams.clone();
    let caching = options.caching;

    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let mut results: Vec<(usize, String)> = pool.install(|| {
        order
            .par_iter()
            .map(|&page_idx| {
                let page = PDFPage::get_page_by_index(doc.as_ref(), page_idx)?;
                let mut arena = PageArena::new();
                arena.reset();
                let mut rsrcmgr = PDFResourceManager::with_caching(caching);
                let mut collector =
                    PDFTableCollector::new(laparams.clone(), page_idx as i32 + 1, &mut arena);
                let page_arena =
                    process_page_arena(&page, &mut collector, &mut rsrcmgr, doc.as_ref())?;
                let arena_lookup = collector.arena_lookup();
                let geom = &geometries[page_idx];
                let (chars, _edges) = collect_table_objects_from_arena(&page_arena, geom);
                Ok((
                    page_idx,
                    extract_text_from_objects(chars, settings.clone(), arena_lookup),
                ))
            })
            .collect::<Result<Vec<_>>>()
    })?;

    results.sort_by_key(|(idx, _)| *idx);
    Ok(results)
}

/// Extract per-page words for selected pages using arena-backed collection.
pub fn extract_words_pages_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    settings: TextSettings,
    geometries: Vec<PageGeometry>,
) -> Result<Vec<(usize, Vec<WordObj>)>> {
    let geom_count = doc.page_index().len();
    if geometries.len() != geom_count {
        return Err(PdfError::DecodeError(format!(
            "geometry count mismatch: expected {}, got {}",
            geom_count,
            geometries.len()
        )));
    }
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    let order = build_page_order(doc.as_ref(), &options);
    let laparams = options.laparams.clone();
    let caching = options.caching;

    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let mut results: Vec<(usize, Vec<WordObj>)> = pool.install(|| {
        order
            .par_iter()
            .map(|&page_idx| {
                let page = PDFPage::get_page_by_index(doc.as_ref(), page_idx)?;
                let mut arena = PageArena::new();
                arena.reset();
                let mut rsrcmgr = PDFResourceManager::with_caching(caching);
                let mut collector =
                    PDFTableCollector::new(laparams.clone(), page_idx as i32 + 1, &mut arena);
                let page_arena =
                    process_page_arena(&page, &mut collector, &mut rsrcmgr, doc.as_ref())?;
                let arena_lookup = collector.arena_lookup();
                let geom = &geometries[page_idx];
                let (chars, _edges) = collect_table_objects_from_arena(&page_arena, geom);
                Ok((
                    page_idx,
                    extract_words_from_objects(chars, settings.clone(), arena_lookup),
                ))
            })
            .collect::<Result<Vec<_>>>()
    })?;

    results.sort_by_key(|(idx, _)| *idx);
    Ok(results)
}

fn extract_tables_stream_from_doc_with_geometries_internal(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
    settings: TableSettings,
    geometries: Option<Arc<Vec<PageGeometry>>>,
) -> Result<TableStream> {
    if options.laparams.is_none() {
        options.laparams = Some(LAParams::default());
    }

    let laparams = options.laparams.clone();
    let caching = options.caching;
    let order = build_page_order(doc.as_ref(), &options);
    let work_order = order.clone();

    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let (tx, rx) = sync_channel(DEFAULT_STREAM_BUFFER_CAPACITY);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);
    let doc_worker = Arc::clone(&doc);
    let next_index = Arc::new(AtomicUsize::new(0));
    let next_index_worker = Arc::clone(&next_index);
    let geom_worker = geometries.clone();

    let worker = std::thread::spawn(move || {
        #[cfg(test)]
        let _worker_lifecycle = StreamWorkerLifecycleCounter::start();

        pool.install(|| {
            (0..thread_count).into_par_iter().for_each(|_| {
                let mut arena = PageArena::new();
                loop {
                    if cancel_worker.load(Ordering::Relaxed) {
                        return;
                    }
                    let pos = next_index_worker.fetch_add(1, Ordering::Relaxed);
                    if pos >= work_order.len() {
                        break;
                    }
                    let page_idx = work_order[pos];
                    let page = match PDFPage::get_page_by_index(doc_worker.as_ref(), page_idx) {
                        Ok(page) => Arc::new(page),
                        Err(e) => {
                            if tx.send((page_idx, Err(e))).is_err() {
                                cancel_worker.store(true, Ordering::Relaxed);
                            }
                            continue;
                        }
                    };
                    doc_worker.cache_page(page_idx, Arc::clone(&page));

                    let has_edges = page_has_edges(&page, doc_worker.as_ref(), caching);
                    if should_skip_tables(&settings, has_edges) {
                        if cancel_worker.load(Ordering::Relaxed) {
                            return;
                        }
                        if tx.send((page_idx, Ok(Vec::new()))).is_err() {
                            cancel_worker.store(true, Ordering::Relaxed);
                            return;
                        }
                        continue;
                    }

                    arena.reset();
                    let mut rsrcmgr = PDFResourceManager::with_caching(caching);
                    let mut collector =
                        PDFTableCollector::new(laparams.clone(), page_idx as i32 + 1, &mut arena);
                    let page_arena = process_page_arena(
                        &page,
                        &mut collector,
                        &mut rsrcmgr,
                        doc_worker.as_ref(),
                    );
                    let tables = match page_arena {
                        Ok(page_arena) => {
                            let arena_lookup = collector.arena_lookup();
                            let geom = match geom_worker.as_ref() {
                                Some(geoms) => geoms[page_idx].clone(),
                                None => PageGeometry {
                                    page_bbox: page_arena.bbox,
                                    mediabox: page_arena.bbox,
                                    initial_doctop: 0.0,
                                    force_crop: false,
                                },
                            };
                            let (chars, edges) =
                                collect_table_objects_from_arena(&page_arena, &geom);
                            Ok(extract_tables_from_objects(
                                chars,
                                edges,
                                &geom,
                                &settings,
                                arena_lookup,
                            ))
                        }
                        Err(err) => Err(err),
                    };
                    if cancel_worker.load(Ordering::Relaxed) {
                        return;
                    }
                    if tx.send((page_idx, tables)).is_err() {
                        cancel_worker.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            });
        });
    });

    Ok(TableStream::new(rx, order, cancel, worker))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn spawn_drop_probe_worker(
        cancel: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            while !cancel.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
            finished.store(true, Ordering::Relaxed);
        })
    }

    #[test]
    fn page_stream_drop_joins_worker_on_early_drop() {
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let worker = spawn_drop_probe_worker(Arc::clone(&cancel), Arc::clone(&finished));
        let (_tx, rx) = sync_channel::<StreamItem>(1);
        let stream = PageStream::new(rx, Vec::new(), cancel, worker);
        drop(stream);
        assert!(finished.load(Ordering::Relaxed));
    }

    #[test]
    fn table_stream_drop_joins_worker_on_early_drop() {
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let worker = spawn_drop_probe_worker(Arc::clone(&cancel), Arc::clone(&finished));
        let (_tx, rx) = sync_channel::<(usize, Result<PageTables>)>(1);
        let stream = TableStream::new(rx, Vec::new(), cancel, worker);
        drop(stream);
        assert!(finished.load(Ordering::Relaxed));
    }

    #[test]
    fn test_page_stream_only_creates_requested_pages() {
        let pdf = build_minimal_pdf_with_pages(5);
        let doc = Arc::new(PDFDocument::new(pdf, "").unwrap());
        crate::pdfpage::reset_page_create_count(doc.as_ref());

        let options = ExtractOptions {
            page_numbers: Some(vec![2]),
            ..ExtractOptions::default()
        };

        let stream = extract_pages_stream_from_doc(Arc::clone(&doc), options).unwrap();
        let _ = stream.collect::<Result<Vec<_>>>().unwrap();

        let created = crate::pdfpage::take_page_create_count(doc.as_ref());
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
        let out = extract_text_pages_from_doc_with_geometries(
            Arc::clone(&doc),
            options,
            crate::table::TextSettings::default(),
            geoms,
        )
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
        let out = extract_words_pages_from_doc_with_geometries(
            Arc::clone(&doc),
            options,
            crate::table::TextSettings::default(),
            geoms,
        )
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
