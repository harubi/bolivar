//! Codec modules for PDF stream compression and encryption.
//!
//! This module contains:
//! - `aes`: AES encryption/decryption
//! - `arcfour`: RC4 encryption
//! - `ascii85`: ASCII85 and ASCIIHex encoding
//! - `ccitt`: CCITT fax decompression
//! - `jbig2`: JBIG2 image decompression
//! - `lzw`: LZW decompression
//! - `runlength`: Run-length decoding

pub mod aes;
pub mod arcfour;
pub mod ascii85;
pub mod ccitt;
pub mod jbig2;
pub mod lzw;
pub mod runlength;

// Re-export main functions for convenience
pub use aes::{aes_cbc_decrypt, aes_cbc_encrypt, unpad_aes};
pub use arcfour::Arcfour;
pub use ascii85::{ascii85decode, asciihexdecode};
pub use ccitt::{CCITTFaxDecoder, ccittfaxdecode};
pub use jbig2::{Jbig2StreamReader, Jbig2StreamWriter};
pub use lzw::lzwdecode_with_earlychange;
pub use runlength::rldecode;
