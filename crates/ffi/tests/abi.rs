//! Drive the C ABI end to end, as the JVM does: raw pointers in, status
//! codes and batches out.

use std::ptr::{null, null_mut};
use std::sync::{Mutex, MutexGuard, PoisonError};

use bolivar::abi::{Code, ErrorOut, bolivar_debug_live_handles, bolivar_error_free};
use bolivar::batch::{
    Batch, BatchHeader, ColumnDesc, Elem, KINDS, Kind, MAGIC, bolivar_batch_free, bolivar_schema,
};
use bolivar::cursor::{
    Cursor, bolivar_cursor_cancel, bolivar_cursor_free, bolivar_cursor_next, bolivar_cursor_open,
};
use bolivar::document::{
    Document, bolivar_buffer_alloc, bolivar_buffer_grow, bolivar_document_counts,
    bolivar_document_free, bolivar_document_metadata, bolivar_document_raw_page,
    bolivar_document_text, bolivar_open_buffer, bolivar_open_path,
};
use bolivar::options::{CursorOptions, Options, TableOptions, copt, opt, topt};
use bolivar::{ABI_VERSION, bolivar_abi_version, bolivar_describe, bolivar_version};

/// The live-handle counter is global, so tests run one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn fixture(name: &str) -> String {
    format!(
        "{}/../core/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// A failed call: its code, message and field. Frees the error strings.
#[derive(Debug)]
struct Failure {
    code: i32,
    message: String,
    field: Option<String>,
}

fn utf16(ptr: *const u16, len: i64) -> String {
    String::from_utf16(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).unwrap()
}

fn take_failure(status: i32, err: &mut ErrorOut) -> Failure {
    let failure = Failure {
        code: status,
        message: if err.message.is_null() {
            String::new()
        } else {
            utf16(err.message, err.message_len)
        },
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

fn static_text(ptr: *const u8, len: i64) -> &'static str {
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).unwrap()
}

fn open(path: &str, options: Option<&Options>) -> Result<*mut Document, Failure> {
    let (mut doc, mut err) = (null_mut(), ErrorOut::default());
    let status = unsafe {
        bolivar_open_path(
            path.as_ptr(),
            path.len() as i64,
            options.map_or(null(), |o| o),
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
struct View(*mut Batch, &'static Kind);

impl View {
    fn header(&self) -> &BatchHeader {
        unsafe { &*self.0.cast::<BatchHeader>() }
    }
    fn column<T>(&self, name: &str, elem: Elem) -> &[T] {
        let index = self.1.columns.iter().position(|(n, _)| *n == name).unwrap();
        let desc: &ColumnDesc = unsafe { &*self.header().cols.add(index) };
        assert_eq!(desc.elem, elem as u32, "column {name}");
        if desc.len == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(desc.ptr.cast::<T>(), desc.len as usize) }
    }
    fn int(&self, name: &str) -> i64 {
        let slot = self.1.ints.iter().position(|n| *n == name).unwrap();
        self.column::<i64>("Ints", Elem::I64)[slot]
    }
    fn text(&self, start: i64, end: i64) -> String {
        String::from_utf16(&self.column::<u16>("Text", Elem::U16)[start as usize..end as usize])
            .unwrap()
    }
}

impl Drop for View {
    fn drop(&mut self) {
        unsafe { bolivar_batch_free(self.0) };
    }
}

fn kind(number: u32) -> &'static Kind {
    KINDS.iter().copied().find(|k| k.number == number).unwrap()
}

fn doc_batch(
    doc: *mut Document,
    number: u32,
    call: impl FnOnce(*const Document, *mut *mut Batch, *mut ErrorOut) -> i32,
) -> View {
    let (mut batch, mut err) = (null_mut(), ErrorOut::default());
    check(call(doc, &mut batch, &mut err), &mut err);
    let view = View(batch, kind(number));
    assert_eq!(view.header().kind, number);
    view
}

fn open_cursor(
    doc: *mut Document,
    number: u32,
    tables: Option<&TableOptions>,
) -> Result<*mut Cursor, Failure> {
    open_cursor_with(doc, number, tables, None)
}

fn open_cursor_with(
    doc: *mut Document,
    number: u32,
    tables: Option<&TableOptions>,
    selection: Option<&CursorOptions>,
) -> Result<*mut Cursor, Failure> {
    let (mut cursor, mut err) = (null_mut(), ErrorOut::default());
    let status = unsafe {
        bolivar_cursor_open(
            doc,
            number,
            tables.map_or(null(), |t| t),
            selection.map_or(null(), |c| c),
            &mut cursor,
            &mut err,
        )
    };
    if status == 0 {
        Ok(cursor)
    } else {
        Err(take_failure(status, &mut err))
    }
}

fn next(cursor: *mut Cursor, number: u32) -> Result<Option<View>, Failure> {
    let (mut batch, mut err) = (null_mut(), ErrorOut::default());
    let status = unsafe { bolivar_cursor_next(cursor, &mut batch, &mut err) };
    if status != 0 {
        return Err(take_failure(status, &mut err));
    }
    Ok((!batch.is_null()).then(|| View(batch, kind(number))))
}

fn drain(doc: *mut Document, number: u32, tables: Option<&TableOptions>) -> Vec<View> {
    drain_with(doc, number, tables, None)
}

fn drain_with(
    doc: *mut Document,
    number: u32,
    tables: Option<&TableOptions>,
    selection: Option<&CursorOptions>,
) -> Vec<View> {
    let cursor = open_cursor_with(doc, number, tables, selection).unwrap();
    let mut out = Vec::new();
    while let Some(view) = next(cursor, number).unwrap() {
        out.push(view);
    }
    unsafe { bolivar_cursor_free(cursor) };
    out
}

const LAYOUT: u32 = 1;
const RAW: u32 = 2;
const TABLES: u32 = 5;
const TABLE_ROWS: u32 = 6;
const METADATA: u32 = 7;
const TEXT: u32 = 8;

fn whole_text(doc: *mut Document) -> String {
    let view = doc_batch(doc, TEXT, |d, out, err| unsafe {
        bolivar_document_text(d, out, err)
    });
    String::from_utf8(view.column::<u8>("Utf8", Elem::U8).to_vec()).unwrap()
}

#[test]
fn versions_schemas_and_descriptions_are_exported() {
    let _serial = serial();
    assert_eq!(bolivar_abi_version(), ABI_VERSION);
    let (mut ptr, mut len) = (null(), 0);
    unsafe { bolivar_version(&mut ptr, &mut len) };
    assert_eq!(static_text(ptr, len), env!("CARGO_PKG_VERSION"));
    for k in KINDS {
        let (mut ptr, mut len, mut err) = (null(), 0, ErrorOut::default());
        check(
            unsafe { bolivar_schema(k.number, &mut ptr, &mut len, &mut err) },
            &mut err,
        );
        let text = static_text(ptr, len);
        assert!(text.starts_with(
            "col:Text:U16\ncol:Strings:U16\ncol:StringOff:I32\ncol:Ints:I64\ncol:Floats:F64"
        ));
        assert_eq!(
            text.lines().filter(|l| l.starts_with("col:")).count(),
            k.columns.len()
        );
        assert_eq!(
            text.lines().filter(|l| l.starts_with("int:")).count(),
            k.ints.len()
        );
    }
    let (mut ptr, mut len, mut err) = (null(), 0, ErrorOut::default());
    let status = unsafe { bolivar_schema(99, &mut ptr, &mut len, &mut err) };
    assert_eq!(
        take_failure(status, &mut err).field.as_deref(),
        Some("kind")
    );

    let name = "Options";
    check(
        unsafe { bolivar_describe(name.as_ptr(), 7, &mut ptr, &mut len, &mut err) },
        &mut err,
    );
    assert!(static_text(ptr, len).starts_with("size:96\nfield:struct_size:0:4"));
    let name = "Status";
    check(
        unsafe { bolivar_describe(name.as_ptr(), 6, &mut ptr, &mut len, &mut err) },
        &mut err,
    );
    assert!(static_text(ptr, len).contains("value:UNSUPPORTED:13"));
}

#[test]
fn documents_open_extract_and_free_without_leaks() {
    let _serial = serial();
    let before = bolivar_debug_live_handles();
    let doc = open(&fixture("simple1.pdf"), None).unwrap();
    let text = whole_text(doc);
    assert!(text.contains("Hello"));

    let (mut declared, mut selected, mut err) = (0, 0, ErrorOut::default());
    check(
        unsafe { bolivar_document_counts(doc, &mut declared, &mut selected, &mut err) },
        &mut err,
    );
    assert_eq!((declared, selected), (1, 1));
    assert_eq!(
        doc_batch(doc, METADATA, |d, out, err| unsafe {
            bolivar_document_metadata(d, out, err)
        })
        .int("PageCount"),
        1
    );

    let pages = drain(doc, LAYOUT, None);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].int("PageNumber"), 1);
    let page_text = pages[0].text(0, pages[0].int("TextEnd"));
    assert_eq!(
        page_text.trim_end(),
        text.trim_end_matches('\u{c}').trim_end()
    );

    drop(pages);
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
    for number in 1..=6 {
        let batches = drain(doc, number, None);
        assert_eq!(batches.len(), 1, "kind {number}");
        assert_eq!(batches[0].header().kind, number);
        assert_eq!(batches[0].header().magic, MAGIC);
    }
    let tables = drain(doc, TABLES, None);
    let cells = tables[0].column::<i32>("CellIndex", Elem::I32).len() as i32 / 4;
    assert_eq!(tables[0].column::<i32>("TableCells", Elem::I32), [0, cells]);
    unsafe { bolivar_document_free(doc) };
}

#[test]
fn errors_carry_codes_messages_and_fields() {
    let _serial = serial();
    let missing = open(&fixture("does-not-exist.pdf"), None).unwrap_err();
    assert_eq!(missing.code, Code::IoNotFound as i32);
    assert!(!missing.message.is_empty());
    assert_eq!(open("", None).unwrap_err().code, Code::InvalidPath as i32);

    let with_password = |password: &'static [u8]| {
        let mut options = Options::empty();
        options.present = opt::PASSWORD;
        options.password = password.as_ptr();
        options.password_len = password.len() as i64;
        options
    };
    let wrong = open(
        &fixture("encryption/rc4-40.pdf"),
        Some(&with_password(b"bar")),
    )
    .unwrap_err();
    assert_eq!(wrong.code, Code::EncryptionError as i32);
    let doc = open(
        &fixture("encryption/rc4-40.pdf"),
        Some(&with_password(b"foo")),
    )
    .unwrap();
    assert!(whole_text(doc).contains("Secret!"));

    let (mut batch, mut err) = (null_mut(), ErrorOut::default());
    let status = unsafe { bolivar_document_raw_page(doc, 0, &mut batch, &mut err) };
    assert_eq!(
        take_failure(status, &mut err).field.as_deref(),
        Some("page_number")
    );

    let mut tables = TableOptions::empty();
    tables.present = topt::SNAP;
    tables.snap = -1.0;
    assert_eq!(
        open_cursor(doc, TABLES, Some(&tables))
            .unwrap_err()
            .field
            .as_deref(),
        Some("snap_tolerance")
    );
    assert_eq!(
        open_cursor(doc, LAYOUT, Some(&tables))
            .unwrap_err()
            .field
            .as_deref(),
        Some("table_options")
    );
    assert_eq!(
        open_cursor(doc, 42, None).unwrap_err().field.as_deref(),
        Some("kind")
    );

    let (mut out, mut err) = (null_mut(), ErrorOut::default());
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

    let (mut buffer, mut data, mut err) = (null_mut(), null_mut(), ErrorOut::default());
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
    assert!(whole_text(doc).contains("Hello"));
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
    let cursor = open_cursor(doc, LAYOUT, None).unwrap();
    unsafe { bolivar_document_free(doc) };
    assert!(
        next(cursor, LAYOUT).unwrap().is_some(),
        "the cursor keeps its document"
    );

    unsafe { bolivar_cursor_cancel(cursor) };
    let mut seen = 0;
    loop {
        match next(cursor, LAYOUT) {
            Ok(Some(_)) => seen += 1,
            Ok(None) => break,
            Err(failure) => {
                assert_eq!(failure.code, Code::Cancelled as i32);
                break;
            }
        }
        assert!(seen < 18);
    }
    assert!(
        next(cursor, LAYOUT).unwrap().is_none(),
        "finished cursors stay finished"
    );
    unsafe { bolivar_cursor_free(cursor) };
}

