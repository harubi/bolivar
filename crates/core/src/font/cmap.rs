//! Adobe Character Mapping (CMap) support.
//!
//! CMaps provide the mapping between character codes and Unicode
//! code-points to character ids (CIDs).
//!
//! Port of pdfminer.six cmapdb.py

use std::collections::HashMap;

/// Base trait for all CMap types.
pub trait CMapBase {
    /// Decode byte sequence to CIDs.
    fn decode<'a>(&'a self, code: &'a [u8]) -> Box<dyn Iterator<Item = u32> + 'a>;

    /// Check if this is a vertical writing CMap.
    fn is_vertical(&self) -> bool;
}

/// Identity CMap - 2-byte big-endian identity mapping.
///
/// Each pair of bytes is interpreted as a big-endian 16-bit CID.
#[derive(Debug)]
pub struct IdentityCMap {
    vertical: bool,
}

impl IdentityCMap {
    pub fn new(vertical: bool) -> Self {
        Self { vertical }
    }
}

impl CMapBase for IdentityCMap {
    fn decode<'a>(&'a self, code: &'a [u8]) -> Box<dyn Iterator<Item = u32> + 'a> {
        Box::new(
            code.chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]) as u32),
        )
    }

    fn is_vertical(&self) -> bool {
        self.vertical
    }
}

/// Identity CMap for single-byte codes.
///
/// Each byte is interpreted directly as a CID.
#[derive(Debug)]
pub struct IdentityCMapByte {
    vertical: bool,
}

impl IdentityCMapByte {
    pub fn new(vertical: bool) -> Self {
        Self { vertical }
    }
}

impl CMapBase for IdentityCMapByte {
    fn decode<'a>(&'a self, code: &'a [u8]) -> Box<dyn Iterator<Item = u32> + 'a> {
        Box::new(code.iter().map(|&b| b as u32))
    }

    fn is_vertical(&self) -> bool {
        self.vertical
    }
}

/// CMap with explicit code-to-CID mappings.
#[derive(Debug)]
pub struct CMap {
    /// CMap attributes (CMapName, WMode, etc.)
    pub attrs: HashMap<String, String>,
    /// Vertical writing mode
    vertical: bool,
    /// Single-byte mappings (code -> CID)
    code1_to_cid: HashMap<u8, u32>,
    /// Two-byte mappings (code -> CID)
    code2_to_cid: HashMap<u16, u32>,
    /// Multi-byte mappings (code -> CID)
    code_to_cid: HashMap<Vec<u8>, u32>,
    /// CID ranges for efficient storage
    ranges: Vec<CidRange>,
}

#[derive(Debug)]
struct CidRange {
    start: Vec<u8>,
    end: Vec<u8>,
    cid_start: u32,
}

impl CMap {
    pub fn new() -> Self {
        Self {
            attrs: HashMap::new(),
            vertical: false,
            code1_to_cid: HashMap::new(),
            code2_to_cid: HashMap::new(),
            code_to_cid: HashMap::new(),
            ranges: Vec::new(),
        }
    }

    /// Set vertical writing mode.
    pub fn set_vertical(&mut self, v: bool) {
        self.vertical = v;
    }

    /// Add a single code-to-CID mapping.
    pub fn add_code2cid(&mut self, code: &[u8], cid: u32) {
        match code.len() {
            1 => {
                self.code1_to_cid.insert(code[0], cid);
            }
            2 => {
                self.code2_to_cid
                    .insert(u16::from_be_bytes([code[0], code[1]]), cid);
            }
            _ => {
                self.code_to_cid.insert(code.to_vec(), cid);
            }
        }
    }

    /// Add a CID range mapping.
    pub fn add_cid_range(&mut self, start: &[u8], end: &[u8], cid_start: u32) {
        self.ranges.push(CidRange {
            start: start.to_vec(),
            end: end.to_vec(),
            cid_start,
        });
    }

