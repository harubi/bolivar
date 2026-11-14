//! Opening documents and one-shot document calls.

use std::sync::Arc;

use bolivar_core::extract::{
    ExecutionPlan, ExtractOptions, extract_text_with_document, layout_tables_page,
};
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::table::TableSettings;

use crate::abi::{
    Code, ErrorOut, FfiError, FfiResult, free_handle, from_handle, guard, handle_mut, handle_ref,
    into_handle, slice, write_handle, write_out,
};
use crate::batch::{Batch, Builder, Built, text};
use crate::metadata::{metadata_built, metadata_from_document};
use crate::options::{DocumentOptions, Options};
use crate::pages::raw_built;

/// An open document and the extraction options it was opened with.
pub struct Document {
    pub(crate) doc: Arc<PDFDocument>,
    pub(crate) extract: ExtractOptions,
}

fn validate_path(path: &str) -> FfiResult<()> {
    if path.trim().is_empty() || path.contains('\0') || path.contains("://") {
        return Err(FfiError::new(Code::InvalidPath, "path is invalid"));
    }
    Ok(())
}

impl Document {
    fn new(doc: PDFDocument, extract: ExtractOptions) -> Self {
        Self {
            doc: Arc::new(doc),
            extract,
        }
    }

    /// The whole document's text, as UTF-8.
    pub fn text_built(&self) -> FfiResult<Built> {
        let value = extract_text_with_document(self.doc.as_ref(), self.extract.clone())?;
        let mut b = Builder::new(&text::SCHEMA);
        b.u8s(text::Col::Utf8).extend_from_slice(value.as_bytes());
        b.finish()
    }

    /// One 1-based page as a raw batch, on the calling thread.
    pub fn raw_page_built(&self, page_number: i64) -> FfiResult<Built> {
        let tree_len = self.doc.page_tree_len() as i64;
        if page_number < 1 || page_number > tree_len {
            return Err(FfiError::invalid(
                "page_number",
                format!("page_number must be within 1..={tree_len}"),
            ));
        }
        let index = (page_number - 1) as usize;
        let (pdf_page, _, page, tables) = layout_tables_page(
            &self.doc,
            index,
            self.extract.clone(),
            TableSettings::default(),
            None,
        )?;
        raw_built(index, &pdf_page, &page, &tables)
    }

    /// The declared page count and the number of pages the options select.
    pub fn counts(&self) -> (i64, i64) {
        let plan = ExecutionPlan::new(
            self.doc.page_tree_len(),
            self.extract.page_numbers.as_deref(),
            self.extract.maxpages,
        );
        (self.doc.page_count() as i64, plan.order.len() as i64)
    }
}

/// Open a memory-mapped file. Keep the file unchanged until the document
/// and every cursor opened from it are closed.
///
/// # Safety
/// `path` must cover `path_len` bytes; `options` must be null or valid;
/// `out` must be writable; `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_open_path(
    path: *const u8,
    path_len: i64,
    options: *const Options,
    out: *mut *mut Document,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let bytes = slice(path, path_len, "path")?;
            let path = std::str::from_utf8(bytes)
                .map_err(|_| FfiError::new(Code::InvalidPath, "path must be UTF-8"))?;
            validate_path(path)?;
            let options = DocumentOptions::from_raw(options)?;
            let doc = PDFDocument::open_path(path, &options.open)?;
            write_handle(out, "out", Document::new(doc, options.extract))
        })
    }
}

/// A Rust-owned byte buffer the JVM fills before opening it.
pub struct Buffer {
    bytes: Vec<u8>,
}

fn capacity(value: i64) -> FfiResult<usize> {
    usize::try_from(value).map_err(|_| FfiError::invalid("capacity", "capacity is negative"))
}

fn fill_len(buffer: &Buffer, filled: i64) -> FfiResult<usize> {
    let filled =
        usize::try_from(filled).map_err(|_| FfiError::invalid("filled", "filled is negative"))?;
    if filled > buffer.bytes.capacity() {
        return Err(FfiError::invalid(
            "filled",
            "filled exceeds the buffer capacity",
        ));
    }
    Ok(filled)
}

/// Allocate a buffer of at least `capacity` bytes and return where to write.
///
/// # Safety
/// The out-pointers must be writable; `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_buffer_alloc(
    capacity_bytes: i64,
    out: *mut *mut Buffer,
    out_data: *mut *mut u8,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            if out.is_null() {
                return Err(FfiError::invalid("out", "out pointer is null"));
            }
            let mut buffer = Buffer {
                bytes: Vec::with_capacity(capacity(capacity_bytes)?),
            };
            write_out(out_data, "out_data", buffer.bytes.as_mut_ptr())?;
            write_handle(out, "out", buffer)
        })
    }
}

