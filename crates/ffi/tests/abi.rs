//! Drive the C ABI end to end, as the JVM does: raw pointers in, status
//! codes and batches out.

use std::ptr::{null, null_mut};
use std::sync::Mutex;

use bolivar::abi::{Code, ErrorOut, bolivar_debug_live_handles, bolivar_error_free};
use bolivar::batch::{
    Batch, BatchHeader, ColumnDesc, Elem, KINDS, MAGIC, bolivar_batch_free, bolivar_schema,
};
use bolivar::cursor::{
    CancelHandle, Cursor, bolivar_cancel, bolivar_cancel_free, bolivar_cursor_free,
    bolivar_cursor_next, bolivar_cursor_open,
};
use bolivar::document::{
    Document, bolivar_buffer_alloc, bolivar_buffer_grow, bolivar_document_counts,
    bolivar_document_free, bolivar_document_metadata, bolivar_document_raw_page,
    bolivar_document_text, bolivar_open_buffer, bolivar_open_path,
};
use bolivar::options::{Options, TableOptions, opt, topt};
use bolivar::{ABI_VERSION, bolivar_abi_version, bolivar_version};

/// The live-handle counter is global, so tests that check it run one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn fixture(name: &str) -> String {
    format!(
        "{}/../core/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn error_out() -> ErrorOut {
    ErrorOut {
        code: 0,
        _pad: 0,
        field: null_mut(),
        field_len: 0,
        message: null_mut(),
        message_len: 0,
    }
}

/// A failed call: its code, message and field. Frees the error strings.
#[derive(Debug)]
struct Failure {
    code: i32,
    message: String,
    field: Option<String>,
}

fn utf16(ptr: *const u16, len: i64) -> String {
    if ptr.is_null() {
        return String::new();
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).unwrap()
}

fn take_failure(status: i32, err: &mut ErrorOut) -> Failure {
    let failure = Failure {
        code: status,
        message: utf16(err.message, err.message_len),
        field: (!err.field.is_null()).then(|| utf16(err.field, err.field_len)),
    };
    unsafe { bolivar_error_free(err) };
    failure
}

fn check(status: i32, err: &mut ErrorOut) {
    if status != 0 {
        panic!("call failed: {:?}", take_failure(status, err));
    }
}

fn options() -> Options {
    Options {
        struct_size: size_of::<Options>() as u32,
        present: 0,
        caching: 0,
        bidi: 0,
        detect_vertical: 0,
        all_texts: 0,
        _pad: 0,
        max_pages: 0,
        line_overlap: 0.0,
        char_margin: 0.0,
        line_margin: 0.0,
        word_margin: 0.0,
        boxes_flow: 0.0,
        password: null(),
        password_len: 0,
        pages: null(),
        pages_len: 0,
    }
}

fn open(path: &str, options: Option<&Options>) -> Result<*mut Document, Failure> {
    let mut doc = null_mut();
    let mut err = error_out();
    let status = unsafe {
        bolivar_open_path(
            path.as_ptr(),
            path.len() as i64,
            options.map_or(null(), |o| o as *const Options),
            &mut doc,
            &mut err,
        )
    };
    if status == 0 {
        Ok(doc)
    } else {
        Err(take_failure(status, &mut err))
    }
}

/// A batch read through its header, as Java reads it.
struct View(*mut Batch);

impl View {
    fn header(&self) -> &BatchHeader {
        unsafe { &*self.0.cast::<BatchHeader>() }
    }
    fn desc(&self, index: usize) -> &ColumnDesc {
        assert!(index < self.header().ncols as usize);
        unsafe { &*self.header().cols.add(index) }
    }
    fn column<T>(&self, index: usize, elem: Elem) -> &[T] {
        let desc = self.desc(index);
        assert_eq!(desc.elem, elem as u32, "column {index}");
        if desc.len == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(desc.ptr.cast::<T>(), desc.len as usize) }
    }
    fn text(&self) -> String {
        String::from_utf16(self.column::<u16>(0, Elem::U16)).unwrap()
    }
    fn ints(&self) -> &[i64] {
        self.column(3, Elem::I64)
    }
    fn range(&self, start: i64, end: i64) -> String {
        String::from_utf16(&self.column::<u16>(0, Elem::U16)[start as usize..end as usize]).unwrap()
    }
}

impl Drop for View {
    fn drop(&mut self) {
        unsafe { bolivar_batch_free(self.0) };
    }
}

fn doc_batch(
    doc: *mut Document,
    call: unsafe extern "C" fn(*const Document, *mut *mut Batch, *mut ErrorOut) -> i32,
) -> View {
    let mut batch = null_mut();
    let mut err = error_out();
    check(unsafe { call(doc, &mut batch, &mut err) }, &mut err);
    View(batch)
}

