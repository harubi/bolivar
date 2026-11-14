//! Cursors: one batch per page, built on the stream's workers.
//!
//! A cursor holds its stream behind a mutex and its cancellation handle
//! beside it, so `next` and `cancel` both take a shared pointer: a cancel from
//! another thread never needs the lock `next` holds. The JVM keeps the cursor
//! alive until every call on it has returned.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use bolivar_core::engine::{CancellationHandle, Stream};
use bolivar_core::extract::{
    CursorSelection, GeometrySource, extract_layout_tables_metadata_stream_with,
    extract_pages_stream_with, extract_tables_metadata_stream_with, extract_tables_stream_with,
};
use bolivar_core::table::{PageGeometry, TableSettings};

use crate::abi::{
    Code, ErrorOut, FfiError, FfiResult, free_handle, guard, handle_ref, into_handle, no_unwind,
    require_out, write_handle, write_out,
};
use crate::batch::{Batch, Built, layout, page_text, raw, summary, table_rows, tables};
use crate::document::Document;
use crate::options::{Crops, CursorArgs, CursorOptions, TableOptions, Tables};
use crate::pages::{
    layout_built, page_text_built, raw_built, summary_built, table_rows_built, tables_built,
};

/// Whether a cursor of `kind` (a batch kind number) reads table options; an error for a kind
/// no cursor yields.
fn takes_table_options(kind: u32) -> FfiResult<bool> {
    match kind {
        tables::KIND | table_rows::KIND => Ok(true),
        layout::KIND | raw::KIND | page_text::KIND | summary::KIND => Ok(false),
        _ => Err(unknown_kind(kind)),
    }
}

fn unknown_kind(kind: u32) -> FfiError {
    FfiError::invalid("kind", format!("unknown cursor kind {kind}"))
}

/// The geometry of each table page: pdfplumber's, with this page's crop.
fn table_geometry(crops: Crops) -> GeometrySource {
    GeometrySource::PerPage(Arc::new(move |page_index, page| {
        let geometry = PageGeometry::for_page(page);
        match crops.for_page(page_index) {
            Some(crop) => geometry.cropped(crop),
            None => geometry,
        }
    }))
}

/// A page's batch, or the binding's own failure building it. Carried inside the stream's item,
/// so its code and field reach the caller unchanged instead of collapsing into a core error.
type PageBatch = FfiResult<Built>;

/// The cursor's selection with a table cursor's own `max_pages` folded in: when both are set,
/// the smaller wins; when one is, it applies alone.
fn cursor_selection(kind: u32, cursor: &CursorArgs, tables: &Tables) -> CursorSelection {
    let table_max = matches!(kind, tables::KIND | table_rows::KIND)
        .then(|| tables.max_pages.and_then(NonZeroUsize::new))
        .flatten();
    match [cursor.selection.max_pages, table_max]
        .into_iter()
        .flatten()
        .min()
    {
        Some(max_pages) => cursor.selection.clone().max_pages(max_pages),
        None => cursor.selection.clone(),
    }
}

fn open_stream(
    kind: u32,
    document: &Document,
    tables: Tables,
    cursor: &CursorArgs,
) -> FfiResult<Stream<PageBatch>> {
    let doc = Arc::clone(&document.doc);
    // Narrows the document's selection, as every cursor selection does.
    let mut options = document
        .extract
        .narrowed(&document.doc, &cursor_selection(kind, cursor, &tables));
    options.read_ahead = cursor.read_ahead;
    let ffi = |result: PageBatch| -> bolivar_core::Result<PageBatch> { Ok(result) };
    Ok(match kind {
        layout::KIND => extract_pages_stream_with(doc, options, move |index, _, page| {
            ffi(layout_built(index, &page))
        })?,
        page_text::KIND => extract_pages_stream_with(doc, options, move |index, _, page| {
            ffi(page_text_built(index, &page))
        })?,
        summary::KIND => extract_pages_stream_with(doc, options, move |index, _, page| {
            ffi(summary_built(index, &page))
        })?,
        raw::KIND => extract_layout_tables_metadata_stream_with(
            doc,
            options,
            TableSettings::default(),
            GeometrySource::for_each_page(),
            move |index, pdf_page, page, page_tables| {
                ffi(raw_built(index, pdf_page, &page, &page_tables))
            },
        )?,
        tables::KIND => {
            let geometry = table_geometry(tables.crops);
            extract_tables_metadata_stream_with(
                doc,
                options,
                tables.settings,
                geometry,
                move |index, geometry, page_tables| {
                    ffi(tables_built(index, geometry, &page_tables))
                },
            )?
        }
        table_rows::KIND => {
            let geometry = table_geometry(tables.crops);
            extract_tables_stream_with(
                doc,
                options,
                tables.settings,
                Some(geometry),
                move |index, page_tables| ffi(table_rows_built(index, page_tables)),
            )?
        }
        _ => return Err(unknown_kind(kind)),
    })
}

