//! ASCII85 and ASCIIHex stream decoders.
//!
//! Port of pdfminer.six ascii85.py

use crate::error::Result;

/// Decode ASCII85-encoded data (PDF variant).
/// Handles: z-encoding, <~ ~> markers, whitespace, missing EOD.
pub fn ascii85decode(data: &[u8]) -> Result<Vec<u8>> {
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
    for &byte in data {
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' | b'\x00' => continue,
            b'z' => filtered.extend_from_slice(b"!!!!!"), // z = 4 zero bytes
            b'!'..=b'u' => filtered.push(byte),
            _ => continue,
        }
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
    let mut result = Vec::new();
    let mut pending: Option<u8> = None;

    for &byte in data {
        let nibble = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            b'>' => break,
            b' ' | b'\t' | b'\n' | b'\r' => continue,
            _ => continue,
        };

        match pending {
            Some(high) => {
                result.push((high << 4) | nibble);
                pending = None;
            }
            None => pending = Some(nibble),
        }
    }

    if let Some(high) = pending {
        result.push(high << 4);
    }

    Ok(result)
}