fn open_cursor(
    doc: *mut Document,
    kind: u32,
    tables: Option<&TableOptions>,
) -> Result<(*mut Cursor, *mut CancelHandle), Failure> {
    let mut cursor = null_mut();
    let mut cancel = null_mut();
    let mut err = error_out();
    let status = unsafe {
        bolivar_cursor_open(
            doc,
            kind,
            tables.map_or(null(), |t| t as *const _),
            &mut cursor,
            &mut cancel,
            &mut err,
        )
    };
    if status == 0 {
        Ok((cursor, cancel))
    } else {
        Err(take_failure(status, &mut err))
    }
}

fn next(cursor: *mut Cursor) -> Result<Option<View>, Failure> {
    let mut batch = null_mut();
    let mut err = error_out();
    let status = unsafe { bolivar_cursor_next(cursor, &mut batch, &mut err) };
    if status != 0 {
        return Err(take_failure(status, &mut err));
    }
    Ok((!batch.is_null()).then(|| View(batch)))
}

fn drain(doc: *mut Document, kind: u32, tables: Option<&TableOptions>) -> Vec<View> {
    let (cursor, cancel) = open_cursor(doc, kind, tables).unwrap();
    let mut out = Vec::new();
    while let Some(view) = next(cursor).unwrap() {
        out.push(view);
    }
    unsafe {
        bolivar_cursor_free(cursor);
        bolivar_cancel_free(cancel);
    }
    out
}

#[test]
fn versions_and_schemas_are_exported() {
    let _serial = serial();
    assert_eq!(bolivar_abi_version(), ABI_VERSION);
    let (mut ptr, mut len) = (null(), 0);
    unsafe { bolivar_version(&mut ptr, &mut len) };
    let version =
        std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).unwrap();
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
    for (kind, columns) in KINDS {
        let (mut ptr, mut len, mut err) = (null(), 0, error_out());
        check(
            unsafe { bolivar_schema(*kind, &mut ptr, &mut len, &mut err) },
            &mut err,
        );
        let text =
            std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).unwrap();
        assert_eq!(text.lines().count(), columns.len());
        assert!(text.starts_with("Text:U16\nStrings:U16\nStringOff:I32\nInts:I64\nFloats:F64"));
    }
    let (mut ptr, mut len, mut err) = (null(), 0, error_out());
    let status = unsafe { bolivar_schema(99, &mut ptr, &mut len, &mut err) };
    assert_eq!(
        take_failure(status, &mut err).field.as_deref(),
        Some("kind")
    );
}

#[test]
fn documents_open_extract_and_free_without_leaks() {
    let _serial = serial();
    let before = bolivar_debug_live_handles();
    let doc = open(&fixture("simple1.pdf"), None).unwrap();

    let text = doc_batch(doc, bolivar_document_text);
    assert_eq!(text.header().magic, MAGIC);
    assert_eq!(text.header().abi_version, ABI_VERSION);
    assert!(text.text().contains("Hello"));

    let (mut declared, mut selected, mut err) = (0, 0, error_out());
    check(
        unsafe { bolivar_document_counts(doc, &mut declared, &mut selected, &mut err) },
        &mut err,
    );
    assert_eq!((declared, selected), (1, 1));

    let metadata = doc_batch(doc, bolivar_document_metadata);
    assert_eq!(metadata.ints()[1], 1);

    let pages = drain(doc, 1, None);
    assert_eq!(pages.len(), 1);
    let ints = pages[0].ints();
    assert_eq!(ints[0], 1);
    assert_eq!(
        pages[0].range(ints[1], ints[2]).trim_end(),
        text.text().trim_end_matches('\u{c}').trim_end()
    );

    drop((text, metadata, pages));
    unsafe { bolivar_document_free(doc) };
    assert_eq!(bolivar_debug_live_handles(), before);
}

#[test]
fn every_cursor_kind_yields_its_batch_kind() {
    let _serial = serial();
    let doc = open(
        &fixture("pdfplumber/nics-background-checks-2015-11.pdf"),
        None,
    )
    .unwrap();
    for kind in 1..=6 {
        let batches = drain(doc, kind, None);
        assert_eq!(batches.len(), 1, "kind {kind}");
        assert_eq!(batches[0].header().kind, kind, "kind {kind}");
    }
    let tables = drain(doc, 5, None);
    // One table on this page, with cells.
    assert_eq!(
        tables[0].column::<i32>(7, Elem::I32),
        [0, tables[0].column::<i32>(8, Elem::I32).len() as i32 / 4]
    );
    unsafe { bolivar_document_free(doc) };
}

