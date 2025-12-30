//! PDF Document - main entry point for PDF parsing.
//!
//! Port of pdfminer.six pdfdocument.py
//!
//! Handles:
//! - XRef table parsing
//! - Object resolution
//! - Trailer parsing (catalog, info)
//! - Page labels

use crate::error::{PdfError, Result};
use crate::pdfparser::PDFParser;
use crate::pdftypes::PDFObject;
use crate::security::{PDFSecurityHandler, create_security_handler};
use std::collections::HashMap;

/// XRef entry - location of an object in the PDF file.
#[derive(Debug, Clone)]
struct XRefEntry {
    /// Byte offset in file (for regular objects) or index in stream (for compressed objects)
    offset: usize,
    /// Generation number
    #[allow(dead_code)]
    genno: u32,
    /// Whether this is in an object stream
    in_stream: bool,
    /// Object stream ID (if in_stream is true)
    stream_objid: Option<u32>,
}

/// Cross-reference table for locating objects in a PDF.
#[derive(Debug, Default)]
struct XRef {
    /// Map from object ID to XRef entry
    offsets: HashMap<u32, XRefEntry>,
    /// Trailer dictionary
    trailer: HashMap<String, PDFObject>,
    /// Whether this xref was loaded via fallback (object scanning)
    is_fallback: bool,
}

impl XRef {
    fn new() -> Self {
        Self::default()
    }

    fn get_pos(&self, objid: u32) -> Option<&XRefEntry> {
        self.offsets.get(&objid)
    }

    fn get_objids(&self) -> impl Iterator<Item = u32> + '_ {
        self.offsets.keys().copied()
    }
}

/// PDF Document - provides access to PDF objects and metadata.
pub struct PDFDocument<'a> {
    /// Raw PDF data
    data: &'a [u8],
    /// Cross-reference tables (may be multiple for incremental updates)
    xrefs: Vec<XRef>,
    /// Document catalog dictionary
    catalog: HashMap<String, PDFObject>,
    /// Document info dictionaries (may be multiple)
    info: Vec<HashMap<String, PDFObject>>,
    /// Cached objects
    cache: HashMap<u32, PDFObject>,
    /// Security handler for encrypted PDFs
    security_handler: Option<Box<dyn PDFSecurityHandler + Send + Sync>>,
}

impl<'a> PDFDocument<'a> {
    /// Create a new PDFDocument from raw PDF data.
    ///
    /// # Arguments
    /// * `data` - The raw PDF file bytes
    /// * `password` - Password for encrypted PDFs (empty string for no password)
    pub fn new(data: &'a [u8], password: &str) -> Result<Self> {
        let mut doc = Self {
            data,
            xrefs: Vec::new(),
            catalog: HashMap::new(),
            info: Vec::new(),
            cache: HashMap::new(),
            security_handler: None,
        };
        doc.parse(password)?;
        Ok(doc)
    }

