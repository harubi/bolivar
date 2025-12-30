//! Tests for RFC 4013 SASLprep implementation.
//! Ported from pdfminer.six _saslprep.py

use bolivar::saslprep::saslprep;

/// Test that ASCII strings pass through unchanged.
#[test]
fn test_ascii_passthrough() {
    assert_eq!(saslprep("hello", true).unwrap(), "hello");
    assert_eq!(saslprep("password123", true).unwrap(), "password123");
    assert_eq!(saslprep("Test String", true).unwrap(), "Test String");
}

/// Test RFC 4013 section 2.1: Non-ASCII spaces mapped to ASCII space (U+0020).
#[test]
fn test_non_ascii_space_mapping() {
    // NO-BREAK SPACE (U+00A0) -> SPACE
    assert_eq!(saslprep("\u{00A0}", true).unwrap(), " ");
    // EN SPACE (U+2002) -> SPACE
    assert_eq!(saslprep("\u{2002}", true).unwrap(), " ");
    // EM SPACE (U+2003) -> SPACE
    assert_eq!(saslprep("\u{2003}", true).unwrap(), " ");
    // IDEOGRAPHIC SPACE (U+3000) -> SPACE
    assert_eq!(saslprep("\u{3000}", true).unwrap(), " ");
    // Mixed with text
    assert_eq!(saslprep("a\u{00A0}b", true).unwrap(), "a b");
}

/// Test RFC 4013 section 2.1: Characters commonly mapped to nothing (table B.1).
#[test]
fn test_mapped_to_nothing() {
    // SOFT HYPHEN (U+00AD) should be removed
    assert_eq!(saslprep("pass\u{00AD}word", true).unwrap(), "password");
    // ZERO WIDTH SPACE (U+200B) should be removed
    assert_eq!(saslprep("pass\u{200B}word", true).unwrap(), "password");
}

/// Test RFC 4013 section 2.3: Prohibited characters cause error.
#[test]
fn test_prohibited_characters() {
    // Control characters (C.2.1) are prohibited
    assert!(saslprep("\u{0000}", true).is_err()); // NULL
    assert!(saslprep("\u{0007}", true).is_err()); // BELL
    assert!(saslprep("test\u{007F}", true).is_err()); // DEL

    // Private use characters (C.3) are prohibited
    assert!(saslprep("\u{E000}", true).is_err());

    // Non-character code points (C.4) are prohibited
    assert!(saslprep("\u{FFFF}", true).is_err());
}

/// Test RFC 3454 section 6: Bidirectional text handling.
/// If string contains RandALCat characters, first and last must be RandALCat.
#[test]
fn test_bidirectional_check() {
    // Pure RTL text should pass
    assert!(
        saslprep(
            "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}",
            true
        )
        .is_ok()
    ); // Arabic text

    // RTL with ASCII in middle should fail bidirectional check
    // (first char is RandALCat but last is not)
    assert!(saslprep("\u{0627}abc", true).is_err());

    // Pure LTR text should pass
    assert!(saslprep("hello", true).is_ok());
}

/// Test RFC 3454 Table A.1: Unassigned code points in Unicode 3.2.
/// When prohibit_unassigned_code_points is true, these should be rejected.
/// When false (query mode), they should be allowed.
#[test]
fn test_unassigned_code_points() {
    // U+0221 is unassigned in Unicode 3.2 (Table A.1)
    // With prohibit_unassigned_code_points=true (stored string), should fail
    assert!(saslprep("\u{0221}", true).is_err());

    // With prohibit_unassigned_code_points=false (query), should pass
    assert!(saslprep("\u{0221}", false).is_ok());

    // U+038B is also unassigned in Unicode 3.2
    assert!(saslprep("\u{038B}", true).is_err());
    assert!(saslprep("\u{038B}", false).is_ok());

    // U+0560 (Armenian small letter turned ayb, unassigned in Unicode 3.2)
    assert!(saslprep("\u{0560}", true).is_err());
    assert!(saslprep("\u{0560}", false).is_ok());

    // Mixed: assigned characters should work in both modes
    assert!(saslprep("hello\u{0221}world", true).is_err());
    assert_eq!(
        saslprep("hello\u{0221}world", false).unwrap(),
        "hello\u{0221}world"
    );
}
