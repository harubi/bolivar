//! Encoding database for Adobe glyph name to Unicode conversion.
//!
//! Follows Adobe Glyph List Specification:
//! https://github.com/adobe-type-tools/agl-specification#2-the-mapping

use crate::error::{PdfError, Result};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Adobe Glyph List data embedded at compile time
const GLYPHLIST_DATA: &str = include_str!("glyphlist.txt");

/// Lazily initialized glyph name to Unicode character map
static GLYPH_TO_CHAR: LazyLock<HashMap<&'static str, char>> = LazyLock::new(|| {
    let mut map = HashMap::with_capacity(4500);
    for line in GLYPHLIST_DATA.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, hex)) = line.split_once(';')
            && let Ok(code) = u32::from_str_radix(hex.trim(), 16)
            && let Some(ch) = char::from_u32(code)
        {
            map.insert(name, ch);
        }
    }
    map
});

/// Return the glyph name to Unicode map.
pub fn glyphname2unicode() -> &'static HashMap<&'static str, char> {
    &GLYPH_TO_CHAR
}

/// Check if a string contains only hexadecimal characters (case-insensitive)
fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Check if a Unicode codepoint is in the surrogate range (invalid for uni format)
fn is_surrogate(cp: u32) -> bool {
    (0xD800..=0xDFFF).contains(&cp)
}

/// Decode a single component of a glyph name
fn decode_component(name: &str) -> Result<String> {
    let lower = name.to_lowercase();

    // Try "uni" + hex format (4*N hex digits)
    if lower.starts_with("uni") && name.len() >= 7 {
        let hex = &name[3..];
        if hex.len().is_multiple_of(4) && is_hex(hex) {
            let mut result = String::new();
            for chunk in hex.as_bytes().chunks(4) {
                let hex_str = std::str::from_utf8(chunk).unwrap();
                let cp = u32::from_str_radix(hex_str, 16)
                    .map_err(|_| PdfError::UnknownGlyph(name.to_string()))?;
                if is_surrogate(cp) {
                    return Err(PdfError::UnknownGlyph(name.to_string()));
                }
                let ch = char::from_u32(cp).ok_or(PdfError::InvalidUnicode(cp))?;
                result.push(ch);
            }
            return Ok(result);
        }
    }

    // Try "u" + hex format (4-6 digits)
    if lower.starts_with('u') && !lower.starts_with("uni") {
        let hex = &name[1..];
        if (4..=6).contains(&hex.len()) && is_hex(hex) {
            let cp = u32::from_str_radix(hex, 16)
                .map_err(|_| PdfError::UnknownGlyph(name.to_string()))?;
            if cp > 0x10FFFF {
                return Err(PdfError::InvalidUnicode(cp));
            }
            let ch = char::from_u32(cp).ok_or(PdfError::InvalidUnicode(cp))?;
            return Ok(ch.to_string());
        }
    }

    // AGL lookup
    if let Some(&ch) = GLYPH_TO_CHAR.get(name) {
        return Ok(ch.to_string());
    }

    Err(PdfError::UnknownGlyph(name.to_string()))
}

/// Convert Adobe glyph name to Unicode string.
///
/// Follows Adobe Glyph Naming convention:
/// 1. Strip suffix after "."
/// 2. Split by "_" for composite glyphs
/// 3. For each component:
///    - "uni" + 4*N hex digits → UTF-16 code units (surrogates rejected)
///    - "u" + 4-6 hex digits → single code point
///    - Otherwise → AGL lookup
///
/// # Errors
///
/// Returns `PdfError::UnknownGlyph` if the glyph name cannot be resolved.
pub fn name2unicode(name: &str) -> Result<String> {
    // Strip suffix after "."
    let name = match name.find('.') {
        Some(idx) => &name[..idx],
        None => name,
    };

    // Handle empty name or ".notdef" (which becomes empty after stripping)
    if name.is_empty() || name == "notdef" {
        return Err(PdfError::UnknownGlyph(name.to_string()));
    }

    // Split by "_" for composite glyphs
    let parts: Vec<&str> = name.split('_').collect();
    let mut result = String::new();

    for part in parts {
        let decoded = decode_component(part)?;
        result.push_str(&decoded);
    }

    Ok(result)
}

/// Standard PDF encoding databases
pub struct EncodingDB;

impl EncodingDB {
    /// Get an encoding dictionary, optionally modified by differences array.
    ///
    /// # Arguments
    ///
    /// * `name` - Encoding name: "StandardEncoding", "MacRomanEncoding",
    ///   "WinAnsiEncoding", or "PDFDocEncoding"
    /// * `differences` - Optional differences array where each entry is either:
    ///   - A code position (u8)
    ///   - A glyph name (String) to place at current position
    ///
    /// Invalid differences are silently ignored per PDF spec.
    pub fn get_encoding(name: &str, differences: Option<&[DiffEntry]>) -> HashMap<u8, String> {
        use super::latin_enc::ENCODING;

        let mut encoding = HashMap::with_capacity(256);

        // Determine which column to use based on encoding name
        let col_idx = match name {
            "StandardEncoding" => 0,
            "MacRomanEncoding" => 1,
            "WinAnsiEncoding" => 2,
            "PDFDocEncoding" => 3,
            _ => 0, // Default to StandardEncoding
        };

        // Build base encoding from latin_enc table
        for &(glyph_name, std, mac, win, pdf) in ENCODING {
            let code = match col_idx {
                0 => std,
                1 => mac,
                2 => win,
                3 => pdf,
                _ => std,
            };

            if let Some(code) = code {
                // Convert glyph name to Unicode using name2unicode
                if let Ok(unicode_str) = name2unicode(glyph_name) {
                    encoding.insert(code, unicode_str);
                }
            }
        }

        // Apply differences if provided
        if let Some(diffs) = differences {
            let mut current_code: Option<u8> = None;

            for entry in diffs {
                match entry {
                    DiffEntry::Code(code) => {
                        current_code = Some(*code);
                    }
                    DiffEntry::Name(glyph_name) => {
                        if let Some(code) = current_code {
                            // Convert glyph name to Unicode
                            if let Ok(unicode_str) = name2unicode(glyph_name) {
                                encoding.insert(code, unicode_str);
                            }
                            // Advance to next code position
                            current_code = code.checked_add(1);
                        }
                        // If no preceding code, silently ignore per PDF spec
                    }
                }
            }
        }

        encoding
    }
}

/// Entry in a Differences array
#[derive(Debug, Clone)]
pub enum DiffEntry {
    /// A code position
    Code(u8),
    /// A glyph name
    Name(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_list_loaded() {
        // Verify the glyph list was loaded
        assert!(GLYPH_TO_CHAR.len() > 4000);
        assert_eq!(GLYPH_TO_CHAR.get("A"), Some(&'A'));
    }
}
