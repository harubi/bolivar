//! 100% port of pdfminer.six test_pdfminer_crypto.py TestArcfour

use bolivar_core::arcfour::Arcfour;

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

#[test]
fn test_arcfour_key() {
    let mut cipher = Arcfour::new(b"Key");
    let result = cipher.process(b"Plaintext");
    assert_eq!(hex(&result), "bbf316e8d940af0ad3");
}

#[test]
fn test_arcfour_wiki() {
    let mut cipher = Arcfour::new(b"Wiki");
    let result = cipher.process(b"pedia");
    assert_eq!(hex(&result), "1021bf0420");
}

#[test]
fn test_arcfour_secret() {
    let mut cipher = Arcfour::new(b"Secret");
    let result = cipher.process(b"Attack at dawn");
    assert_eq!(hex(&result), "45a01f645fc35b383552544b9bf5");
}
