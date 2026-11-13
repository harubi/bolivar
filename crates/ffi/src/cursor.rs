//! Cursors: one batch per page, streamed from the core engine.
//!
//! A cursor is used by one thread at a time; the JVM holds a lock around
//! every call. Cancelling goes through a separate [`CancelHandle`], so a
//! cancel from another thread never touches the cursor itself.

use std::sync::Arc;
use std::vec::IntoIter;

use bolivar_core::engine::{CancellationHandle, PageTables, Stream};
use bolivar_core::extract::{
    extract_layout_tables_metadata_stream_from_doc_with_geometries, extract_pages_stream_from_doc,
    extract_tables_metadata_stream_from_doc_with_geometries,
    extract_tables_stream_from_doc_with_geometries,
};
use bolivar_core::layout::LTPage;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::table::{TableMetadata, TableSettings};

use crate::abi::{
    Code, ErrorOut, FfiError, FfiResult, from_handle, guard, handle_mut, handle_ref, into_handle,
    write_handle, write_out,
};
use crate::batch::Batch;
use crate::document::Document;
use crate::geometry::{TablePage, page_geometry, table_plan};
use crate::options::{DocOptions, TableOptions, Tables};
use crate::pages::{
    layout_batch, page_text_batch, raw_batch, summary_batch, table_rows_batch, tables_batch,
};

/// Cursor kinds. Java mirrors these.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Layout = 1,
    Raw = 2,
    PageText = 3,
    Summary = 4,
    Tables = 5,
    TableRows = 6,
}

impl Kind {
    fn from_code(code: u32) -> FfiResult<Self> {
        Ok(match code {
            1 => Self::Layout,
            2 => Self::Raw,
            3 => Self::PageText,
            4 => Self::Summary,
            5 => Self::Tables,
            6 => Self::TableRows,
            _ => {
                return Err(FfiError::invalid(
                    "kind",
                    format!("unknown cursor kind {code}"),
                ));
            }
        })
    }
}

pub(crate) enum CursorState {
    Layout(Stream<LTPage>),
    PageText(Stream<LTPage>),
    Summary(Stream<LTPage>),
    Raw {
        stream: Stream<(LTPage, Vec<TableMetadata>)>,
        /// The pages resolved for their geometry at open, kept so each page
        /// is parsed once even without a page cache.
        pages: IntoIter<(usize, Arc<PDFPage>)>,
    },
    Tables {
        stream: Stream<Vec<TableMetadata>>,
        pages: IntoIter<TablePage>,
    },
    TableRows(Stream<PageTables>),
    Done,
}

impl CursorState {
    pub(crate) fn open(
        kind: Kind,
        doc: &Arc<PDFDocument>,
        options: DocOptions,
        tables: Tables,
    ) -> FfiResult<Self> {
        let core = options.core;
        Ok(match kind {
            Kind::Layout => Self::Layout(extract_pages_stream_from_doc(Arc::clone(doc), core)?),
            Kind::PageText => Self::PageText(extract_pages_stream_from_doc(Arc::clone(doc), core)?),
            Kind::Summary => Self::Summary(extract_pages_stream_from_doc(Arc::clone(doc), core)?),
            Kind::Raw => {
                let selected = bolivar_core::engine::select_pages(
                    doc.page_tree_len(),
                    core.page_numbers.clone(),
                    core.maxpages,
                );
                // The core stream takes every page's geometry up front.
                let mut pages = Vec::with_capacity(selected.len());
                let mut geometries = Vec::with_capacity(selected.len());
                for page_index in selected {
                    let page = doc.get_page_cached(page_index)?;
                    geometries.push(page_geometry(page.as_ref()));
                    pages.push((page_index, page));
                }
                Self::Raw {
                    stream: extract_layout_tables_metadata_stream_from_doc_with_geometries(
                        Arc::clone(doc),
                        core,
                        TableSettings::default(),
                        geometries,
                    )?,
                    pages: pages.into_iter(),
                }
            }
            Kind::Tables => {
                let plan = table_plan(doc, core, tables)?;
                let geometries = plan.geometries();
                Self::Tables {
                    stream: extract_tables_metadata_stream_from_doc_with_geometries(
                        Arc::clone(doc),
                        plan.options,
                        plan.settings,
                        geometries,
                    )?,
                    pages: plan.pages.into_iter(),
                }
            }
            Kind::TableRows => {
                let plan = table_plan(doc, core, tables)?;
                let geometries = plan.geometries();
                Self::TableRows(extract_tables_stream_from_doc_with_geometries(
                    Arc::clone(doc),
                    plan.options,
                    plan.settings,
                    geometries,
                )?)
            }
        })
    }

    fn cancellation(&self) -> Option<CancellationHandle> {
        Some(match self {
            Self::Layout(s) | Self::PageText(s) | Self::Summary(s) => s.cancellation_handle(),
            Self::Raw { stream, .. } => stream.cancellation_handle(),
            Self::Tables { stream, .. } => stream.cancellation_handle(),
            Self::TableRows(s) => s.cancellation_handle(),
            Self::Done => return None,
        })
    }

