//! Page-selection plan.

use crate::error::{PdfError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub order: Vec<usize>,
}

impl ExecutionPlan {
    #[must_use]
    pub fn new(page_count: usize, page_numbers: Option<&[usize]>, maxpages: usize) -> Self {
        Self {
            order: select_pages_ref(page_count, page_numbers, maxpages),
        }
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

/// The pages of a list that exist in a document, one bit per page. Built once, it makes a
/// selection O(pages + list), where looking each page up in the list would be O(pages × list).
pub(crate) struct PageSet {
    bits: Vec<u64>,
}

impl PageSet {
    /// The pages of `page_numbers` below `page_count`; the others are ignored.
    pub(crate) fn new(page_count: usize, page_numbers: &[usize]) -> Self {
        let mut bits = vec![0u64; page_count.div_ceil(64)];
        for &page_idx in page_numbers {
            if page_idx < page_count {
                bits[page_idx / 64] |= 1 << (page_idx % 64);
            }
        }
        Self { bits }
    }

    pub(crate) fn contains(&self, page_idx: usize) -> bool {
        self.bits
            .get(page_idx / 64)
            .is_some_and(|word| word & (1 << (page_idx % 64)) != 0)
    }
}

fn select_pages_ref(
    page_count: usize,
    page_numbers: Option<&[usize]>,
    maxpages: usize,
) -> Vec<usize> {
    let wanted = page_numbers.map(|nums| PageSet::new(page_count, nums));
    let limit = if maxpages > 0 { maxpages } else { usize::MAX };

    (0..page_count)
        .filter(|&page_idx| wanted.as_ref().is_none_or(|set| set.contains(page_idx)))
        .take(limit)
        .collect()
}

pub(crate) fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The selection as it was first written: a list lookup per page.
    fn reference(page_count: usize, page_numbers: Option<&[usize]>, maxpages: usize) -> Vec<usize> {
        let mut order = Vec::new();
        for page_idx in 0..page_count {
            if page_numbers.is_some_and(|nums| !nums.contains(&page_idx)) {
                continue;
            }
            if maxpages > 0 && order.len() >= maxpages {
                break;
            }
            order.push(page_idx);
        }
        order
    }

    #[test]
    fn selection_matches_the_list_lookup_it_replaces() {
        const SEED: u64 = 0x05EE_D0F9_A6E5;
        let mut state = SEED;
        let mut next = |bound: usize| {
            // xorshift64: deterministic, so a failure reproduces from the seed alone.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % bound as u64) as usize
        };
        for case in 0..2_000 {
            let page_count = next(150);
            let page_numbers: Option<Vec<usize>> =
                (next(4) != 0).then(|| (0..next(40)).map(|_| next(page_count + 10)).collect());
            let maxpages = if next(3) == 0 { 0 } else { next(20) };
            assert_eq!(
                select_pages_ref(page_count, page_numbers.as_deref(), maxpages),
                reference(page_count, page_numbers.as_deref(), maxpages),
                "seed {SEED:#x}, case {case}: {page_count} pages, {page_numbers:?}, max {maxpages}"
            );
        }
    }

    #[test]
    fn selection_ignores_missing_pages_and_keeps_document_order() {
        assert_eq!(select_pages(5, Some(vec![9, 3, 3, 0]), 0), vec![0, 3]);
        assert_eq!(select_pages(0, Some(vec![0]), 0), Vec::<usize>::new());
        assert_eq!(
            select_pages(70, Some(vec![69, 64, 63]), 0),
            vec![63, 64, 69]
        );
    }
}
