use bolivar_core::api::ExtractOptions;
use bolivar_core::api::pipeline;
use bolivar_core::api::stream::{
    extract_tables_stream_from_doc, extract_tables_stream_from_doc_with_geometries,
};
use bolivar_core::document::PDFDocument;
use bolivar_core::high_level::extract_tables_with_document_geometries;
use bolivar_core::table::{PageGeometry, TableSettings};
use std::sync::Arc;

fn build_minimal_pdf_with_pages(page_count: usize) -> Vec<u8> {
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

    let kids: String = (0..page_count)
        .map(|i| format!("{} 0 R", 3 + i))
        .collect::<Vec<_>>()
        .join(" ");
    push_obj(
        &mut out,
        format!(
            "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
            kids, page_count
        ),
        &mut offsets,
    );

    for i in 0..page_count {
        let page_id = 3 + i;
        let contents_id = 3 + page_count + i;
        push_obj(
            &mut out,
            format!(
                "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents {} 0 R >>\nendobj\n",
                page_id, contents_id
            ),
            &mut offsets,
        );
    }

    for i in 0..page_count {
        let contents_id = 3 + page_count + i;
        push_obj(
            &mut out,
            format!(
                "{} 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n",
                contents_id
            ),
            &mut offsets,
        );
    }

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

#[test]
fn test_table_stream_orders_pages() {
    let pdf = build_minimal_pdf_with_pages(3);
    let doc = PDFDocument::new(pdf, "").unwrap();
    let stream = extract_tables_stream_from_doc(doc.into(), ExtractOptions::default()).unwrap();
    let results: Vec<_> = stream.collect::<Result<Vec<_>, _>>().unwrap();
    let page_ids: Vec<usize> = results.iter().map(|(idx, _)| *idx).collect();
    assert_eq!(page_ids, vec![0, 1, 2]);
}

#[test]
fn planner_rejects_geometry_count_mismatch_against_selected_pages() {
    let err = pipeline::validate_geometry_count(&[0, 2], 3).unwrap_err();
    assert!(err.to_string().contains("geometry count mismatch"));
}

#[test]
fn batch_and_stream_tables_match_for_selected_pages() {
    let pdf = build_minimal_pdf_with_pages(2);
    let doc = Arc::new(PDFDocument::new(pdf, "").unwrap());
    let options = ExtractOptions {
        page_numbers: Some(vec![0]),
        ..ExtractOptions::default()
    };
    let settings = TableSettings::default();
    let geometry = PageGeometry {
        page_bbox: (0.0, 0.0, 200.0, 200.0),
        mediabox: (0.0, 0.0, 200.0, 200.0),
        initial_doctop: 0.0,
        force_crop: false,
    };

    let batch = extract_tables_with_document_geometries(
        Arc::clone(&doc),
        options.clone(),
        &settings,
        std::slice::from_ref(&geometry),
    )
    .unwrap();
    let stream = extract_tables_stream_from_doc_with_geometries(
        Arc::clone(&doc),
        options,
        settings,
        vec![geometry],
    )
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap();

    assert_eq!(batch.len(), stream.len());
}
