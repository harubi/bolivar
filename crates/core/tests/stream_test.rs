use bolivar_core::high_level::{ExtractOptions, extract_pages_stream};

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
fn test_stream_orders_pages() {
    let pdf_data = build_minimal_pdf_with_pages(3);

    let options = ExtractOptions::default();
    let pages: Vec<_> = extract_pages_stream(&pdf_data, Some(options))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let page_ids: Vec<i32> = pages.iter().map(|page| page.pageid).collect();
    assert_eq!(page_ids, vec![1, 2, 3]);
}
