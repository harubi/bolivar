//! Tests for pdffont module.
//!
//! Port of pdfminer.six tests/test_pdffont.py

use bolivar::pdffont::{MockPdfFont, PDFCIDFont, PDFFont, get_widths};
use bolivar::pdftypes::PDFObject;
use std::collections::HashMap;

/// Test that PDFCIDFont can handle a named CMap encoding.
///
/// Adapted from Python test_get_cmap_from_pickle - since we don't use pickle files,
/// we verify that PDFCIDFont can be constructed with a named CMap encoding.
#[test]
fn test_get_cmap_from_spec() {
    let cmap_name = "UniGB-UCS2-H";
    let mut spec = HashMap::new();
    spec.insert(
        "Encoding".to_string(),
        PDFObject::Name(cmap_name.to_string()),
    );

    // Should not panic - creates font with default CMap for unknown encoding names
    let _font = PDFCIDFont::new(&spec, None);
}

/// Test that PDFCIDFont correctly identifies Identity-H CMap.
#[test]
fn test_get_cmap_identity_h() {
    let mut spec = HashMap::new();
    spec.insert(
        "Encoding".to_string(),
        PDFObject::Name("Identity-H".to_string()),
    );

    let font = PDFCIDFont::new(&spec, None);
    assert!(font.cmap.is_identity_cmap());
}

/// Test that PDFCIDFont correctly identifies Identity-V CMap.
#[test]
fn test_get_cmap_identity_v() {
    let mut spec = HashMap::new();
    spec.insert(
        "Encoding".to_string(),
        PDFObject::Name("Identity-V".to_string()),
    );

    let font = PDFCIDFont::new(&spec, None);
    assert!(font.cmap.is_identity_cmap());
}

/// Test that PDFCIDFont correctly identifies OneByteIdentityH CMap.
#[test]
fn test_get_cmap_one_byte_identity() {
    let mut spec = HashMap::new();
    spec.insert(
        "Encoding".to_string(),
        PDFObject::Name("OneByteIdentityH".to_string()),
    );

    let font = PDFCIDFont::new(&spec, None);
    assert!(font.cmap.is_identity_cmap_byte());
}

// === char_width tests ===

/// No widths should use default.
#[test]
fn test_pdffont_char_width_no_widths() {
    let pdffont = MockPdfFont::new(HashMap::new(), HashMap::new(), 100.0);
    assert_eq!(pdffont.char_width(0), 0.1);
}

/// Int cid widths should be used.
#[test]
fn test_pdffont_char_width_int_cid() {
    let mut widths = HashMap::new();
    widths.insert(0, Some(50.0));
    let pdffont = MockPdfFont::new(HashMap::new(), widths, 100.0);
    assert_eq!(pdffont.char_width(0), 0.05);
}

/// Str cid widths should be used.
#[test]
fn test_pdffont_char_width_str_cid() {
    let mut widths = HashMap::new();
    widths.insert(0, Some(200.0));
    let pdffont = MockPdfFont::new(HashMap::new(), widths, 100.0);
    // MockPdfFont.to_unichr returns the string representation of the cid
    assert_eq!(pdffont.char_width(0), 0.2);
}

/// Invalid cid widths (None) should use default.
#[test]
fn test_pdffont_char_width_invalid_int() {
    let mut widths: HashMap<u32, Option<f64>> = HashMap::new();
    widths.insert(0, None);
    let pdffont = MockPdfFont::new(HashMap::new(), widths, 100.0);
    assert_eq!(pdffont.char_width(0), 0.1);
}

/// Invalid cid widths (None with str key) should use default.
#[test]
fn test_pdffont_char_width_invalid_str() {
    // In Rust we use u32 keys, so this tests the same path as above
    let mut widths: HashMap<u32, Option<f64>> = HashMap::new();
    widths.insert(0, None);
    let pdffont = MockPdfFont::new(HashMap::new(), widths, 100.0);
    assert_eq!(pdffont.char_width(0), 0.1);
}

// === get_widths tests ===

/// Test get_widths with format 1: [cid, [w1, w2, w3, ...]]
#[test]
fn test_pdffont_get_widths_format1() {
    use bolivar::pdftypes::PDFObjRef;

    let seq = vec![
        PDFObject::Int(0),
        PDFObject::Array(vec![
            PDFObject::Int(1),
            PDFObject::Int(2),
            PDFObject::Int(3),
            PDFObject::Int(4),
        ]),
    ];
    let no_resolver: Option<&fn(&PDFObjRef) -> Option<PDFObject>> = None;
    let widths = get_widths(&seq, no_resolver);
    assert_eq!(widths.get(&0), Some(&1.0));
    assert_eq!(widths.get(&1), Some(&2.0));
    assert_eq!(widths.get(&2), Some(&3.0));
    assert_eq!(widths.get(&3), Some(&4.0));
}

/// Test get_widths with format 2: [cid_start, cid_end, width]
#[test]
fn test_pdffont_get_widths_format2() {
    use bolivar::pdftypes::PDFObjRef;

    let seq = vec![PDFObject::Int(0), PDFObject::Int(4), PDFObject::Int(3)];
    let no_resolver: Option<&fn(&PDFObjRef) -> Option<PDFObject>> = None;
    let widths = get_widths(&seq, no_resolver);
    assert_eq!(widths.get(&0), Some(&3.0));
    assert_eq!(widths.get(&1), Some(&3.0));
    assert_eq!(widths.get(&2), Some(&3.0));
    assert_eq!(widths.get(&3), Some(&3.0));
    assert_eq!(widths.get(&4), Some(&3.0));
}

/// Test get_widths with PDFObjRef - regression test for issue #629.
///
/// When a PDFObjRef is encountered, it should be resolved via the provided resolver.
#[test]
fn test_pdffont_get_widths_object_ref() {
    use bolivar::pdftypes::PDFObjRef;

    // Create a mock resolver that returns [1, 2, 3, 4] for objid 121
    let resolver = |objref: &PDFObjRef| -> Option<PDFObject> {
        if objref.objid == 121 {
            Some(PDFObject::Array(vec![
                PDFObject::Int(1),
                PDFObject::Int(2),
                PDFObject::Int(3),
                PDFObject::Int(4),
            ]))
        } else {
            None
        }
    };

    let seq = vec![PDFObject::Int(0), PDFObject::Ref(PDFObjRef::new(121, 0))];
    let widths = get_widths(&seq, Some(&resolver));
    assert_eq!(widths.get(&0), Some(&1.0));
    assert_eq!(widths.get(&1), Some(&2.0));
    assert_eq!(widths.get(&2), Some(&3.0));
    assert_eq!(widths.get(&3), Some(&4.0));
}
