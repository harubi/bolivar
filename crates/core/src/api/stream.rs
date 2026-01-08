//! Streaming extraction primitives.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

use super::high_level::{ExtractOptions, default_thread_count, process_page};

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

pub(crate) fn extract_pages_stream_from_doc(
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

    let mut order: Vec<usize> = Vec::new();
    let mut pending: Vec<(usize, PDFPage)> = Vec::new();
    let mut errors: Vec<StreamItem> = Vec::new();
    let mut page_count = 0;

    for (page_idx, page_result) in PDFPage::create_pages(doc.as_ref()).enumerate() {
        if let Some(ref nums) = options.page_numbers {
            if !nums.contains(&page_idx) {
                continue;
            }
        }

        if options.maxpages > 0 && page_count >= options.maxpages {
            break;
        }

        order.push(page_idx);

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

    let thread_count = default_thread_count();
    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .map_err(|e| PdfError::DecodeError(e.to_string()))?;

    let (tx, rx) = sync_channel(DEFAULT_STREAM_BUFFER_CAPACITY);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);
    let doc_worker = Arc::clone(&doc);

    std::thread::spawn(move || {
        for (page_idx, result) in errors {
            if cancel_worker.load(Ordering::Relaxed) {
                return;
            }
            if tx.send((page_idx, result)).is_err() {
                return;
            }
        }

        pool.install(|| {
            pending
                .into_par_iter()
                .map_init(PageArena::new, |arena, (page_idx, page)| {
                    if cancel_worker.load(Ordering::Relaxed) {
                        return None;
                    }

                    arena.reset();
                    let mut rsrcmgr = PDFResourceManager::with_caching(caching);
                    let mut aggregator =
                        PDFPageAggregator::new(laparams.clone(), page_idx as i32 + 1, arena);
                    let ltpage =
                        process_page(&page, &mut aggregator, &mut rsrcmgr, doc_worker.as_ref());
                    Some((page_idx, ltpage))
                })
                .filter_map(|item| item)
                .for_each_with(tx, |sender, (page_idx, result)| {
                    if cancel_worker.load(Ordering::Relaxed) {
                        return;
                    }
                    if sender.send((page_idx, result)).is_err() {
                        cancel_worker.store(true, Ordering::Relaxed);
                    }
                });
        });
    });

    Ok(PageStream::new(rx, order, cancel))
}
