use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bolivar_uniffi::{
    BolivarError, extract_layout_pages_from_bytes, extract_layout_pages_from_bytes_async,
    extract_layout_pages_from_path, extract_page_summaries_from_bytes,
    extract_page_summaries_from_bytes_async, extract_page_summaries_from_path,
    extract_tables_from_bytes, extract_tables_from_bytes_async, extract_tables_from_path,
    extract_tables_from_path_async, extract_text_from_bytes, extract_text_from_bytes_async,
    extract_text_from_path, extract_text_from_path_async,
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

#[test]
fn extract_text_path_matches_bytes_for_same_pdf() {
    let pdf = build_minimal_pdf_with_pages(1);
    let path = write_temp_pdf(&pdf);

    let text_from_bytes = extract_text_from_bytes(pdf, None).expect("extract from bytes");
    let text_from_path = extract_text_from_path(path.to_string_lossy().to_string(), None)
        .expect("extract from path");

    assert_eq!(text_from_bytes, text_from_path);

    let _ = std::fs::remove_file(path);
}

#[test]
fn extract_text_path_async_matches_sync() {
    let pdf = build_minimal_pdf_with_pages(1);
    let path = write_temp_pdf(&pdf);

    let sync = extract_text_from_path(path.to_string_lossy().to_string(), None).expect("sync");
    let async_out = pollster::block_on(extract_text_from_path_async(
        path.to_string_lossy().to_string(),
        None,
    ))
    .expect("async");

    assert_eq!(sync, async_out);

    let _ = std::fs::remove_file(path);
}

#[test]
fn extract_text_bytes_async_matches_sync() {
    let pdf = build_minimal_pdf_with_pages(2);

    let sync = extract_text_from_bytes(pdf.clone(), None).expect("sync");
    let async_out = pollster::block_on(extract_text_from_bytes_async(pdf, None)).expect("async");

    assert_eq!(sync, async_out);
}

#[test]
fn extract_page_summaries_path_matches_bytes() {
    let pdf = build_minimal_pdf_with_pages(2);
    let path = write_temp_pdf(&pdf);

    let from_bytes =
        extract_page_summaries_from_bytes(pdf.clone(), None).expect("summaries from bytes");
    let from_path = extract_page_summaries_from_path(path.to_string_lossy().to_string(), None)
        .expect("summaries from path");

    assert_eq!(from_bytes.len(), 2);
    assert_eq!(from_bytes, from_path);

    let _ = std::fs::remove_file(path);
}

#[test]
fn extract_page_summaries_async_matches_sync() {
    let pdf = build_minimal_pdf_with_pages(1);
    let sync = extract_page_summaries_from_bytes(pdf.clone(), None).expect("sync");
    let async_out =
        pollster::block_on(extract_page_summaries_from_bytes_async(pdf, None)).expect("async");
    assert_eq!(sync, async_out);
}

#[test]
fn extract_layout_pages_contains_text_lines_and_chars() {
    let pdf = build_single_page_text_pdf("Hello");
    let path = write_temp_pdf(&pdf);

    let from_bytes = extract_layout_pages_from_bytes(pdf, None).expect("layout bytes");
    let from_path = extract_layout_pages_from_path(path.to_string_lossy().to_string(), None)
        .expect("layout path");
    let from_async = pollster::block_on(extract_layout_pages_from_bytes_async(
        build_single_page_text_pdf("Hello"),
        None,
    ))
    .expect("layout async");

    assert_eq!(from_bytes, from_path);
    assert_eq!(from_bytes, from_async);

    assert_eq!(from_bytes.len(), 1);
    assert!(from_bytes[0].text.contains("Hello"));
    assert!(!from_bytes[0].text_boxes.is_empty());
    assert!(!from_bytes[0].text_boxes[0].lines.is_empty());
    assert!(from_bytes[0].text_boxes[0].lines[0].text.contains("Hello"));
    assert!(!from_bytes[0].text_boxes[0].lines[0].chars.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn extract_tables_rich_metadata_sync_and_async_match() {
    let fixture_path = table_fixture_path();
    let fixture_bytes = std::fs::read(&fixture_path).expect("read table fixture");

    let from_bytes = extract_tables_from_bytes(fixture_bytes.clone(), None).expect("tables bytes");
    let from_path = extract_tables_from_path(fixture_path.to_string_lossy().to_string(), None)
        .expect("tables path");
    let from_async = pollster::block_on(extract_tables_from_bytes_async(fixture_bytes, None))
        .expect("tables async bytes");
    let from_async_path = pollster::block_on(extract_tables_from_path_async(
        fixture_path.to_string_lossy().to_string(),
        None,
    ))
    .expect("tables async path");

    assert_eq!(from_bytes, from_path);
    assert_eq!(from_bytes, from_async);
    assert_eq!(from_path, from_async_path);

    assert!(!from_bytes.is_empty());
    let table = &from_bytes[0];
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
}

#[test]
fn extract_layout_pages_multiline_preserves_line_separator() {
    let pdf = build_single_page_multiline_text_pdf("Hello", "World");
    let pages = extract_layout_pages_from_bytes(pdf, None).expect("layout pages");
    assert_eq!(pages.len(), 1);
    assert!(pages[0].text.contains("Hello"));
    assert!(pages[0].text.contains("World"));
    assert!(!pages[0].text.contains("HelloWorld"));
    assert!(!pages[0].text.contains("WorldHello"));
}

#[test]
fn extract_text_from_path_reports_not_found_distinctly() {
    let err = extract_text_from_path("/definitely/missing/file.pdf".to_string(), None)
        .expect_err("missing path should fail");
    assert!(matches!(err, BolivarError::IoNotFound));
}

#[test]
fn extract_text_from_path_rejects_invalid_path_inputs() {
    let err = extract_text_from_path(String::new(), None).expect_err("empty path should fail");
    assert!(matches!(err, BolivarError::InvalidPath));

    let err = extract_text_from_path("content://example/document/1".to_string(), None)
        .expect_err("uri-like path should fail");
    assert!(matches!(err, BolivarError::InvalidPath));
}
