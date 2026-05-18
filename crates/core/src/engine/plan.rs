//! Page-selection plan and worker-pool construction.

use rayon::{ThreadPool, ThreadPoolBuilder};

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

pub(crate) fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}
