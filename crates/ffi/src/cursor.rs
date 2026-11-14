//! Cursors: one batch per page, built on the stream's workers.
//!
//! A cursor holds its stream behind a mutex and its cancellation handle
//! beside it, so `next` and `cancel` both take a shared pointer: a cancel from
//! another thread never needs the lock `next` holds. The JVM keeps the cursor
//! alive until every call on it has returned.

use std::sync::{Arc, Mutex};

use bolivar_core::engine::{CancellationHandle, Stream};
use bolivar_core::extract::{
    GeometrySource, extract_layout_tables_metadata_stream_with, extract_pages_stream_with,
    extract_tables_metadata_stream_with, extract_tables_stream_with,
};
use bolivar_core::table::{PageGeometry, TableSettings};

use crate::abi::{
    Code, ErrorOut, FfiError, FfiResult, free_handle, guard, handle_ref, into_handle, no_unwind,
    write_handle, write_out,
};
use crate::batch::{Batch, Built, layout, page_text, raw, summary, table_rows, tables};
use crate::document::Document;
use crate::options::{Crops, TableOptions, Tables};
use crate::pages::{
    layout_built, page_text_built, raw_built, summary_built, table_rows_built, tables_built,
};

/// A cursor's work, which is also the kind of batch it yields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Layout,
    Raw,
    PageText,
    Summary,
    Tables,
    TableRows,
}

impl Kind {
    fn from_code(code: u32) -> FfiResult<Self> {
        Ok(match code {
            layout::KIND => Self::Layout,
            raw::KIND => Self::Raw,
            page_text::KIND => Self::PageText,
            summary::KIND => Self::Summary,
            tables::KIND => Self::Tables,
            table_rows::KIND => Self::TableRows,
            _ => {
                return Err(FfiError::invalid(
                    "kind",
                    format!("unknown cursor kind {code}"),
                ));
            }
        })
    }

    const fn takes_table_options(self) -> bool {
        matches!(self, Self::Tables | Self::TableRows)
    }
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

fn open_stream(kind: Kind, document: &Document, tables: Tables) -> FfiResult<Stream<PageBatch>> {
    let doc = Arc::clone(&document.doc);
    let mut options = document.extract.clone();
    if kind.takes_table_options()
        && let Some(max_pages) = tables.max_pages
    {
        options.maxpages = max_pages;
    }
    let ffi = |result: PageBatch| -> bolivar_core::Result<PageBatch> { Ok(result) };
    Ok(match kind {
        Kind::Layout => extract_pages_stream_with(doc, options, move |index, _, page| {
            ffi(layout_built(index, &page))
        })?,
        Kind::PageText => extract_pages_stream_with(doc, options, move |index, _, page| {
            ffi(page_text_built(index, &page))
        })?,
        Kind::Summary => extract_pages_stream_with(doc, options, move |index, _, page| {
            ffi(summary_built(index, &page))
        })?,
        Kind::Raw => extract_layout_tables_metadata_stream_with(
            doc,
            options,
            TableSettings::default(),
            GeometrySource::for_each_page(),
            move |index, pdf_page, _, page, page_tables| {
                ffi(raw_built(index, pdf_page, &page, &page_tables))
            },
        )?,
        Kind::Tables => {
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
        Kind::TableRows => {
            let geometry = table_geometry(tables.crops);
            extract_tables_stream_with(
                doc,
                options,
                tables.settings,
                Some(geometry),
                move |index, page_tables| ffi(table_rows_built(index, page_tables)),
            )?
        }
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
        match stream.next() {
            Some(Ok((_, Ok(built)))) => Ok(Some(built.into_batch())),
            Some(Ok((_, Err(error)))) => {
                *guard = None;
                Err(error)
            }
            Some(Err(error)) => {
                *guard = None;
                Err(error.into())
            }
            None => {
                *guard = None;
                Ok(None)
            }
        }
    }
}

/// Open a cursor of `kind` (a batch kind number) over a document.
/// `table_options` applies to the table kinds and must be null otherwise.
///
/// # Safety
/// `document` must be a live document; `table_options` must be null or
/// valid; `out` must be writable; `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_cursor_open(
    document: *const Document,
    kind: u32,
    table_options: *const TableOptions,
    out: *mut *mut Cursor,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let document = handle_ref(document)?;
            let kind = Kind::from_code(kind)?;
            if !kind.takes_table_options() && !table_options.is_null() {
                return Err(FfiError::invalid(
                    "table_options",
                    "table options only apply to table cursors",
                ));
            }
            if out.is_null() {
                return Err(FfiError::invalid("out", "out pointer is null"));
            }
            let tables = Tables::from_raw(table_options)?;
            let stream = open_stream(kind, document, tables)?;
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
            if out.is_null() {
                return Err(FfiError::invalid("out", "out pointer is null"));
            }
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
