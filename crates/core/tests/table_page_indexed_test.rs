use bolivar_core::document::PDFDocument;
use bolivar_core::high_level::{ExtractOptions, extract_tables_for_page_indexed};
use bolivar_core::table::{PageGeometry, TableSettings};

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
fn test_extract_tables_for_page_indexed_empty_page() {
    let pdf = build_minimal_pdf_with_pages(2);
    let doc = PDFDocument::new(pdf, "").unwrap();
    let geom = PageGeometry {
        page_bbox: (0.0, 0.0, 200.0, 200.0),
        mediabox: (0.0, 0.0, 200.0, 200.0),
        initial_doctop: 0.0,
        force_crop: false,
    };
    let settings = TableSettings::default();
    let tables =
        extract_tables_for_page_indexed(&doc, 0, &geom, ExtractOptions::default(), &settings)
            .unwrap();
    assert!(tables.is_empty());
}
