//! Ordered streaming worker pool for selected pages.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread::JoinHandle;

use rayon::prelude::*;

use crate::arena::PageArena;
use crate::document::{PDFDocument, PDFPage};
use crate::error::{PdfError, Result};

use super::plan::ExecutionPlan;

pub const DEFAULT_STREAM_BUFFER_CAPACITY: usize = 50;

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

/// Ordered, page-indexed stream of results from a per-page worker pool.
///
/// Replaces both `PageStream` and `TableStream`. `Item = Result<(usize, R)>` —
/// callers receive each result tagged with its page index in selection order.
pub struct Stream<R> {
    rx: Option<Receiver<(usize, Result<R>)>>,
    order: Vec<usize>,
    next_pos: usize,
    buffer: BTreeMap<usize, Result<R>>,
    done: bool,
    failed: bool,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl<R> Stream<R> {
    fn new(
        rx: Receiver<(usize, Result<R>)>,
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
            cancel,
            worker: Some(worker),
        }
    }
}

impl<R> Iterator for Stream<R> {
    type Item = Result<(usize, R)>;

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
                return Some(result.map(|v| (expected, v)));
            }
            if self.done {
                self.failed = true;
                self.cancel.store(true, Ordering::Relaxed);
                return Some(Err(PdfError::DecodeError(format!(
                    "stream closed before expected page {expected} arrived"
                ))));
            }
            let recv = match self.rx.as_ref() {
                Some(rx) => rx.recv(),
                None => {
                    self.done = true;
                    continue;
                }
            };
            match recv {
                Ok((page_idx, result)) => {
                    self.buffer.insert(page_idx, result);
                }
                Err(_) => self.done = true,
            }
        }
    }
}

impl<R> Drop for Stream<R> {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.rx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Spawn a worker pool that processes selected pages and emits ordered `R` values.
///
/// `precheck(page_idx, page, doc)` runs first per page:
///   - `Ok(Some(R))` short-circuits with that value (e.g., tables stream skipping
///     edge-less pages without invoking the interpreter)
///   - `Ok(None)` proceeds to `per_page`
///   - `Err(e)` propagates as the page's error
///
/// `per_page(arena, page_idx, page, doc)` runs the interpreter and returns the result.
pub fn run_stream<R, P, F>(
    doc: Arc<PDFDocument>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    precheck: P,
    per_page: F,
) -> Result<Stream<R>>
where
    R: Send + 'static,
    P: Fn(usize, &PDFPage, &PDFDocument) -> Result<Option<R>> + Send + Sync + 'static,
    F: Fn(&mut PageArena, usize, &PDFPage, &PDFDocument) -> Result<R> + Send + Sync + 'static,
{
    let plan = ExecutionPlan::new(doc.page_index().len(), page_numbers.as_deref(), maxpages);
    let order = plan.order.clone();
    let work_order = order.clone();
    let worker_count = plan.worker_count;
    let pool = plan.build_pool()?;

    let (tx, rx) = sync_channel(DEFAULT_STREAM_BUFFER_CAPACITY);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);
    let doc_worker = Arc::clone(&doc);
    let next_index = Arc::new(AtomicUsize::new(0));
    let next_index_worker = Arc::clone(&next_index);
    let precheck = Arc::new(precheck);
    let per_page = Arc::new(per_page);

    let worker = std::thread::spawn(move || {
        #[cfg(test)]
        let _worker_lifecycle = StreamWorkerLifecycleCounter::start();

        pool.install(|| {
            (0..worker_count).into_par_iter().for_each(|_| {
                let precheck = Arc::clone(&precheck);
                let per_page = Arc::clone(&per_page);
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
                    let page = match doc_worker.get_page_cached(page_idx) {
                        Ok(page) => page,
                        Err(e) => {
                            if tx.send((page_idx, Err(e))).is_err() {
                                cancel_worker.store(true, Ordering::Relaxed);
                            }
                            continue;
                        }
                    };
                    let pre = precheck(page_idx, page.as_ref(), doc_worker.as_ref());
                    let result = match pre {
                        Ok(Some(short_circuit)) => Ok(short_circuit),
                        Err(e) => Err(e),
                        Ok(None) => {
                            arena.reset();
                            per_page(&mut arena, page_idx, page.as_ref(), doc_worker.as_ref())
                        }
                    };
                    if cancel_worker.load(Ordering::Relaxed) {
                        return;
                    }
                    if tx.send((page_idx, result)).is_err() {
                        cancel_worker.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            });
        });
    });

    Ok(Stream::new(rx, order, cancel, worker))
}

/// Convenience: no-op precheck for callers that don't need short-circuit behavior.
pub fn no_precheck<R>(_page_idx: usize, _page: &PDFPage, _doc: &PDFDocument) -> Result<Option<R>> {
    Ok(None)
}
