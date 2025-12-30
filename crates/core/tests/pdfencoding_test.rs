//! 100% port of pdfminer.six test_pdfencoding.py
//!
//! Tests PDFCIDFont encoding detection and CMap selection.

#![allow(non_snake_case)]

use bolivar_core::pdffont::PDFCIDFont;
use bolivar_core::pdftypes::{PDFObject, PDFStream};
use std::collections::HashMap;

/// Helper to create a PDFStream with CMapName as Name (PSLiteral equivalent)
fn make_stream_with_name(cmap_name: &str) -> PDFObject {
    let mut attrs = HashMap::new();
    attrs.insert(
        "CMapName".to_string(),
        PDFObject::Name(cmap_name.to_string()),
    );
    PDFObject::Stream(Box::new(PDFStream::new(attrs, vec![])))
}

/// Helper to create a PDFStream with CMapName as String (byte string)
fn make_stream_with_string(cmap_name: &str) -> PDFObject {
    let mut attrs = HashMap::new();
    attrs.insert(
        "CMapName".to_string(),
        PDFObject::String(cmap_name.as_bytes().to_vec()),
    );
    PDFObject::Stream(Box::new(PDFStream::new(attrs, vec![])))
}

/// Helper to create font spec with Encoding as Name (PSLiteral)
fn make_spec_name(encoding_name: &str) -> HashMap<String, PDFObject> {
    let mut spec = HashMap::new();
    spec.insert(
        "Encoding".to_string(),
        PDFObject::Name(encoding_name.to_string()),
    );
    spec
}

/// Helper to create font spec with Encoding as PDFStream with Name CMapName
fn make_spec_stream_name(cmap_name: &str) -> HashMap<String, PDFObject> {
    let mut spec = HashMap::new();
    spec.insert("Encoding".to_string(), make_stream_with_name(cmap_name));
    spec
}

/// Helper to create font spec with Encoding as PDFStream with String CMapName
fn make_spec_stream_string(cmap_name: &str) -> HashMap<String, PDFObject> {
    let mut spec = HashMap::new();
    spec.insert("Encoding".to_string(), make_stream_with_string(cmap_name));
    spec
}

// === OneByteIdentity CMap tests (IdentityCMapByte) ===

#[test]
fn test_cmapname_onebyteidentityV() {
    let spec = make_spec_stream_name("OneByteIdentityV");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap_byte(),
        "Expected IdentityCMapByte for OneByteIdentityV"
    );
}

#[test]
fn test_cmapname_onebyteidentityH() {
    let spec = make_spec_stream_name("OneByteIdentityH");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap_byte(),
        "Expected IdentityCMapByte for OneByteIdentityH"
    );
}

// === Named CMap tests (CMap) ===

#[test]
fn test_cmapname_V() {
    let spec = make_spec_stream_name("V");
    let font = PDFCIDFont::new(&spec, None);
    assert!(font.cmap.is_cmap(), "Expected CMap for V");
}

#[test]
fn test_cmapname_H() {
    let spec = make_spec_stream_name("H");
    let font = PDFCIDFont::new(&spec, None);
    assert!(font.cmap.is_cmap(), "Expected CMap for H");
}

// === Identity-H/V as Name (PSLiteral) ===

#[test]
fn test_encoding_identityH() {
    let spec = make_spec_name("Identity-H");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for Identity-H"
    );
}

#[test]
fn test_encoding_identityV() {
    let spec = make_spec_name("Identity-V");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for Identity-V"
    );
}

// === Identity-H/V in stream with Name CMapName ===

#[test]
fn test_encoding_identityH_as_PSLiteral_stream() {
    let spec = make_spec_stream_name("Identity-H");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for Identity-H in stream"
    );
}

#[test]
fn test_encoding_identityV_as_PSLiteral_stream() {
    let spec = make_spec_stream_name("Identity-V");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for Identity-V in stream"
    );
}

// === Identity-H/V in stream with String CMapName ===

#[test]
fn test_encoding_identityH_as_stream() {
    let spec = make_spec_stream_string("Identity-H");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for Identity-H string in stream"
    );
}

#[test]
fn test_encoding_identityV_as_stream() {
    let spec = make_spec_stream_string("Identity-V");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for Identity-V string in stream"
    );
}

// === DLIdent-H/V as Name (PSLiteral) ===

#[test]
fn test_encoding_DLIdentH() {
    let spec = make_spec_name("DLIdent-H");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for DLIdent-H"
    );
}

#[test]
fn test_encoding_DLIdentV() {
    let spec = make_spec_name("DLIdent-V");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for DLIdent-V"
    );
}

// === DLIdent-H/V in stream with Name CMapName ===

#[test]
fn test_encoding_DLIdentH_as_PSLiteral_stream() {
    let spec = make_spec_stream_name("DLIdent-H");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for DLIdent-H in stream"
    );
}

#[test]
fn test_encoding_DLIdentV_as_PSLiteral_stream() {
    let spec = make_spec_stream_name("DLIdent-V");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for DLIdent-V in stream"
    );
}

// === DLIdent-H/V in stream with String CMapName ===

#[test]
fn test_encoding_DLIdentH_as_stream() {
    let spec = make_spec_stream_string("DLIdent-H");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for DLIdent-H string in stream"
    );
}

#[test]
fn test_encoding_DLIdentV_as_stream() {
    let spec = make_spec_stream_string("DLIdent-V");
    let font = PDFCIDFont::new(&spec, None);
    assert!(
        font.cmap.is_identity_cmap(),
        "Expected IdentityCMap for DLIdent-V string in stream"
    );
}

// === Empty spec test ===

#[test]
fn test_font_without_spec() {
    let spec = HashMap::new();
    let font = PDFCIDFont::new(&spec, None);
    assert!(font.cmap.is_cmap(), "Expected CMap for empty spec");
}
