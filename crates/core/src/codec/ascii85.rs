//! ASCII85 and ASCIIHex stream decoders.
//!
//! Port of pdfminer.six ascii85.py

use crate::error::Result;
use crate::simd::{SimdU8, U8_LANES};
#[rustversion::since(1.95)]
use std::simd::Select;
use std::simd::prelude::*;

const LANES: usize = U8_LANES;

/// Decode ASCII85-encoded data (PDF variant).
/// Handles: z-encoding, <~ ~> markers, whitespace, missing EOD.
pub fn ascii85decode(data: &[u8]) -> Result<Vec<u8>> {
    ascii85decode_simd(data)
}

fn ascii85decode_simd(data: &[u8]) -> Result<Vec<u8>> {
    // Strip <~ prefix if present (only if followed by ~)
    let data = if data.starts_with(b"<~") {
        &data[2..]
    } else {
        data
    };

    // Find ~> end marker, strip trailing junk
    let data = match data.iter().position(|&b| b == b'~') {
        Some(pos) => &data[..pos],
        None => data,
    };

    // Filter whitespace and expand 'z'
    let mut filtered = Vec::with_capacity(data.len());
    let mut idx = 0;
    while idx < data.len() {
        let remaining = data.len() - idx;
        if remaining >= LANES {
            let chunk = &data[idx..idx + LANES];
            let bytes = SimdU8::from_slice(chunk);
            let is_ws = bytes.simd_eq(Simd::splat(b' '))
                | bytes.simd_eq(Simd::splat(b'\t'))
                | bytes.simd_eq(Simd::splat(b'\n'))
                | bytes.simd_eq(Simd::splat(b'\r'))
                | bytes.simd_eq(Simd::splat(b'\x00'));
            let is_z = bytes.simd_eq(Simd::splat(b'z'));
            let is_data = bytes.simd_ge(Simd::splat(b'!')) & bytes.simd_le(Simd::splat(b'u'));
            let has_ws = is_ws.any();
            let has_z = is_z.any();
            let all_data = is_data.all();
            if !has_ws && !has_z && all_data {
                filtered.extend_from_slice(chunk);
                idx += LANES;
                continue;
            }
        }

        let byte = data[idx];
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' | b'\x00' => {}
            b'z' => filtered.extend_from_slice(b"!!!!!"), // z = 4 zero bytes
            b'!'..=b'u' => filtered.push(byte),
            _ => {}
        }
        idx += 1;
    }

    decode_ascii85_bytes(&filtered)
}

fn decode_ascii85_bytes(data: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::new();

    for chunk in data.chunks(5) {
        if chunk.len() == 5 {
            let mut value: u32 = 0;
            for &byte in chunk {
                value = value * 85 + (byte - b'!') as u32;
            }
            result.extend_from_slice(&value.to_be_bytes());
        } else if !chunk.is_empty() {
            let mut padded = [b'u'; 5];
            padded[..chunk.len()].copy_from_slice(chunk);
            let mut value: u32 = 0;
            for &byte in &padded {
                value = value * 85 + (byte - b'!') as u32;
            }
            let bytes = value.to_be_bytes();
            result.extend_from_slice(&bytes[..chunk.len() - 1]);
        }
    }

    Ok(result)
}

/// Decode ASCIIHex-encoded data.
pub fn asciihexdecode(data: &[u8]) -> Result<Vec<u8>> {
    asciihexdecode_simd(data)
}

fn asciihexdecode_simd(data: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(data.len() / 2);
    let mut pending: Option<u8> = None;
    let mut idx = 0;

    while idx < data.len() {
        let remaining = data.len() - idx;
        if remaining >= LANES {
            let chunk = &data[idx..idx + LANES];
            let bytes = SimdU8::from_slice(chunk);
            let is_gt = bytes.simd_eq(Simd::splat(b'>'));
            if is_gt.any() {
                let lanes = is_gt.to_array();
                let mut stop = 0;
                while stop < LANES && !lanes[stop] {
                    stop += 1;
                }
                for &byte in &chunk[..stop] {
                    if let Some(nibble) = hex_nibble(byte) {
                        if let Some(high) = pending.take() {
                            result.push((high << 4) | nibble);
                        } else {
                            pending = Some(nibble);
                        }
                    }
                }
                break;
            }

            let is_ws = bytes.simd_eq(Simd::splat(b' '))
                | bytes.simd_eq(Simd::splat(b'\t'))
                | bytes.simd_eq(Simd::splat(b'\n'))
                | bytes.simd_eq(Simd::splat(b'\r'));
            let is_digit = bytes.simd_ge(Simd::splat(b'0')) & bytes.simd_le(Simd::splat(b'9'));
            let is_upper = bytes.simd_ge(Simd::splat(b'A')) & bytes.simd_le(Simd::splat(b'F'));
            let is_lower = bytes.simd_ge(Simd::splat(b'a')) & bytes.simd_le(Simd::splat(b'f'));
            let is_hex = is_digit | is_upper | is_lower;
            let all_allowed = (is_hex | is_ws).all();
            if all_allowed && !is_ws.any() {
                let nibble_digit = bytes - Simd::splat(b'0');
                let nibble_upper = bytes - Simd::splat(b'A') + Simd::splat(10);
                let nibble_lower = bytes - Simd::splat(b'a') + Simd::splat(10);
                let nibble =
                    is_digit.select(nibble_digit, is_upper.select(nibble_upper, nibble_lower));
                let nibbles = nibble.to_array();
                let mut lane = 0;
                if let Some(high) = pending.take() {
                    let low = nibbles[0];
                    result.push((high << 4) | low);
                    lane = 1;
                }
                while lane + 1 < LANES {
                    let high = nibbles[lane];
                    let low = nibbles[lane + 1];
                    result.push((high << 4) | low);
                    lane += 2;
                }
                if lane < LANES {
                    pending = Some(nibbles[lane]);
                }
                idx += LANES;
                continue;
            }
        }

        let byte = data[idx];
        if byte == b'>' {
            break;
        }
        if let Some(nibble) = hex_nibble(byte) {
            if let Some(high) = pending.take() {
                result.push((high << 4) | nibble);
            } else {
                pending = Some(nibble);
            }
        }
        idx += 1;
    }

    if let Some(high) = pending {
        result.push(high << 4);
    }

    Ok(result)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b' ' | b'\t' | b'\n' | b'\r' => None,
        _ => None,
    }
}

#[cfg(test)]
mod simd_decode_tests {
    use super::*;

    #[test]
    fn asciihex_decode_expected() {
        let data = b"<48656c6c6f 20776f726c64>"; // "Hello world"
        let decoded = asciihexdecode_simd(data).unwrap();
        assert_eq!(decoded, b"Hello world");
    }

    #[test]
    fn ascii85_decode_expected() {
        let data = b"<~87cURD]i,\"Ebo7~>"; // "Hello world"
        let decoded = ascii85decode_simd(data).unwrap();
        assert_eq!(decoded, b"Hello World");
    }
}
