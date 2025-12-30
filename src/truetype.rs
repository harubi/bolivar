//! TrueType font parsing for Unicode extraction.
//!
//! Parses the cmap table from embedded TrueType fonts to create
//! a mapping from glyph IDs (GIDs) to Unicode codepoints.
//!
//! Port of pdfminer.six TrueTypeFont class.

use crate::cmapdb::UnicodeMap;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

/// Error during TrueType font parsing.
#[derive(Debug)]
pub enum TrueTypeFontError {
    /// No cmap table found in font.
    CMapNotFound,
    /// IO error reading font data.
    IoError(std::io::Error),
    /// Invalid font format.
    InvalidFormat(String),
}

impl From<std::io::Error> for TrueTypeFontError {
    fn from(e: std::io::Error) -> Self {
        TrueTypeFontError::IoError(e)
    }
}

/// TrueType font parser.
///
/// Parses TrueType font files to extract the cmap (character mapping) table,
/// which provides the mapping between Unicode codepoints and glyph IDs.
pub struct TrueTypeFont<R> {
    /// Font data reader.
    reader: R,
    /// Table directory: tag -> (offset, length).
    tables: HashMap<[u8; 4], (u32, u32)>,
}

impl<R: Read + Seek> TrueTypeFont<R> {
    /// Create a new TrueType font parser.
    pub fn new(mut reader: R) -> Result<Self, TrueTypeFontError> {
        // Read font type (sfnt version)
        let mut font_type = [0u8; 4];
        reader.read_exact(&mut font_type)?;

        // Read number of tables
        let mut header = [0u8; 8];
        reader.read_exact(&mut header)?;
        let ntables = u16::from_be_bytes([header[0], header[1]]) as usize;

        // Read table directory
        let mut tables = HashMap::new();
        for _ in 0..ntables {
            let mut entry = [0u8; 16];
            if reader.read_exact(&mut entry).is_err() {
                // Corrupted font, but continue with what we have
                break;
            }

            let tag: [u8; 4] = [entry[0], entry[1], entry[2], entry[3]];
            let offset = u32::from_be_bytes([entry[8], entry[9], entry[10], entry[11]]);
            let length = u32::from_be_bytes([entry[12], entry[13], entry[14], entry[15]]);
            tables.insert(tag, (offset, length));
        }

        Ok(Self { reader, tables })
    }

    /// Create a Unicode map from the font's cmap table.
    ///
    /// Parses the cmap table and inverts it to create a mapping from
    /// glyph IDs (GIDs) to Unicode codepoints.
    pub fn create_unicode_map(&mut self) -> Result<UnicodeMap, TrueTypeFontError> {
        let cmap_tag = *b"cmap";
        let (base_offset, _length) = self
            .tables
            .get(&cmap_tag)
            .ok_or(TrueTypeFontError::CMapNotFound)?;
        let base_offset = *base_offset as u64;

        self.reader.seek(SeekFrom::Start(base_offset))?;

        // Read cmap header
        let mut header = [0u8; 4];
        self.reader.read_exact(&mut header)?;
        let _version = u16::from_be_bytes([header[0], header[1]]);
        let nsubtables = u16::from_be_bytes([header[2], header[3]]) as usize;

        // Read subtable records
        let mut subtables = Vec::with_capacity(nsubtables);
        for _ in 0..nsubtables {
            let mut record = [0u8; 8];
            self.reader.read_exact(&mut record)?;
            let platform_id = u16::from_be_bytes([record[0], record[1]]);
            let encoding_id = u16::from_be_bytes([record[2], record[3]]);
            let offset = u32::from_be_bytes([record[4], record[5], record[6], record[7]]);
            subtables.push((platform_id, encoding_id, offset));
        }

        // Parse subtables to build char->gid mapping
        let mut char2gid: HashMap<u32, u32> = HashMap::new();

        for (platform_id, encoding_id, st_offset) in subtables {
            // Skip non-Unicode cmaps
            // Platform 0 = Unicode, Platform 3 with encoding 1 or 10 = Windows Unicode
            if !(platform_id == 0 || (platform_id == 3 && (encoding_id == 1 || encoding_id == 10)))
            {
                continue;
            }

            self.reader
                .seek(SeekFrom::Start(base_offset + st_offset as u64))?;

            // Read format header
            let mut fmt_header = [0u8; 6];
            self.reader.read_exact(&mut fmt_header)?;
            let fmttype = u16::from_be_bytes([fmt_header[0], fmt_header[1]]);
            let _fmtlen = u16::from_be_bytes([fmt_header[2], fmt_header[3]]);
            let _fmtlang = u16::from_be_bytes([fmt_header[4], fmt_header[5]]);

            match fmttype {
                0 => {
                    // Format 0: Byte encoding table
                    let mut glyph_ids = [0u8; 256];
                    self.reader.read_exact(&mut glyph_ids)?;
                    for (char_code, &gid) in glyph_ids.iter().enumerate() {
                        char2gid.insert(char_code as u32, gid as u32);
                    }
                }
                2 => {
                    // Format 2: High-byte mapping through table
                    self.parse_format_2(&mut char2gid)?;
                }
                4 => {
                    // Format 4: Segment mapping to delta values
                    self.parse_format_4(&mut char2gid)?;
                }
                _ => {
                    // Skip unsupported formats
                    continue;
                }
            }
        }

        if char2gid.is_empty() {
            return Err(TrueTypeFontError::CMapNotFound);
        }

        // Invert the mapping: gid -> unicode
        let mut unicode_map = UnicodeMap::new();
        for (char_code, gid) in char2gid {
            if let Some(c) = char::from_u32(char_code) {
                unicode_map.add_cid2unichr(gid, c.to_string());
            }
        }

        Ok(unicode_map)
    }

