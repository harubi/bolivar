//! PDF Font handling - CIDFont and Type1/TrueType fonts.
//!
//! Port of pdfminer.six pdffont.py

use super::cmap::{
    CMap, CMapBase, CMapDB, IdentityCMap, IdentityCMapByte, IdentityUnicodeMap, UnicodeMap,
    parse_tounicode_cmap,
};
use super::truetype::create_unicode_map_from_ttf;
use crate::pdftypes::{PDFObjRef, PDFObject};
use std::collections::HashMap;

/// Type alias for font width dictionaries.
/// Maps CID (u32) to optional width (f64).
/// None represents an invalid/missing width entry.
pub type FontWidthDict = HashMap<u32, Option<f64>>;

/// Character displacement for vertical fonts.
/// For horizontal fonts: (width, 0.0)
/// For vertical fonts: (vx, vy) where vx can be None (use fontsize * 0.5)
#[derive(Debug, Clone, Copy)]
pub enum CharDisp {
    /// Horizontal displacement (width only)
    Horizontal(f64),
    /// Vertical displacement (vx, vy) - vx can be None meaning use fontsize * 0.5
    Vertical(Option<f64>, f64),
}

/// PDF Font trait - base interface for all font types.
pub trait PDFFont {
    /// Convert a CID to Unicode character(s).
    fn to_unichr(&self, cid: u32) -> Option<String>;

    /// Get the width of a character by CID.
    ///
    /// Looks up width in widths map, falls back to default_width if not found.
    /// Returns width / 1000.0 (per PDF spec, widths are in 1/1000 em units).
    fn char_width(&self, cid: u32) -> f64;

    /// Get the default width for this font.
    fn default_width(&self) -> f64;

    /// Get the widths map for this font.
    fn widths(&self) -> &FontWidthDict;

    /// Get the font name (BaseFont) if available.
    fn fontname(&self) -> Option<&str> {
        None
    }

    /// Horizontal scale factor (default 0.001).
    fn hscale(&self) -> f64 {
        0.001
    }

    /// Check if this is a vertical writing font.
    fn is_vertical(&self) -> bool {
        false
    }

    /// Get character displacement.
    /// For horizontal fonts: returns Horizontal(width)
    /// For vertical fonts: returns Vertical(vx, vy)
    fn char_disp(&self, cid: u32) -> CharDisp {
        CharDisp::Horizontal(self.char_width(cid))
    }

    /// Get font descent (negative value below baseline).
    fn get_descent(&self) -> f64 {
        -0.25 // Default: 25% of em below baseline
    }
}

/// Mock PDF Font for testing.
///
/// A simple font implementation that returns CID as string for to_unichr.
pub struct MockPdfFont {
    descriptor: HashMap<String, PDFObject>,
    widths: FontWidthDict,
    default_width: f64,
}

impl MockPdfFont {
    /// Create a new mock font.
    pub fn new(
        descriptor: HashMap<String, PDFObject>,
        widths: FontWidthDict,
        default_width: f64,
    ) -> Self {
        Self {
            descriptor,
            widths,
            default_width,
        }
    }

    /// Get the descriptor (unused in tests, but matches Python).
    #[allow(dead_code)]
    pub fn descriptor(&self) -> &HashMap<String, PDFObject> {
        &self.descriptor
    }
}

impl PDFFont for MockPdfFont {
    fn to_unichr(&self, cid: u32) -> Option<String> {
        Some(cid.to_string())
    }

    fn char_width(&self, cid: u32) -> f64 {
        // Try to get width by CID
        if let Some(Some(width)) = self.widths.get(&cid) {
            return width * self.hscale();
        }
        // Fall back to default width
        self.default_width * self.hscale()
    }

    fn default_width(&self) -> f64 {
        self.default_width
    }

    fn widths(&self) -> &FontWidthDict {
        &self.widths
    }