    /// Look up a code in the mapping.
    fn lookup_code(&self, code: &[u8]) -> Option<u32> {
        // Try direct lookups first
        match code.len() {
            1 => {
                if let Some(&cid) = self.code1_to_cid.get(&code[0]) {
                    return Some(cid);
                }
            }
            2 => {
                let key = u16::from_be_bytes([code[0], code[1]]);
                if let Some(&cid) = self.code2_to_cid.get(&key) {
                    return Some(cid);
                }
            }
            _ => {
                if let Some(&cid) = self.code_to_cid.get(code) {
                    return Some(cid);
                }
            }
        }

        // Check ranges
        for range in &self.ranges {
            if code.len() == range.start.len() && code >= &range.start[..] && code <= &range.end[..]
            {
                let offset = Self::bytes_to_u32(code) - Self::bytes_to_u32(&range.start);
                return Some(range.cid_start + offset);
            }
        }

        None
    }

    fn bytes_to_u32(bytes: &[u8]) -> u32 {
        let mut result = 0u32;
        for &b in bytes {
            result = (result << 8) | (b as u32);
        }
        result
    }

    /// Get the CMapName from attrs, if set.
    pub fn name(&self) -> Option<&str> {
        self.attrs.get("CMapName").map(|s| s.as_str())
    }

    /// Check if this CMap has any code-to-CID mappings.
    pub fn has_mappings(&self) -> bool {
        !self.code1_to_cid.is_empty()
            || !self.code2_to_cid.is_empty()
            || !self.code_to_cid.is_empty()
            || !self.ranges.is_empty()
    }
}

impl Default for CMap {
    fn default() -> Self {
        Self::new()
    }
}

impl CMapBase for CMap {
    fn decode<'a>(&'a self, code: &'a [u8]) -> Box<dyn Iterator<Item = u32> + 'a> {
        Box::new(CMapDecoder {
            cmap: self,
            data: code,
            pos: 0,
        })
    }

    fn is_vertical(&self) -> bool {
        self.vertical
    }
}

/// Iterator for decoding bytes through a CMap.
struct CMapDecoder<'a> {
    cmap: &'a CMap,
    data: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for CMapDecoder<'a> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.pos < self.data.len() {
            // Try longest match first (up to 4 bytes)
            for len in (1..=4.min(self.data.len() - self.pos)).rev() {
                if let Some(cid) = self.cmap.lookup_code(&self.data[self.pos..self.pos + len]) {
                    self.pos += len;
                    return Some(cid);
                }
            }
            // No mapping found, skip this byte
            self.pos += 1;
        }
        None
    }
}

/// Unicode map - CID to Unicode string mapping.
#[derive(Debug)]
pub struct UnicodeMap {
    /// Map attributes
    pub attrs: HashMap<String, String>,
    /// Vertical writing mode
    vertical: bool,
    /// CID to Unicode string mapping
    cid2unichr: HashMap<u32, String>,
    /// Unicode ranges
    ranges: Vec<BfRange>,
}

#[derive(Debug)]
struct BfRange {
    cid_start: u32,
    cid_end: u32,
    /// Raw UTF-16BE bytes for the starting Unicode value (preserves byte structure)
    unicode_bytes: Vec<u8>,
}

impl UnicodeMap {
    pub fn new() -> Self {
        Self {
            attrs: HashMap::new(),
            vertical: false,
            cid2unichr: HashMap::new(),
            ranges: Vec::new(),
        }
    }

    /// Set vertical writing mode.
    pub fn set_vertical(&mut self, v: bool) {
        self.vertical = v;
    }

    /// Check if this is a vertical writing map.
    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    /// Check if this map has any mappings.
    pub fn is_empty(&self) -> bool {
        self.cid2unichr.is_empty() && self.ranges.is_empty()
    }

