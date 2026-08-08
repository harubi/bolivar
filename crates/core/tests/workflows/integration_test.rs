//! Integration tests for bolivar.
//!
//! Tests that verify cross-module functionality and
//! realistic usage patterns.

use bolivar_core::cmapdb::{CMap, CMapBase, IdentityCMap, UnicodeMap};
use bolivar_core::encodingdb::name2unicode;
use bolivar_core::pdfparser::{PDFContentParser, PDFParser};
use bolivar_core::psparser::Keyword;

// === Text extraction flow ===

#[test]
fn test_parse_and_decode_text() {
    // Parse a simple content stream with Tj operator
    let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
    let ops = PDFContentParser::parse(content).unwrap();

    // Find the Tj operation
    let tj = ops.iter().find(|op| op.operator == Keyword::Tj).unwrap();
    let text_bytes = tj.operands[0].as_string().unwrap();

    assert_eq!(text_bytes, b"Hello");
}

#[test]
fn test_cmap_with_unicode_map() {
    // Simulate CID font decoding flow:
    // bytes → CMap → CIDs → UnicodeMap → text

    // Setup CMap (code → CID)
    let mut cmap = CMap::new();
    cmap.add_code2cid(&[0x01], 100);
    cmap.add_code2cid(&[0x02], 101);
    cmap.add_code2cid(&[0x03], 102);

    // Setup Unicode map (CID → Unicode)
    let mut umap = UnicodeMap::new();
    umap.add_cid2unichr(100, "A".to_string());
    umap.add_cid2unichr(101, "B".to_string());
    umap.add_cid2unichr(102, "C".to_string());

    // Decode bytes → CIDs → Unicode
    let cids: Vec<u32> = cmap.decode(&[0x01, 0x02, 0x03]).collect();
    let text: String = cids
        .iter()
        .filter_map(|&cid| umap.get_unichr(cid))
        .collect();

    assert_eq!(text, "ABC");
}

#[test]
fn test_identity_cmap_flow() {
    // Identity CMap interprets 2-byte sequences as Unicode codepoints
    let cmap = IdentityCMap::new(false);

    // "Hi" in UTF-16BE
    let data = [0x00, 0x48, 0x00, 0x69];
    let cids: Vec<u32> = cmap.decode(&data).collect();

    assert_eq!(cids, vec![0x0048, 0x0069]); // H, i

    // Convert CIDs to text (identity = CID is the Unicode codepoint)
    let text: String = cids.iter().filter_map(|&cid| char::from_u32(cid)).collect();

    assert_eq!(text, "Hi");
}

#[test]
fn test_glyph_name_fallback() {
    // Test the encoding fallback chain:
    // 1. Standard AGL name
    // 2. uni prefix
    // 3. u prefix

    // Standard name
    assert_eq!(name2unicode("A").unwrap(), "A");
    assert_eq!(name2unicode("space").unwrap(), " ");
    assert_eq!(name2unicode("Euro").unwrap(), "€");

    // uni prefix (4-digit hex)
    assert_eq!(name2unicode("uni0041").unwrap(), "A");
    assert_eq!(name2unicode("uni20AC").unwrap(), "€");

    // u prefix (variable length hex)
    assert_eq!(name2unicode("u0041").unwrap(), "A");
    assert_eq!(name2unicode("u20AC").unwrap(), "€");
}

#[test]
fn test_parse_page_dict() {
    // Parse a realistic page dictionary
    let page = b"<< /Type /Page /MediaBox [ 0 0 612 792 ] /Contents 5 0 R /Resources << /Font << /F1 6 0 R >> >> >>";
    let mut parser = PDFParser::new(page);
    let obj = parser.parse_object().unwrap();

    let dict = obj.as_dict().unwrap();

    // Check page type
    assert_eq!(dict.get("Type").unwrap().as_name().unwrap(), "Page");

    // Check media box (US Letter)
    let media_box = dict.get("MediaBox").unwrap().as_array().unwrap();
    assert_eq!(media_box[2].as_int().unwrap(), 612);
    assert_eq!(media_box[3].as_int().unwrap(), 792);

    // Check contents reference
    let contents = dict.get("Contents").unwrap().as_ref().unwrap();
    assert_eq!(contents.objid, 5);

    // Check nested resources
    let resources = dict.get("Resources").unwrap().as_dict().unwrap();
    let font = resources.get("Font").unwrap().as_dict().unwrap();
    let f1 = font.get("F1").unwrap().as_ref().unwrap();
    assert_eq!(f1.objid, 6);
}

