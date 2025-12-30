//! LZW stream decoder using weezl crate.
//!
//! Port of pdfminer.six lzwdecode.

use crate::error::Result;
use weezl::{BitOrder, decode::Decoder};

/// Decode LZW-encoded data (PDF variant: MSB first, 8-bit).
pub fn lzwdecode(data: &[u8]) -> Result<Vec<u8>> {
    lzwdecode_with_earlychange(data, 1)
}

/// Decode LZW-encoded data with EarlyChange setting.
///
/// EarlyChange=1 is the PDF default; EarlyChange=0 uses TIFF size switching.
pub fn lzwdecode_with_earlychange(data: &[u8], early_change: i32) -> Result<Vec<u8>> {
    let mut decoder = if early_change == 0 {
        Decoder::with_tiff_size_switch(BitOrder::Msb, 8)
    } else {
        Decoder::new(BitOrder::Msb, 8)
    };
    let mut output = Vec::new();
    // Be lenient like pdfminer.six: ignore errors and return partial output on corrupt data.
    let _ = decoder.into_vec(&mut output).decode(data);
    Ok(output)
}
