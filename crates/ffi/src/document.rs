//! Opening documents and one-shot document calls.

use std::sync::Arc;

use bolivar_core::extract::extract_text_with_document;
use bolivar_core::pdfdocument::{DEFAULT_CACHE_CAPACITY, PDFDocument};

use crate::abi::{
    Code, ErrorOut, FfiError, FfiResult, from_handle, guard, handle_mut, handle_ref, slice,
    write_handle, write_out,
};
use crate::batch::{Batch, Builder, text};
use crate::cursor::{CursorState, Kind};
use crate::metadata::{metadata_batch, metadata_from_document};
use crate::options::{DocOptions, Options, Tables};

/// An open document and the options it was opened with.
pub struct Document {
    pub(crate) doc: Arc<PDFDocument>,
    pub(crate) options: DocOptions,
}

fn cache_capacity(caching: bool) -> usize {
    if caching { DEFAULT_CACHE_CAPACITY } else { 0 }
}

fn validate_path(path: &str) -> FfiResult<()> {
    if path.trim().is_empty() || path.contains('\0') || path.contains("://") {
        return Err(FfiError::new(Code::InvalidPath, "path is invalid"));
    }
    Ok(())
}

impl Document {
    fn open_path(path: &str, mut options: DocOptions) -> FfiResult<Self> {
        validate_path(path)?;
        let doc = PDFDocument::new_from_path_with_cache_and_fallback(
            path,
            &options.core.password,
            cache_capacity(options.core.caching),
            true,
        )?;
        // The password has done its work; do not keep it in memory.
        options.core.password = String::new();
        Ok(Self {
            doc: Arc::new(doc),
            options,
        })
    }

    fn open_bytes(bytes: Vec<u8>, mut options: DocOptions) -> FfiResult<Self> {
        let doc = PDFDocument::new_from_vec_with_cache(
            bytes,
            &options.core.password,
            cache_capacity(options.core.caching),
        )?;
        options.core.password = String::new();
        Ok(Self {
            doc: Arc::new(doc),
            options,
        })
    }

    /// The whole document's text.
    pub fn text_batch(&self) -> FfiResult<Batch> {
        let value = extract_text_with_document(self.doc.as_ref(), self.options.core.clone())?;
        let mut b = Builder::new(text::KIND, text::COLUMNS);
        b.text(&value)?;
        b.finish(&[])
    }

    /// One 1-based page as a raw batch.
    pub fn raw_page_batch(&self, page_number: i64) -> FfiResult<Batch> {
        let tree_len = self.doc.page_tree_len() as i64;
        if page_number < 1 || page_number > tree_len {
            return Err(FfiError::invalid(
                "page_number",
                format!("page_number must be within 1..={tree_len}"),
            ));
        }
        let mut options = self.options.clone();
        options.core.page_numbers = Some(vec![(page_number - 1) as usize]);
        options.core.maxpages = 1;
        let mut state = CursorState::open(Kind::Raw, &self.doc, options, Tables::default())?;
        state
            .next_batch()?
            .ok_or_else(|| FfiError::invalid("page_number", "page produced no output"))
    }

    /// The declared page count and the number of pages the options select.
    pub fn counts(&self) -> (i64, i64) {
        let selected = bolivar_core::engine::select_pages(
            self.doc.page_tree_len(),
            self.options.core.page_numbers.clone(),
            self.options.core.maxpages,
        );
        (self.doc.page_count() as i64, selected.len() as i64)
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
            let options = DocOptions::from_raw(options)?;
            let document = Document::open_path(path, options)?;
            write_handle(out, "out", document)
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
            let mut buffer = Buffer {
                bytes: Vec::with_capacity(capacity(capacity_bytes)?),
            };
            let data = buffer.bytes.as_mut_ptr();
            write_out(out_data, "out_data", data)?;
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
            let options = DocOptions::from_raw(options)?;
            let document = Document::open_bytes(std::mem::take(&mut buffer.bytes), options)?;
            write_handle(out, "out", document)
        })
    }
}

/// Free a buffer that was never opened.
///
/// # Safety
/// `buffer` must be null or a live buffer, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_buffer_free(buffer: *mut Buffer) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(unsafe { from_handle(buffer) });
    }));
}

/// Free a document. Cursors opened from it stay valid: each holds the
/// document it reads.
///
/// # Safety
/// `document` must be null or a live document with no call in progress, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bolivar_document_free(document: *mut Document) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(unsafe { from_handle(document) });
    }));
}

/// Run a call that returns a batch.
unsafe fn batch_call(
    document: *const Document,
    out: *mut *mut Batch,
    err: *mut ErrorOut,
    call: impl FnOnce(&Document) -> FfiResult<Batch>,
) -> i32 {
    unsafe {
        guard(err, || {
            let document = handle_ref(document)?;
            let batch = call(document)?;
            write_handle(out, "out", batch)
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
    unsafe { batch_call(document, out, err, Document::text_batch) }
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
            metadata_batch(&metadata_from_document(&d.doc))
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
    unsafe { batch_call(document, out, err, |d| d.raw_page_batch(page_number)) }
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
