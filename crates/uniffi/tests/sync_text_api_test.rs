use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bolivar_uniffi::{
    BolivarError, ExtractOptions, LayoutParams, NativePdfDocument, quick_extract_text,
    quick_extract_text_from_bytes,
};
mod common;
use common::build_minimal_pdf_with_pages;

fn build_single_page_text_pdf(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");

    let mut offsets: Vec<usize> = Vec::new();
    let push_obj = |buf: &mut Vec<u8>, obj: String, offsets: &mut Vec<usize>| {
        offsets.push(buf.len());
        buf.extend_from_slice(obj.as_bytes());
    };

    push_obj(
        &mut out,
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n".to_string(),
        &mut offsets,
    );

    let escaped = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 12 Tf 10 20 Td ({escaped}) Tj ET\n");
    push_obj(
        &mut out,
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            stream.len(),
            stream
        ),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string(),
        &mut offsets,
    );

    let xref_pos = out.len();
    let obj_count = offsets.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", obj_count + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
    }
    out.extend_from_slice(b"trailer\n<< /Size ");
    out.extend_from_slice((obj_count + 1).to_string().as_bytes());
    out.extend_from_slice(b" /Root 1 0 R >>\nstartxref\n");
    out.extend_from_slice(xref_pos.to_string().as_bytes());
    out.extend_from_slice(b"\n%%EOF");

    out
}

fn build_single_page_multiline_text_pdf(first: &str, second: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");

    let mut offsets: Vec<usize> = Vec::new();
    let push_obj = |buf: &mut Vec<u8>, obj: String, offsets: &mut Vec<usize>| {
        offsets.push(buf.len());
        buf.extend_from_slice(obj.as_bytes());
    };

    push_obj(
        &mut out,
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n".to_string(),
        &mut offsets,
    );

    let first = first
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let second = second
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!(
        "BT /F1 12 Tf 10 40 Td ({first}) Tj ET\nBT /F1 12 Tf 120 120 Td ({second}) Tj ET\n"
    );
    push_obj(
        &mut out,
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            stream.len(),
            stream
        ),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string(),
        &mut offsets,
    );

    let xref_pos = out.len();
    let obj_count = offsets.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", obj_count + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
    }
    out.extend_from_slice(b"trailer\n<< /Size ");
    out.extend_from_slice((obj_count + 1).to_string().as_bytes());
    out.extend_from_slice(b" /Root 1 0 R >>\nstartxref\n");
    out.extend_from_slice(xref_pos.to_string().as_bytes());
    out.extend_from_slice(b"\n%%EOF");

    out
}

fn build_metadata_pdf() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");

    let mut offsets: Vec<usize> = Vec::new();
    let push_obj = |buf: &mut Vec<u8>, obj: String, offsets: &mut Vec<usize>| {
        offsets.push(buf.len());
        buf.extend_from_slice(obj.as_bytes());
    };

    push_obj(
        &mut out,
        "1 0 obj\n<< /Type /Catalog /Version /1.7 /Pages 2 0 R /Names << /JavaScript 6 0 R >> /MarkInfo << /Marked true /UserProperties true /Suspects false >> /StructTreeRoot 9 0 R /AcroForm << >> /Metadata 7 0 R >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "4 0 obj\n<< >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "5 0 obj\n<< >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "6 0 obj\n<< /Names [(startup) 10 0 R] >>\nendobj\n".to_string(),
        &mut offsets,
    );
    let xmp = "<x:xmpmeta>fixture</x:xmpmeta>";
    push_obj(
        &mut out,
        format!(
            "7 0 obj\n<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            xmp.len(),
            xmp
        ),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "8 0 obj\n<< /Title (Example Document) /Author (Example Author) /Creator (Octagon 5.0) /Producer (iText) /CreationDate (D:20260704114317+03'00') /CustomField (custom value) >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "9 0 obj\n<< /Type /StructTreeRoot >>\nendobj\n".to_string(),
        &mut offsets,
    );
    push_obj(
        &mut out,
        "10 0 obj\n<< /S /JavaScript /JS (app.alert\\(1\\)) >>\nendobj\n".to_string(),
        &mut offsets,
    );

    let xref_pos = out.len();
    let obj_count = offsets.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", obj_count + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
    }
    out.extend_from_slice(b"trailer\n<< /Size ");
    out.extend_from_slice((obj_count + 1).to_string().as_bytes());
    out.extend_from_slice(b" /Root 1 0 R /Info 8 0 R >>\nstartxref\n");
    out.extend_from_slice(xref_pos.to_string().as_bytes());
    out.extend_from_slice(b"\n%%EOF");

    out
}