    fn fontname(&self) -> Option<&str> {
        self.descriptor
            .get("FontName")
            .and_then(|obj| obj.as_name().ok())
    }
}

/// Parse PDF width array into a CID -> width mapping.
///
/// Handles two formats:
/// - Format 1: `[cid, [w1, w2, w3, ...]]` - consecutive widths starting at cid
/// - Format 2: `[cid_start, cid_end, width]` - range with same width
///
/// The optional resolver function resolves PDFObjRef to their target objects.
pub fn get_widths<F>(seq: &[PDFObject], resolver: Option<&F>) -> HashMap<u32, f64>
where
    F: Fn(&PDFObjRef) -> Option<PDFObject>,
{
    let mut widths: HashMap<u32, f64> = HashMap::new();
    let mut r: Vec<f64> = Vec::new();

    for v in seq {
        // Resolve object references if resolver provided
        let resolved = match v {
            PDFObject::Ref(objref) => resolver.and_then(|f| f(objref)).unwrap_or(PDFObject::Null),
            other => other.clone(),
        };

        match &resolved {
            PDFObject::Array(arr) => {
                if !r.is_empty() {
                    // Format 1: [cid, [w1, w2, w3, ...]]
                    let char1 = r.pop().unwrap() as u32;
                    for (i, w) in arr.iter().enumerate() {
                        if let Some(width) = pdf_object_to_f64(w) {
                            widths.insert(char1 + i as u32, width);
                        }
                    }
                    r.clear();
                }
            }
            _ => {
                if let Some(n) = pdf_object_to_f64(&resolved) {
                    r.push(n);
                    if r.len() == 3 {
                        // Format 2: [cid_start, cid_end, width]
                        let char1 = r[0] as u32;
                        let char2 = r[1] as u32;
                        let w = r[2];
                        for i in char1..=char2 {
                            widths.insert(i, w);
                        }
                        r.clear();
                    }
                }
                // Skip invalid values (matching Python's warning behavior)
            }
        }
    }

    widths
}

/// Convert a PDFObject to f64 if it's a number.
fn pdf_object_to_f64(obj: &PDFObject) -> Option<f64> {
    match obj {
        PDFObject::Int(n) => Some(*n as f64),
        PDFObject::Real(n) => Some(*n),
        _ => None,
    }
}

/// Dynamic CMap holder for runtime type dispatch.
///
/// Wraps different CMap types in an enum to allow type checking
/// at runtime (matching Python's isinstance() behavior).
#[derive(Debug)]
pub enum DynCMap {
    /// Standard CMap with explicit mappings (boxed to reduce enum size)
    CMap(Box<CMap>),
    /// 2-byte identity CMap (Identity-H/V, DLIdent-H/V)
    IdentityCMap(IdentityCMap),
    /// 1-byte identity CMap (OneByteIdentityH/V)
    IdentityCMapByte(IdentityCMapByte),
}

impl DynCMap {
    /// Check if this is a standard CMap.
    pub fn is_cmap(&self) -> bool {
        matches!(self, DynCMap::CMap(_))
    }

    /// Check if this is a 2-byte identity CMap.
    pub fn is_identity_cmap(&self) -> bool {
        matches!(self, DynCMap::IdentityCMap(_))
    }

    /// Check if this is a 1-byte identity CMap.
    pub fn is_identity_cmap_byte(&self) -> bool {
        matches!(self, DynCMap::IdentityCMapByte(_))
    }

    /// Check if this CMap has any mappings.
    ///
    /// For identity CMaps, this always returns true since they map all codes.
    /// For standard CMaps, this checks if any code-to-CID mappings exist.
    pub fn has_mappings(&self) -> bool {
        match self {
            DynCMap::CMap(c) => c.has_mappings(),
            // Identity CMaps always have mappings (they map all valid codes)
            DynCMap::IdentityCMap(_) | DynCMap::IdentityCMapByte(_) => true,
        }
    }