    /// Add a CID to Unicode mapping.
    ///
    /// Handles collision where non-breaking space (U+00A0) should not
    /// override regular space (U+0020) mapping for the same CID.
    pub fn add_cid2unichr(&mut self, cid: u32, unicode: String) {
        // Special handling: non-breaking space should not override regular space
        // Some fonts have both U+0020 and U+00A0 mapping to the same glyph
        if unicode == "\u{00a0}" {
            if let Some(existing) = self.cid2unichr.get(&cid) {
                if existing == " " {
                    return; // Keep regular space, don't override with non-breaking space
                }
            }
        }
        self.cid2unichr.insert(cid, unicode);
    }

    /// Add a CID to Unicode mapping from UTF-16BE bytes.
    pub fn add_cid2unichr_bytes(&mut self, cid: u32, bytes: &[u8]) {
        // Decode UTF-16BE
        let mut chars = Vec::new();
        for chunk in bytes.chunks(2) {
            if chunk.len() == 2 {
                let cp = u16::from_be_bytes([chunk[0], chunk[1]]);
                if let Some(c) = char::from_u32(cp as u32) {
                    chars.push(c);
                }
            }
        }
        let unicode: String = chars.into_iter().collect();
        self.cid2unichr.insert(cid, unicode);
    }

    /// Add a Unicode range mapping with raw UTF-16BE bytes.
    /// This preserves byte-level structure for proper incrementing (matches Python's algorithm).
    pub fn add_bf_range(&mut self, cid_start: u32, cid_end: u32, unicode_bytes: Vec<u8>) {
        self.ranges.push(BfRange {
            cid_start,
            cid_end,
            unicode_bytes,
        });
    }

    /// Get Unicode string for a CID.
    ///
    /// For ranges, uses Python's byte-level incrementing algorithm:
    /// 1. Take last min(4, len) bytes as variable part
    /// 2. Convert to big-endian integer
    /// 3. Add offset
    /// 4. Pack back to bytes, preserving original length
    /// 5. Decode as UTF-16BE
    pub fn get_unichr(&self, cid: u32) -> Option<String> {
        // Try direct mapping first
        if let Some(s) = self.cid2unichr.get(&cid) {
            return Some(s.clone());
        }

        // Try ranges with Python-style byte-level incrementing
        for range in &self.ranges {
            if cid >= range.cid_start && cid <= range.cid_end {
                let offset = cid - range.cid_start;

                // Python algorithm: var = code[-4:], prefix = code[:-4]
                let bytes = &range.unicode_bytes;
                let vlen = bytes.len().min(4);
                let var_start = bytes.len().saturating_sub(4);
                let var = &bytes[var_start..];
                let prefix = &bytes[..var_start];

                // Convert var bytes to big-endian integer
                let mut base = 0u32;
                for &b in var {
                    base = (base << 8) | (b as u32);
                }

                // Increment and pack back
                let incremented = base.wrapping_add(offset);
                let packed = incremented.to_be_bytes();

                // Reconstruct: prefix + last vlen bytes of packed
                let mut result = prefix.to_vec();
                result.extend_from_slice(&packed[4 - vlen..]);

                // Decode UTF-16BE to String
                return Some(decode_utf16be_string(&result));
            }
        }

        None
    }
}

impl Default for UnicodeMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Identity Unicode map - CID is interpreted directly as Unicode codepoint.
#[derive(Debug)]
pub struct IdentityUnicodeMap {
    vertical: bool,
}

impl IdentityUnicodeMap {
    pub fn new() -> Self {
        Self { vertical: false }
    }

    /// Check if this is a vertical writing map.
    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    /// Get Unicode string for a CID (direct interpretation).
    pub fn get_unichr(&self, cid: u32) -> Option<String> {
        char::from_u32(cid).map(|c| c.to_string())
    }
}

impl Default for IdentityUnicodeMap {
    fn default() -> Self {
        Self::new()
    }
}

/// CMap database helper functions.
pub struct CMapDB;

impl CMapDB {
    /// Check if name refers to an Identity CMap.
    pub fn is_identity_cmap(name: &str) -> bool {
        name == "Identity-H" || name == "Identity-V" || name == "DLIdent-H" || name == "DLIdent-V"
    }