fn write_temp_pdf(data: &[u8]) -> PathBuf {
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut path = std::env::temp_dir();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let pid = std::process::id();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.push(format!("bolivar_uniffi_test_{pid}_{stamp}_{counter}.pdf"));
    std::fs::write(&path, data).expect("write temp pdf");
    path
}

fn table_fixture_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../references/pdfplumber/tests/pdfs/table-curves-example.pdf");
    assert!(path.exists(), "table fixture should exist at {path:?}");
    path
}

fn options_with_page_range(page_numbers: Vec<u32>, max_pages: Option<u32>) -> ExtractOptions {
    ExtractOptions {
        password: None,
        page_numbers: Some(page_numbers),
        max_pages,
        caching: Some(true),
        layout_params: None,
        bidi: None,
    }
}

#[test]
fn native_document_from_path_matches_from_bytes_for_same_pdf() {
    let pdf = build_minimal_pdf_with_pages(1);
    let path = write_temp_pdf(&pdf);

    let from_bytes = NativePdfDocument::from_bytes(pdf.clone(), None).expect("doc bytes");
    let from_path =
        NativePdfDocument::from_path(path.to_string_lossy().to_string(), None).expect("doc path");

    let text_from_bytes = from_bytes.extract_text().expect("extract text bytes");
    let text_from_path = from_path.extract_text().expect("extract text path");

    assert_eq!(text_from_bytes, text_from_path);

    let quick_path =
        quick_extract_text(path.to_string_lossy().to_string(), None).expect("quick extract path");
    let quick_bytes = quick_extract_text_from_bytes(pdf, None).expect("quick extract bytes");
    assert_eq!(quick_path, quick_bytes);

    let _ = std::fs::remove_file(path);
}

#[test]
fn native_document_page_summaries_respect_page_filters() {
    let pdf = build_minimal_pdf_with_pages(3);
    let options = options_with_page_range(vec![2, 3], Some(1));

    let doc = NativePdfDocument::from_bytes(pdf, Some(options)).expect("doc from bytes");
    let summaries = doc.page_summaries().expect("page summaries");

    assert_eq!(
        summaries
            .next()
            .expect("first page")
            .expect("page summary")
            .page_number,
        2
    );
    assert!(summaries.next().expect("cursor end").is_none());
}

#[test]
fn native_document_extract_layout_pages_contains_text_lines_and_chars() {
    let pdf = build_single_page_text_pdf("Hello");
    let doc = NativePdfDocument::from_bytes(pdf, None).expect("doc from bytes");

    let pages = doc.extract_layout_pages().expect("layout pages");

    assert_eq!(pages.len(), 1);
    assert!(pages[0].text.contains("Hello"));
    assert!(!pages[0].text_boxes.is_empty());
    assert!(!pages[0].text_boxes[0].lines.is_empty());
    assert!(pages[0].text_boxes[0].lines[0].text.contains("Hello"));
    assert!(!pages[0].text_boxes[0].lines[0].chars.is_empty());
}

