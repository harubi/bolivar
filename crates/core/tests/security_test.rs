//! Tests for PDF security handlers.
//!
//! Port of pdfminer.six encryption tests.

use bolivar_core::pdftypes::PDFObject;
use bolivar_core::security::{
    PDFSecurityHandler, PDFStandardSecurityHandlerV2, PDFStandardSecurityHandlerV5,
    create_security_handler,
};
use std::collections::HashMap;

// Test data from tests/fixtures/encryption/rc4-40.pdf
// V=1, R=2, 40-bit key, password="foo"
const RC4_40_V: i64 = 1;
const RC4_40_R: i64 = 2;
const RC4_40_P: i64 = -4;
const RC4_40_LENGTH: i64 = 40;
const RC4_40_O: [u8; 32] = [
    1, 169, 240, 206, 242, 141, 0, 248, 223, 176, 37, 143, 94, 240, 197, 92, 157, 247, 200, 22,
    149, 143, 54, 49, 0, 175, 119, 236, 2, 38, 36, 84,
];
const RC4_40_U: [u8; 32] = [
    105, 75, 157, 162, 248, 9, 199, 124, 114, 119, 140, 251, 202, 194, 4, 129, 178, 114, 5, 208,
    231, 211, 34, 98, 54, 130, 131, 100, 102, 106, 151, 8,
];
const RC4_40_DOCID: [u8; 16] = [
    101, 26, 148, 254, 235, 120, 104, 211, 18, 169, 123, 55, 114, 112, 134, 14,
];

// Test data from tests/fixtures/encryption/rc4-128.pdf
// V=2, R=3, 128-bit key, password="foo"
const RC4_128_V: i64 = 2;
const RC4_128_R: i64 = 3;
const RC4_128_P: i64 = -4;
const RC4_128_LENGTH: i64 = 128;
const RC4_128_O: [u8; 32] = [
    208, 72, 209, 82, 158, 83, 93, 24, 132, 205, 56, 86, 54, 123, 24, 75, 74, 144, 223, 1, 230, 55,
    209, 110, 202, 6, 91, 175, 78, 100, 144, 11,
];
const RC4_128_U: [u8; 32] = [
    9, 52, 18, 54, 59, 157, 50, 124, 122, 197, 1, 68, 199, 199, 85, 241, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0,
];
const RC4_128_DOCID: [u8; 16] = [
    101, 26, 148, 254, 235, 120, 104, 211, 18, 169, 123, 55, 114, 112, 134, 14,
];

/// Helper to build an encrypt dict from test constants.
fn make_encrypt_dict(
    v: i64,
    r: i64,
    p: i64,
    length: i64,
    o: &[u8],
    u: &[u8],
) -> HashMap<String, PDFObject> {
    let mut dict = HashMap::new();
    dict.insert("V".into(), PDFObject::Int(v));
    dict.insert("R".into(), PDFObject::Int(r));
    dict.insert("P".into(), PDFObject::Int(p));
    dict.insert("Length".into(), PDFObject::Int(length));
    dict.insert("O".into(), PDFObject::String(o.to_vec()));
    dict.insert("U".into(), PDFObject::String(u.to_vec()));
    dict.insert("Filter".into(), PDFObject::Name("Standard".into()));
    dict
}

// --- Basic API Tests ---

