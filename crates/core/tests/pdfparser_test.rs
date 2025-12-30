//! Tests for PDF parser.
//!
//! Based on pdfminer.six pdfparser.py functionality.

use bolivar_core::pdfparser::{PDFContentParser, PDFParser};

// === PDFParser tests ===

#[test]
fn test_parse_simple_dict() {
    let data = b"<< /Type /Page /Count 5 >>";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let dict = obj.as_dict().unwrap();
    assert_eq!(dict.get("Type").unwrap().as_name().unwrap(), "Page");
    assert_eq!(dict.get("Count").unwrap().as_int().unwrap(), 5);
}

#[test]
fn test_parse_nested_dict() {
    let data = b"<< /Resources << /Font << /F1 1 0 R >> >> >>";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let dict = obj.as_dict().unwrap();
    let resources = dict.get("Resources").unwrap().as_dict().unwrap();
    let font = resources.get("Font").unwrap().as_dict().unwrap();
    let f1 = font.get("F1").unwrap().as_ref().unwrap();
    assert_eq!(f1.objid, 1);
    assert_eq!(f1.genno, 0);
}

#[test]
fn test_parse_array() {
    let data = b"[ 1 2 3 /Name (string) ]";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let arr = obj.as_array().unwrap();
    assert_eq!(arr.len(), 5);
    assert_eq!(arr[0].as_int().unwrap(), 1);
    assert_eq!(arr[1].as_int().unwrap(), 2);
    assert_eq!(arr[2].as_int().unwrap(), 3);
    assert_eq!(arr[3].as_name().unwrap(), "Name");
    assert_eq!(arr[4].as_string().unwrap(), b"string");
}

#[test]
fn test_parse_indirect_ref() {
    let data = b"10 0 R";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let objref = obj.as_ref().unwrap();
    assert_eq!(objref.objid, 10);
    assert_eq!(objref.genno, 0);
}

#[test]
fn test_parse_multiple_refs() {
    let data = b"[ 1 0 R 2 0 R 3 0 R ]";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let arr = obj.as_array().unwrap();
    assert_eq!(arr.len(), 3);

    let ref1 = arr[0].as_ref().unwrap();
    assert_eq!(ref1.objid, 1);

    let ref2 = arr[1].as_ref().unwrap();
    assert_eq!(ref2.objid, 2);

    let ref3 = arr[2].as_ref().unwrap();
    assert_eq!(ref3.objid, 3);
}

#[test]
fn test_parse_null() {
    let data = b"null";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();
    assert!(obj.is_null());
}

#[test]
fn test_parse_bool() {
    let mut parser = PDFParser::new(b"true");
    assert_eq!(parser.parse_object().unwrap().as_bool().unwrap(), true);

    let mut parser = PDFParser::new(b"false");
    assert_eq!(parser.parse_object().unwrap().as_bool().unwrap(), false);
}
#[allow(clippy::approx_constant)]
#[test]
fn test_parse_numbers() {
    let data = b"[ 42 -17 3.14 -0.5 0 ]";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let arr = obj.as_array().unwrap();
    assert_eq!(arr[0].as_int().unwrap(), 42);
    assert_eq!(arr[1].as_int().unwrap(), -17);
    assert_eq!(arr[2].as_num().unwrap(), 3.14);
    assert_eq!(arr[3].as_num().unwrap(), -0.5);
    assert_eq!(arr[4].as_int().unwrap(), 0);
}

#[test]
fn test_parse_hex_string() {
    let data = b"<48656C6C6F>"; // "Hello"
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();
    assert_eq!(obj.as_string().unwrap(), b"Hello");
}

#[test]
fn test_parse_literal_string() {
    let data = b"(Hello World)";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();
    assert_eq!(obj.as_string().unwrap(), b"Hello World");
}

#[test]
fn test_parse_string_escapes() {
    let data = b"(Line1\\nLine2\\r\\n)";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();
    assert_eq!(obj.as_string().unwrap(), b"Line1\nLine2\r\n");
}

// === PDFContentParser tests ===

