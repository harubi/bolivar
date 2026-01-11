//! Streaming extraction primitives.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, sync_channel};

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::arena::PageArena;
use crate::converter::PDFPageAggregator;
use crate::error::{PdfError, Result};
use crate::layout::{LAParams, LTPage};
use crate::pdfdocument::PDFDocument;
use crate::pdfinterp::PDFResourceManager;
use crate::pdfpage::PDFPage;
use crate::table::{PageGeometry, TableSettings, extract_tables_from_ltpage};

use super::high_level::{ExtractOptions, PageTables, default_thread_count, process_page};

pub const DEFAULT_STREAM_BUFFER_CAPACITY: usize = 50;

#[cfg(test)]
static STREAM_USAGE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static STREAM_USAGE_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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

type StreamItem = (usize, Result<LTPage>);

fn build_page_order(doc: &PDFDocument, options: &ExtractOptions) -> Vec<usize> {
    let mut order = Vec::new();
    let page_count = doc.page_index().len();
    let mut selected = 0usize;

    for page_idx in 0..page_count {
        if let Some(ref nums) = options.page_numbers {
            if !nums.contains(&page_idx) {
                continue;
            }
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
    rx: Receiver<StreamItem>,
    order: Vec<usize>,
    next_pos: usize,
    buffer: BTreeMap<usize, Result<LTPage>>,
    done: bool,
    failed: bool,
    max_buffered: usize,
    cancel: Arc<AtomicBool>,
}

pub struct TableStream {
    rx: Receiver<(usize, Result<PageTables>)>,
    order: Vec<usize>,
    next_pos: usize,
    buffer: BTreeMap<usize, Result<PageTables>>,
    done: bool,
    failed: bool,
    max_buffered: usize,
    cancel: Arc<AtomicBool>,
}

impl TableStream {
    fn new(
        rx: Receiver<(usize, Result<PageTables>)>,
        order: Vec<usize>,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            rx,
            order,
            next_pos: 0,
            buffer: BTreeMap::new(),
            done: false,
            failed: false,
            max_buffered: 0,
            cancel,
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

            match self.rx.recv() {
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
    }
}

impl PageStream {
    fn new(rx: Receiver<StreamItem>, order: Vec<usize>, cancel: Arc<AtomicBool>) -> Self {
        Self {
            rx,
            order,
            next_pos: 0,
            buffer: BTreeMap::new(),
            done: false,
            failed: false,
            max_buffered: 0,
            cancel,
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

            match self.rx.recv() {
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

    std::thread::spawn(move || {
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

    Ok(PageStream::new(rx, order, cancel))
}

pub fn extract_tables_stream_from_doc(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
) -> Result<TableStream> {
    extract_tables_stream_from_doc_with_geometries_internal(
        doc,
        options,
        TableSettings::default(),
        None,
    )
}

pub fn extract_tables_stream_from_doc_with_geometries(
    doc: Arc<PDFDocument>,
    mut options: ExtractOptions,
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

    std::thread::spawn(move || {
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
                    let tables = ltpage.map(|page| {
                        let geom = match geom_worker.as_ref() {
                            Some(geoms) => geoms[page_idx].clone(),
                            None => page_geometry_from_ltpage(&page),
                        };
                        extract_tables_from_ltpage(&page, &geom, &settings)
                    });
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

    Ok(TableStream::new(rx, order, cancel))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let geoms = vec![PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        }];

        let err =
            extract_tables_stream_from_doc_with_geometries(doc.into(), options, settings, geoms);
        assert!(err.is_err());
        if let Err(err) = err {
            assert!(err.to_string().contains("geometry count"));
        }
    }
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
