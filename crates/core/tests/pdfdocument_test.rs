//! Port of pdfminer.six test_pdfdocument.py
//!
//! Tests for PDFDocument functionality including:
//! - Object resolution (getobj)
//! - Document info extraction
//! - Page labels
//! - Page iteration with annotations

use bolivar_core::error::PdfError;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdfpage::PDFPage;

// Embed test fixtures at compile time (Miri-compatible)
const SIMPLE1_PDF: &[u8] = include_bytes!("fixtures/simple1.pdf");
const ENCRYPTED_NO_ID_PDF: &[u8] = include_bytes!("fixtures/encryption/encrypted_doc_no_id.pdf");
const PAGELABELS_PDF: &[u8] = include_bytes!("fixtures/contrib/pagelabels.pdf");
const ANNOTATIONS_PDF: &[u8] = include_bytes!("fixtures/contrib/issue-1082-annotations.pdf");
const IMAGE_STRUCTURE_PDF: &[u8] =
    include_bytes!("../../../references/pdfplumber/tests/pdfs/image_structure.pdf");
const OBJSTM_DICT_SEGMENT: &[u8] = b"/First 60/Filter/FlateDecode/Length 382";

fn replace_once_fixed_len(input: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    assert_eq!(
        needle.len(),
        replacement.len(),
        "replacement must preserve PDF byte length for stable xref offsets"
    );
    let pos = input
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("needle not found in fixture PDF");
    let mut out = input.to_vec();
    out[pos..pos + needle.len()].copy_from_slice(replacement);
    out
}

fn assert_objstm_lookup_fails_without_panic(pdf_bytes: &[u8]) {
    let doc = PDFDocument::new(pdf_bytes, "").expect("Failed to parse mutated PDF");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.getobj(11)));
    assert!(
        outcome.is_ok(),
        "Expected getobj(11) to return an error, not panic"
    );
    let result = outcome.expect("panic already checked");
    assert!(
        matches!(result, Err(PdfError::ObjectNotFound(11))),
        "Expected ObjectNotFound(11), got {:?}",
        result
    );
}

/// Test that requesting object ID 0 raises PDFObjectNotFound.
/// Port of: test_get_zero_objid_raises_pdfobjectnotfound
#[test]
fn test_get_zero_objid_raises_pdfobjectnotfound() {
    let doc = PDFDocument::new(SIMPLE1_PDF, "").expect("Failed to parse PDF");

    let result = doc.getobj(0);
    assert!(
        matches!(result, Err(PdfError::ObjectNotFound(0))),
        "Expected PDFObjectNotFound for objid 0, got {:?}",
        result
    );
}

/// Test that encrypted documents without /ID can still be parsed.
/// Regression test for https://github.com/pdfminer/pdfminer.six/issues/594
/// Port of: test_encrypted_no_id
///
/// This document is encrypted with an empty password and no /ID array.
/// Verifies:
/// 1. The document parses without error (main point of the regression test)
/// 2. Info dict exists and contains Producer key
/// 3. The Producer value is correctly decrypted to "European Patent Office"
#[test]
fn test_encrypted_no_id() {
    let doc = PDFDocument::new(ENCRYPTED_NO_ID_PDF, "").expect("Failed to parse PDF");

    // Check that info exists and contains Producer
    let info = doc.info();
    assert!(!info.is_empty(), "Document info should not be empty");

    let first_info = &info[0];
    assert!(
        first_info.contains_key("Producer"),
        "Expected Producer key in info"
    );

    // Verify the Producer value is correctly decrypted
    let producer = first_info.get("Producer").unwrap();
    let producer_bytes = producer.as_string().expect("Producer should be a string");
    let producer_str = String::from_utf8_lossy(producer_bytes);
    assert_eq!(
        producer_str, "European Patent Office",
        "Producer should be decrypted to 'European Patent Office'"
    );
}

/// Test that page labels are correctly extracted.
/// Port of: test_page_labels
#[test]
fn test_page_labels() {
    let doc = PDFDocument::new(PAGELABELS_PDF, "").expect("Failed to parse PDF");

    // Get total pages from catalog
    let catalog = doc.catalog();
    let pages_ref = catalog.get("Pages").expect("Expected Pages in catalog");
    let pages_obj = doc.resolve(pages_ref).expect("Failed to resolve Pages");
    let pages_dict = pages_obj.as_dict().expect("Pages should be a dict");
    let count = pages_dict
        .get("Count")
        .expect("Expected Count in Pages")
        .as_int()
        .expect("Count should be int") as usize;

    // Get page labels
    let labels: Vec<String> = doc
        .get_page_labels()
        .expect("Expected page labels")
        .take(count)
        .collect();

    assert_eq!(labels, vec!["iii", "iv", "1", "2", "1"]);
}

