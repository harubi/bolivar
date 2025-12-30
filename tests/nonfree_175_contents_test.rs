use bolivar::pdfdocument::PDFDocument;
use bolivar::pdfpage::PDFPage;

#[test]
fn nonfree_175_content_streams_are_small() {
    let pdf_bytes = include_bytes!("fixtures/nonfree/175.pdf");
    let doc = PDFDocument::new(pdf_bytes, "").expect("parse 175.pdf");
    let mut pages = PDFPage::create_pages(&doc);

    let page1 = pages.next().expect("page 1").expect("page 1 ok");
    let page2 = pages.next().expect("page 2").expect("page 2 ok");

    for (idx, page) in [page1, page2].into_iter().enumerate() {
        assert!(
            !page.contents.is_empty(),
            "page {} has no contents",
            idx + 1
        );
        for (stream_idx, stream) in page.contents.iter().enumerate() {
            assert!(
                stream.len() < 1024,
                "page {} contents[{}] too large: {} bytes",
                idx + 1,
                stream_idx,
                stream.len()
            );
        }
    }
}