#[test]
fn native_document_extract_raw_document_preserves_page_and_character_details() {
    let pdf = build_single_page_text_pdf("Hello");
    let doc = NativePdfDocument::from_bytes(pdf, None).expect("doc from bytes");

    let raw = doc.extract_raw_document().expect("raw document");

    assert_eq!(raw.declared_page_count, 1);
    assert_eq!(raw.page_count, 1);
    assert_eq!(raw.pages[0].page_index, 0);
    assert_eq!(raw.pages[0].page_number, 1);
    assert_eq!(raw.pages[0].object_id, 3);
    assert_eq!(raw.pages[0].boxes.media, Some(vec![0.0, 0.0, 200.0, 200.0]));
    assert!(raw.pages[0].text.contains("Hello"));
    let character = &raw.pages[0].text_boxes[0].lines[0].characters[0];
    assert_eq!(character.text, "H");
    assert_eq!(character.matrix.len(), 6);
    assert!(character.advance > 0.0);
}

#[test]
fn native_document_extract_raw_page_returns_only_the_requested_page() {
    let pdf = build_minimal_pdf_with_pages(3);
    let doc = NativePdfDocument::from_bytes(pdf, None).expect("doc from bytes");

    let page = doc.extract_raw_page(2).expect("raw page 2");

    assert_eq!(page.page_index, 1);
    assert_eq!(page.page_number, 2);
}

#[test]
fn native_document_metadata_preserves_info_and_derives_pdf_flags() {
    let pdf = build_metadata_pdf();
    let expected_size = pdf.len() as u64;
    let doc = NativePdfDocument::from_bytes(pdf, None).expect("doc from bytes");

    let metadata = doc.metadata().expect("document metadata");
    let info = metadata
        .document_info
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect::<std::collections::BTreeMap<_, _>>();

    assert_eq!(
        info.get("Title").map(String::as_str),
        Some("Example Document")
    );
    assert_eq!(
        info.get("CustomField").map(String::as_str),
        Some("custom value")
    );
    assert_eq!(metadata.title.as_deref(), Some("Example Document"));
    assert_eq!(metadata.author.as_deref(), Some("Example Author"));
    assert_eq!(
        metadata.creation_date_raw.as_deref(),
        Some("D:20260704114317+03'00'")
    );
    assert_eq!(
        metadata.creation_date_iso.as_deref(),
        Some("2026-07-04T11:43:17+03:00")
    );
    assert_eq!(metadata.version.header.as_deref(), Some("1.4"));
    assert_eq!(metadata.version.catalog.as_deref(), Some("1.7"));
    assert_eq!(metadata.version.effective.as_deref(), Some("1.7"));
    assert_eq!(metadata.file_size_bytes, expected_size);
    assert_eq!(metadata.page_count, 1);
    assert!(!metadata.encrypted);
    assert!(metadata.permissions.printable);
    assert!(metadata.permissions.modifiable);
    assert!(metadata.permissions.extractable);
    assert!(!metadata.linearized);
    assert!(metadata.tagged);
    assert!(metadata.user_properties);
    assert!(!metadata.suspects);
    assert_eq!(metadata.form, "acroform");
    assert!(metadata.has_javascript);
    assert!(metadata.has_metadata_stream);
    assert_eq!(
        metadata.xmp_metadata.as_deref(),
        Some("<x:xmpmeta>fixture</x:xmpmeta>")
    );
}