/// Test that documents without page labels raise PDFNoPageLabels.
/// Port of: test_no_page_labels
#[test]
fn test_no_page_labels() {
    let doc = PDFDocument::new(SIMPLE1_PDF, "").expect("Failed to parse PDF");

    let result = doc.get_page_labels();
    assert!(result.is_err(), "Expected PDFNoPageLabels error");
    match result {
        Err(PdfError::NoPageLabels) => (),
        Err(e) => panic!("Expected PDFNoPageLabels, got {:?}", e),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

/// Test that pages with annotations can be iterated without crashing.
/// Port of: test_annotations
#[test]
fn test_annotations() {
    let doc = PDFDocument::new(ANNOTATIONS_PDF, "").expect("Failed to parse PDF");

    // Iterate through all pages - the test just verifies no crash occurs
    let mut page_count = 0;
    for page in PDFPage::create_pages(&doc) {
        let _page = page.expect("Failed to get page");
        page_count += 1;
    }

    // The document should have at least one page
    assert!(page_count > 0, "Expected at least one page");
}

/// Ensure objects stored in object streams can be resolved via xref streams.
#[test]
fn test_getobj_struct_tree_root_from_objstm() {
    let doc = PDFDocument::new(IMAGE_STRUCTURE_PDF, "").expect("Failed to parse PDF");

    let obj = doc
        .getobj(11)
        .expect("Expected StructTreeRoot object to be resolvable");
    let dict = obj.as_dict().expect("StructTreeRoot should be a dict");
    let typ = dict.get("Type").expect("Type missing in StructTreeRoot");
    let name = typ.as_name().expect("Type should be a name");
    assert_eq!(name, "StructTreeRoot");
}

#[test]
fn test_getobj_objstm_negative_first_does_not_panic() {
    let pdf = replace_once_fixed_len(
        IMAGE_STRUCTURE_PDF,
        OBJSTM_DICT_SEGMENT,
        b"/First -1/Filter/FlateDecode/Length 382",
    );
    assert_objstm_lookup_fails_without_panic(&pdf);
}

#[test]
fn test_getobj_objstm_first_beyond_data_len_does_not_panic() {
    let pdf = replace_once_fixed_len(
        IMAGE_STRUCTURE_PDF,
        OBJSTM_DICT_SEGMENT,
        b"/First 999/Filter/FlateDecode/Length 38",
    );
    assert_objstm_lookup_fails_without_panic(&pdf);
}

#[test]
fn test_getobj_objstm_object_offset_beyond_data_len_does_not_panic() {
    let pdf = replace_once_fixed_len(
        IMAGE_STRUCTURE_PDF,
        OBJSTM_DICT_SEGMENT,
        b"/First 760/Filter/FlateDecode/Length 38",
    );
    assert_objstm_lookup_fails_without_panic(&pdf);
}

// === Encryption Integration Tests ===

// Encrypted test fixtures
const RC4_40_PDF: &[u8] = include_bytes!("fixtures/encryption/rc4-40.pdf");
const RC4_128_PDF: &[u8] = include_bytes!("fixtures/encryption/rc4-128.pdf");
const AES_128_PDF: &[u8] = include_bytes!("fixtures/encryption/aes-128.pdf");
const AES_256_PDF: &[u8] = include_bytes!("fixtures/encryption/aes-256.pdf");
const AES_256_R6_PDF: &[u8] = include_bytes!("fixtures/encryption/aes-256-r6.pdf");

/// Test that is_encrypted returns false for unencrypted documents.
#[test]
fn test_is_encrypted_false_for_unencrypted() {
    let doc = PDFDocument::new(SIMPLE1_PDF, "").expect("Failed to parse PDF");
    assert!(
        !doc.is_encrypted(),
        "Unencrypted PDF should return is_encrypted=false"
    );
}

/// Test that RC4-40 encrypted PDF is correctly identified and authenticated.
#[test]
fn test_rc4_40_encrypted_correct_password() {
    let doc = PDFDocument::new(RC4_40_PDF, "foo").expect("Failed to parse encrypted PDF");
    assert!(
        doc.is_encrypted(),
        "RC4-40 encrypted PDF should return is_encrypted=true"
    );
}

/// Test that RC4-40 encrypted PDF fails with wrong password.
#[test]
fn test_rc4_40_encrypted_wrong_password() {
    let result = PDFDocument::new(RC4_40_PDF, "wrong");
    assert!(result.is_err(), "Wrong password should fail authentication");
}

/// Test that RC4-128 encrypted PDF is correctly identified and authenticated.
#[test]
fn test_rc4_128_encrypted_correct_password() {
    let doc = PDFDocument::new(RC4_128_PDF, "foo").expect("Failed to parse encrypted PDF");
    assert!(
        doc.is_encrypted(),
        "RC4-128 encrypted PDF should return is_encrypted=true"
    );
}

/// Test that AES-128 encrypted PDF is correctly identified and authenticated.
#[test]
fn test_aes_128_encrypted_correct_password() {
    let doc = PDFDocument::new(AES_128_PDF, "foo").expect("Failed to parse encrypted PDF");
    assert!(
        doc.is_encrypted(),
        "AES-128 encrypted PDF should return is_encrypted=true"
    );
}

/// Test that AES-256 (R5) encrypted PDF is correctly identified and authenticated.
#[test]
fn test_aes_256_encrypted_correct_password() {
    let doc = PDFDocument::new(AES_256_PDF, "foo").expect("Failed to parse encrypted PDF");
    assert!(
        doc.is_encrypted(),
        "AES-256 encrypted PDF should return is_encrypted=true"
    );
}

/// Test that AES-256-R6 encrypted PDF is correctly identified and authenticated.
#[test]
fn test_aes_256_r6_encrypted_correct_password() {
    // R6 file uses different password
    let doc =
        PDFDocument::new(AES_256_R6_PDF, "usersecret").expect("Failed to parse encrypted PDF");
    assert!(
        doc.is_encrypted(),
        "AES-256-R6 encrypted PDF should return is_encrypted=true"
    );
}

/// Test that encrypted document without password fails authentication.
#[test]
fn test_encrypted_empty_password_fails() {
    let result = PDFDocument::new(RC4_40_PDF, "");
    assert!(
        result.is_err(),
        "Empty password should fail for password-protected PDF"
    );
}