#[test]
fn test_create_handler_returns_none_for_empty() {
    let encrypt: HashMap<String, PDFObject> = HashMap::new();
    let id: Vec<Vec<u8>> = vec![];
    let result = create_security_handler(&encrypt, &id, "");
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_create_handler_errors_on_unsupported_version() {
    let mut encrypt: HashMap<String, PDFObject> = HashMap::new();
    encrypt.insert("V".into(), PDFObject::Int(99));
    encrypt.insert("R".into(), PDFObject::Int(99));
    encrypt.insert("O".into(), PDFObject::String(vec![0; 32]));
    encrypt.insert("U".into(), PDFObject::String(vec![0; 32]));
    encrypt.insert("P".into(), PDFObject::Int(-4));
    let id: Vec<Vec<u8>> = vec![];
    let result = create_security_handler(&encrypt, &id, "");
    assert!(result.is_err());
}

// --- RC4 40-bit (V=1, R=2) Tests ---

#[test]
fn test_rc4_40_correct_password() {
    let encrypt = make_encrypt_dict(
        RC4_40_V,
        RC4_40_R,
        RC4_40_P,
        RC4_40_LENGTH,
        &RC4_40_O,
        &RC4_40_U,
    );
    let doc_id = vec![RC4_40_DOCID.to_vec()];

    let result = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo");
    assert!(
        result.is_ok(),
        "Authentication with correct password should succeed"
    );
}

#[test]
fn test_rc4_40_wrong_password() {
    let encrypt = make_encrypt_dict(
        RC4_40_V,
        RC4_40_R,
        RC4_40_P,
        RC4_40_LENGTH,
        &RC4_40_O,
        &RC4_40_U,
    );
    let doc_id = vec![RC4_40_DOCID.to_vec()];

    let result = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "wrong");
    assert!(
        result.is_err(),
        "Authentication with wrong password should fail"
    );
}

#[test]
fn test_rc4_40_empty_password_fails() {
    let encrypt = make_encrypt_dict(
        RC4_40_V,
        RC4_40_R,
        RC4_40_P,
        RC4_40_LENGTH,
        &RC4_40_O,
        &RC4_40_U,
    );
    let doc_id = vec![RC4_40_DOCID.to_vec()];

    let result = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "");
    assert!(
        result.is_err(),
        "Authentication with empty password should fail for password-protected PDF"
    );
}

#[test]
fn test_rc4_40_via_create_handler() {
    let encrypt = make_encrypt_dict(
        RC4_40_V,
        RC4_40_R,
        RC4_40_P,
        RC4_40_LENGTH,
        &RC4_40_O,
        &RC4_40_U,
    );
    let doc_id = vec![RC4_40_DOCID.to_vec()];

    let result = create_security_handler(&encrypt, &doc_id, "foo");
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}

// --- RC4 128-bit (V=2, R=3) Tests ---

#[test]
fn test_rc4_128_correct_password() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];

    let result = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo");
    assert!(
        result.is_ok(),
        "Authentication with correct password should succeed"
    );
}

#[test]
fn test_rc4_128_wrong_password() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];

    let result = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "wrong");
    assert!(
        result.is_err(),
        "Authentication with wrong password should fail"
    );
}

#[test]
fn test_rc4_128_via_create_handler() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];

    let result = create_security_handler(&encrypt, &doc_id, "foo");
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}

// --- Decryption Tests ---

#[test]
fn test_rc4_40_decryption_roundtrip() {
    let encrypt = make_encrypt_dict(
        RC4_40_V,
        RC4_40_R,
        RC4_40_P,
        RC4_40_LENGTH,
        &RC4_40_O,
        &RC4_40_U,
    );
    let doc_id = vec![RC4_40_DOCID.to_vec()];
    let handler = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo").unwrap();

    // Test that encrypt(decrypt(x)) == x for RC4 (symmetric)
    let plaintext = b"Hello, PDF encryption!";
    let encrypted = handler.decrypt(1, 0, plaintext, None);
    let decrypted = handler.decrypt(1, 0, &encrypted, None);

    assert_eq!(&decrypted, plaintext);
}

#[test]
fn test_rc4_128_decryption_roundtrip() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];
    let handler = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo").unwrap();

    let plaintext = b"Hello, PDF encryption!";
    let encrypted = handler.decrypt(1, 0, plaintext, None);
    let decrypted = handler.decrypt(1, 0, &encrypted, None);

    assert_eq!(&decrypted, plaintext);
}