/// Grow a buffer whose first `filled` bytes the caller wrote, keeping them,
/// to at least `capacity` bytes. The data pointer may move.
///
/// # Safety
/// `buffer` must be a live buffer whose first `filled` bytes were written;
/// `out_data` must be writable; `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_buffer_grow(
    buffer: *mut Buffer,
    filled: i64,
    capacity_bytes: i64,
    out_data: *mut *mut u8,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let buffer = handle_mut(buffer)?;
            let filled = fill_len(buffer, filled)?;
            let wanted = capacity(capacity_bytes)?;
            // SAFETY: `filled` is within capacity and the caller wrote it.
            buffer.bytes.set_len(filled);
            buffer.bytes.reserve(wanted.saturating_sub(filled));
            buffer.bytes.set_len(0);
            write_out(out_data, "out_data", buffer.bytes.as_mut_ptr())
        })
    }
}

/// Open a document from a buffer's first `filled` bytes. The buffer is
/// consumed whether or not the open succeeds.
///
/// # Safety
/// `buffer` must be a live buffer whose first `filled` bytes were written;
/// `options` must be null or valid; `out` must be writable; `err` must be
/// null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_open_buffer(
    buffer: *mut Buffer,
    filled: i64,
    options: *const Options,
    out: *mut *mut Document,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let mut buffer =
                from_handle(buffer).ok_or_else(|| FfiError::invalid("handle", "handle is null"))?;
            let filled = fill_len(&buffer, filled)?;
            // SAFETY: `filled` is within capacity and the caller wrote it.
            buffer.bytes.set_len(filled);
            let options = DocumentOptions::from_raw(options)?;
            let doc = PDFDocument::open_vec(std::mem::take(&mut buffer.bytes), &options.open)?;
            write_handle(out, "out", Document::new(doc, options.extract))
        })
    }
}

/// Free a buffer that was never opened.
///
/// # Safety
/// `buffer` must be null or a live buffer, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_buffer_free(buffer: *mut Buffer) {
    unsafe { free_handle(buffer) }
}

/// Free a document. Cursors opened from it stay valid: each holds the
/// document it reads.
///
/// # Safety
/// `document` must be null or a live document with no call in progress, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_document_free(document: *mut Document) {
    unsafe { free_handle(document) }
}

/// Run a call that returns a batch.
unsafe fn batch_call(
    document: *const Document,
    out: *mut *mut Batch,
    err: *mut ErrorOut,
    call: impl FnOnce(&Document) -> FfiResult<Built>,
) -> i32 {
    unsafe {
        guard(err, || {
            let document = handle_ref(document)?;
            if out.is_null() {
                return Err(FfiError::invalid("out", "out pointer is null"));
            }
            let batch = call(document)?.into_batch();
            write_out(out, "out", into_handle(batch))
        })
    }
}

/// The whole document's text, as a text batch.
///
/// # Safety
/// `document` must be a live document; `out` must be writable; `err` must
/// be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_document_text(
    document: *const Document,
    out: *mut *mut Batch,
    err: *mut ErrorOut,
) -> i32 {
    unsafe { batch_call(document, out, err, Document::text_built) }
}

/// The document's metadata, as a metadata batch.
///
/// # Safety
/// As for [`bolivar_document_text`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_document_metadata(
    document: *const Document,
    out: *mut *mut Batch,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        batch_call(document, out, err, |d| {
            metadata_built(&metadata_from_document(&d.doc))
        })
    }
}

/// One 1-based page as a raw batch.
///
/// # Safety
/// As for [`bolivar_document_text`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_document_raw_page(
    document: *const Document,
    page_number: i64,
    out: *mut *mut Batch,
    err: *mut ErrorOut,
) -> i32 {
    unsafe { batch_call(document, out, err, |d| d.raw_page_built(page_number)) }
}

/// The declared page count and the number of pages the options select.
///
/// # Safety
/// `document` must be a live document; the out-pointers must be writable;
/// `err` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_document_counts(
    document: *const Document,
    out_declared: *mut i64,
    out_selected: *mut i64,
    err: *mut ErrorOut,
) -> i32 {
    unsafe {
        guard(err, || {
            let (declared, selected) = handle_ref(document)?.counts();
            write_out(out_declared, "out_declared", declared)?;
            write_out(out_selected, "out_selected", selected)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_reject_blank_nul_and_urls() {
        for path in ["", "  ", "a\0b", "http://x/y.pdf"] {
            assert_eq!(
                validate_path(path).unwrap_err().code,
                Code::InvalidPath,
                "{path:?}"
            );
        }
        assert!(validate_path("a.pdf").is_ok());
    }
}
