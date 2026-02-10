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
fn native_document_extract_page_summaries_with_page_filters() {
    let pdf = build_minimal_pdf_with_pages(3);
    let options = options_with_page_range(vec![2, 3], Some(1));

    let doc = NativePdfDocument::from_bytes(pdf, Some(options)).expect("doc from bytes");
    let summaries = doc.extract_page_summaries().expect("page summaries");

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].page_number, 2);
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
fn native_document_extract_layout_pages_with_custom_laparams() {
    let pdf = build_single_page_multiline_text_pdf("Hello", "World");
    let options = ExtractOptions {
        password: None,
        page_numbers: None,
        max_pages: None,
        caching: Some(true),
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
    let all_tables = all_doc.extract_tables().expect("all tables");

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
    let filtered_tables = filtered_doc.extract_tables().expect("filtered tables");
    for filtered in &filtered_tables {
        assert_eq!(filtered.page_number, 1);
    }
    assert!(filtered_tables.len() <= all_tables.len());
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
    let doc = NativePdfDocument::from_bytes(pdf, Some(options)).expect("doc construction");

    let err = doc.extract_text().expect_err("page numbers are 1-based");
    assert!(matches!(err, BolivarError::InvalidArgument));
}