#[test]
fn test_different_objects_get_different_keys() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];
    let handler = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo").unwrap();

    let plaintext = b"Test data";

    // Different object IDs should produce different ciphertext
    let encrypted1 = handler.decrypt(1, 0, plaintext, None);
    let encrypted2 = handler.decrypt(2, 0, plaintext, None);

    assert_ne!(encrypted1, encrypted2);
}

#[test]
fn test_different_generations_get_different_keys() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];
    let handler = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo").unwrap();

    let plaintext = b"Test data";

    // Different generation numbers should produce different ciphertext
    let encrypted1 = handler.decrypt(1, 0, plaintext, None);
    let encrypted2 = handler.decrypt(1, 1, plaintext, None);

    assert_ne!(encrypted1, encrypted2);
}

// --- Security Handler Trait Tests ---

#[test]
fn test_decrypt_string_uses_decrypt() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];
    let handler = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo").unwrap();

    let data = b"test string";
    let result1 = handler.decrypt(1, 0, data, None);
    let result2 = handler.decrypt_string(1, 0, data);

    assert_eq!(result1, result2);
}

#[test]
fn test_decrypt_stream_uses_decrypt() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];
    let handler = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo").unwrap();

    let data = b"test stream";
    let attrs: HashMap<String, PDFObject> = HashMap::new();
    let result1 = handler.decrypt(1, 0, data, Some(&attrs));
    let result2 = handler.decrypt_stream(1, 0, data, &attrs);

    assert_eq!(result1, result2);
}

// --- Edge Cases ---

#[test]
fn test_empty_data_decryption() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];
    let handler = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo").unwrap();

    let result = handler.decrypt(1, 0, &[], None);
    assert!(result.is_empty());
}

#[test]
fn test_large_object_id() {
    let encrypt = make_encrypt_dict(
        RC4_128_V,
        RC4_128_R,
        RC4_128_P,
        RC4_128_LENGTH,
        &RC4_128_O,
        &RC4_128_U,
    );
    let doc_id = vec![RC4_128_DOCID.to_vec()];
    let handler = PDFStandardSecurityHandlerV2::new(&encrypt, &doc_id, "foo").unwrap();

    // Should handle large object IDs without panicking
    let plaintext = b"test";
    let encrypted = handler.decrypt(0xFFFFFF, 0xFFFF, plaintext, None);
    let decrypted = handler.decrypt(0xFFFFFF, 0xFFFF, &encrypted, None);

    assert_eq!(&decrypted, plaintext);
}

#[test]
fn test_handler_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PDFStandardSecurityHandlerV2>();
}

// --- AES-256 (V=5, R=5) Tests ---
// Test data from tests/fixtures/encryption/aes-256.pdf

const AES256_V: i64 = 5;
const AES256_R: i64 = 5;
const AES256_P: i64 = -4;
const AES256_O: [u8; 48] = [
    197, 126, 60, 46, 218, 22, 190, 91, 132, 46, 198, 222, 145, 49, 111, 125, 24, 147, 223, 122, 6,
    21, 159, 78, 155, 195, 49, 220, 252, 161, 203, 182, 215, 56, 115, 236, 23, 247, 193, 14, 39,
    184, 210, 207, 56, 201, 114, 199,
];
const AES256_U: [u8; 48] = [
    179, 236, 138, 87, 238, 76, 63, 44, 188, 66, 38, 224, 89, 1, 136, 216, 233, 86, 206, 51, 43,
    103, 248, 173, 26, 183, 85, 55, 229, 239, 180, 149, 88, 136, 28, 124, 249, 186, 223, 59, 180,
    7, 178, 19, 84, 51, 249, 188,
];
const AES256_OE: [u8; 32] = [
    91, 206, 49, 194, 37, 90, 49, 81, 128, 220, 14, 148, 72, 121, 213, 222, 45, 98, 227, 35, 15,
    76, 191, 10, 54, 211, 184, 43, 81, 250, 80, 231,
];
const AES256_UE: [u8; 32] = [
    121, 209, 78, 72, 9, 195, 93, 96, 16, 97, 189, 216, 198, 84, 195, 205, 125, 73, 208, 81, 173,
    33, 196, 195, 9, 4, 57, 3, 226, 247, 31, 8,
];

