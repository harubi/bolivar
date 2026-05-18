//! Parallel batch processing of selected pages with ordered results.

use rayon::prelude::*;

use crate::arena::PageArena;
use crate::document::{PDFDocument, PDFPage};
use crate::error::Result;

use super::plan::ExecutionPlan;

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
