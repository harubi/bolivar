use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::arena::PageArena;
use crate::document::{PDFDocument, PDFPage};
use crate::error::{PdfError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub order: Vec<usize>,
    pub worker_count: usize,
}

impl ExecutionPlan {
    #[must_use]
    pub fn new(page_count: usize, page_numbers: Option<&[usize]>, maxpages: usize) -> Self {
        Self {
            order: select_pages_ref(page_count, page_numbers, maxpages),
            worker_count: default_worker_count(),
        }
    }

    pub fn build_pool(&self) -> Result<ThreadPool> {
        ThreadPoolBuilder::new()
            .num_threads(self.worker_count)
            .build()
            .map_err(|e| PdfError::DecodeError(e.to_string()))
    }
}

#[must_use]
pub fn select_pages(
    page_count: usize,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
) -> Vec<usize> {
    select_pages_ref(page_count, page_numbers.as_deref(), maxpages)
}

pub fn validate_geometry_count(order: &[usize], geometry_count: usize) -> Result<()> {
    if geometry_count != order.len() {
        return Err(PdfError::DecodeError(format!(
            "geometry count mismatch: expected {}, got {}",
            order.len(),
            geometry_count
        )));
    }
    Ok(())
}

fn select_pages_ref(
    page_count: usize,
    page_numbers: Option<&[usize]>,
    maxpages: usize,
) -> Vec<usize> {
    let mut order = Vec::new();
    let mut selected = 0usize;

    for page_idx in 0..page_count {
        if let Some(nums) = page_numbers
            && !nums.contains(&page_idx)
        {
            continue;
        }
        if maxpages > 0 && selected >= maxpages {
            break;
        }
        order.push(page_idx);
        selected += 1;
    }

    order
}

fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Run `per_page` against every selected page in parallel. Each invocation receives
/// a freshly-reset arena, the page index, a borrowed page handle, and the document;
/// it owns the full per-page lifecycle (resource manager, device construction,
/// interpreter run, result extraction). Returns results sorted by page index.
///
/// Replaces the four `par_iter().map_init(PageArena::new, …)` loops previously
/// copy-pasted across `high_level.rs` and `stream.rs`.
pub fn run_batch<R, F>(
    doc: &PDFDocument,
    page_numbers: Option<&[usize]>,
    maxpages: usize,
    per_page: F,
) -> Result<Vec<(usize, R)>>
where
    R: Send,
    F: Fn(&mut PageArena, usize, &PDFPage, &PDFDocument) -> Result<R> + Sync,
{
    let plan = ExecutionPlan::new(doc.page_index().len(), page_numbers, maxpages);
    let pool = plan.build_pool()?;

    let mut results: Vec<(usize, Result<R>)> = pool.install(|| {
        plan.order
            .par_iter()
            .map_init(PageArena::new, |arena, &page_idx| {
                let page = match doc.get_page_cached(page_idx) {
                    Ok(page) => page,
                    Err(e) => return (page_idx, Err(e)),
                };
                arena.reset();
                let r = per_page(arena, page_idx, page.as_ref(), doc);
                (page_idx, r)
            })
            .collect()
    });

    results.sort_by_key(|(idx, _)| *idx);
    results
        .into_iter()
        .map(|(idx, r)| r.map(|v| (idx, v)))
        .collect()
}
