//! Per-page processing and standard finisher helpers.

use crate::arena::types::ArenaPage;
use crate::device::{PDFDevice, PDFPageAggregator, PDFTableCollector};
use crate::document::{PDFDocument, PDFPage};
use crate::error::{PdfError, Result};
use crate::interp::{PDFPageInterpreter, PDFResourceManager};
use crate::layout::LTPage;

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) struct ThreadRecord {
    pub id: std::thread::ThreadId,
    pub in_pool: bool,
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
pub(crate) fn take_thread_log() -> Vec<ThreadRecord> {
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

const TABLE_COLLECTOR_NO_RESULT: &str = "table collector produced no result";

/// Standard finisher for `PDFPageAggregator`-backed `process_page` calls: clones the result `LTPage`.
pub fn aggregator_result(agg: &mut PDFPageAggregator<'_>) -> Result<LTPage> {
    Ok(agg.get_result().clone())
}

/// Standard finisher for `PDFTableCollector`-backed `process_page` calls: takes the arena page or errors.
pub fn collector_result<'a>(collector: &mut PDFTableCollector<'a>) -> Result<ArenaPage<'a>> {
    collector
        .take_result()
        .ok_or_else(|| PdfError::DecodeError(TABLE_COLLECTOR_NO_RESULT.to_string()))
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
    D: PDFDevice,
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