#[test]
fn errors_carry_codes_messages_and_fields() {
    let _serial = serial();
    let missing = open(&fixture("does-not-exist.pdf"), None).unwrap_err();
    assert_eq!(missing.code, Code::IoNotFound as i32);
    assert!(!missing.message.is_empty());

    assert_eq!(open("", None).unwrap_err().code, Code::InvalidPath as i32);

    let password = b"bar";
    let mut wrong = options();
    wrong.present = opt::PASSWORD;
    wrong.password = password.as_ptr();
    wrong.password_len = 3;
    assert_eq!(
        open(&fixture("encryption/rc4-40.pdf"), Some(&wrong))
            .unwrap_err()
            .code,
        Code::EncryptionError as i32
    );

    let password = b"foo";
    let mut right = options();
    right.present = opt::PASSWORD;
    right.password = password.as_ptr();
    right.password_len = 3;
    let doc = open(&fixture("encryption/rc4-40.pdf"), Some(&right)).unwrap();
    assert!(
        doc_batch(doc, bolivar_document_text)
            .text()
            .contains("Secret!")
    );

    let mut batch = null_mut();
    let mut err = error_out();
    let status = unsafe { bolivar_document_raw_page(doc, 0, &mut batch, &mut err) };
    assert_eq!(
        take_failure(status, &mut err).field.as_deref(),
        Some("page_number")
    );

    let tables = TableOptions {
        struct_size: size_of::<TableOptions>() as u32,
        present: topt::SNAP,
        vertical_strategy: 0,
        horizontal_strategy: 0,
        snap: -1.0,
        snap_x: 0.0,
        snap_y: 0.0,
        join: 0.0,
        join_x: 0.0,
        join_y: 0.0,
        intersection: 0.0,
        intersection_x: 0.0,
        intersection_y: 0.0,
        crop: [0.0; 4],
        first_page_crop: [0.0; 4],
        max_pages: 0,
        explicit_vertical: null(),
        explicit_vertical_len: 0,
        explicit_horizontal: null(),
        explicit_horizontal_len: 0,
    };
    assert_eq!(
        open_cursor(doc, 5, Some(&tables))
            .unwrap_err()
            .field
            .as_deref(),
        Some("snap_tolerance")
    );
    assert_eq!(
        open_cursor(doc, 1, Some(&tables))
            .unwrap_err()
            .field
            .as_deref(),
        Some("table_options")
    );
    assert_eq!(
        open_cursor(doc, 42, None).unwrap_err().field.as_deref(),
        Some("kind")
    );

    let mut out = null_mut();
    let mut err = error_out();
    let status = unsafe { bolivar_document_text(null(), &mut out, &mut err) };
    assert_eq!(
        take_failure(status, &mut err).field.as_deref(),
        Some("handle")
    );
    unsafe { bolivar_document_free(doc) };
}

#[test]
fn buffers_fill_grow_and_open_once() {
    let _serial = serial();
    let before = bolivar_debug_live_handles();
    let bytes = std::fs::read(fixture("simple1.pdf")).unwrap();
    let half = bytes.len() / 2;

    let (mut buffer, mut data, mut err) = (null_mut(), null_mut(), error_out());
    check(
        unsafe { bolivar_buffer_alloc(half as i64, &mut buffer, &mut data, &mut err) },
        &mut err,
    );
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, half) };
    check(
        unsafe {
            bolivar_buffer_grow(buffer, half as i64, bytes.len() as i64, &mut data, &mut err)
        },
        &mut err,
    );
    unsafe {
        std::ptr::copy_nonoverlapping(bytes[half..].as_ptr(), data.add(half), bytes.len() - half)
    };
    let mut doc = null_mut();
    check(
        unsafe { bolivar_open_buffer(buffer, bytes.len() as i64, null(), &mut doc, &mut err) },
        &mut err,
    );
    assert!(
        doc_batch(doc, bolivar_document_text)
            .text()
            .contains("Hello")
    );
    unsafe { bolivar_document_free(doc) };

    // A failed open still consumes the buffer.
    let garbage = b"not a pdf";
    check(
        unsafe { bolivar_buffer_alloc(16, &mut buffer, &mut data, &mut err) },
        &mut err,
    );
    unsafe { std::ptr::copy_nonoverlapping(garbage.as_ptr(), data, garbage.len()) };
    let status =
        unsafe { bolivar_open_buffer(buffer, garbage.len() as i64, null(), &mut doc, &mut err) };
    assert_ne!(take_failure(status, &mut err).code, 0);

    // Filling past capacity is refused.
    check(
        unsafe { bolivar_buffer_alloc(4, &mut buffer, &mut data, &mut err) },
        &mut err,
    );
    let status = unsafe { bolivar_open_buffer(buffer, 5, null(), &mut doc, &mut err) };
    assert_eq!(
        take_failure(status, &mut err).field.as_deref(),
        Some("filled")
    );

    assert_eq!(bolivar_debug_live_handles(), before);
}