#[test]
fn test_content_parser_simple() {
    let data = b"BT /F1 12 Tf ET";
    let ops = PDFContentParser::parse(data).unwrap();

    assert_eq!(ops.len(), 3); // BT, Tf, ET
    assert_eq!(&ops[0].operator, b"BT");
    assert_eq!(&ops[1].operator, b"Tf");
    assert_eq!(ops[1].operands.len(), 2);
    assert_eq!(&ops[2].operator, b"ET");
}

#[test]
fn test_content_parser_text() {
    let data = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
    let ops = PDFContentParser::parse(data).unwrap();

    // Find Tj operation
    let tj = ops.iter().find(|op| op.operator == b"Tj").unwrap();
    assert_eq!(tj.operands.len(), 1);
    assert_eq!(tj.operands[0].as_string().unwrap(), b"Hello");
}

#[test]
fn test_content_parser_graphics() {
    let data = b"q 1 0 0 1 100 200 cm 0.5 g Q";
    let ops = PDFContentParser::parse(data).unwrap();

    assert_eq!(ops.len(), 4); // q, cm, g, Q
    assert_eq!(&ops[0].operator, b"q");

    let cm = &ops[1];
    assert_eq!(&cm.operator, b"cm");
    assert_eq!(cm.operands.len(), 6);

    let g = &ops[2];
    assert_eq!(&g.operator, b"g");
    assert_eq!(g.operands.len(), 1);
    assert_eq!(g.operands[0].as_num().unwrap(), 0.5);
}

#[test]
fn test_content_parser_inline_image() {
    // Inline images: BI ... ID <data> EI
    let data = b"BI /W 10 /H 10 /BPC 8 ID xxxxxxxxxx EI";
    let ops = PDFContentParser::parse(data).unwrap();

    // Should have BI with dict and EI
    let bi = ops.iter().find(|op| op.operator == b"BI").unwrap();
    assert!(!bi.operands.is_empty());
}

#[test]
fn test_content_parser_array_operand() {
    let data = b"[ 0.0 1.0 ] 0 d";
    let ops = PDFContentParser::parse(data).unwrap();

    let d = &ops[0];
    assert_eq!(&d.operator, b"d");
    assert_eq!(d.operands.len(), 2);
    // First operand is array
    let arr = d.operands[0].as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

#[test]
fn test_content_parser_tj_array() {
    // TJ operator takes array of strings and positioning values
    let data = b"[ (Hello) -50 (World) ] TJ";
    let ops = PDFContentParser::parse(data).unwrap();

    let tj = &ops[0];
    assert_eq!(&tj.operator, b"TJ");
    assert_eq!(tj.operands.len(), 1);

    let arr = tj.operands[0].as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_string().unwrap(), b"Hello");
    assert_eq!(arr[1].as_int().unwrap(), -50);
    assert_eq!(arr[2].as_string().unwrap(), b"World");
}

// === Edge cases ===

#[test]
fn test_parse_empty_dict() {
    let data = b"<< >>";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let dict = obj.as_dict().unwrap();
    assert!(dict.is_empty());
}

#[test]
fn test_parse_empty_array() {
    let data = b"[ ]";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let arr = obj.as_array().unwrap();
    assert!(arr.is_empty());
}

#[test]
fn test_parse_mixed_content() {
    // A realistic page content dictionary
    let data = b"<< /Type /Page /MediaBox [ 0 0 612 792 ] /Contents 5 0 R /Resources << /Font << /F1 6 0 R >> >> >>";
    let mut parser = PDFParser::new(data);
    let obj = parser.parse_object().unwrap();

    let dict = obj.as_dict().unwrap();
    assert_eq!(dict.get("Type").unwrap().as_name().unwrap(), "Page");

    let media_box = dict.get("MediaBox").unwrap().as_array().unwrap();
    assert_eq!(media_box.len(), 4);
    assert_eq!(media_box[2].as_int().unwrap(), 612);

    let contents = dict.get("Contents").unwrap().as_ref().unwrap();
    assert_eq!(contents.objid, 5);
}