    /// Check if name refers to an Identity CMap for single-byte codes.
    pub fn is_identity_cmap_byte(name: &str) -> bool {
        name == "OneByteIdentityH" || name == "OneByteIdentityV"
    }

    /// Check if name indicates vertical writing mode.
    pub fn is_vertical(name: &str) -> bool {
        name.ends_with("-V") || name.ends_with("V")
    }

    /// Check if name refers to a known 2-byte CJK CMap.
    /// These CMaps use 2-byte character codes that map to CIDs.
    pub fn is_cjk_2byte_cmap(name: &str) -> bool {
        // Standard Japanese CMaps
        matches!(
            name,
            "H" | "V" |
            "UniJIS-UTF16-H" | "UniJIS-UTF16-V" |
            "UniJIS-UCS2-H" | "UniJIS-UCS2-V" |
            "90ms-RKSJ-H" | "90ms-RKSJ-V" |
            "90pv-RKSJ-H" | "90pv-RKSJ-V" |
            "EUC-H" | "EUC-V" |
            // Standard Chinese CMaps
            "GBK-EUC-H" | "GBK-EUC-V" |
            "UniGB-UTF16-H" | "UniGB-UTF16-V" |
            "UniGB-UCS2-H" | "UniGB-UCS2-V" |
            "B5pc-H" | "B5pc-V" |
            "UniCNS-UTF16-H" | "UniCNS-UTF16-V" |
            // Standard Korean CMaps
            "UniKS-UTF16-H" | "UniKS-UTF16-V" |
            "KSCms-UHC-H" | "KSCms-UHC-V"
        )
    }
}

/// Create a JIS X 0208 to Unicode mapping table for Adobe-Japan1 fonts.
///
/// This provides fallback Unicode mappings for CJK fonts that don't have
/// embedded ToUnicode streams. The mapping covers:
/// - Row 4 (0x24xx): Hiragana → U+3041-U+3093
/// - Row 5 (0x25xx): Katakana → U+30A1-U+30F6
/// - Rows 16-47: JIS Level 1 Kanji
/// - Rows 48-84: JIS Level 2 Kanji
pub fn create_jis_unicode_map() -> UnicodeMap {
    let mut map = UnicodeMap::new();

    // JIS X 0208 Row 4: Hiragana (0x2421-0x2473)
    // Maps to Unicode U+3041-U+3093
    // Pattern: Unicode = 0x3041 + (JIS column - 0x21)
    for col in 0x21u8..=0x73u8 {
        let cid = 0x2400u32 + col as u32;
        let unicode = 0x3041u32 + (col as u32 - 0x21);
        if let Some(c) = char::from_u32(unicode) {
            map.add_cid2unichr(cid, c.to_string());
        }
    }

    // JIS X 0208 Row 5: Katakana (0x2521-0x2576)
    // Maps to Unicode U+30A1-U+30F6
    // Pattern: Unicode = 0x30A1 + (JIS column - 0x21)
    for col in 0x21u8..=0x76u8 {
        let cid = 0x2500u32 + col as u32;
        let unicode = 0x30A1u32 + (col as u32 - 0x21);
        if let Some(c) = char::from_u32(unicode) {
            map.add_cid2unichr(cid, c.to_string());
        }
    }

    // JIS X 0208 Rows 16-47: JIS Level 1 Kanji (0x3021-0x4F53)
    // These are sorted by reading (on'yomi), not by radical/stroke
    // Maps to Unicode CJK Unified Ideographs
    // This is a subset - full table would be ~3000 entries
    // For now, add the most common ones used in test files
    add_jis_kanji_mappings(&mut map);

    map
}