#[test]
fn bolivar_version_matches_the_crate_version() {
    assert_eq!(bolivar_uniffi::bolivar_version(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn native_document_extract_layout_pages_with_custom_laparams() {
    let pdf = build_single_page_multiline_text_pdf("Hello", "World");
    let options = ExtractOptions {
        password: None,
        page_numbers: None,
        max_pages: None,
        caching: Some(true),
        bidi: None,
        layout_params: Some(LayoutParams {
            line_overlap: Some(0.5),
            char_margin: Some(3.0),
            line_margin: Some(0.5),
            word_margin: Some(0.1),
            boxes_flow: Some(0.5),
            detect_vertical: Some(true),
            all_texts: Some(false),
        }),
    };

    let doc = NativePdfDocument::from_bytes(pdf, Some(options)).expect("doc with layout params");
    let pages = doc.extract_layout_pages().expect("layout pages");

    assert_eq!(pages.len(), 1);
    assert!(pages[0].text.contains("Hello"));
    assert!(pages[0].text.contains("World"));
}

#[test]
fn native_document_extract_tables_rich_metadata_and_filters() {
    let fixture_path = table_fixture_path();
    let fixture_bytes = std::fs::read(&fixture_path).expect("read table fixture");

    let all_doc = NativePdfDocument::from_bytes(fixture_bytes.clone(), None).expect("all doc");
    let all_cursor = all_doc.tables(None).expect("all tables cursor");
    let mut all_tables = Vec::new();
    while let Some(table) = all_cursor.next().expect("next table") {
        all_tables.push(table);
    }

    assert!(!all_tables.is_empty());
    let table = &all_tables[0];
    assert!(table.bbox.x1 > table.bbox.x0);
    assert!(table.bbox.y1 > table.bbox.y0);
    assert!(table.row_count > 0);
    assert!(table.column_count > 0);
    assert!(!table.cells.is_empty());

    for cell in &table.cells {
        assert!(cell.row_index < table.row_count);
        assert!(cell.column_index < table.column_count);
        assert!(cell.row_span >= 1);
        assert!(cell.column_span >= 1);
        assert!(cell.row_index + cell.row_span <= table.row_count);
        assert!(cell.column_index + cell.column_span <= table.column_count);
        assert!(cell.bbox.x1 >= cell.bbox.x0);
        assert!(cell.bbox.y1 >= cell.bbox.y0);
    }

    let filtered_doc = NativePdfDocument::from_bytes(
        fixture_bytes,
        Some(options_with_page_range(vec![1], Some(1))),
    )
    .expect("filtered doc");
    let filtered_cursor = filtered_doc.tables(None).expect("filtered tables cursor");
    let mut filtered_tables = Vec::new();
    while let Some(table) = filtered_cursor.next().expect("next filtered table") {
        filtered_tables.push(table);
    }
    for filtered in &filtered_tables {
        assert_eq!(filtered.page_number, 1);
    }
    assert!(filtered_tables.len() <= all_tables.len());
}

#[test]
fn native_table_cursor_owns_the_document_and_reports_cancellation() {
    let pdf = build_minimal_pdf_with_pages(2);
    let cursor = {
        let document = NativePdfDocument::from_bytes(pdf, None).expect("document");
        document.tables(None).expect("table cursor")
    };

    cursor.cancel();
    let error = cursor.next().expect_err("cancelled cursor");
    assert!(matches!(error, BolivarError::Cancelled));
    assert!(cursor.next().expect("terminal cursor").is_none());
}

#[test]
fn native_page_table_rows_cursor_yields_one_page_at_a_time() {
    let pdf = build_minimal_pdf_with_pages(2);
    let document = NativePdfDocument::from_bytes(pdf, None).expect("document");
    let cursor = document.table_rows(None).expect("rows cursor");

    let first = cursor.next().expect("first page").expect("page one");
    let second = cursor.next().expect("second page").expect("page two");
    assert_eq!((first.page_number, second.page_number), (1, 2));
    assert!(cursor.next().expect("end of cursor").is_none());
}

#[test]
fn native_document_from_path_reports_not_found_distinctly() {
    let err = NativePdfDocument::from_path("/definitely/missing/file.pdf".to_string(), None)
        .expect_err("missing path should fail");
    assert!(matches!(err, BolivarError::IoNotFound));
}

#[test]
fn native_document_from_path_rejects_invalid_path_inputs() {
    let err = NativePdfDocument::from_path(String::new(), None).expect_err("empty path");
    assert!(matches!(err, BolivarError::InvalidPath));

    let err = NativePdfDocument::from_path("content://example/document/1".to_string(), None)
        .expect_err("uri-like path should fail");
    assert!(matches!(err, BolivarError::InvalidPath));
}

#[test]
fn native_document_rejects_zero_page_number() {
    let pdf = build_minimal_pdf_with_pages(2);
    let options = options_with_page_range(vec![0], None);
    let err =
        NativePdfDocument::from_bytes(pdf, Some(options)).expect_err("page numbers are 1-based");
    assert!(matches!(err, BolivarError::InvalidArgument));
}
