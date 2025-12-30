//! RunLength stream decoder.
//!
//! Port of pdfminer.six runlength.py

use crate::error::Result;

/// Decode RunLength-encoded data.
///
/// Format:
/// - Length byte 0-127: Copy next (length + 1) bytes literally
/// - Length byte 128: End of data (EOD marker)
/// - Length byte 129-255: Repeat next byte (257 - length) times
///
/// # Lenient Handling
///
/// Matches pdfminer.six behavior: truncated/malformed input is tolerated.
/// If the stream ends mid-sequence (not enough bytes for literal run or
/// missing repeat byte), decoding stops gracefully without error.
pub fn rldecode(data: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < data.len() {
        let length = data[i];
        i += 1;

        match length {
            128 => break, // EOD
            0..=127 => {
                // Copy next (length + 1) bytes literally
                let count = length as usize + 1;
                if i + count <= data.len() {
                    result.extend_from_slice(&data[i..i + count]);
                    i += count;
                }
            }
            129..=255 => {
                // Repeat next byte (257 - length) times
                if i < data.len() {
                    let count = 257 - length as usize;
                    let byte = data[i];
                    i += 1;
                    result.extend(std::iter::repeat_n(byte, count));
                }
            }
        }
    }

    Ok(result)
}