pub struct Cursor {
    /// `None` once the cursor is finished: at its end or after an error.
    stream: Mutex<Option<Stream<PageBatch>>>,
    cancel: CancellationHandle,
}

impl Cursor {
    fn next_batch(&self) -> FfiResult<Option<Batch>> {
        let mut guard = self
            .stream
            .lock()
            .map_err(|_| FfiError::new(Code::Panicked, "cursor failed after an earlier panic"))?;
        let Some(stream) = guard.as_mut() else {
            return Ok(None);
        };
        let next = match stream.next() {
            Some(Ok((_, built))) => built.map(Some),
            Some(Err(error)) => Err(error.into()),
            None => Ok(None),
        };
        // At its end or after an error, the cursor is finished.
        if !matches!(next, Ok(Some(_))) {
            *guard = None;
        }
        next.map(|built| built.map(Built::into_batch))
    }
}

/// Open a cursor of `kind` (a batch kind number) over a document.
/// `table_options` applies to the table kinds and must be null otherwise.
/// `cursor_options` narrows the document's pages and sets the read-ahead;
/// null reads every page the document selects.
///
/// # Safety
/// `document` must be a live document; `table_options` and `cursor_options`
/// must be null or valid; `out` must be writable; `err` must be null or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cursor_open(
    document: *const Document,
    kind: u32,
    table_options: *const TableOptions,
    cursor_options: *const CursorOptions,
    out: *mut *mut Cursor,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let document = handle_ref(document)?;
            if !takes_table_options(kind)? && !table_options.is_null() {
                return Err(FfiError::invalid(
                    "table_options",
                    "table options only apply to table cursors",
                ));
            }
            require_out(out, "out")?;
            let tables = Tables::from_raw(table_options)?;
            let cursor = CursorArgs::from_raw(cursor_options)?;
            let stream = open_stream(kind, document, tables, &cursor)?;
            let cancel = stream.cancellation_handle();
            write_handle(
                out,
                "out",
                Cursor {
                    stream: Mutex::new(Some(stream)),
                    cancel,
                },
            )
        })
    }
}

/// Write the next page's batch to `out`, or null at the end. A finished or
/// failed cursor keeps returning null.
///
/// # Safety
/// `cursor` must be a live cursor; `out` must be writable; `err` must be null
/// or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cursor_next(
    cursor: *const Cursor,
    out: *mut *mut Batch,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let cursor = handle_ref(cursor)?;
            require_out(out, "out")?;
            let batch = cursor.next_batch()?;
            write_out(out, "out", batch.map_or(std::ptr::null_mut(), into_handle))
        })
    }
}

/// Ask a cursor to stop; a blocked `next` then returns `Cancelled`. Safe from
/// any thread while the cursor is alive.
///
/// # Safety
/// `cursor` must be null or a live cursor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cursor_cancel(cursor: *const Cursor) {
    no_unwind(|| {
        if let Ok(cursor) = unsafe { handle_ref(cursor) } {
            cursor.cancel.cancel();
        }
    });
}

/// Free a cursor.
///
/// # Safety
/// `cursor` must be null or a live cursor with no call in progress, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cursor_free(cursor: *mut Cursor) {
    unsafe { free_handle(cursor) }
}
