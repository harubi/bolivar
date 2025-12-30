//! 100% port of pdfminer.six test_pdfminer_crypto.py TestAES

use bolivar::aes::{aes_cbc_decrypt, unpad_aes};

#[test]
fn test_unpad_aes_full_padding() {
    // Full block of padding (16 bytes of 0x10) -> empty
    assert_eq!(unpad_aes(&[0x10; 16]), b"");
}

#[test]
fn test_unpad_aes_block_plus_padding() {
    // One data block + full padding block
    let mut input = b"0123456789abcdef".to_vec();
    input.extend([0x10; 16]);
    assert_eq!(unpad_aes(&input), b"0123456789abcdef");
}

#[test]
fn test_unpad_aes_partial() {
    // Data with 3-byte padding
    assert_eq!(unpad_aes(b"0123456789abc\x03\x03\x03"), b"0123456789abc");
}

#[test]
fn test_unpad_aes_two_blocks() {
    // Two blocks with padding
    assert_eq!(
        unpad_aes(b"0123456789abcdef0123456789abc\x03\x03\x03"),
        b"0123456789abcdef0123456789abc"
    );
}

#[test]
fn test_unpad_aes_embedded_padding_bytes() {
    // Data containing 0x01 bytes that look like padding, followed by real 0x01 padding
    assert_eq!(
        unpad_aes(b"foo\x01bar\x01bazquux\x01"),
        b"foo\x01bar\x01bazquux"
    );
}

#[test]
fn test_unpad_aes_invalid_padding_1() {
    // Invalid padding (last 3 bytes are \x02\x03\x04, not consistent) - should preserve
    assert_eq!(
        unpad_aes(b"0123456789abc\x02\x03\x04"),
        b"0123456789abc\x02\x03\x04"
    );
}

#[test]
fn test_unpad_aes_invalid_padding_2() {
    // Invalid padding (5 bytes of \x05 but only 3 present) - should preserve
    assert_eq!(
        unpad_aes(b"0123456789abc\x05\x05\x05"),
        b"0123456789abc\x05\x05\x05"
    );
}

#[test]
fn test_aes128_cbc_decrypt() {
    // Test vector with known plaintext
    let key = [0u8; 16];
    let iv = [0u8; 16];
    let ciphertext = hex::decode("66e94bd4ef8a2c3b884cfa59ca342b2e").unwrap();
    let plaintext = aes_cbc_decrypt(&key, &iv, &ciphertext);
    assert_eq!(plaintext, vec![0u8; 16]);
}

#[test]
fn test_aes256_cbc_decrypt() {
    let key = [0u8; 32];
    let iv = [0u8; 16];
    let ciphertext = hex::decode("dc95c078a2408989ad48a21492842087").unwrap();
    let plaintext = aes_cbc_decrypt(&key, &iv, &ciphertext);
    assert_eq!(plaintext, vec![0u8; 16]);
}
