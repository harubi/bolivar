//! 100% port of pdfminer.six test_encodingdb.py
//!
//! Tests based on the Adobe Glyph List Specification
//! See: https://github.com/adobe-type-tools/agl-specification#2-the-mapping
//!
//! While not in the specification, lowercase unicode often occurs in PDFs.
//! Therefore lowercase unittest variants are added.

use bolivar_core::encodingdb::{EncodingDB, name2unicode};

// === name2unicode() tests (20 tests) ===

/// Test 1: The name "Lcommaaccent" has a single component,
/// which is mapped to the string U+013B by AGL
#[test]
fn test_name2unicode_name_in_agl() {
    assert_eq!(name2unicode("Lcommaaccent").unwrap(), "\u{013b}");
}

/// Test 2: The components "Lcommaaccent," "uni013B," and "u013B"
/// all map to the string U+013B
#[test]
fn test_name2unicode_uni() {
    assert_eq!(name2unicode("uni013B").unwrap(), "\u{013b}");
}

/// Test 3: Lowercase variant of uni prefix
#[test]
fn test_name2unicode_uni_lowercase() {
    assert_eq!(name2unicode("uni013b").unwrap(), "\u{013b}");
}

/// Test 4: The name "uni20AC0308" has a single component,
/// which is mapped to the string U+20AC U+0308
#[test]
fn test_name2unicode_uni_with_sequence_of_digits() {
    assert_eq!(name2unicode("uni20AC0308").unwrap(), "\u{20ac}\u{0308}");
}

/// Test 5: Lowercase variant of sequence
#[test]
fn test_name2unicode_uni_with_sequence_of_digits_lowercase() {
    assert_eq!(name2unicode("uni20ac0308").unwrap(), "\u{20ac}\u{0308}");
}

/// Test 6: The name "uni20ac" has a single component,
/// which is mapped to a euro-sign.
#[test]
fn test_name2unicode_uni_empty_string() {
    assert_eq!(name2unicode("uni20ac").unwrap(), "\u{20ac}");
}

/// Test 7: The name "uniD801DC0C" has a single component,
/// which is mapped to an empty string.
/// Neither D801 nor DC0C are in the appropriate set.
/// This form cannot be used to map to the character which is
/// expressed as D801 DC0C in UTF-16, specifically U+1040C.
#[test]
fn test_name2unicode_uni_empty_string_long() {
    assert!(name2unicode("uniD801DC0C").is_err());
}

/// Test 8: Lowercase variant of surrogate error
#[test]
fn test_name2unicode_uni_empty_string_long_lowercase() {
    // Note: Python test uses same input as above, but we test lowercase
    assert!(name2unicode("unid801dc0c").is_err());
}

/// Test 9: "Ogoneksmall" and "uniF6FB" both map to the string
/// that corresponds to U+F6FB.
#[test]
fn test_name2unicode_uni_pua() {
    assert_eq!(name2unicode("uniF6FB").unwrap(), "\u{f6fb}");
}

/// Test 10: Lowercase variant of PUA
#[test]
fn test_name2unicode_uni_pua_lowercase() {
    assert_eq!(name2unicode("unif6fb").unwrap(), "\u{f6fb}");
}

/// Test 11: The components "Lcommaaccent," "uni013B," and "u013B"
/// all map to the string U+013B
#[test]
fn test_name2unicode_u_with_4_digits() {
    assert_eq!(name2unicode("u013B").unwrap(), "\u{013b}");
}

/// Test 12: Lowercase variant
#[test]
fn test_name2unicode_u_with_4_digits_lowercase() {
    assert_eq!(name2unicode("u013b").unwrap(), "\u{013b}");
}

/// Test 13: The name "u1040C" has a single component,
/// which is mapped to the string U+1040C (supplementary plane)
#[test]
fn test_name2unicode_u_with_5_digits() {
    assert_eq!(name2unicode("u1040C").unwrap(), "\u{1040c}");
}

/// Test 14: Lowercase variant
#[test]
fn test_name2unicode_u_with_5_digits_lowercase() {
    assert_eq!(name2unicode("u1040c").unwrap(), "\u{1040c}");
}

/// Test 15: The name "Lcommaaccent_uni20AC0308_u1040C.alternate" is mapped
/// to the string U+013B U+20AC U+0308 U+1040C
#[test]
fn test_name2unicode_multiple_components() {
    assert_eq!(
        name2unicode("Lcommaaccent_uni20AC0308_u1040C.alternate").unwrap(),
        "\u{013b}\u{20ac}\u{0308}\u{1040c}"
    );
}

/// Test 16: Lowercase variant of composite
#[test]
fn test_name2unicode_multiple_components_lowercase() {
    assert_eq!(
        name2unicode("Lcommaaccent_uni20ac0308_u1040c.alternate").unwrap(),
        "\u{013b}\u{20ac}\u{0308}\u{1040c}"
    );
}

/// Test 17: The name 'foo' maps to an error,
/// because 'foo' is not in AGL, and it does not start with 'u'
#[test]
fn test_name2unicode_foo() {
    assert!(name2unicode("foo").is_err());
}

/// Test 18: The name ".notdef" is reduced to an empty string (step 1)
/// and mapped to an error (step 3)
#[test]
fn test_name2unicode_notdef() {
    assert!(name2unicode(".notdef").is_err());
}

/// Test 19: "Ogoneksmall" and "uniF6FB" both map to U+F6FB (via AGL)
#[test]
fn test_name2unicode_pua_ogoneksmall() {
    assert_eq!(name2unicode("Ogoneksmall").unwrap(), "\u{f6fb}");
}

/// Test 20: Very long hex string should produce error (overflow/invalid)
#[test]
fn test_name2unicode_overflow_error() {
    assert!(name2unicode("226215240241240240240240").is_err());
}

// === EncodingDB tests (1 test) ===

/// Test 21: Invalid differences should be silently ignored
/// Regression test for https://github.com/pdfminer/pdfminer.six/issues/385
#[test]
fn test_get_encoding_with_invalid_differences() {
    use bolivar_core::encodingdb::DiffEntry;

    // In Python: invalid_differences = [PSLiteral("ubuntu"), PSLiteral("1234")]
    // Invalid differences don't have a valid code position preceding them
    let invalid_differences: Vec<DiffEntry> = vec![
        DiffEntry::Name("ubuntu".to_string()), // Invalid - no code position before it
        DiffEntry::Name("1234".to_string()),   // Invalid - no code position before it
    ];

    // Should not panic, invalid differences silently ignored
    let encoding = EncodingDB::get_encoding("StandardEncoding", Some(&invalid_differences));
    // Encoding should be returned (not panic), but invalid diffs are ignored
    assert!(!encoding.is_empty());
}