    /// Get the CMap name from attrs, if this is a standard CMap with a name set.
    ///
    /// Returns None for identity CMaps (they don't store attrs).
    pub fn name(&self) -> Option<&str> {
        match self {
            DynCMap::CMap(c) => c.name(),
            DynCMap::IdentityCMap(_) | DynCMap::IdentityCMapByte(_) => None,
        }
    }
}

/// Unicode map holder for runtime type dispatch.
#[derive(Debug)]
pub enum DynUnicodeMap {
    /// Standard UnicodeMap with explicit CID->Unicode mappings
    UnicodeMap(Box<UnicodeMap>),
    /// Identity UnicodeMap where CID is interpreted as Unicode codepoint
    IdentityUnicodeMap(IdentityUnicodeMap),
}

impl DynUnicodeMap {
    /// Get Unicode string for a CID.
    pub fn get_unichr(&self, cid: u32) -> Option<String> {
        match self {
            DynUnicodeMap::UnicodeMap(map) => map.get_unichr(cid),
            DynUnicodeMap::IdentityUnicodeMap(map) => map.get_unichr(cid),
        }
    }
}

/// PDF CID Font.
///
/// Handles CID-keyed fonts with character code to CID mapping via CMap.
#[derive(Debug)]
pub struct PDFCIDFont {
    /// The CMap for this font
    pub cmap: DynCMap,
    /// Unicode map for CID to Unicode conversion (from ToUnicode stream)
    pub unicode_map: Option<DynUnicodeMap>,
    /// Simple encoding map for simple fonts (byte code → Unicode string)
    pub cid2unicode: Option<HashMap<u8, String>>,
    /// Whether this is a vertical writing font
    pub vertical: bool,
    /// Default width for characters
    pub default_width: f64,
    /// Width map: CID -> width
    pub widths: FontWidthDict,
    /// Displacement map for vertical fonts: CID -> (vx, vy)
    pub disps: HashMap<u32, (Option<f64>, f64)>,
    /// Default displacement for vertical fonts: (vx, vy) where vx can be None
    pub default_disp: (Option<f64>, f64),
    /// Font descent (from FontDescriptor)
    pub descent: f64,
    /// Base font name (for Standard 14 font metric lookup)
    pub basefont: Option<String>,
    /// Font name from FontDescriptor (preferred for pdfminer compatibility)
    pub fontname: Option<String>,
}

impl PDFCIDFont {
    /// Create a new PDFCIDFont from spec dictionary.
    ///
    /// # Arguments
    ///
    /// * `spec` - Font specification dictionary
    /// * `tounicode_data` - Optional decoded ToUnicode stream data
    pub fn new(spec: &HashMap<String, PDFObject>, tounicode_data: Option<&[u8]>) -> Self {
        Self::new_with_ttf(spec, tounicode_data, None, false, None)
    }

