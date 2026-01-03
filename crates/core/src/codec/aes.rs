//! AES utilities for PDF encryption.
//!
//! Port of pdfminer.six utils.unpad_aes

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use cbc::{Decryptor, Encryptor};

type Aes128CbcDec = Decryptor<aes::Aes128>;
type Aes256CbcDec = Decryptor<aes::Aes256>;
type Aes128CbcEnc = Encryptor<aes::Aes128>;

/// Decrypt data using AES-CBC with 128 or 256 bit key.
///
/// The key must be exactly 16 bytes (AES-128) or 32 bytes (AES-256).
/// The IV must be exactly 16 bytes.
/// Data length must be a multiple of 16 bytes.
///
/// # Panics
/// Panics if key length is not 16 or 32 bytes, or if IV is not 16 bytes.
pub fn aes_cbc_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    assert!(iv.len() == 16, "AES IV must be 16 bytes");
    let mut buf = data.to_vec();
    match key.len() {
        16 => {
            let cipher = Aes128CbcDec::new(key.into(), iv.into());
            cipher
                .decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf)
                .unwrap();
        }
        32 => {
            let cipher = Aes256CbcDec::new(key.into(), iv.into());
            cipher
                .decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf)
                .unwrap();
        }
        _ => panic!("AES key must be 16 or 32 bytes"),
    }
    buf
}

/// Encrypt data using AES-128-CBC.
///
/// The key must be exactly 16 bytes.
/// The IV must be exactly 16 bytes.
/// Data length must be a multiple of 16 bytes (no padding is applied).
///
/// # Panics
/// Panics if key length is not 16 bytes, or if IV is not 16 bytes.
pub fn aes_cbc_encrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    assert!(key.len() == 16, "AES-128 key must be 16 bytes");
    assert!(iv.len() == 16, "AES IV must be 16 bytes");
    let mut buf = data.to_vec();
    let cipher = Aes128CbcEnc::new(key.into(), iv.into());
    cipher
        .encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf, data.len())
        .unwrap();
    buf
}

/// Remove PKCS#7 padding from AES-decrypted data.
///
/// Returns data unchanged if padding is invalid:
/// - Padding byte value is 0 or > 16
/// - Not enough bytes for claimed padding
/// - Padding bytes are not all equal to the padding length
pub fn unpad_aes(data: &[u8]) -> &[u8] {
    if data.is_empty() {
        return data;
    }

    let pad_len = data[data.len() - 1] as usize;

    // Validate padding
    if pad_len == 0 || pad_len > 16 || pad_len > data.len() {
        return data;
    }

    // Check all padding bytes match
    let start = data.len() - pad_len;
    for &byte in &data[start..] {
        if byte as usize != pad_len {
            return data;
        }
    }

    &data[..start]
}
