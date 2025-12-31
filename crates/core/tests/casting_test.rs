//! Port of pdfminer.six tests/test_casting.py

use bolivar_core::casting::{safe_float, safe_rect_list};
use bolivar_core::pdftypes::PDFObject;

// ============================================================================
// test_safe_rect_list - 9 parameterized test cases
// ============================================================================

#[test]
fn test_safe_rect_list_zeros() {
    // ([0, 0, 0, 0], (0.0, 0.0, 0.0, 0.0))
    let arr = PDFObject::Array(vec![
        PDFObject::Int(0),
        PDFObject::Int(0),
        PDFObject::Int(0),
        PDFObject::Int(0),
    ]);
    assert_eq!(safe_rect_list(&arr), Some((0.0, 0.0, 0.0, 0.0)));
}

#[test]
fn test_safe_rect_list_ints() {
    // ([1, 2, 3, 4], (1.0, 2.0, 3.0, 4.0))
    let arr = PDFObject::Array(vec![
        PDFObject::Int(1),
        PDFObject::Int(2),
        PDFObject::Int(3),
        PDFObject::Int(4),
    ]);
    assert_eq!(safe_rect_list(&arr), Some((1.0, 2.0, 3.0, 4.0)));
}

#[test]
fn test_safe_rect_list_with_null() {
    // ([0, 0, 0, None], None)
    let arr = PDFObject::Array(vec![
        PDFObject::Int(0),
        PDFObject::Int(0),
        PDFObject::Int(0),
        PDFObject::Null,
    ]);
    assert_eq!(safe_rect_list(&arr), None);
}

#[test]
fn test_safe_rect_list_with_string() {
    // Python: ([0, 0, 0, "0"], (0.0, 0.0, 0.0, 0.0))
    // String "0" can be parsed to float in pdfminer
    let arr = PDFObject::Array(vec![
        PDFObject::Int(0),
        PDFObject::Int(0),
        PDFObject::Int(0),
        PDFObject::String(b"0".to_vec()),
    ]);
    assert_eq!(safe_rect_list(&arr), Some((0.0, 0.0, 0.0, 0.0)));
}

#[test]
fn test_safe_rect_list_empty() {
    // ([], None)
    let arr = PDFObject::Array(vec![]);
    assert_eq!(safe_rect_list(&arr), None);
}

#[test]
fn test_safe_rect_list_too_few() {
    // ([0, 0, 0], None)
    let arr = PDFObject::Array(vec![
        PDFObject::Int(0),
        PDFObject::Int(0),
        PDFObject::Int(0),
    ]);
    assert_eq!(safe_rect_list(&arr), None);
}

#[test]
fn test_safe_rect_list_extra_elements() {
    // ([1, 2, 3, 4, 5], (1.0, 2.0, 3.0, 4.0)) - takes first 4
    let arr = PDFObject::Array(vec![
        PDFObject::Int(1),
        PDFObject::Int(2),
        PDFObject::Int(3),
        PDFObject::Int(4),
        PDFObject::Int(5),
    ]);
    assert_eq!(safe_rect_list(&arr), Some((1.0, 2.0, 3.0, 4.0)));
}

#[test]
fn test_safe_rect_list_null_input() {
    // (None, None)
    let obj = PDFObject::Null;
    assert_eq!(safe_rect_list(&obj), None);
}

#[test]
fn test_safe_rect_list_non_iterable() {
    // (object(), None) - non-iterable returns None
    // Use a Bool as a non-iterable type
    let obj = PDFObject::Bool(true);
    assert_eq!(safe_rect_list(&obj), None);
}

// ============================================================================
// test_safe_float - 7 parameterized test cases
// ============================================================================

#[test]
fn test_safe_float_zero_int() {
    // (0, 0.0)
    let obj = PDFObject::Int(0);
    assert_eq!(safe_float(&obj), Some(0.0));
}

#[test]
fn test_safe_float_one_int() {
    // (1, 1.0)
    let obj = PDFObject::Int(1);
    assert_eq!(safe_float(&obj), Some(1.0));
}

#[test]
fn test_safe_float_string_zero() {
    // Python: ("0", 0.0)
    let obj = PDFObject::String(b"0".to_vec());
    assert_eq!(safe_float(&obj), Some(0.0));
}

#[test]
fn test_safe_float_string_decimal() {
    // Python: ("1.5", 1.5)
    let obj = PDFObject::String(b"1.5".to_vec());
    assert_eq!(safe_float(&obj), Some(1.5));
}

#[test]
fn test_safe_float_null() {
    // (None, None)
    let obj = PDFObject::Null;
    assert_eq!(safe_float(&obj), None);
}

#[test]
fn test_safe_float_non_numeric() {
    // (object(), None) - non-numeric returns None
    let obj = PDFObject::Bool(true);
    assert_eq!(safe_float(&obj), None);
}

#[test]
fn test_safe_float_overflow() {
    // Python: (2**1024, None) - Integer too large to convert to float
    // In Rust, we test that converting a very large string number returns None
    let obj = PDFObject::String(b"1e309".to_vec()); // Larger than f64::MAX
    assert_eq!(safe_float(&obj), None);
}