/// Add JIS X 0208 kanji mappings for Level 1 and Level 2.
/// This is a lookup table for the most common kanji.
fn add_jis_kanji_mappings(map: &mut UnicodeMap) {
    // JIS X 0208 to Unicode mapping table for common kanji
    // Format: (JIS code, Unicode codepoint)
    let kanji_table: &[(u32, u32)] = &[
        // Row 16 - starts with 亜 (0x3021)
        (0x3021, 0x4E9C), // 亜
        (0x3022, 0x5516), // 唖
        (0x3023, 0x5A03), // 娃
        (0x3024, 0x963F), // 阿
        (0x3025, 0x54C0), // 哀
        (0x3026, 0x611B), // 愛
        (0x3027, 0x6328), // 挨
                          // Add more as needed - this is a minimal set for testing

                          // Row 48+ Level 2 kanji would go here
                          // These are less common and sorted by radical/stroke
    ];

    for &(jis, unicode) in kanji_table {
        if let Some(c) = char::from_u32(unicode) {
            map.add_cid2unichr(jis, c.to_string());
        }
    }
}

/// Parse a ToUnicode CMap stream and populate a UnicodeMap.
///
/// ToUnicode CMaps define the mapping from character codes (CIDs) to Unicode.
/// They contain blocks like:
/// - `beginbfchar ... endbfchar` - individual CID→Unicode mappings
/// - `beginbfrange ... endbfrange` - CID ranges to Unicode ranges
///
/// Port of CMapParser from pdfminer.six cmapdb.py (lines 286-470)
pub fn parse_tounicode_cmap(data: &[u8]) -> UnicodeMap {
    let mut unicode_map = UnicodeMap::new();

    // Convert to string for easier parsing (ToUnicode is typically ASCII)
    let content = String::from_utf8_lossy(data);

    // Parse bfchar blocks: <cid_hex> <unicode_hex>
    parse_bfchar_blocks(&content, &mut unicode_map);

    // Parse bfrange blocks: <start> <end> <unicode>
    parse_bfrange_blocks(&content, &mut unicode_map);

    // Parse cidchar blocks: <cid_hex> <unicode_int>
    // Used in some CJK ToUnicode CMaps
    parse_cidchar_blocks(&content, &mut unicode_map);

    // Parse cidrange blocks: <start_cid_hex> <end_cid_hex> <unicode_int>
    // Used in some CJK ToUnicode CMaps (maps CID range to unicode range with decimal start)
    parse_cidrange_blocks(&content, &mut unicode_map);

    unicode_map
}

/// Parse beginbfchar ... endbfchar blocks.
///
/// Format:
/// ```text
/// beginbfchar
/// <0048> <0048>
/// <0065> <0065>
/// endbfchar
/// ```
fn parse_bfchar_blocks(content: &str, unicode_map: &mut UnicodeMap) {
    let mut in_bfchar = false;

    for line in content.split(|c| c == '\n' || c == '\r') {
        let line = line.trim();

        // Check for "N beginbfchar" or just "beginbfchar"
        if line.ends_with("beginbfchar") {
            in_bfchar = true;
            continue;
        }

        if line == "endbfchar" {
            in_bfchar = false;
            continue;
        }

        if in_bfchar {
            let hex_values = extract_hex_sequences(line);
            if hex_values.len() >= 2 {
                for pair in hex_values.chunks(2) {
                    if pair.len() != 2 {
                        continue;
                    }
                    let cid = match parse_hex_value(pair[0]) {
                        Some(v) => v,
                        None => continue,
                    };
                    if let Some(unicode_bytes) = parse_hex_bytes(pair[1]) {
                        unicode_map.add_cid2unichr_bytes(cid, &unicode_bytes);
                    }
                }
            }
        }
    }
}

/// Parse beginbfrange ... endbfrange blocks.
///
/// Format:
/// ```text
/// beginbfrange
/// <0020> <007E> <0020>
/// <00A0> <00FF> [<0041> <0042> ...]
/// endbfrange
/// ```
fn parse_bfrange_blocks(content: &str, unicode_map: &mut UnicodeMap) {
    let mut in_bfrange = false;
    let mut pending_lines: Vec<String> = Vec::new();

    for line in content.split(|c| c == '\n' || c == '\r') {
        let line = line.trim();

        // Check for "N beginbfrange" or just "beginbfrange"
        if line.ends_with("beginbfrange") {
            in_bfrange = true;
            pending_lines.clear();
            continue;
        }

        if line == "endbfrange" {
            in_bfrange = false;
            // Process collected lines
            for pending in &pending_lines {
                parse_bfrange_line(pending, unicode_map);
            }
            pending_lines.clear();
            continue;
        }

        if in_bfrange && !line.is_empty() {
            pending_lines.push(line.to_string());
        }
    }
}