    /// Parse cmap format 2 subtable.
    fn parse_format_2(
        &mut self,
        char2gid: &mut HashMap<u32, u32>,
    ) -> Result<(), TrueTypeFontError> {
        // Read subheader keys (256 * 2 bytes)
        let mut subheader_keys = [0u8; 512];
        self.reader.read_exact(&mut subheader_keys)?;

        let mut firstbytes = [0u8; 8192];
        let mut max_key = 0u16;
        for i in 0..256 {
            let k = u16::from_be_bytes([subheader_keys[i * 2], subheader_keys[i * 2 + 1]]);
            if k > 0 {
                firstbytes[(k / 8) as usize] = i as u8;
                max_key = max_key.max(k);
            }
        }

        let nhdrs = (max_key / 8 + 1) as usize;

        // Read subheaders
        let mut hdrs = Vec::with_capacity(nhdrs);
        let _current_pos = self.reader.stream_position()?;

        for i in 0..nhdrs {
            let mut hdr = [0u8; 8];
            self.reader.read_exact(&mut hdr)?;
            let firstcode = u16::from_be_bytes([hdr[0], hdr[1]]);
            let entcount = u16::from_be_bytes([hdr[2], hdr[3]]);
            let delta = i16::from_be_bytes([hdr[4], hdr[5]]);
            let offset = u16::from_be_bytes([hdr[6], hdr[7]]);
            let pos = self.reader.stream_position()? - 2 + offset as u64;
            hdrs.push((i, firstcode, entcount, delta, pos));
        }

        // Process each subheader
        for (i, firstcode, entcount, delta, pos) in hdrs {
            if entcount == 0 {
                continue;
            }
            let first = firstcode as u32 + ((firstbytes[i] as u32) << 8);
            self.reader.seek(SeekFrom::Start(pos))?;

            for c in 0..entcount {
                let mut gid_bytes = [0u8; 2];
                self.reader.read_exact(&mut gid_bytes)?;
                let mut gid = u16::from_be_bytes(gid_bytes) as i32;
                if gid != 0 {
                    gid = (gid + delta as i32) & 0xFFFF;
                }
                char2gid.insert(first + c as u32, gid as u32);
            }
        }

        Ok(())
    }

    /// Parse cmap format 4 subtable.
    fn parse_format_4(
        &mut self,
        char2gid: &mut HashMap<u32, u32>,
    ) -> Result<(), TrueTypeFontError> {
        // Read segment count and other header fields
        let mut header = [0u8; 8];
        self.reader.read_exact(&mut header)?;
        let segcount = (u16::from_be_bytes([header[0], header[1]]) / 2) as usize;

        // Read end codes
        let mut end_codes = vec![0u8; segcount * 2];
        self.reader.read_exact(&mut end_codes)?;
        let ecs: Vec<u16> = (0..segcount)
            .map(|i| u16::from_be_bytes([end_codes[i * 2], end_codes[i * 2 + 1]]))
            .collect();

        // Skip reserved pad
        let mut _pad = [0u8; 2];
        self.reader.read_exact(&mut _pad)?;

        // Read start codes
        let mut start_codes = vec![0u8; segcount * 2];
        self.reader.read_exact(&mut start_codes)?;
        let scs: Vec<u16> = (0..segcount)
            .map(|i| u16::from_be_bytes([start_codes[i * 2], start_codes[i * 2 + 1]]))
            .collect();

        // Read id deltas (signed)
        let mut id_deltas = vec![0u8; segcount * 2];
        self.reader.read_exact(&mut id_deltas)?;
        let idds: Vec<i16> = (0..segcount)
            .map(|i| i16::from_be_bytes([id_deltas[i * 2], id_deltas[i * 2 + 1]]))
            .collect();

        // Read id range offsets
        let pos = self.reader.stream_position()?;
        let mut id_range_offsets = vec![0u8; segcount * 2];
        self.reader.read_exact(&mut id_range_offsets)?;
        let idrs: Vec<u16> = (0..segcount)
            .map(|i| u16::from_be_bytes([id_range_offsets[i * 2], id_range_offsets[i * 2 + 1]]))
            .collect();

        // Process segments
        for (i, ((&ec, &sc), (&idd, &idr))) in ecs
            .iter()
            .zip(scs.iter())
            .zip(idds.iter().zip(idrs.iter()))
            .enumerate()
        {
            if idr != 0 {
                // Use glyph id array
                let offset_pos = pos + (i * 2) as u64 + idr as u64;
                self.reader.seek(SeekFrom::Start(offset_pos))?;

                for c in sc..=ec {
                    let mut gid_bytes = [0u8; 2];
                    self.reader.read_exact(&mut gid_bytes)?;
                    let b = u16::from_be_bytes(gid_bytes) as i32;
                    let gid = ((b + idd as i32) & 0xFFFF) as u32;
                    char2gid.insert(c as u32, gid);
                }
            } else {
                // Use delta
                for c in sc..=ec {
                    let gid = ((c as i32 + idd as i32) & 0xFFFF) as u32;
                    char2gid.insert(c as u32, gid);
                }
            }
        }

        Ok(())
    }
}

/// Create a Unicode map from TrueType font data.
///
/// Convenience function that wraps TrueTypeFont parsing.
pub fn create_unicode_map_from_ttf(data: &[u8]) -> Result<UnicodeMap, TrueTypeFontError> {
    let cursor = std::io::Cursor::new(data);
    let mut ttf = TrueTypeFont::new(cursor)?;
    ttf.create_unicode_map()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_unicode_map_empty() {
        // Empty data should fail
        let result = create_unicode_map_from_ttf(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_unicode_map_invalid() {
        // Invalid data should fail
        let result = create_unicode_map_from_ttf(&[0u8; 100]);
        assert!(result.is_err());
    }
}