    /// Create a new PDFCIDFont from spec dictionary with optional TrueType font data.
    ///
    /// # Arguments
    ///
    /// * `spec` - Font specification dictionary
    /// * `tounicode_data` - Optional decoded ToUnicode stream data
    /// * `ttf_data` - Optional decoded TrueType font file data (FontFile2)
    /// * `is_type0` - True if this is a Type0 (CID) font; CID fonts don't use cid2unicode
    pub fn new_with_ttf(
        spec: &HashMap<String, PDFObject>,
        tounicode_data: Option<&[u8]>,
        ttf_data: Option<&[u8]>,
        is_type0: bool,
        fallback_fontname: Option<String>,
    ) -> Self {
        let cmap = Self::get_cmap_from_spec(spec);
        let vertical = match &cmap {
            DynCMap::CMap(c) => c.is_vertical(),
            DynCMap::IdentityCMap(c) => c.is_vertical(),
            DynCMap::IdentityCMapByte(c) => c.is_vertical(),
        };

        // Extract cidcoding from CIDSystemInfo
        let cidcoding = Self::get_cidcoding(spec);

        // Parse ToUnicode CMap if provided as a stream (highest priority)
        // Only use if stream has data AND parsing produces actual mappings
        let unicode_map = tounicode_data.and_then(|data| {
            if data.is_empty() {
                return None;
            }
            let map = parse_tounicode_cmap(data);
            if !map.is_empty() {
                Some(DynUnicodeMap::UnicodeMap(Box::new(map)))
            } else {
                None
            }
        });

        // If ToUnicode is a Name containing "Identity", use IdentityUnicodeMap
        // This takes priority over TrueType cmap extraction
        let unicode_map = unicode_map.or_else(|| {
            if let Some(PDFObject::Name(name)) = spec.get("ToUnicode") {
                if name.contains("Identity") {
                    return Some(DynUnicodeMap::IdentityUnicodeMap(IdentityUnicodeMap::new()));
                }
            }
            None
        });

        // If no ToUnicode, try TrueType cmap for Adobe-Identity/Adobe-UCS fonts
        let unicode_map = unicode_map.or_else(|| {
            if let Some(ref coding) = cidcoding {
                if (coding == "Adobe-Identity" || coding == "Adobe-UCS") && ttf_data.is_some() {
                    if let Some(ttf) = ttf_data {
                        if let Ok(map) = create_unicode_map_from_ttf(ttf) {
                            return Some(DynUnicodeMap::UnicodeMap(Box::new(map)));
                        }
                    }
                }
            }
            None
        });

        // Fallback for Adobe-Japan1 fonts without ToUnicode: use JIS→Unicode mapping
        let unicode_map = unicode_map.or_else(|| {
            if let Some(ref coding) = cidcoding {
                if coding == "Adobe-Japan1" {
                    return Some(DynUnicodeMap::UnicodeMap(Box::new(
                        super::cmap::create_jis_unicode_map(),
                    )));
                }
            }
            None
        });

        // NOTE: Python NEVER uses IdentityUnicodeMap just because Encoding="Identity-H".
        // IdentityUnicodeMap is ONLY used when ToUnicode is a NAME containing "Identity".
        // When there's no ToUnicode (or ToUnicode stream is empty), unicode_map stays None,
        // causing to_unichr() to return None → renders as (cid:X).
        let unicode_map = unicode_map;

        // Build cid2unicode from Encoding for simple fonts only (not Type0/CID fonts)
        // Type0 fonts use CMap for decoding, not Encoding entries
        let cid2unicode = if is_type0 {
            None
        } else {
            Self::build_cid2unicode(spec)
        };

        // Parse widths from spec
        let widths = Self::parse_widths(spec);
        let default_width = Self::get_default_width(spec, vertical);

        // Parse vertical displacement info (DW2 and disps from W2)
        let (default_disp, disps) = Self::parse_vertical_disps(spec, vertical);

        // Parse descent from FontDescriptor
        let descent = Self::get_descent_from_descriptor(spec);

        let obj_to_name = |obj: &PDFObject| -> Option<String> {
            if let Ok(name) = obj.as_name() {
                Some(name.to_string())
            } else if let Ok(bytes) = obj.as_string() {
                String::from_utf8(bytes.to_vec()).ok()
            } else {
                None
            }
        };

        // Parse BaseFont name for Standard 14 font metric lookup
        let basefont = spec.get("BaseFont").and_then(obj_to_name);

        let fontname = spec
            .get("FontDescriptor")
            .and_then(|v| v.as_dict().ok())
            .and_then(|d| d.get("FontName"))
            .and_then(obj_to_name)
            .or_else(|| basefont.clone())
            .or_else(|| fallback_fontname.clone());

        Self {
            cmap,
            unicode_map,
            cid2unicode,
            vertical,
            default_width,
            widths,
            disps,
            default_disp,
            descent,
            basefont,
            fontname,
        }
    }