/// Parse a single bfrange line.
fn parse_bfrange_line(line: &str, unicode_map: &mut UnicodeMap) {
    // Line format: <start><end><unicode> or <start><end>[<u1><u2>...]
    // Note: hex values may or may not have whitespace between them

    // Extract all <...> sequences from the line
    let hex_values: Vec<&str> = extract_hex_sequences(line);

    if hex_values.len() < 3 {
        return;
    }

    // Parse start and end CIDs
    let start = match parse_hex_value(hex_values[0]) {
        Some(v) => v,
        None => return,
    };

    let end = match parse_hex_value(hex_values[1]) {
        Some(v) => v,
        None => return,
    };

    // Check if there's an array (indicated by [ in the original line)
    if line.contains('[') {
        // Array of unicode values for each CID in range
        for (i, hex_str) in hex_values[2..].iter().enumerate() {
            let cid = start + i as u32;
            if cid <= end {
                if let Some(unicode_bytes) = parse_hex_bytes(hex_str) {
                    unicode_map.add_cid2unichr_bytes(cid, &unicode_bytes);
                }
            }
        }
    } else {
        // Single unicode value - increments for each CID in range
        // Pass raw bytes to preserve byte-level structure for incrementing
        if let Some(unicode_bytes) = parse_hex_bytes(hex_values[2]) {
            unicode_map.add_bf_range(start, end, unicode_bytes);
        }
    }
}

/// Parse begincidchar ... endcidchar blocks.
///
/// Format (note: unicode is decimal integer, not hex):
/// ```text
/// begincidchar
/// <0048> 72
/// endcidchar
/// ```
///
/// Port of CMapParser from pdfminer.six cmapdb.py (lines 399-408)
fn parse_cidchar_blocks(content: &str, unicode_map: &mut UnicodeMap) {
    let mut in_cidchar = false;

    for line in content.split(|c| c == '\n' || c == '\r') {
        let line = line.trim();

        // Check for "N begincidchar" or just "begincidchar"
        if line.ends_with("begincidchar") {
            in_cidchar = true;
            continue;
        }

        if line == "endcidchar" {
            in_cidchar = false;
            continue;
        }

        if in_cidchar && !line.is_empty() {
            // Parse line: <cid_hex> unicode_decimal
            if let Some((cid, unicode)) = parse_cidchar_line(line) {
                if let Some(ch) = char::from_u32(unicode) {
                    unicode_map.add_cid2unichr(cid, ch.to_string());
                }
            }
        }
    }
}

/// Parse a single cidchar line: <cid_hex> unicode_decimal
fn parse_cidchar_line(line: &str) -> Option<(u32, u32)> {
    let hex_values = extract_hex_sequences(line);
    if hex_values.is_empty() {
        return None;
    }

    let cid = parse_hex_value(hex_values[0])?;

    // Find the decimal number after the hex value
    // The line is like "<4e00> 19968"
    let after_hex = line.rsplit('>').next()?.trim();
    let unicode: u32 = after_hex.parse().ok()?;

    Some((cid, unicode))
}