    /// The next page's batch, or `None` at the end. After an error the
    /// cursor is finished.
    pub(crate) fn next_batch(&mut self) -> FfiResult<Option<Batch>> {
        let result = self.step();
        if !matches!(result, Ok(Some(_))) {
            *self = Self::Done;
        }
        result
    }

    fn step(&mut self) -> FfiResult<Option<Batch>> {
        match self {
            Self::Layout(stream) => next(stream, |_, page| layout_batch(&page)),
            Self::PageText(stream) => next(stream, |index, page| page_text_batch(index, &page)),
            Self::Summary(stream) => next(stream, |_, page| summary_batch(&page)),
            Self::Raw { stream, pages } => {
                let item = stream.next().transpose()?;
                match (item, pages.next()) {
                    (Some((index, (page, tables))), Some((page_index, pdf_page)))
                        if page_index == index =>
                    {
                        raw_batch(index, pdf_page.as_ref(), &page, &tables).map(Some)
                    }
                    (None, None) => Ok(None),
                    _ => Err(FfiError::new(
                        Code::RuntimeError,
                        "raw stream and page plan disagree",
                    )),
                }
            }
            Self::Tables { stream, pages } => {
                let item = stream.next().transpose()?;
                match (item, pages.next()) {
                    (Some((index, tables)), Some(page)) if page.page_index == index => {
                        tables_batch(index, &page.geometry, tables).map(Some)
                    }
                    (None, None) => Ok(None),
                    _ => Err(FfiError::new(
                        Code::RuntimeError,
                        "table stream and page plan disagree",
                    )),
                }
            }
            Self::TableRows(stream) => next(stream, table_rows_batch),
            Self::Done => Ok(None),
        }
    }
}

fn next<T: Send + 'static>(
    stream: &mut Stream<T>,
    build: impl FnOnce(usize, T) -> FfiResult<Batch>,
) -> FfiResult<Option<Batch>> {
    match stream.next() {
        Some(Ok((index, item))) => build(index, item).map(Some),
        Some(Err(error)) => Err(error.into()),
        None => Ok(None),
    }
}

pub struct Cursor {
    state: CursorState,
    /// Set while a call runs; still set afterwards only if that call panicked.
    poisoned: bool,
}

/// Cancels a cursor from any thread. Freed separately from the cursor.
pub struct CancelHandle {
    handle: Option<CancellationHandle>,
}

/// Open a cursor of `kind` over a document. `table_options` applies to the
/// table kinds and must be null for the others.
///
/// # Safety
/// `document` must be a live document; `table_options` must be null or
/// valid; the out-pointers must be writable; `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cursor_open(
    document: *const Document,
    kind: u32,
    table_options: *const TableOptions,
    out: *mut *mut Cursor,
    out_cancel: *mut *mut CancelHandle,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let document = handle_ref(document)?;
            let kind = Kind::from_code(kind)?;
            let is_table = matches!(kind, Kind::Tables | Kind::TableRows);
            if !is_table && !table_options.is_null() {
                return Err(FfiError::invalid(
                    "table_options",
                    "table options only apply to table cursors",
                ));
            }
            let tables = Tables::from_raw(table_options)?;
            if out.is_null() || out_cancel.is_null() {
                return Err(FfiError::invalid("out", "out pointer is null"));
            }
            let state = CursorState::open(kind, &document.doc, document.options.clone(), tables)?;
            let cancel = CancelHandle {
                handle: state.cancellation(),
            };
            write_handle(out_cancel, "out_cancel", cancel)?;
            write_handle(
                out,
                "out",
                Cursor {
                    state,
                    poisoned: false,
                },
            )
        })
    }
}

/// Write the next page's batch to `out`, or null at the end.
///
/// # Safety
/// `cursor` must be a live cursor used by no other thread; `out` must be
/// writable; `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cursor_next(
    cursor: *mut Cursor,
    out: *mut *mut Batch,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let cursor = handle_mut(cursor)?;
            if out.is_null() {
                return Err(FfiError::invalid("out", "out pointer is null"));
            }
            if cursor.poisoned {
                return Err(FfiError::new(
                    Code::Panicked,
                    "cursor failed after an earlier panic",
                ));
            }
            cursor.poisoned = true;
            let result = cursor.state.next_batch();
            cursor.poisoned = false;
            let batch = result?.map_or(std::ptr::null_mut(), into_handle);
            write_out(out, "out", batch)
        })
    }
}

/// Ask a cursor to stop. Safe from any thread, and after the cursor is freed.
///
/// # Safety
/// `cancel` must be null or a live cancel handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cancel(cancel: *const CancelHandle) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Ok(cancel) = unsafe { handle_ref(cancel) }
            && let Some(handle) = &cancel.handle
        {
            handle.cancel();
        }
    }));
}

/// Free a cursor. Its cancel handle stays valid until freed separately.
///
/// # Safety
/// `cursor` must be null or a live cursor with no call in progress, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cursor_free(cursor: *mut Cursor) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(unsafe { from_handle(cursor) });
    }));
}

/// Free a cancel handle.
///
/// # Safety
/// `cancel` must be null or a live cancel handle with no call in progress, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cancel_free(cancel: *mut CancelHandle) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(unsafe { from_handle(cancel) });
    }));
}
