use bolivar::pdfdocument::PDFDocument;
use bolivar::pdftypes::PDFObject;

#[test]
fn nonfree_175_object_offsets_match_expected_sizes() {
    let pdf_bytes = include_bytes!("fixtures/nonfree/175.pdf");
    let doc = PDFDocument::new(pdf_bytes, "").expect("parse 175.pdf");

    let obj1 = doc.getobj(1).expect("obj 1");
    let obj2 = doc.getobj(2).expect("obj 2");

    let len1 = match obj1 {
        PDFObject::Stream(ref s) => s.get_rawdata().len(),
        _ => panic!("obj 1 expected stream"),
    };
    let len2 = match obj2 {
        PDFObject::Stream(ref s) => s.get_rawdata().len(),
        _ => panic!("obj 2 expected stream"),
    };

    assert!(len1 > 100_000, "obj 1 should be image stream");
    assert!(len2 < 1024, "obj 2 should be content stream");
}