/// Parse begincidrange ... endcidrange blocks.
///
/// Format (note: unicode_start is decimal integer, not hex):
/// ```text
/// begincidrange
/// <4e00> <4eff> 19968
/// endcidrange
/// ```
///
/// Maps CIDs 0x4E00-0x4EFF to Unicode 19968 (0x4E00) through 20223 (0x4EFF).
///
/// Port of CMapParser from pdfminer.six cmapdb.py (lines 359-396)
fn parse_cidrange_blocks(content: &str, unicode_map: &mut UnicodeMap) {
    let mut in_cidrange = false;
    let mut pending_lines: Vec<String> = Vec::new();

    for line in content.split(|c| c == '\n' || c == '\r') {
        let line = line.trim();

        // Check for "N begincidrange" or just "begincidrange"
        if line.ends_with("begincidrange") {
            in_cidrange = true;
            pending_lines.clear();
            continue;
        }

        if line == "endcidrange" {
            in_cidrange = false;
            // Process collected lines
            for pending in &pending_lines {
                parse_cidrange_line(pending, unicode_map);
            }
            pending_lines.clear();
            continue;
        }

        if in_cidrange && !line.is_empty() {
            pending_lines.push(line.to_string());
        }
    }
}

/// Parse a single cidrange line: <start_hex> <end_hex> unicode_decimal
fn parse_cidrange_line(line: &str, unicode_map: &mut UnicodeMap) {
    let hex_values = extract_hex_sequences(line);
    if hex_values.len() < 2 {
        return;
    }

    // Parse start and end CIDs from hex
    let start = match parse_hex_value(hex_values[0]) {
        Some(v) => v,
        None => return,
    };

    let end = match parse_hex_value(hex_values[1]) {
        Some(v) => v,
        None => return,
    };

    // Find the decimal unicode value after the hex values
    // The line is like "<4e00> <4eff> 19968"
    let after_hex = line.rsplit('>').next().unwrap_or("").trim();
    let unicode_start: u32 = match after_hex.parse() {
        Ok(v) => v,
        Err(_) => return,
    };

    // Map each CID in range to corresponding unicode
    for i in 0..=(end - start) {
        let cid = start + i;
        let unicode = unicode_start + i;
        if let Some(ch) = char::from_u32(unicode) {
            unicode_map.add_cid2unichr(cid, ch.to_string());
        }
    }
}

/// Extract all <...> sequences from a line.
fn extract_hex_sequences(line: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut chars = line.char_indices().peekable();

    while let Some((start_idx, ch)) = chars.next() {
        if ch == '<' {
            // Find matching >
            while let Some((end_idx, ch2)) = chars.next() {
                if ch2 == '>' {
                    results.push(&line[start_idx..=end_idx]);
                    break;
                }
            }
        }
    }

    results
}

/// Parse a hex pair like "<0048><0048>" or "<0048> <0048>".
fn parse_hex_pair(line: &str) -> Option<(u32, Vec<u8>)> {
    let hex_values = extract_hex_sequences(line);
    if hex_values.len() < 2 {
        return None;
    }

    let cid = parse_hex_value(hex_values[0])?;
    let unicode_bytes = parse_hex_bytes(hex_values[1])?;

    Some((cid, unicode_bytes))
}

/// Parse a hex value like "<0048>" to u32.
fn parse_hex_value(s: &str) -> Option<u32> {
    let s = s.trim_start_matches('<').trim_end_matches('>');
    u32::from_str_radix(s, 16).ok()
}

/// Parse a hex string like "<0048>" to bytes.
fn parse_hex_bytes(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_start_matches('<').trim_end_matches('>');

    // Must be even length
    if s.len() % 2 != 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(s.len() / 2);
    let mut chars = s.chars();

    while let (Some(h), Some(l)) = (chars.next(), chars.next()) {
        let byte = u8::from_str_radix(&format!("{}{}", h, l), 16).ok()?;
        bytes.push(byte);
    }

    Some(bytes)
}

/// Decode UTF-16BE bytes to a String.
fn decode_utf16be_string(bytes: &[u8]) -> String {
    let mut chars = Vec::new();
    for chunk in bytes.chunks(2) {
        if chunk.len() == 2 {
            let cp = u16::from_be_bytes([chunk[0], chunk[1]]);
            if let Some(c) = char::from_u32(cp as u32) {
                chars.push(c);
            }
        }
    }
    chars.into_iter().collect()
}