#[test]
fn cursors_outlive_their_document_and_cancel_safely() {
    let _serial = serial();
    let doc = open(&fixture("nonfree/dmca.pdf"), None).unwrap();
    let (cursor, cancel) = open_cursor(doc, 1, None).unwrap();
    unsafe { bolivar_document_free(doc) };
    assert!(
        next(cursor).unwrap().is_some(),
        "the cursor keeps its document"
    );

    unsafe { bolivar_cancel(cancel) };
    // After a cancel the cursor either reports it or has already finished.
    let mut seen = 0;
    loop {
        match next(cursor) {
            Ok(Some(_)) => seen += 1,
            Ok(None) => break,
            Err(failure) => {
                assert_eq!(failure.code, Code::Cancelled as i32);
                break;
            }
        }
        assert!(seen < 18);
    }
    // Finished cursors stay finished.
    assert!(next(cursor).unwrap().is_none());
    unsafe {
        bolivar_cursor_free(cursor);
        // Cancelling after the cursor is gone is harmless.
        bolivar_cancel(cancel);
        bolivar_cancel_free(cancel);
    }
}

#[test]
fn options_select_pages() {
    let _serial = serial();
    let pages = [2_i64, 4];
    let mut o = options();
    o.present = opt::PAGES;
    o.pages = pages.as_ptr();
    o.pages_len = 2;
    let doc = open(&fixture("contrib/pagelabels.pdf"), Some(&o)).unwrap();
    let numbers: Vec<i64> = drain(doc, 1, None).iter().map(|v| v.ints()[0]).collect();
    assert_eq!(numbers, [2, 4]);
    let raw = drain(doc, 2, None);
    assert_eq!(raw.iter().map(|v| v.ints()[1]).collect::<Vec<_>>(), [2, 4]);
    let label = &raw[0];
    assert_eq!(label.range(label.ints()[4], label.ints()[5]), "iv");
    unsafe { bolivar_document_free(doc) };
}

#[test]
fn boxes_flow_none_disables_box_ordering() {
    let _serial = serial();
    let page_text = |options: Option<&Options>| {
        let doc = open(&fixture("nonfree/naacl06-shinyama.pdf"), options).unwrap();
        let first = drain(doc, 1, None).into_iter().next().unwrap();
        let text = first.range(first.ints()[1], first.ints()[2]);
        unsafe { bolivar_document_free(doc) };
        text
    };
    let mut none = options();
    none.present = opt::BOXES_FLOW_NONE;
    let mut half = options();
    half.present = opt::BOXES_FLOW;
    half.boxes_flow = 0.5;
    // The UniFFI binding read "none" as the 0.5 default; it now disables ordering.
    assert_eq!(page_text(Some(&half)), page_text(None));
    assert_ne!(page_text(Some(&none)), page_text(None));
}

#[test]
fn struct_sizes_are_exported() {
    let mut sizes = [0_i64; 5];
    let count = unsafe { bolivar::bolivar_debug_struct_sizes(sizes.as_mut_ptr(), 5) };
    assert_eq!(count, 5);
    assert_eq!(sizes, [96, 192, 40, 24, 24]);
}

#[test]
fn null_out_pointers_leak_no_handles() {
    let _serial = serial();
    let doc = open(&fixture("simple1.pdf"), None).unwrap();
    let before = bolivar_debug_live_handles();
    let mut err = error_out();
    let status = unsafe { bolivar_document_text(doc, null_mut(), &mut err) };
    assert_eq!(take_failure(status, &mut err).field.as_deref(), Some("out"));
    let path = fixture("simple1.pdf");
    let status = unsafe {
        bolivar_open_path(
            path.as_ptr(),
            path.len() as i64,
            null(),
            null_mut(),
            &mut err,
        )
    };
    assert_eq!(take_failure(status, &mut err).field.as_deref(), Some("out"));
    let mut data = null_mut();
    let status = unsafe { bolivar_buffer_alloc(8, null_mut(), &mut data, &mut err) };
    assert_eq!(take_failure(status, &mut err).field.as_deref(), Some("out"));
    assert_eq!(bolivar_debug_live_handles(), before);
    unsafe { bolivar_document_free(doc) };
}
