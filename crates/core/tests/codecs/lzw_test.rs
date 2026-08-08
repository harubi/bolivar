//! 100% port of pdfminer.six test_pdfminer_crypto.py TestLzw

use bolivar_core::lzw::lzwdecode;

#[test]
fn test_lzwdecode() {
    let input = b"\x80\x0b\x60\x50\x22\x0c\x0c\x85\x01";
    let expected = b"\x2d\x2d\x2d\x2d\x2d\x41\x2d\x2d\x2d\x42";
    assert_eq!(lzwdecode(input).unwrap(), expected);
}