#[test]
fn options_select_pages_and_raw_pages_carry_labels() {
    let _serial = serial();
    let pages = [2_i64, 4];
    let mut options = Options::empty();
    options.present = opt::PAGES;
    options.pages = pages.as_ptr();
    options.pages_len = 2;
    let doc = open(&fixture("contrib/pagelabels.pdf"), Some(&options)).unwrap();
    let numbers: Vec<i64> = drain(doc, LAYOUT, None)
        .iter()
        .map(|v| v.int("PageNumber"))
        .collect();
    assert_eq!(numbers, [2, 4]);
    let raw = drain(doc, RAW, None);
    assert_eq!(
        raw.iter().map(|v| v.int("PageIndex")).collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(
        raw[0].text(raw[0].int("LabelStart"), raw[0].int("LabelEnd")),
        "iv"
    );

    let one = doc_batch(doc, RAW, |d, out, err| unsafe {
        bolivar_document_raw_page(d, 2, out, err)
    });
    assert_eq!(one.int("PageIndex"), 1);
    assert_eq!(
        one.text(0, one.int("TextEnd")),
        raw[0].text(0, raw[0].int("TextEnd"))
    );
    unsafe { bolivar_document_free(doc) };
}

fn table_row_pages(document_max: Option<i64>, table_max: Option<i64>) -> Vec<i64> {
    let mut options = Options::empty();
    if let Some(max) = document_max {
        options.present = opt::MAX_PAGES;
        options.max_pages = max;
    }
    let doc = open(&fixture("contrib/pagelabels.pdf"), Some(&options)).unwrap();
    let mut tables = TableOptions::empty();
    if let Some(max) = table_max {
        tables.present = topt::MAX_PAGES;
        tables.max_pages = max;
    }
    let pages = drain(doc, TABLE_ROWS, Some(&tables))
        .iter()
        .map(|v| v.int("PageNumber"))
        .collect();
    unsafe { bolivar_document_free(doc) };
    pages
}

#[test]
fn table_max_pages_stays_inside_the_document_selection() {
    let _serial = serial();
    assert_eq!(table_row_pages(None, Some(3)), [1, 2, 3]);
    assert_eq!(table_row_pages(Some(4), None), [1, 2, 3, 4]);
    assert_eq!(table_row_pages(Some(4), Some(2)), [1, 2]);
    assert_eq!(
        table_row_pages(Some(1), Some(3)),
        [1],
        "a table cursor never reads pages the document does not select"
    );
}

/// Cursor options with the given 1-based pages, skip, max pages and read-ahead.
fn cursor_options(
    pages: Option<&[i64]>,
    skip: Option<i64>,
    max_pages: Option<i64>,
    read_ahead: Option<i64>,
) -> CursorOptions {
    let mut raw = CursorOptions::empty();
    if let Some(pages) = pages {
        raw.present |= copt::PAGES;
        raw.pages = pages.as_ptr();
        raw.pages_len = pages.len() as i64;
    }
    if let Some(skip) = skip {
        raw.present |= copt::SKIP;
        raw.skip = skip;
    }
    if let Some(max) = max_pages {
        raw.present |= copt::MAX_PAGES;
        raw.max_pages = max;
    }
    if let Some(read_ahead) = read_ahead {
        raw.present |= copt::READ_AHEAD;
        raw.read_ahead = read_ahead;
    }
    raw
}

fn page_numbers(doc: *mut Document, number: u32, selection: &CursorOptions) -> Vec<i64> {
    drain_with(doc, number, None, Some(selection))
        .iter()
        .map(|v| v.int("PageNumber"))
        .collect()
}

#[test]
fn cursor_options_narrow_the_document_selection() {
    let _serial = serial();
    let document_pages = [2_i64, 3, 5];
    let mut options = Options::empty();
    options.present = opt::PAGES;
    options.pages = document_pages.as_ptr();
    options.pages_len = 3;
    let doc = open(&fixture("contrib/pagelabels.pdf"), Some(&options)).unwrap();
    const SUMMARY: u32 = 4;

    let head = cursor_options(None, None, Some(2), Some(1));
    let rest = cursor_options(None, Some(2), None, None);
    assert_eq!(page_numbers(doc, SUMMARY, &head), [2, 3]);
    assert_eq!(
        page_numbers(doc, SUMMARY, &rest),
        [5],
        "head plus skip reads each page once"
    );
    let inside = [1_i64, 3, 4, 5];
    assert_eq!(
        page_numbers(
            doc,
            LAYOUT,
            &cursor_options(Some(&inside), None, None, None)
        ),
        [3, 5],
        "only pages the document selects"
    );
    let none: [i64; 0] = [];
    assert!(page_numbers(doc, SUMMARY, &cursor_options(Some(&none), None, None, None)).is_empty());
    unsafe { bolivar_document_free(doc) };
}

#[test]
fn cursor_and_table_max_pages_combine() {
    let _serial = serial();
    let doc = open(&fixture("contrib/pagelabels.pdf"), None).unwrap();
    let rows = |table_max: Option<i64>, cursor_max: Option<i64>| -> Vec<i64> {
        let mut tables = TableOptions::empty();
        if let Some(max) = table_max {
            tables.present = topt::MAX_PAGES;
            tables.max_pages = max;
        }
        let selection = cursor_options(None, None, cursor_max, None);
        drain_with(doc, TABLE_ROWS, Some(&tables), Some(&selection))
            .iter()
            .map(|v| v.int("PageNumber"))
            .collect()
    };
    assert_eq!(rows(Some(2), None), [1, 2]);
    assert_eq!(rows(None, Some(3)), [1, 2, 3]);
    assert_eq!(rows(Some(4), Some(1)), [1]);
    assert_eq!(rows(Some(1), Some(4)), [1]);
    unsafe { bolivar_document_free(doc) };
}

#[test]
fn first_page_crop_stays_on_the_documents_first_page() {
    let _serial = serial();
    let doc = open(&fixture("contrib/pagelabels.pdf"), None).unwrap();
    // Text strategies find a table in any text, so a crop visibly changes a page.
    let tables = |crop: bool| {
        let mut t = TableOptions::empty();
        t.present = topt::VERTICAL_STRATEGY | topt::HORIZONTAL_STRATEGY;
        t.vertical_strategy = 2;
        t.horizontal_strategy = 2;
        if crop {
            t.present |= topt::FIRST_PAGE_CROP;
            t.first_page_crop = [0.0, 0.0, 1.0, 1.0];
        }
        t
    };
    let cells = |pages: &[i64], crop: bool| -> Vec<Vec<i32>> {
        let selection = cursor_options(Some(pages), None, None, None);
        drain_with(doc, TABLE_ROWS, Some(&tables(crop)), Some(&selection))
            .iter()
            .map(|v| v.column::<i32>("CellText", Elem::I32).to_vec())
            .collect()
    };
    assert_ne!(
        cells(&[1], true),
        cells(&[1], false),
        "the crop applies to page 1"
    );
    assert_eq!(
        cells(&[2, 3], true),
        cells(&[2, 3], false),
        "pages 2 and 3 never take it"
    );
    unsafe { bolivar_document_free(doc) };
}

#[test]
fn null_out_pointers_leak_no_handles() {
    let _serial = serial();
    let doc = open(&fixture("simple1.pdf"), None).unwrap();
    let before = bolivar_debug_live_handles();
    let mut err = ErrorOut::default();
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

#[test]
fn boxes_flow_none_disables_box_ordering() {
    let _serial = serial();
    let page_text = |options: Option<&Options>| {
        let doc = open(&fixture("nonfree/naacl06-shinyama.pdf"), options).unwrap();
        let first = drain(doc, LAYOUT, None).into_iter().next().unwrap();
        let text = first.text(0, first.int("TextEnd"));
        unsafe { bolivar_document_free(doc) };
        text
    };
    let mut none = Options::empty();
    none.present = opt::BOXES_FLOW_NONE;
    let mut half = Options::empty();
    half.present = opt::BOXES_FLOW;
    half.boxes_flow = 0.5;
    assert_eq!(page_text(Some(&half)), page_text(None));
    assert_ne!(page_text(Some(&none)), page_text(None));
}