    /// Parse vertical displacement info from DW2 and W2.
    /// Returns (default_disp, disps_map)
    fn parse_vertical_disps(
        spec: &HashMap<String, PDFObject>,
        is_vertical: bool,
    ) -> ((Option<f64>, f64), HashMap<u32, (Option<f64>, f64)>) {
        if !is_vertical {
            // Horizontal font - use 0 displacement
            return ((None, 0.0), HashMap::new());
        }

        // Parse DW2 - default vertical displacement [vy, w]
        // Default is [880, -1000] per PDF spec
        let (default_vy, _default_w) = match spec.get("DW2") {
            Some(PDFObject::Array(arr)) if arr.len() >= 2 => {
                let vy = arr[0].as_num().ok().unwrap_or(880.0);
                let w = arr[1].as_num().ok().unwrap_or(-1000.0);
                (vy, w)
            }
            _ => (880.0, -1000.0),
        };

        // TODO: Parse W2 array for per-CID vertical displacements
        // For now, use empty map - most fonts use default displacement
        let disps = HashMap::new();

        ((None, default_vy), disps)
    }

    /// Get descent from FontDescriptor.
    fn get_descent_from_descriptor(spec: &HashMap<String, PDFObject>) -> f64 {
        // Try to get FontDescriptor
        if let Some(PDFObject::Dict(desc)) = spec.get("FontDescriptor") {
            if let Some(descent) = desc.get("Descent").and_then(|d| d.as_num().ok()) {
                // Descent is in 1/1000 em units, convert to normalized units
                return descent / 1000.0;
            }
        }
        // Default descent: 25% below baseline
        -0.25
    }

    /// Build cid2unicode map from font's Encoding entry.
    fn build_cid2unicode(spec: &HashMap<String, PDFObject>) -> Option<HashMap<u8, String>> {
        use super::encoding::EncodingDB;

        let encoding_obj = spec.get("Encoding");

        match encoding_obj {
            Some(PDFObject::Name(name)) => {
                // Simple encoding name
                Some(EncodingDB::get_encoding(name, None))
            }
            Some(PDFObject::Dict(dict)) => {
                // Encoding dictionary with BaseEncoding and/or Differences
                let base_encoding = dict
                    .get("BaseEncoding")
                    .and_then(|obj| obj.as_name().ok())
                    .unwrap_or("StandardEncoding");

                let differences = Self::parse_differences(dict.get("Differences"));

                Some(EncodingDB::get_encoding(
                    base_encoding,
                    differences.as_deref(),
                ))
            }
            // If Encoding is missing, default to StandardEncoding for simple fonts
            None => Some(EncodingDB::get_encoding("StandardEncoding", None)),
            _ => None,
        }
    }

    /// Parse Differences array from encoding dictionary.
    fn parse_differences(diff_obj: Option<&PDFObject>) -> Option<Vec<super::encoding::DiffEntry>> {
        use super::encoding::DiffEntry;

        let arr = match diff_obj {
            Some(PDFObject::Array(a)) => a,
            _ => return None,
        };

        let mut result = Vec::with_capacity(arr.len());

        for item in arr {
            match item {
                PDFObject::Int(n) => {
                    if *n >= 0 && *n <= 255 {
                        result.push(DiffEntry::Code(*n as u8));
                    }
                }
                PDFObject::Name(name) => {
                    result.push(DiffEntry::Name(name.clone()));
                }
                _ => {}
            }
        }

        Some(result)
    }

    /// Extract cidcoding (Registry-Ordering) from CIDSystemInfo.
    fn get_cidcoding(spec: &HashMap<String, PDFObject>) -> Option<String> {
        let cid_system_info = spec.get("CIDSystemInfo")?;
        let dict = match cid_system_info {
            PDFObject::Dict(d) => d,
            _ => return None,
        };

        let registry = match dict.get("Registry") {
            Some(PDFObject::String(s)) => String::from_utf8_lossy(s).trim().to_string(),
            _ => return None,
        };

        let ordering = match dict.get("Ordering") {
            Some(PDFObject::String(s)) => String::from_utf8_lossy(s).trim().to_string(),
            _ => return None,
        };

        Some(format!("{}-{}", registry, ordering))
    }