#[test]
fn test_content_stream_text_extraction() {
    // A more realistic content stream with multiple text operators
    let content = b"BT
        /F1 12 Tf
        1 0 0 1 72 720 Tm
        (Hello ) Tj
        (World) Tj
        10 TL
        T*
        (New line) Tj
        ET";

    let ops = PDFContentParser::parse(content).unwrap();

    // Extract all text from Tj operators
    let text_ops: Vec<_> = ops.iter().filter(|op| op.operator == Keyword::Tj).collect();

    assert_eq!(text_ops.len(), 3);
    assert_eq!(text_ops[0].operands[0].as_string().unwrap(), b"Hello ");
    assert_eq!(text_ops[1].operands[0].as_string().unwrap(), b"World");
    assert_eq!(text_ops[2].operands[0].as_string().unwrap(), b"New line");
}

#[test]
fn test_content_stream_tj_array() {
    // TJ operator with kerning adjustments
    let content = b"BT /F1 12 Tf [ (T) -80 (e) -15 (st) ] TJ ET";
    let ops = PDFContentParser::parse(content).unwrap();

    let tj_op = ops.iter().find(|op| op.operator == Keyword::TJ).unwrap();
    let arr = tj_op.operands[0].as_array().unwrap();

    // Should have: string, number, string, number, string
    assert_eq!(arr.len(), 5);
    assert_eq!(arr[0].as_string().unwrap(), b"T");
    assert_eq!(arr[1].as_int().unwrap(), -80);
    assert_eq!(arr[2].as_string().unwrap(), b"e");
    assert_eq!(arr[3].as_int().unwrap(), -15);
    assert_eq!(arr[4].as_string().unwrap(), b"st");
}

#[test]
fn test_cmap_range_mapping() {
    // Test CMap range mapping for CJK-style encoding
    let mut cmap = CMap::new();

    // Map single-byte codes 0x20-0x7F to CIDs 1-96
    cmap.add_cid_range(&[0x20], &[0x7F], 1);

    // Decode some ASCII characters
    let cids: Vec<u32> = cmap.decode(b"ABC").collect();
    // A=0x41, B=0x42, C=0x43
    // CID = (code - 0x20) + 1 = code - 0x1F
    assert_eq!(cids, vec![0x22, 0x23, 0x24]); // 34, 35, 36
}

#[test]
fn test_unicode_map_range() {
    // Test UnicodeMap range mapping
    let mut umap = UnicodeMap::new();

    // Map CIDs 100-102 to "A", "B", "C" using UTF-16BE bytes
    // 'A' = U+0041 = [0x00, 0x41] in UTF-16BE
    umap.add_bf_range(100, 102, vec![0x00, 0x41]);

    assert_eq!(umap.get_unichr(100), Some("A".to_string()));
    assert_eq!(umap.get_unichr(101), Some("B".to_string()));
    assert_eq!(umap.get_unichr(102), Some("C".to_string()));
    assert_eq!(umap.get_unichr(103), None); // Out of range
}

#[test]
fn test_composite_glyph_name() {
    // Test composite glyph name parsing
    // Format: component1_component2.variant

    // Underscore separates components
    assert_eq!(name2unicode("A_B_C").unwrap(), "ABC");

    // Dot introduces variant (ignored for Unicode)
    assert_eq!(name2unicode("A.swash").unwrap(), "A");

    // Combined
    assert_eq!(name2unicode("A_B.alt").unwrap(), "AB");
}

// === Error handling ===

#[test]
fn test_unknown_glyph_name() {
    // Unknown glyph names should return an error
    let result = name2unicode("unknownglyph123");
    assert!(result.is_err());
}

#[test]
fn test_invalid_indirect_ref() {
    // Numbers without R are just numbers
    let mut parser = PDFParser::new(b"10 0");
    let obj1 = parser.parse_object().unwrap();
    let obj2 = parser.parse_object().unwrap();

    assert_eq!(obj1.as_int().unwrap(), 10);
    assert_eq!(obj2.as_int().unwrap(), 0);
}

// === Color space ===

#[test]
fn test_colorspace_lookup() {
    use bolivar_core::pdfcolor::PREDEFINED_COLORSPACE;

    // Standard device color spaces
    assert_eq!(
        PREDEFINED_COLORSPACE.get("DeviceGray").unwrap().ncomponents,
        1
    );
    assert_eq!(
        PREDEFINED_COLORSPACE.get("DeviceRGB").unwrap().ncomponents,
        3
    );
    assert_eq!(
        PREDEFINED_COLORSPACE.get("DeviceCMYK").unwrap().ncomponents,
        4
    );

    // CIE-based
    assert_eq!(PREDEFINED_COLORSPACE.get("CalGray").unwrap().ncomponents, 1);
    assert_eq!(PREDEFINED_COLORSPACE.get("CalRGB").unwrap().ncomponents, 3);
}