    /// Parse the PDF document structure.
    fn parse(&mut self, password: &str) -> Result<()> {
        // Find startxref
        let startxref = self.find_startxref();

        // Try loading xrefs, fallback if necessary
        let mut loaded = false;
        if let Ok(pos) = startxref {
            if self.load_xrefs(pos).is_ok() && !self.xrefs.is_empty() {
                loaded = true;
            }
        }

        // Fallback: scan file for objects
        if !loaded {
            let xref = self.load_xref_fallback()?;
            self.xrefs.push(xref);
        }

        // Handle encryption - extract /Encrypt dict and /ID from trailer
        for xref in &self.xrefs {
            if let Some(encrypt_ref) = xref.trailer.get("Encrypt") {
                // Resolve the encrypt dict (it might be an indirect reference)
                let encrypt_obj = self.resolve_internal(encrypt_ref)?;
                let encrypt = encrypt_obj.as_dict()?;

                // Get document ID array from trailer
                let doc_id = if let Some(id_obj) = xref.trailer.get("ID") {
                    if let Ok(id_arr) = id_obj.as_array() {
                        id_arr
                            .iter()
                            .filter_map(|o| o.as_string().ok().map(|s| s.to_vec()))
                            .collect()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };

                // Create the security handler
                self.security_handler = create_security_handler(encrypt, &doc_id, password)?;
                break;
            }
        }

        // Extract catalog and info from trailer
        for xref in &self.xrefs {
            if let Some(root_ref) = xref.trailer.get("Root") {
                if self.catalog.is_empty() {
                    if let Ok(root_obj) = self.resolve_internal(root_ref) {
                        if let Ok(dict) = root_obj.as_dict() {
                            self.catalog = dict.clone();
                        }
                    }
                }
            }
            if let Some(info_ref) = xref.trailer.get("Info") {
                if let Ok(info_obj) = self.resolve_internal(info_ref) {
                    if let Ok(dict) = info_obj.as_dict() {
                        self.info.push(dict.clone());
                    }
                }
            }
        }

        Ok(())
    }

    /// Find the startxref position by scanning backwards from end of file.
    fn find_startxref(&self) -> Result<usize> {
        // Search backwards for "startxref"
        let search = b"startxref";
        let data = self.data;

        // Start from near end of file
        let search_start = if data.len() > 1024 {
            data.len() - 1024
        } else {
            0
        };

        for i in (search_start..data.len() - search.len()).rev() {
            if &data[i..i + search.len()] == search {
                // Found startxref, now find the number after it
                let rest = &data[i + search.len()..];
                // Skip whitespace
                let mut pos = 0;
                while pos < rest.len()
                    && (rest[pos] == b' ' || rest[pos] == b'\n' || rest[pos] == b'\r')
                {
                    pos += 1;
                }
                // Read number
                let mut num_end = pos;
                while num_end < rest.len() && rest[num_end].is_ascii_digit() {
                    num_end += 1;
                }
                if num_end > pos {
                    let num_str = std::str::from_utf8(&rest[pos..num_end])
                        .map_err(|_| PdfError::NoValidXRef)?;
                    return num_str.parse().map_err(|_| PdfError::NoValidXRef);
                }
            }
        }

        Err(PdfError::NoValidXRef)
    }

    /// Load xref tables starting from given position.
    fn load_xrefs(&mut self, mut pos: usize) -> Result<()> {
        let mut visited = std::collections::HashSet::new();

        while !visited.contains(&pos) {
            visited.insert(pos);

            let xref = self.load_xref_at(pos)?;

            // Check for Prev pointer to previous xref
            let prev = xref
                .trailer
                .get("Prev")
                .and_then(|p| p.as_int().ok())
                .map(|n| n as usize);

            self.xrefs.push(xref);

            if let Some(prev_pos) = prev {
                pos = prev_pos;
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Load xref table at given position.
    fn load_xref_at(&self, pos: usize) -> Result<XRef> {
        let data = &self.data[pos..];

        // Check if it starts with "xref" (traditional) or a number (xref stream)
        if data.starts_with(b"xref") {
            self.load_traditional_xref(pos)
        } else {
            // Could be xref stream, try to parse as object
            self.load_xref_stream(pos)
        }
    }

    /// Load traditional xref table.
    fn load_traditional_xref(&self, pos: usize) -> Result<XRef> {
        let mut xref = XRef::new();
        let data = &self.data[pos..];

        // Skip "xref" and whitespace
        let mut cursor = 4;
        while cursor < data.len()
            && (data[cursor] == b' ' || data[cursor] == b'\n' || data[cursor] == b'\r')
        {
            cursor += 1;
        }

        // Parse xref sections until we hit "trailer"
        loop {
            // Skip whitespace
            while cursor < data.len()
                && (data[cursor] == b' ' || data[cursor] == b'\n' || data[cursor] == b'\r')
            {
                cursor += 1;
            }

            if cursor >= data.len() {
                break;
            }

            // Check for trailer
            if data[cursor..].starts_with(b"trailer") {
                cursor += 7;
                break;
            }

            // Read start object ID
            let (start_objid, consumed) = self.read_number(&data[cursor..])?;
            cursor += consumed;

            // Skip whitespace
            while cursor < data.len()
                && (data[cursor] == b' ' || data[cursor] == b'\n' || data[cursor] == b'\r')
            {
                cursor += 1;
            }

            // Read count
            let (count, consumed) = self.read_number(&data[cursor..])?;
            cursor += consumed;

            // Skip to next line
            while cursor < data.len() && data[cursor] != b'\n' && data[cursor] != b'\r' {
                cursor += 1;
            }
            while cursor < data.len() && (data[cursor] == b'\n' || data[cursor] == b'\r') {
                cursor += 1;
            }

            // Read entries
            let mut base_objid = start_objid;
            for i in 0..count {
                // Each entry is exactly 20 bytes: "nnnnnnnnnn ggggg n \n"
                // But may vary, so we parse flexibly
                let (offset, consumed1) = self.read_number(&data[cursor..])?;
                cursor += consumed1;

                // Skip space
                while cursor < data.len() && data[cursor] == b' ' {
                    cursor += 1;
                }

                let (genno, consumed2) = self.read_number(&data[cursor..])?;
                cursor += consumed2;

                // Skip space
                while cursor < data.len() && data[cursor] == b' ' {
                    cursor += 1;
                }

                // Read marker (n = in use, f = free)
                let marker = if cursor < data.len() {
                    data[cursor]
                } else {
                    b'f'
                };
                cursor += 1;

                // Some PDFs incorrectly start a subsection at 1 but still include
                // the object 0 free entry (0000000000 65535 f). Adjust base_objid
                // so the free entry maps to object 0 and subsequent entries align.
                if i == 0 && base_objid > 0 && marker == b'f' && offset == 0 && genno == 65535 {
                    base_objid -= 1;
                }

                let objid = base_objid + i;

                // Skip to end of line
                while cursor < data.len() && data[cursor] != b'\n' && data[cursor] != b'\r' {
                    cursor += 1;
                }
                while cursor < data.len() && (data[cursor] == b'\n' || data[cursor] == b'\r') {
                    cursor += 1;
                }

                if marker == b'n' {
                    xref.offsets.insert(
                        objid as u32,
                        XRefEntry {
                            offset: offset as usize,
                            genno: genno as u32,
                            in_stream: false,
                            stream_objid: None,
                        },
                    );
                }
            }
        }

        // Parse trailer dictionary
        // Skip whitespace
        let data = &self.data[pos + cursor..];
        let mut cursor = 0;
        while cursor < data.len()
            && (data[cursor] == b' ' || data[cursor] == b'\n' || data[cursor] == b'\r')
        {
            cursor += 1;
        }

        // Parse trailer dict
        if data[cursor..].starts_with(b"<<") {
            let mut parser = PDFParser::new(&data[cursor..]);
            if let Ok(trailer_obj) = parser.parse_object() {
                if let Ok(dict) = trailer_obj.as_dict() {
                    xref.trailer = dict.clone();
                }
            }
        }

        Ok(xref)
    }

    /// Load xref stream (PDF 1.5+).
    fn load_xref_stream(&self, pos: usize) -> Result<XRef> {
        // Parse the xref stream object
        let obj = self.parse_object_at(pos, 0, false)?;
        let stream = obj.as_stream()?;

        // Get parameters
        let w = stream
            .get("W")
            .ok_or_else(|| PdfError::SyntaxError("missing W in xref stream".into()))?;
        let w_arr = w.as_array()?;
        if w_arr.len() != 3 {
            return Err(PdfError::SyntaxError("W must have 3 elements".into()));
        }
        let w0 = w_arr[0].as_int()? as usize;
        let w1 = w_arr[1].as_int()? as usize;
        let w2 = w_arr[2].as_int()? as usize;
        let entry_size = w0 + w1 + w2;

        let size = stream
            .get("Size")
            .ok_or_else(|| PdfError::SyntaxError("missing Size in xref stream".into()))?
            .as_int()? as usize;

        // Get index (default is [0 Size])
        let index = if let Some(idx) = stream.get("Index") {
            let arr = idx.as_array()?;
            let mut pairs = Vec::new();
            let mut i = 0;
            while i + 1 < arr.len() {
                let start = arr[i].as_int()? as u32;
                let count = arr[i + 1].as_int()? as usize;
                pairs.push((start, count));
                i += 2;
            }
            pairs
        } else {
            vec![(0, size)]
        };

        // Decompress stream data
        let data = self.decode_stream(stream)?;

        let mut xref = XRef::new();
        let mut data_pos = 0;

        for (start_objid, count) in index {
            for i in 0..count {
                if data_pos + entry_size > data.len() {
                    break;
                }

                let objid = start_objid + i as u32;

                // Read type (default 1 if w0 == 0)
                let obj_type = if w0 > 0 {
                    Self::read_bytes_as_int(&data[data_pos..data_pos + w0])
                } else {
                    1
                };
                let field1 = Self::read_bytes_as_int(&data[data_pos + w0..data_pos + w0 + w1]);
                let field2 =
                    Self::read_bytes_as_int(&data[data_pos + w0 + w1..data_pos + entry_size]);
                data_pos += entry_size;

                match obj_type {
                    0 => {
                        // Free object, skip
                    }
                    1 => {
                        // Regular object: field1 = offset, field2 = genno
                        xref.offsets.insert(
                            objid,
                            XRefEntry {
                                offset: field1 as usize,
                                genno: field2 as u32,
                                in_stream: false,
                                stream_objid: None,
                            },
                        );
                    }
                    2 => {
                        // Compressed object: field1 = stream objid, field2 = index
                        xref.offsets.insert(
                            objid,
                            XRefEntry {
                                offset: field2 as usize,
                                genno: 0,
                                in_stream: true,
                                stream_objid: Some(field1 as u32),
                            },
                        );
                    }
                    _ => {}
                }
            }
        }

        // Copy trailer from stream attrs
        for (key, value) in &stream.attrs {
            if key != "Length"
                && key != "Filter"
                && key != "DecodeParms"
                && key != "W"
                && key != "Index"
            {
                xref.trailer.insert(key.clone(), value.clone());
            }
        }

        Ok(xref)
    }

    fn read_bytes_as_int(bytes: &[u8]) -> u64 {
        let mut val: u64 = 0;
        for &b in bytes {
            val = (val << 8) | (b as u64);
        }
        val
    }

    /// Decode a PDF stream (handle decryption and FlateDecode, etc.)
    ///
    /// # Arguments
    /// * `stream` - The PDF stream to decode
    /// * `objid` - Object ID (used for decryption key derivation)
    /// * `genno` - Generation number (used for decryption key derivation)
    ///
    /// Decryption happens BEFORE decompression, as per PDF specification.
    pub fn decode_stream_with_objid(
        &self,
        stream: &crate::pdftypes::PDFStream,
        objid: u32,
        genno: u16,
    ) -> Result<Vec<u8>> {
        let mut raw = stream.get_rawdata().to_vec();

        // Decrypt stream data BEFORE decompressing (if encrypted), unless already done
        if !stream.rawdata_is_decrypted() {
            if let Some(ref handler) = self.security_handler {
                raw = handler.decrypt_stream(objid, genno, &raw, &stream.attrs);
            }
        }

        // Apply filters (decompression)
        self.apply_filters(&raw, stream)
    }

    /// Decode a PDF stream without explicit objid/genno.
    ///
    /// Uses the stream's embedded objid/genno if available.
    pub fn decode_stream(&self, stream: &crate::pdftypes::PDFStream) -> Result<Vec<u8>> {
        let objid = stream.objid.unwrap_or(0);
        let genno = stream.genno.unwrap_or(0) as u16;
        self.decode_stream_with_objid(stream, objid, genno)
    }

    /// Apply decompression filters to stream data.
    fn apply_filters(&self, data: &[u8], stream: &crate::pdftypes::PDFStream) -> Result<Vec<u8>> {
        let mut output = data.to_vec();

        // Check for Filter
        if let Some(filter) = stream.get("Filter") {
            let filter_name = match filter {
                PDFObject::Name(name) => Some(name.as_str()),
                PDFObject::Array(arr) if arr.len() == 1 => {
                    if let PDFObject::Name(name) = &arr[0] {
                        Some(name.as_str())
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if filter_name == Some("FlateDecode") {
                use std::io::Read;
                let mut decoder = flate2::read::ZlibDecoder::new(&output[..]);
                let mut decompressed = Vec::new();
                if decoder.read_to_end(&mut decompressed).is_err() {
                    // Fall back to lenient decompression for corrupted streams.
                    decompressed = Self::decompress_corrupted(&output);
                }
                output = decompressed;
            }
        }

        // Apply predictor if specified in DecodeParms
        if let Some(parms) = stream.get("DecodeParms") {
            let parms_dict = match parms {
                PDFObject::Dict(d) => Some(d),
                PDFObject::Array(arr) if !arr.is_empty() => {
                    if let PDFObject::Dict(d) = &arr[0] {
                        Some(d)
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(parms) = parms_dict {
                let predictor = parms
                    .get("Predictor")
                    .and_then(|p| p.as_int().ok())
                    .unwrap_or(1) as usize;

                if predictor >= 10 {
                    // PNG predictor
                    let columns = parms
                        .get("Columns")
                        .and_then(|c| c.as_int().ok())
                        .unwrap_or(1) as usize;
                    let colors = parms
                        .get("Colors")
                        .and_then(|c| c.as_int().ok())
                        .unwrap_or(1) as usize;
                    let bits = parms
                        .get("BitsPerComponent")
                        .and_then(|b| b.as_int().ok())
                        .unwrap_or(8) as usize;

                    output = Self::apply_png_predictor(&output, columns, colors, bits)?;
                }
            }
        }

        Ok(output)
    }

    /// Best-effort zlib decompression for corrupted streams.
    ///
    /// Mirrors pdfminer.six behavior: return partial output up to the point
    /// the decoder fails (often due to CRC errors near the end).
    fn decompress_corrupted(data: &[u8]) -> Vec<u8> {
        use flate2::{Decompress, FlushDecompress, Status};
        let mut decoder = Decompress::new(true);
        let mut out = Vec::with_capacity(data.len() * 2);
        let mut buf = [0u8; 4096];
        let mut i = 0usize;
        while i < data.len() {
            let before_out = decoder.total_out();
            let before_in = decoder.total_in();
            let res = decoder.decompress(&data[i..i + 1], &mut buf, FlushDecompress::None);
            let produced = (decoder.total_out() - before_out) as usize;
            if produced > 0 {
                out.extend_from_slice(&buf[..produced]);
            }
            let consumed = (decoder.total_in() - before_in) as usize;
            if consumed == 0 {
                i += 1;
            } else {
                i += consumed;
            }
            match res {
                Ok(Status::StreamEnd) | Err(_) => break,
                Ok(_) => {}
            }
        }
        out
    }

    /// Apply PNG predictor to decompress predicted data.
    ///
    /// PNG prediction adds a filter byte at the start of each row.
    /// This reverses the prediction to get the original data.
    fn apply_png_predictor(
        data: &[u8],
        columns: usize,
        colors: usize,
        bits_per_component: usize,
    ) -> Result<Vec<u8>> {
        let row_bytes = colors * columns * bits_per_component / 8;
        let bpp = std::cmp::max(1, colors * bits_per_component / 8); // bytes per pixel
        let row_size = row_bytes + 1; // +1 for filter byte

        let mut result = Vec::with_capacity(data.len());
        let mut prev_row = vec![0u8; row_bytes];

        for row_start in (0..data.len()).step_by(row_size) {
            if row_start + row_size > data.len() {
                break;
            }

            let filter_type = data[row_start];
            let row_data = &data[row_start + 1..row_start + row_size];
            let mut current_row = vec![0u8; row_bytes];

            match filter_type {
                0 => {
                    // None - no filtering
                    current_row.copy_from_slice(row_data);
                }
                1 => {
                    // Sub - each byte depends on byte to the left
                    for i in 0..row_bytes {
                        let left = if i >= bpp { current_row[i - bpp] } else { 0 };
                        current_row[i] = row_data[i].wrapping_add(left);
                    }
                }
                2 => {
                    // Up - each byte depends on byte above
                    for i in 0..row_bytes {
                        current_row[i] = row_data[i].wrapping_add(prev_row[i]);
                    }
                }
                3 => {
                    // Average - average of left and above
                    for i in 0..row_bytes {
                        let left = if i >= bpp {
                            current_row[i - bpp] as u16
                        } else {
                            0
                        };
                        let above = prev_row[i] as u16;
                        current_row[i] = row_data[i].wrapping_add(((left + above) / 2) as u8);
                    }
                }
                4 => {
                    // Paeth - uses Paeth predictor function
                    for i in 0..row_bytes {
                        let left = if i >= bpp { current_row[i - bpp] } else { 0 };
                        let above = prev_row[i];
                        let upper_left = if i >= bpp { prev_row[i - bpp] } else { 0 };
                        let paeth = Self::paeth_predictor(left, above, upper_left);
                        current_row[i] = row_data[i].wrapping_add(paeth);
                    }
                }
                _ => {
                    // Unknown filter, just copy the data
                    current_row.copy_from_slice(row_data);
                }
            }

            result.extend_from_slice(&current_row);
            prev_row = current_row;
        }

        Ok(result)
    }

    /// Paeth predictor function used in PNG filtering.
    fn paeth_predictor(left: u8, above: u8, upper_left: u8) -> u8 {
        let a = left as i32;
        let b = above as i32;
        let c = upper_left as i32;
        let p = a + b - c;
        let pa = (p - a).abs();
        let pb = (p - b).abs();
        let pc = (p - c).abs();

        if pa <= pb && pa <= pc {
            left
        } else if pb <= pc {
            above
        } else {
            upper_left
        }
    }

    /// Fallback xref loading: scan file for "N M obj" patterns.
    fn load_xref_fallback(&self) -> Result<XRef> {
        use regex::bytes::Regex;

        let mut xref = XRef::new();
        xref.is_fallback = true;
        let re = Regex::new(r"(\d+)\s+(\d+)\s+obj\b").unwrap();

        for cap in re.captures_iter(self.data) {
            let objid: u32 = std::str::from_utf8(&cap[1]).unwrap().parse().unwrap();
            let genno: u32 = std::str::from_utf8(&cap[2]).unwrap().parse().unwrap();
            let pos = cap.get(0).unwrap().start();

            xref.offsets.insert(
                objid,
                XRefEntry {
                    offset: pos,
                    genno,
                    in_stream: false,
                    stream_objid: None,
                },
            );
        }

        // Find trailer
        if let Some(trailer_pos) = self.find_trailer() {
            let data = &self.data[trailer_pos..];
            // Skip "trailer" and whitespace
            let mut skip = 7;
            while skip < data.len()
                && (data[skip] == b' ' || data[skip] == b'\n' || data[skip] == b'\r')
            {
                skip += 1;
            }
            if data[skip..].starts_with(b"<<") {
                let mut parser = PDFParser::new(&data[skip..]);
                if let Ok(trailer_obj) = parser.parse_object() {
                    if let Ok(dict) = trailer_obj.as_dict() {
                        xref.trailer = dict.clone();
                    }
                }
            }
        }

        if xref.offsets.is_empty() {
            return Err(PdfError::NoValidXRef);
        }

        Ok(xref)
    }

    /// Find "trailer" keyword position.
    fn find_trailer(&self) -> Option<usize> {
        let search = b"trailer";
        (0..self.data.len().saturating_sub(search.len()))
            .rev()
            .find(|&i| &self.data[i..i + search.len()] == search)
    }

    /// Read a decimal number from data, return (value, bytes_consumed).
    fn read_number(&self, data: &[u8]) -> Result<(i64, usize)> {
        let mut pos = 0;
        let negative = if pos < data.len() && data[pos] == b'-' {
            pos += 1;
            true
        } else {
            false
        };

        let start = pos;
        while pos < data.len() && data[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            return Err(PdfError::SyntaxError("expected number".into()));
        }

        let num_str = std::str::from_utf8(&data[start..pos])
            .map_err(|_| PdfError::SyntaxError("invalid number".into()))?;
        let mut num: i64 = num_str
            .parse()
            .map_err(|_| PdfError::SyntaxError("invalid number".into()))?;

        if negative {
            num = -num;
        }

        Ok((num, pos))
    }

    /// Get an object by ID.
    pub fn getobj(&self, objid: u32) -> Result<PDFObject> {
        if objid == 0 {
            return Err(PdfError::ObjectNotFound(0));
        }

        // Check cache first
        if let Some(obj) = self.cache.get(&objid) {
            return Ok(obj.clone());
        }

        // Find in xrefs
        for xref in &self.xrefs {
            if let Some(entry) = xref.get_pos(objid) {
                let genno = entry.genno as u16;
                let (obj, needs_decryption) = if entry.in_stream {
                    // Object is in an object stream - already decrypted when stream was decrypted
                    let stream_objid = match entry.stream_objid {
                        Some(id) => id,
                        None => continue, // Try next xref like Python does
                    };
                    let index = entry.offset;
                    match self.parse_object_from_stream(stream_objid, index) {
                        Ok(o) => (o, false), // Don't double-decrypt
                        Err(_) => continue,  // Try next xref like Python does
                    }
                } else {
                    // Parse object at file offset - needs decryption if encrypted
                    match self.parse_object_at(entry.offset, objid, xref.is_fallback) {
                        Ok(o) => (o, true),
                        Err(_) => continue, // Try next xref like Python does
                    }
                };

                // Decrypt strings within the object if needed
                let obj = if needs_decryption && self.security_handler.is_some() {
                    self.decrypt_object(obj, objid, genno)
                } else {
                    obj
                };

                return Ok(obj);
            }
        }

        Err(PdfError::ObjectNotFound(objid))
    }

    /// Decrypt strings within a PDF object recursively.
    fn decrypt_object(&self, obj: PDFObject, objid: u32, genno: u16) -> PDFObject {
        let handler = match &self.security_handler {
            Some(h) => h,
            None => return obj,
        };

        match obj {
            PDFObject::String(data) => {
                let decrypted = handler.decrypt_string(objid, genno, &data);
                PDFObject::String(decrypted)
            }
            PDFObject::Array(arr) => {
                let decrypted_arr: Vec<PDFObject> = arr
                    .into_iter()
                    .map(|item| self.decrypt_object(item, objid, genno))
                    .collect();
                PDFObject::Array(decrypted_arr)
            }
            PDFObject::Dict(dict) => {
                let decrypted_dict: HashMap<String, PDFObject> = dict
                    .into_iter()
                    .map(|(k, v)| (k, self.decrypt_object(v, objid, genno)))
                    .collect();
                PDFObject::Dict(decrypted_dict)
            }
            PDFObject::Stream(mut stream) => {
                // Decrypt stream attributes (they may contain strings)
                let decrypted_attrs: HashMap<String, PDFObject> = stream
                    .attrs
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k, self.decrypt_object(v, objid, genno)))
                    .collect();
                stream.attrs = decrypted_attrs;
                // Set objid/genno on stream for later decryption of stream data
                stream.set_objid(objid, genno as u32);
                // Decrypt raw stream data once so image decoding can use it directly
                if let Some(handler) = &self.security_handler {
                    let decrypted =
                        handler.decrypt_stream(objid, genno, stream.get_rawdata(), &stream.attrs);
                    stream.set_rawdata_decrypted(decrypted);
                }
                PDFObject::Stream(stream)
            }
            // Other types don't need decryption
            other => other,
        }
    }

    /// Parse an object from an object stream (ObjStm).
    fn parse_object_from_stream(&self, stream_objid: u32, index: usize) -> Result<PDFObject> {
        // Get the stream object
        let stream_obj = self.getobj(stream_objid)?;
        let stream = stream_obj.as_stream()?;

        // Decode stream data
        let data = self.decode_stream(stream)?;

        // Get N (number of objects) and First (offset of first object)
        let n = stream
            .get("N")
            .ok_or_else(|| PdfError::SyntaxError("missing N in ObjStm".into()))?
            .as_int()? as usize;
        let first = stream
            .get("First")
            .ok_or_else(|| PdfError::SyntaxError("missing First in ObjStm".into()))?
            .as_int()? as usize;

        if index >= n {
            return Err(PdfError::SyntaxError(format!("index {} >= N {}", index, n)));
        }

        // Parse the header: objid1 offset1 objid2 offset2 ...
        let mut header_parser = PDFParser::new(&data[..first]);
        let mut offsets = Vec::with_capacity(n);
        for _ in 0..n {
            let _obj_id = header_parser.parse_object()?.as_int()?;
            let offset = header_parser.parse_object()?.as_int()? as usize;
            offsets.push(offset);
        }

        // Get the offset for our object
        let obj_offset = first + offsets.get(index).copied().unwrap_or(0);

        // Parse the object
        let mut obj_parser = PDFParser::new(&data[obj_offset..]);
        obj_parser.parse_object()
    }

    /// Parse an indirect object at the given offset.
    fn parse_object_at(
        &self,
        offset: usize,
        _expected_objid: u32,
        fallback: bool,
    ) -> Result<PDFObject> {
        if offset >= self.data.len() {
            return Err(PdfError::SyntaxError(format!(
                "object offset {} exceeds file size {}",
                offset,
                self.data.len()
            )));
        }
        let data = &self.data[offset..];

        // Parse "objid genno obj"
        let (_objid, consumed1) = self.read_number(data)?;
        let data = &data[consumed1..];

        // Skip whitespace
        let mut skip = 0;
        while skip < data.len()
            && (data[skip] == b' ' || data[skip] == b'\n' || data[skip] == b'\r')
        {
            skip += 1;
        }
        let data = &data[skip..];

        let (_genno, consumed2) = self.read_number(data)?;
        let data = &data[consumed2..];

        // Skip whitespace
        let mut skip = 0;
        while skip < data.len()
            && (data[skip] == b' ' || data[skip] == b'\n' || data[skip] == b'\r')
        {
            skip += 1;
        }
        let data = &data[skip..];

        // Expect "obj"
        if !data.starts_with(b"obj") {
            return Err(PdfError::SyntaxError(format!(
                "expected 'obj' at offset {}, got {:?}",
                offset,
                String::from_utf8_lossy(&data[..std::cmp::min(10, data.len())])
            )));
        }
        let data = &data[3..];

        // Skip whitespace
        let mut skip = 0;
        while skip < data.len()
            && (data[skip] == b' ' || data[skip] == b'\n' || data[skip] == b'\r')
        {
            skip += 1;
        }
        let data = &data[skip..];

        // Parse the object
        let mut parser = PDFParser::new(data);
        let obj = parser.parse_object()?;

        // Check if this is a stream (dict followed by "stream")
        if let PDFObject::Dict(ref dict) = obj {
            // Check for stream keyword after dict
            let remaining = parser.remaining();
            let mut pos = 0;
            while pos < remaining.len()
                && (remaining[pos] == b' ' || remaining[pos] == b'\n' || remaining[pos] == b'\r')
            {
                pos += 1;
            }
            if pos < remaining.len() && remaining[pos..].starts_with(b"stream") {
                pos += 6;
                // Skip optional \r
                if pos < remaining.len() && remaining[pos] == b'\r' {
                    pos += 1;
                }
                // Must have \n
                if pos < remaining.len() && remaining[pos] == b'\n' {
                    pos += 1;
                }

                // Get length from dict (resolve indirect refs) unless fallback mode
                let length: usize = if fallback {
                    0
                } else {
                    dict.get("Length")
                        .and_then(|len_obj| self.resolve_internal(len_obj).ok())
                        .and_then(|resolved| resolved.as_int().ok())
                        .filter(|&len| len > 0)
                        .map(|len| len as usize)
                        .unwrap_or(0)
                };

                // Extract stream data
                let stream_start = pos;

                // Prefer declared /Length, but fall back to endstream if length looks corrupted.
                let end_pos = Self::find_endstream(&remaining[stream_start..]);
                // Allow up to 64 bytes of padding/whitespace between stream data and endstream.
                // This threshold handles malformed PDFs with extra line endings or garbage.
                let use_endstream = fallback
                    || length == 0
                    || end_pos
                        .map(|pos| pos > length.saturating_add(64))
                        .unwrap_or(false);

                let stream_data = if use_endstream {
                    if let Some(end_pos) = end_pos {
                        let end = (stream_start + end_pos).min(remaining.len());
                        remaining[stream_start..end].to_vec()
                    } else if length > 0 && stream_start + length <= remaining.len() {
                        remaining[stream_start..stream_start + length].to_vec()
                    } else {
                        remaining[stream_start..].to_vec()
                    }
                } else if length > 0 && stream_start + length <= remaining.len() {
                    remaining[stream_start..stream_start + length].to_vec()
                } else if let Some(end_pos) = end_pos {
                    let end = (stream_start + end_pos).min(remaining.len());
                    remaining[stream_start..end].to_vec()
                } else {
                    remaining[stream_start..].to_vec()
                };

                return Ok(PDFObject::Stream(Box::new(
                    crate::pdftypes::PDFStream::new(dict.clone(), stream_data),
                )));
            }
        }

        Ok(obj)
    }

    fn find_endstream(data: &[u8]) -> Option<usize> {
        let needle = b"endstream";
        for i in 0..data.len().saturating_sub(needle.len()) {
            if &data[i..i + needle.len()] == needle {
                // Trim trailing whitespace
                let mut end = i;
                while end > 0
                    && (data[end - 1] == b' ' || data[end - 1] == b'\n' || data[end - 1] == b'\r')
                {
                    end -= 1;
                }
                return Some(end);
            }
        }
        None
    }

    /// Get document catalog.
    pub fn catalog(&self) -> &HashMap<String, PDFObject> {
        &self.catalog
    }

    /// Get document info dictionaries.
    pub fn info(&self) -> &Vec<HashMap<String, PDFObject>> {
        &self.info
    }

    /// Check if the document is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.security_handler.is_some()
    }

    /// Resolve a reference to its actual object.
    pub fn resolve(&self, obj: &PDFObject) -> Result<PDFObject> {
        self.resolve_internal(obj)
    }

    /// Internal resolve that doesn't require mutable access.
    fn resolve_internal(&self, obj: &PDFObject) -> Result<PDFObject> {
        match obj {
            PDFObject::Ref(r) => {
                let resolved = self.getobj(r.objid)?;
                // Recursively resolve in case it's a ref to a ref
                self.resolve_internal(&resolved)
            }
            _ => Ok(obj.clone()),
        }
    }

    /// Get the total page count from the Pages tree.
    fn get_page_count(&self) -> usize {
        if let Some(pages_ref) = self.catalog.get("Pages") {
            if let Ok(pages) = self.resolve_internal(pages_ref) {
                if let Ok(dict) = pages.as_dict() {
                    if let Some(count) = dict.get("Count") {
                        if let Ok(n) = count.as_int() {
                            return n as usize;
                        }
                    }
                }
            }
        }
        0
    }

    /// Get page labels iterator.
    pub fn get_page_labels(&self) -> Result<PageLabels<'_>> {
        if let Some(page_labels_obj) = self.catalog.get("PageLabels") {
            let page_labels = self.resolve_internal(page_labels_obj)?;
            let page_count = self.get_page_count();
            PageLabels::new(self, page_labels, page_count)
        } else {
            Err(PdfError::NoPageLabels)
        }
    }

    /// Get all object IDs from xrefs.
    pub fn get_objids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.xrefs.iter().flat_map(|x| x.get_objids()).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Iterate over all xref trailers.
    ///
    /// Returns tuples of (is_fallback, trailer_dict).
    pub fn get_trailers(&self) -> impl Iterator<Item = (bool, &HashMap<String, PDFObject>)> {
        self.xrefs.iter().map(|x| (x.is_fallback, &x.trailer))
    }

    /// Check if all xrefs are fallback xrefs.
    pub fn all_xrefs_are_fallback(&self) -> bool {
        self.xrefs.iter().all(|x| x.is_fallback)
    }

    /// Resolve a named destination.
    ///
    /// Looks up the name in the document's Names/Dests tree or catalog Dests dict.
    pub fn get_dest(&self, name: &[u8]) -> Result<PDFObject> {
        // First try Names/Dests tree (PDF 1.2+)
        if let Some(names_ref) = self.catalog.get("Names") {
            if let Ok(names) = self.resolve_internal(names_ref) {
                if let Ok(names_dict) = names.as_dict() {
                    if let Some(dests_ref) = names_dict.get("Dests") {
                        if let Ok(dests) = self.resolve_internal(dests_ref) {
                            if let Some(result) = self.lookup_name_tree(&dests, name)? {
                                return Ok(result);
                            }
                        }
                    }
                }
            }
        }

        // Try catalog Dests dict (PDF 1.1)
        if let Some(dests_ref) = self.catalog.get("Dests") {
            if let Ok(dests) = self.resolve_internal(dests_ref) {
                if let Ok(dests_dict) = dests.as_dict() {
                    let name_str = String::from_utf8_lossy(name);
                    if let Some(dest) = dests_dict.get(name_str.as_ref()) {
                        return self.resolve_internal(dest);
                    }
                }
            }
        }

        Err(PdfError::DestinationNotFound(
            String::from_utf8_lossy(name).to_string(),
        ))
    }

    /// Look up a name in a name tree.
    fn lookup_name_tree(&self, tree: &PDFObject, name: &[u8]) -> Result<Option<PDFObject>> {
        let dict = match tree.as_dict() {
            Ok(d) => d,
            Err(_) => return Ok(None),
        };

        // Check Names array (leaf node)
        if let Some(names_arr) = dict.get("Names") {
            if let Ok(arr) = self.resolve_internal(names_arr)?.as_array() {
                // Names array is pairs: [name1, value1, name2, value2, ...]
                let mut i = 0;
                while i + 1 < arr.len() {
                    if let Ok(key) = arr[i].as_string() {
                        if key == name {
                            return Ok(Some(self.resolve_internal(&arr[i + 1])?));
                        }
                    }
                    i += 2;
                }
            }
        }

        // Check Kids array (intermediate node)
        if let Some(kids) = dict.get("Kids") {
            if let Ok(kids_arr) = self.resolve_internal(kids)?.as_array() {
                for kid in kids_arr {
                    if let Ok(kid_obj) = self.resolve_internal(kid) {
                        // Check Limits to optimize search
                        if let Ok(kid_dict) = kid_obj.as_dict() {
                            if let Some(limits) = kid_dict.get("Limits") {
                                if let Ok(limits_arr) = limits.as_array() {
                                    if limits_arr.len() >= 2 {
                                        let min = limits_arr[0].as_string().unwrap_or(&[]);
                                        let max = limits_arr[1].as_string().unwrap_or(&[]);
                                        if name < min || name > max {
                                            continue;
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(result) = self.lookup_name_tree(&kid_obj, name)? {
                            return Ok(Some(result));
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}

/// Page label iterator.
pub struct PageLabels<'a> {
    /// Reference to doc for future use (e.g., resolving indirect refs in label dicts)
    #[allow(dead_code)]
    doc: &'a PDFDocument<'a>,
    /// Sorted list of (page_index, label_dict)
    ranges: Vec<(i64, HashMap<String, PDFObject>)>,
    /// Current range index
    range_idx: usize,
    /// Current value within range
    current_value: i64,
    /// Current page index
    current_page: i64,
    /// End of current range (exclusive)
    range_end: Option<i64>,
    /// Current style
    style: Option<String>,
    /// Current prefix
    prefix: String,
    /// Total number of pages in the document
    page_count: usize,
}

impl<'a> PageLabels<'a> {
    fn new(doc: &'a PDFDocument<'a>, obj: PDFObject, page_count: usize) -> Result<Self> {
        let dict = obj.as_dict()?;

        // Parse number tree
        let ranges = Self::parse_number_tree(doc, dict)?;

        if ranges.is_empty() {
            return Err(PdfError::NoPageLabels);
        }

        // Ensure it starts at 0
        let mut ranges = ranges;
        if ranges[0].0 != 0 {
            ranges.insert(0, (0, HashMap::new()));
        }

        let mut labels = Self {
            doc,
            ranges,
            range_idx: 0,
            current_value: 1,
            current_page: 0,
            range_end: None,
            style: None,
            prefix: String::new(),
            page_count,
        };

        labels.init_range(0);
        Ok(labels)
    }

    fn parse_number_tree(
        doc: &'a PDFDocument<'a>,
        dict: &HashMap<String, PDFObject>,
    ) -> Result<Vec<(i64, HashMap<String, PDFObject>)>> {
        let mut items = Vec::new();

        // Check for Nums (leaf node)
        if let Some(nums) = dict.get("Nums") {
            let nums = doc.resolve_internal(nums)?;
            let nums_arr = nums.as_array()?;

            // Pairs of (page_index, label_dict)
            let mut i = 0;
            while i + 1 < nums_arr.len() {
                let idx = nums_arr[i].as_int()?;
                let label_dict = doc.resolve_internal(&nums_arr[i + 1])?;
                let dict = label_dict.as_dict().cloned().unwrap_or_default();
                items.push((idx, dict));
                i += 2;
            }
        }

        // Check for Kids (intermediate node)
        if let Some(kids) = dict.get("Kids") {
            let kids = doc.resolve_internal(kids)?;
            let kids_arr = kids.as_array()?;

            for kid in kids_arr {
                let kid_obj = doc.resolve_internal(kid)?;
                let kid_dict = kid_obj.as_dict()?;
                items.extend(Self::parse_number_tree(doc, kid_dict)?);
            }
        }

        // Sort by page index
        items.sort_by_key(|(idx, _)| *idx);
        Ok(items)
    }

    fn init_range(&mut self, range_idx: usize) {
        if range_idx >= self.ranges.len() {
            return;
        }

        self.range_idx = range_idx;
        let (start, ref dict) = self.ranges[range_idx];
        self.current_page = start;

        // Get style
        self.style = dict
            .get("S")
            .and_then(|s| s.as_name().ok())
            .map(|s| s.to_string());

        // Get prefix
        self.prefix = dict
            .get("P")
            .and_then(|p| p.as_string().ok())
            .and_then(|b| String::from_utf8(b.to_vec()).ok())
            .unwrap_or_default();

        // Get starting value (St), default 1
        self.current_value = dict.get("St").and_then(|s| s.as_int().ok()).unwrap_or(1);

        // Calculate range end
        self.range_end = if range_idx + 1 < self.ranges.len() {
            Some(self.ranges[range_idx + 1].0)
        } else {
            None
        };
    }

    fn format_label(&self) -> String {
        let value_str = match self.style.as_deref() {
            Some("D") => self.current_value.to_string(),
            Some("R") => format_roman(self.current_value as u32).to_uppercase(),
            Some("r") => format_roman(self.current_value as u32),
            Some("A") => format_alpha(self.current_value as u32).to_uppercase(),
            Some("a") => format_alpha(self.current_value as u32),
            _ => String::new(),
        };

        format!("{}{}", self.prefix, value_str)
    }
}

impl<'a> Iterator for PageLabels<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        // Stop when we've exhausted all pages
        if self.current_page as usize >= self.page_count {
            return None;
        }

        // Check if we need to move to next range
        if let Some(end) = self.range_end {
            if self.current_page >= end {
                self.init_range(self.range_idx + 1);
            }
        }

        let label = self.format_label();
        self.current_page += 1;
        self.current_value += 1;

        Some(label)
    }
}

/// Format integer as roman numerals.
fn format_roman(mut n: u32) -> String {
    if n == 0 {
        return String::new();
    }

    let numerals = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];

    let mut result = String::new();
    for (value, numeral) in numerals {
        while n >= value {
            result.push_str(numeral);
            n -= value;
        }
    }
    result
}

/// Format integer as alphabetic (a-z, aa-zz, ...).
fn format_alpha(n: u32) -> String {
    if n == 0 {
        return String::new();
    }

    let mut result = String::new();
    let mut n = n - 1; // 1-indexed

    loop {
        result.insert(0, (b'a' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_roman() {
        assert_eq!(format_roman(1), "i");
        assert_eq!(format_roman(3), "iii");
        assert_eq!(format_roman(4), "iv");
        assert_eq!(format_roman(5), "v");
        assert_eq!(format_roman(9), "ix");
        assert_eq!(format_roman(10), "x");
    }

    #[test]
    fn test_format_alpha() {
        assert_eq!(format_alpha(1), "a");
        assert_eq!(format_alpha(26), "z");
        assert_eq!(format_alpha(27), "aa");
        assert_eq!(format_alpha(28), "ab");
    }
}
