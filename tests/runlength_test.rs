//! 100% port of pdfminer.six test_pdfminer_crypto.py TestRunlength

use bolivar::runlength::rldecode;

#[test]
fn test_rldecode() {
    // Exact test from Python:
    // rldecode(b"\x05123456\xfa7\x04abcde\x80junk") == b"1234567777777abcde"
    let input = b"\x05123456\xfa7\x04abcde\x80junk";
    let expected = b"1234567777777abcde";
    assert_eq!(rldecode(input).unwrap(), expected);
}
