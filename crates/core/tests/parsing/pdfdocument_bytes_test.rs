use bolivar_core::pdfdocument::PDFDocument;

#[test]
fn pdfdocument_new_from_bytes_parses() {
    let fixture = format!("{}/tests/fixtures/simple1.pdf", env!("CARGO_MANIFEST_DIR"));
    let data = std::fs::read(fixture).unwrap();
    let bytes = bytes::Bytes::from(data);
    let doc = PDFDocument::new_from_bytes(bytes, "").unwrap();
    assert!(!doc.get_objids().is_empty());
}
