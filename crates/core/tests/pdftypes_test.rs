//! Tests for PDF object types.
//!
//! Based on pdfminer.six pdftypes.py functionality.

use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdftypes::{PDFObjRef, PDFObject, PDFStream};
use std::collections::HashMap;

// === PDFObject type tests ===

#[test]
fn test_object_null() {
    assert!(PDFObject::Null.is_null());
    assert!(!PDFObject::Bool(true).is_null());
}

#[test]
fn test_object_bool() {
    assert_eq!(PDFObject::Bool(true).as_bool().unwrap(), true);
    assert_eq!(PDFObject::Bool(false).as_bool().unwrap(), false);
    assert!(PDFObject::Null.as_bool().is_err());
}

#[test]
fn test_object_int() {
    assert_eq!(PDFObject::Int(42).as_int().unwrap(), 42);
    assert_eq!(PDFObject::Int(-100).as_int().unwrap(), -100);
    assert!(PDFObject::Null.as_int().is_err());
}

#[test]
#[allow(clippy::approx_constant)]
fn test_object_real() {
    assert_eq!(PDFObject::Real(3.14).as_real().unwrap(), 3.14);
    assert!(PDFObject::Int(42).as_real().is_err());
}

#[test]
#[allow(clippy::approx_constant)]
fn test_object_num_coercion() {
    // Int should coerce to num (f64)
    assert_eq!(PDFObject::Int(42).as_num().unwrap(), 42.0);
    assert_eq!(PDFObject::Real(3.14).as_num().unwrap(), 3.14);
    assert!(PDFObject::Null.as_num().is_err());
}

#[test]
fn test_object_name() {
    let obj = PDFObject::Name("Type".to_string());
    assert_eq!(obj.as_name().unwrap(), "Type");
    assert!(PDFObject::Null.as_name().is_err());
}

#[test]
fn test_object_string() {
    let obj = PDFObject::String(b"Hello".to_vec());
    assert_eq!(obj.as_string().unwrap(), b"Hello");
    assert!(PDFObject::Null.as_string().is_err());
}

#[test]
fn test_object_array() {
    let arr = PDFObject::Array(vec![
        PDFObject::Int(1),
        PDFObject::Int(2),
        PDFObject::Int(3),
    ]);
    let inner = arr.as_array().unwrap();
    assert_eq!(inner.len(), 3);
    assert_eq!(inner[0].as_int().unwrap(), 1);
    assert!(PDFObject::Null.as_array().is_err());
}

#[test]
fn test_object_dict() {
    let mut m = HashMap::new();
    m.insert("Type".to_string(), PDFObject::Name("Page".to_string()));
    m.insert("Count".to_string(), PDFObject::Int(5));

    let dict = PDFObject::Dict(m);
    let inner = dict.as_dict().unwrap();
    assert_eq!(inner.get("Type").unwrap().as_name().unwrap(), "Page");
    assert_eq!(inner.get("Count").unwrap().as_int().unwrap(), 5);
    assert!(PDFObject::Null.as_dict().is_err());
}

// === PDFObjRef tests ===

#[test]
fn test_objref_creation() {
    let objref = PDFObjRef::new(10, 0);
    assert_eq!(objref.objid, 10);
    assert_eq!(objref.genno, 0);
}

#[test]
fn test_objref_in_object() {
    let obj = PDFObject::Ref(PDFObjRef::new(42, 0));
    let r = obj.as_ref().unwrap();
    assert_eq!(r.objid, 42);
}

// === PDFStream tests ===

#[test]
fn test_stream_creation() {
    let mut attrs = HashMap::new();
    attrs.insert("Length".to_string(), PDFObject::Int(100));

    let stream = PDFStream::new(attrs, b"raw data".to_vec());
    assert_eq!(stream.get_rawdata(), b"raw data");
}

#[test]
fn test_stream_attrs() {
    let mut attrs = HashMap::new();
    attrs.insert("Length".to_string(), PDFObject::Int(100));
    attrs.insert(
        "Filter".to_string(),
        PDFObject::Name("FlateDecode".to_string()),
    );

    let stream = PDFStream::new(attrs, b"compressed".to_vec());
    assert_eq!(stream.attrs.get("Length").unwrap().as_int().unwrap(), 100);
    assert_eq!(
        stream.attrs.get("Filter").unwrap().as_name().unwrap(),
        "FlateDecode"
    );
}

#[test]
fn test_decode_stream_bytes() {
    let pdf_bytes = include_bytes!("fixtures/contrib/issue-1062-filters.pdf");
    let doc = PDFDocument::new(pdf_bytes.as_slice(), "").expect("parse fixture");
    let mut decoded_any = false;

    for objid in doc.get_objids() {
        let obj = match doc.getobj(objid) {
            Ok(obj) => obj,
            Err(_) => continue,
        };
        if let Ok(stream) = obj.as_stream() {
            let decoded = doc.decode_stream(stream).expect("decode stream");
            assert!(!decoded.is_empty());
            decoded_any = true;
            break;
        }
    }

    assert!(decoded_any, "expected at least one stream to decode");
}

// === Type conversion helper tests ===

#[test]
#[allow(clippy::approx_constant)]
fn test_int_value() {
    use bolivar_core::pdftypes::int_value;
    assert_eq!(int_value(&PDFObject::Int(42)).unwrap(), 42);
    assert!(int_value(&PDFObject::Real(3.14)).is_err());
}

#[test]
#[allow(clippy::approx_constant)]
fn test_num_value() {
    use bolivar_core::pdftypes::num_value;
    assert_eq!(num_value(&PDFObject::Int(42)).unwrap(), 42.0);
    assert_eq!(num_value(&PDFObject::Real(3.14)).unwrap(), 3.14);
}

#[test]
fn test_str_value() {
    use bolivar_core::pdftypes::str_value;
    assert_eq!(
        str_value(&PDFObject::String(b"test".to_vec())).unwrap(),
        b"test"
    );
}

#[test]
fn test_list_value() {
    use bolivar_core::pdftypes::list_value;
    let arr = PDFObject::Array(vec![PDFObject::Int(1), PDFObject::Int(2)]);
    let list = list_value(&arr).unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn test_dict_value() {
    use bolivar_core::pdftypes::dict_value;
    let mut m = HashMap::new();
    m.insert("key".to_string(), PDFObject::Int(123));
    let dict = PDFObject::Dict(m);
    let d = dict_value(&dict).unwrap();
    assert!(d.contains_key("key"));
}
