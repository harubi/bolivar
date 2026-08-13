use crate::cancellation::CancellationToken;
use crate::device::PDFEdgeProbe;
use crate::document::{PDFDocument, PDFPage};
use crate::error::Result;
use crate::interp::{PDFPageInterpreter, PDFResourceManager};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use super::types::{TableProbePolicy, TableSettings};

pub(crate) fn page_has_edges_with_cancellation(
    page: &PDFPage,
    doc: &PDFDocument,
    caching: bool,
    cancellation: &CancellationToken,
) -> Result<bool> {
    #[cfg(test)]
    PROBE_CALLS.fetch_add(1, Ordering::Relaxed);
    let mut rsrcmgr = PDFResourceManager::with_caching(caching);
    let mut probe = PDFEdgeProbe::new();
    let mut interpreter =
        PDFPageInterpreter::new_with_cancellation(&mut rsrcmgr, &mut probe, cancellation.clone());
    interpreter.process_page(page, Some(doc))?;
    Ok(probe.has_edges())
}

pub(crate) fn should_probe_tables(settings: &TableSettings) -> bool {
    match settings.probe_policy {
        TableProbePolicy::Never => false,
        TableProbePolicy::Always => true,
        TableProbePolicy::Auto => !uses_text_strategy(settings),
    }
}

fn uses_text_strategy(settings: &TableSettings) -> bool {
    settings.vertical_strategy.uses_text() || settings.horizontal_strategy.uses_text()
}

#[cfg(test)]
static PROBE_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn take_probe_calls() -> usize {
    PROBE_CALLS.swap(0, Ordering::Relaxed)
}