    /// Parse widths from font spec.
    ///
    /// Handles two formats:
    /// - CID fonts: Use "W" array with complex structure
    /// - Simple fonts (Type1, TrueType): Use "Widths" array with FirstChar/LastChar
    fn parse_widths(spec: &HashMap<String, PDFObject>) -> FontWidthDict {
        let mut result = FontWidthDict::new();

        // CID fonts use W array
        if let Some(PDFObject::Array(w_array)) = spec.get("W") {
            let widths = get_widths(w_array, None::<&fn(&PDFObjRef) -> Option<PDFObject>>);
            for (cid, width) in widths {
                result.insert(cid, Some(width));
            }
        }

        // Simple fonts use Widths array with FirstChar/LastChar
        if result.is_empty() {
            if let Some(PDFObject::Array(widths_array)) = spec.get("Widths") {
                let first_char = spec
                    .get("FirstChar")
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(0) as u32;

                for (i, width_obj) in widths_array.iter().enumerate() {
                    if let Ok(width) = width_obj.as_num() {
                        result.insert(first_char + i as u32, Some(width));
                    }
                }
            }
        }

        result
    }

    /// Get default width from spec.
    fn get_default_width(spec: &HashMap<String, PDFObject>, is_vertical: bool) -> f64 {
        if is_vertical {
            // For vertical fonts, use DW2[1] (width component) which defaults to -1000
            // DW2 format is [vy, w] per PDF spec
            match spec.get("DW2") {
                Some(PDFObject::Array(arr)) if arr.len() >= 2 => {
                    arr[1].as_num().ok().unwrap_or(-1000.0)
                }
                _ => -1000.0, // Default per PDF spec
            }
        } else {
            // For horizontal fonts, use DW which defaults to 1000
            spec.get("DW")
                .and_then(|obj| match obj {
                    PDFObject::Int(n) => Some(*n as f64),
                    PDFObject::Real(n) => Some(*n),
                    _ => None,
                })
                .unwrap_or(1000.0)
        }
    }

    /// Extract CMap from font specification.
    ///
    /// Handles three encoding specification formats:
    /// 1. Name (PSLiteral in Python) - e.g., /Identity-H
    /// 2. Stream with CMapName as Name - e.g., stream with /CMapName /Identity-H
    /// 3. Stream with CMapName as String - e.g., stream with /CMapName (Identity-H)
    fn get_cmap_from_spec(spec: &HashMap<String, PDFObject>) -> DynCMap {
        let encoding = spec.get("Encoding");

        // Extract the CMap name from various encoding formats
        let cmap_name = match encoding {
            // Format 1: Direct name (PSLiteral in Python)
            Some(PDFObject::Name(name)) => Some(name.as_str()),

            // Format 2 & 3: Stream with CMapName attribute
            Some(PDFObject::Stream(stream)) => {
                match stream.get("CMapName") {
                    // CMapName as Name (PSLiteral)
                    Some(PDFObject::Name(name)) => Some(name.as_str()),
                    // CMapName as String (bytes)
                    Some(PDFObject::String(bytes)) => std::str::from_utf8(bytes).ok(),
                    _ => None,
                }
            }

            _ => None,
        };

        // Select CMap type based on name
        match cmap_name {
            Some(name) if CMapDB::is_identity_cmap_byte(name) => {
                let vertical = CMapDB::is_vertical(name);
                DynCMap::IdentityCMapByte(IdentityCMapByte::new(vertical))
            }
            Some(name) if CMapDB::is_identity_cmap(name) => {
                let vertical = CMapDB::is_vertical(name);
                DynCMap::IdentityCMap(IdentityCMap::new(vertical))
            }
            Some(name) if CMapDB::is_cjk_2byte_cmap(name) => {
                let mut cmap = CMap::new();
                cmap.set_vertical(CMapDB::is_vertical(name));
                cmap.attrs.insert("CMapName".to_string(), name.to_string());
                cmap.add_cid_range(&[0x00, 0x00], &[0xFF, 0xFF], 0);
                DynCMap::CMap(Box::new(cmap))
            }
            Some(name) => {
                let mut cmap = CMap::new();
                cmap.set_vertical(CMapDB::is_vertical(name));
                cmap.attrs.insert("CMapName".to_string(), name.to_string());
                DynCMap::CMap(Box::new(cmap))
            }
            None => DynCMap::CMap(Box::new(CMap::new())),
        }
    }