/// Helper to build a V5 encrypt dict.
fn make_aes256_encrypt_dict(
    v: i64,
    r: i64,
    p: i64,
    o: &[u8],
    u: &[u8],
    oe: &[u8],
    ue: &[u8],
) -> HashMap<String, PDFObject> {
    let mut dict = HashMap::new();
    dict.insert("V".into(), PDFObject::Int(v));
    dict.insert("R".into(), PDFObject::Int(r));
    dict.insert("P".into(), PDFObject::Int(p));
    dict.insert("O".into(), PDFObject::String(o.to_vec()));
    dict.insert("U".into(), PDFObject::String(u.to_vec()));
    dict.insert("OE".into(), PDFObject::String(oe.to_vec()));
    dict.insert("UE".into(), PDFObject::String(ue.to_vec()));
    dict.insert("Filter".into(), PDFObject::Name("Standard".into()));
    dict.insert("Length".into(), PDFObject::Int(256));
    dict.insert("StrF".into(), PDFObject::Name("StdCF".into()));
    dict.insert("StmF".into(), PDFObject::Name("StdCF".into()));

    // Build CF dict with StdCF filter using AESV3
    let mut stdcf = HashMap::new();
    stdcf.insert("CFM".into(), PDFObject::Name("AESV3".into()));
    stdcf.insert("Length".into(), PDFObject::Int(32));
    stdcf.insert("AuthEvent".into(), PDFObject::Name("DocOpen".into()));

    let mut cf = HashMap::new();
    cf.insert("StdCF".into(), PDFObject::Dict(stdcf));

    dict.insert("CF".into(), PDFObject::Dict(cf));
    dict
}

#[test]
fn test_aes256_correct_password() {
    let encrypt = make_aes256_encrypt_dict(
        AES256_V, AES256_R, AES256_P, &AES256_O, &AES256_U, &AES256_OE, &AES256_UE,
    );
    let doc_id: Vec<Vec<u8>> = vec![];

    let result = PDFStandardSecurityHandlerV5::new(&encrypt, &doc_id, "foo");
    assert!(
        result.is_ok(),
        "Authentication with correct password should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_aes256_wrong_password() {
    let encrypt = make_aes256_encrypt_dict(
        AES256_V, AES256_R, AES256_P, &AES256_O, &AES256_U, &AES256_OE, &AES256_UE,
    );
    let doc_id: Vec<Vec<u8>> = vec![];

    let result = PDFStandardSecurityHandlerV5::new(&encrypt, &doc_id, "wrong");
    assert!(
        result.is_err(),
        "Authentication with wrong password should fail"
    );
}

#[test]
fn test_aes256_empty_password_fails() {
    let encrypt = make_aes256_encrypt_dict(
        AES256_V, AES256_R, AES256_P, &AES256_O, &AES256_U, &AES256_OE, &AES256_UE,
    );
    let doc_id: Vec<Vec<u8>> = vec![];

    let result = PDFStandardSecurityHandlerV5::new(&encrypt, &doc_id, "");
    assert!(
        result.is_err(),
        "Authentication with empty password should fail for password-protected PDF"
    );
}

#[test]
fn test_aes256_via_create_handler() {
    let encrypt = make_aes256_encrypt_dict(
        AES256_V, AES256_R, AES256_P, &AES256_O, &AES256_U, &AES256_OE, &AES256_UE,
    );
    let doc_id: Vec<Vec<u8>> = vec![];

    let result = create_security_handler(&encrypt, &doc_id, "foo");
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}

#[test]
fn test_aes256_handler_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PDFStandardSecurityHandlerV5>();
}
