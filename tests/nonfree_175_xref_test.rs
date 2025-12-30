use bolivar::pdfdocument::PDFDocument;

#[test]
fn nonfree_175_uses_primary_xref() {
    let pdf_bytes = include_bytes!("fixtures/nonfree/175.pdf");
    let doc = PDFDocument::new(pdf_bytes, "").expect("parse 175.pdf");
    assert!(
        !doc.all_xrefs_are_fallback(),
        "expected primary xref, got fallback parsing"
    );
}

#[test]
fn nonfree_175_xref_contains_object_1() {
    let pdf_bytes = include_bytes!("fixtures/nonfree/175.pdf");
    let doc = PDFDocument::new(pdf_bytes, "").expect("parse 175.pdf");
    let objids = doc.get_objids();
    assert!(
        objids.contains(&1),
        "xref missing object 1; has {} ids",
        objids.len()
    );
}
