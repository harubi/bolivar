//! 100% port of pdfminer.six test_pdfminer_crypto.py TestAscii85

use bolivar_core::ascii85::{ascii85decode, asciihexdecode};

// === test_ascii85decode (10 assertions) ===

#[test]
fn test_ascii85decode_wikipedia() {
    assert_eq!(
        ascii85decode(b"9jqo^BlbD-BleB1DJ+*+F(f,q").unwrap(),
        b"Man is distinguished"
    );
}

#[test]
fn test_ascii85decode_with_eod() {
    assert_eq!(ascii85decode(b"E,9)oF*2M7/c~>").unwrap(), b"pleasure.");
}

#[test]
fn test_ascii85decode_z_encoding() {
    assert_eq!(
        ascii85decode(b"zE,9)oF*2M7/c~>").unwrap(),
        b"\0\0\0\0pleasure."
    );
}

#[test]
fn test_ascii85decode_no_eod() {
    assert_eq!(ascii85decode(b"E,9)oF*2M7/c").unwrap(), b"pleasure.");
}

#[test]
fn test_ascii85decode_partial_eod() {
    assert_eq!(ascii85decode(b"E,9)oF*2M7/c~").unwrap(), b"pleasure.");
}

#[test]
fn test_ascii85decode_with_prefix() {
    assert_eq!(ascii85decode(b"<~E,9)oF*2M7/c~").unwrap(), b"pleasure.");
}

#[test]
fn test_ascii85decode_with_prefix_and_newline() {
    assert_eq!(ascii85decode(b"<~E,9)oF*2M7/c~\n>").unwrap(), b"pleasure.");
}

#[test]
fn test_ascii85decode_various_1() {
    assert_eq!(
        ascii85decode(b"<^BVT:K:=9<E)pd;BS_1:/aSV;ag~>").unwrap(),
        b"VARIOUS UTTER NONSENSE"
    );
}

#[test]
fn test_ascii85decode_various_2() {
    assert_eq!(
        ascii85decode(b"<~<^BVT:K:=9<E)pd;BS_1:/aSV;ag~>").unwrap(),
        b"VARIOUS UTTER NONSENSE"
    );
}

#[test]
fn test_ascii85decode_various_3() {
    assert_eq!(
        ascii85decode(b"<^BVT:K:=9<E)pd;BS_1:/aSV;ag~").unwrap(),
        b"VARIOUS UTTER NONSENSE"
    );
}

// === test_asciihexdecode (3 assertions) ===

#[test]
fn test_asciihexdecode_whitespace() {
    assert_eq!(asciihexdecode(b"61 62 2e6364   65").unwrap(), b"ab.cde");
}

#[test]
fn test_asciihexdecode_odd_with_eod() {
    assert_eq!(asciihexdecode(b"61 62 2e6364   657>").unwrap(), b"ab.cdep");
}

#[test]
fn test_asciihexdecode_single_odd() {
    assert_eq!(asciihexdecode(b"7>").unwrap(), b"p");
}