    /// Decode bytes to character IDs.
    pub fn decode(&self, data: &[u8]) -> Vec<u32> {
        if self.cid2unicode.is_some() {
            return data.iter().map(|&b| b as u32).collect();
        }

        match &self.cmap {
            DynCMap::CMap(c) => c.decode(data).collect(),
            DynCMap::IdentityCMap(c) => c.decode(data).collect(),
            DynCMap::IdentityCMapByte(c) => c.decode(data).collect(),
        }
    }

    /// Check if this font is for vertical writing.
    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    /// Check if this is a multibyte font.
    pub fn is_multibyte(&self) -> bool {
        true // CID fonts are always multibyte
    }
}

impl PDFFont for PDFCIDFont {
    fn to_unichr(&self, cid: u32) -> Option<String> {
        // Try unicode_map first (from ToUnicode stream)
        if let Some(ref map) = self.unicode_map {
            if let Some(s) = map.get_unichr(cid) {
                return Some(s);
            }
            // If ToUnicode exists but CID not found, fall through to cid2unicode
            // (matches pdfminer.six PDFSimpleFont behavior)
        }

        // Try cid2unicode (from Encoding entry for simple fonts)
        if let Some(ref enc) = self.cid2unicode {
            if cid <= 255 {
                if let Some(s) = enc.get(&(cid as u8)) {
                    return Some(s.clone());
                }
            }
        }

        // No ASCII fallback - matches Python behavior
        // Unmapped CIDs render as (cid:X)
        None
    }

    fn char_width(&self, cid: u32) -> f64 {
        // Try width from PDF (W or Widths array)
        if let Some(Some(width)) = self.widths.get(&cid) {
            return width * self.hscale();
        }

        // Try Standard 14 font metrics as fallback
        if let Some(ref basefont) = self.basefont {
            if let Some(metrics) = super::metrics::get_font_metrics(basefont) {
                // CID for simple fonts is the byte value, which maps to a char
                if let Some(ch) = char::from_u32(cid) {
                    if let Some(&width) = metrics.widths.get(&ch) {
                        return (width as f64) * self.hscale();
                    }
                }
            }
        }

        self.default_width * self.hscale()
    }

    fn default_width(&self) -> f64 {
        self.default_width
    }

    fn widths(&self) -> &FontWidthDict {
        &self.widths
    }

    fn fontname(&self) -> Option<&str> {
        self.fontname
            .as_deref()
            .or_else(|| self.basefont.as_deref())
    }

    fn is_vertical(&self) -> bool {
        self.vertical
    }

    fn char_disp(&self, cid: u32) -> CharDisp {
        if self.vertical {
            // Look up per-CID displacement or use default
            let (vx, vy) = self.disps.get(&cid).copied().unwrap_or(self.default_disp);
            CharDisp::Vertical(vx, vy)
        } else {
            CharDisp::Horizontal(self.char_width(cid))
        }
    }

    fn get_descent(&self) -> f64 {
        self.descent
    }
}
