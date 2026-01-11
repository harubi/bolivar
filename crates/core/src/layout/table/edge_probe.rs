use crate::converter::PDFEdgeProbe;
use crate::pdfdocument::PDFDocument;
use crate::pdfinterp::PDFPageInterpreter;
use crate::pdfinterp::PDFResourceManager;
use crate::pdfpage::PDFPage;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::types::{TableProbePolicy, TableSettings};

pub(crate) fn page_has_edges(page: &PDFPage, doc: &PDFDocument, caching: bool) -> bool {
    #[cfg(test)]
    PROBE_CALLS.fetch_add(1, Ordering::Relaxed);
    let mut rsrcmgr = PDFResourceManager::with_caching(caching);
    let mut probe = PDFEdgeProbe::new();
    let mut interpreter = PDFPageInterpreter::new(&mut rsrcmgr, &mut probe);
    interpreter.process_page(page, Some(doc));
    probe.has_edges()
}

pub(crate) fn should_skip_tables(settings: &TableSettings, has_edges: bool) -> bool {
    if has_edges {
        return false;
    }
    match settings.probe_policy {
        TableProbePolicy::Never => false,
        TableProbePolicy::Always => true,
        TableProbePolicy::Auto => !uses_text_strategy(settings),
    }
}

fn uses_text_strategy(settings: &TableSettings) -> bool {
    settings.vertical_strategy == "text" || settings.horizontal_strategy == "text"
}

#[cfg(test)]
static PROBE_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn take_probe_calls() -> usize {
    PROBE_CALLS.swap(0, Ordering::Relaxed)
}
