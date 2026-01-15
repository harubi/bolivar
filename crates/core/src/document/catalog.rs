//! PDF Document - main entry point for PDF parsing.
//!
//! Port of pdfminer.six pdfdocument.py
//!
//! Handles:
//! - XRef table parsing
//! - Object resolution
//! - Trailer parsing (catalog, info)
//! - Page labels

use super::page::PageIndex;
use super::security::{PDFSecurityHandler, create_security_handler};
use crate::error::{PdfError, Result};
use crate::font::encoding::{DiffEntry, EncodingDB};
use crate::model::objects::PDFObject;
use crate::parser::pdf_parser::PDFParser;
use crate::simd::U8_LANES;
use bytes::Bytes;
use indexmap::IndexMap;
use memmap2::Mmap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::simd::prelude::*;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

const PNG_SIMD_LANES: usize = U8_LANES;
pub const DEFAULT_CACHE_CAPACITY: usize = 1024;
pub const DEFAULT_PAGE_CACHE_CAPACITY: usize = 64;

struct ObjectCache {
    capacity: usize,
    map: IndexMap<u32, Arc<PDFObject>>,
}

struct PageCache {
    capacity: usize,
    map: IndexMap<usize, Arc<super::page::PDFPage>>,
}

impl PageCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: IndexMap::new(),
        }
    }

    fn get(&mut self, index: usize) -> Option<Arc<super::page::PDFPage>> {
        if self.capacity == 0 {
            return None;
        }
        let pos = self.map.get_index_of(&index)?;
        let value = Arc::clone(self.map.get_index(pos)?.1);
        if pos + 1 != self.map.len() {
            self.map.move_index(pos, self.map.len() - 1);
        }
        Some(value)
    }

    fn insert(&mut self, index: usize, page: Arc<super::page::PDFPage>) {
        if self.capacity == 0 {
            return;
        }
        if self.map.contains_key(&index) {
            self.map.shift_remove(&index);
        }
        self.map.insert(index, page);
        if self.map.len() > self.capacity {
            self.map.shift_remove_index(0);
        }
    }
}
impl ObjectCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: IndexMap::new(),
        }
    }

    fn get(&mut self, objid: u32) -> Option<Arc<PDFObject>> {
        if self.capacity == 0 {
            return None;
        }
        let index = self.map.get_index_of(&objid)?;
        let value = Arc::clone(self.map.get_index(index)?.1);
        if index + 1 != self.map.len() {
            self.map.move_index(index, self.map.len() - 1);
        }
        Some(value)
    }

    fn insert(&mut self, objid: u32, value: Arc<PDFObject>) {
        if self.capacity == 0 {
            return;
        }
        if self.map.contains_key(&objid) {
            self.map.shift_remove(&objid);
        }
        self.map.insert(objid, value);
        if self.map.len() > self.capacity {
            self.map.shift_remove_index(0);
        }
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        self.map.clear();
    }
}

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

#[derive(Clone)]
pub enum PdfBytes {
    Owned(Bytes),
    Shared(Bytes),
}

impl PdfBytes {
    const fn as_bytes(&self) -> &Bytes {
        match self {
            Self::Owned(data) => data,
            Self::Shared(data) => data,
        }
    }

    fn as_slice(&self) -> &[u8] {
        self.as_bytes().as_ref()
    }

    fn len(&self) -> usize {
        self.as_slice().len()
    }
}

/// PDF Document - provides access to PDF objects and metadata.
/// Owns its data via Bytes for thread-safe sharing.
pub struct PDFDocument {
    data: PdfBytes,
    xrefs: Vec<XRef>,
    catalog: HashMap<String, PDFObject>,
    info: Vec<HashMap<String, PDFObject>>,
    cache: Mutex<ObjectCache>,
    page_cache: Mutex<PageCache>,
    font_encoding_cache: Mutex<HashMap<u32, Arc<HashMap<u8, String>>>>,
    objstm_index: RwLock<Option<HashMap<u32, (u32, usize)>>>,
    security_handler: Option<Box<dyn PDFSecurityHandler + Send + Sync>>,
    page_index: OnceLock<PageIndex>,
}

impl PDFDocument {
    fn new_with_cache_inner(data: PdfBytes, password: &str, cache_capacity: usize) -> Result<Self> {
        let mut doc = Self {
            data,
            xrefs: Vec::new(),
            catalog: HashMap::new(),
            info: Vec::new(),
            cache: Mutex::new(ObjectCache::new(cache_capacity)),
            page_cache: Mutex::new(PageCache::new(DEFAULT_PAGE_CACHE_CAPACITY)),
            font_encoding_cache: Mutex::new(HashMap::new()),
            objstm_index: RwLock::new(None),
            security_handler: None,
            page_index: OnceLock::new(),
        };
        doc.parse(password)?;
        Ok(doc)
    }

    /// Create a new PDFDocument from raw PDF data.
    pub fn new<D: AsRef<[u8]>>(data: D, password: &str) -> Result<Self> {
        Self::new_with_cache(data, password, DEFAULT_CACHE_CAPACITY)
    }

    /// Create a new PDFDocument with an explicit object cache capacity.
    pub fn new_with_cache<D: AsRef<[u8]>>(
        data: D,
        password: &str,
        cache_capacity: usize,
    ) -> Result<Self> {
        Self::new_with_cache_inner(
            PdfBytes::Owned(Bytes::copy_from_slice(data.as_ref())),
            password,
            cache_capacity,
        )
    }

    /// Create a new PDFDocument from a memory-mapped PDF.
    pub fn new_from_mmap(mmap: Mmap, password: &str) -> Result<Self> {
        Self::new_from_mmap_with_cache(mmap, password, DEFAULT_CACHE_CAPACITY)
    }

    /// Create a new PDFDocument from a memory-mapped PDF with an explicit cache capacity.
    pub fn new_from_mmap_with_cache(
        mmap: Mmap,
        password: &str,
        cache_capacity: usize,
    ) -> Result<Self> {
        Self::new_with_cache_inner(
            PdfBytes::Shared(Bytes::from_owner(mmap)),
            password,
            cache_capacity,
        )
    }

    /// Create a new PDFDocument from shared bytes (zero-copy).
    pub fn new_from_bytes(data: Bytes, password: &str) -> Result<Self> {
        Self::new_from_bytes_with_cache(data, password, DEFAULT_CACHE_CAPACITY)
    }

    /// Create a new PDFDocument from shared bytes (zero-copy) with an explicit cache capacity.
    pub fn new_from_bytes_with_cache(
        data: Bytes,
        password: &str,
        cache_capacity: usize,
    ) -> Result<Self> {
        Self::new_with_cache_inner(PdfBytes::Shared(data), password, cache_capacity)
    }

    /// Returns the raw PDF bytes.
    pub fn bytes(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub fn cache_page(&self, index: usize, page: Arc<super::page::PDFPage>) {
        if let Ok(mut cache) = self.page_cache.lock() {
            cache.insert(index, page);
        }
    }

    pub fn get_cached_page(&self, index: usize) -> Option<Arc<super::page::PDFPage>> {
        self.page_cache
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(index))
    }

    pub fn get_page_cached(&self, index: usize) -> Result<Arc<super::page::PDFPage>> {
        if let Some(page) = self.get_cached_page(index) {
            return Ok(page);
        }
        let page = Arc::new(super::page::PDFPage::get_page_by_index(self, index)?);
        self.cache_page(index, Arc::clone(&page));
        Ok(page)
    }

    fn parse_encoding_differences(diff_obj: Option<&PDFObject>) -> Option<Vec<DiffEntry>> {
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

    fn build_font_encoding(encoding: &PDFObject) -> Option<HashMap<u8, String>> {
        match encoding {
            PDFObject::Name(name) => Some(EncodingDB::get_encoding(name, None)),
            PDFObject::Dict(dict) => {
                let base_encoding = dict
                    .get("BaseEncoding")
                    .and_then(|obj| obj.as_name().ok())
                    .unwrap_or("StandardEncoding");
                let differences = Self::parse_encoding_differences(dict.get("Differences"));
                Some(EncodingDB::get_encoding(
                    base_encoding,
                    differences.as_deref(),
                ))
            }
            _ => None,
        }
    }

    pub fn get_or_build_font_encoding(
        &self,
        objid: u32,
        encoding: &PDFObject,
    ) -> Option<Arc<HashMap<u8, String>>> {
        if let Ok(cache) = self.font_encoding_cache.lock()
            && let Some(entry) = cache.get(&objid)
        {
            return Some(Arc::clone(entry));
        }

        let map = Self::build_font_encoding(encoding)?;
        let shared = Arc::new(map);

        if let Ok(mut cache) = self.font_encoding_cache.lock() {
            cache.insert(objid, Arc::clone(&shared));
        }

        Some(shared)
    }

    /// Returns the cached page index for O(1) page lookup.
    pub(crate) fn page_index(&self) -> &PageIndex {
        self.page_index.get_or_init(|| PageIndex::new(self))
    }

    /// Parse the PDF document structure.
    fn parse(&mut self, password: &str) -> Result<()> {
        // Find startxref
        let startxref = self.find_startxref();

        // Try loading xrefs, fallback if necessary
        let mut loaded = false;
        if let Ok(pos) = startxref
            && self.load_xrefs(pos).is_ok()
            && !self.xrefs.is_empty()
        {
            loaded = true;
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
            if let Some(root_ref) = xref.trailer.get("Root")
                && self.catalog.is_empty()
                && let Ok(root_obj) = self.resolve_internal(root_ref)
                && let Ok(dict) = root_obj.as_dict()
            {
                self.catalog = dict.clone();
            }
            if let Some(info_ref) = xref.trailer.get("Info")
                && let Ok(info_obj) = self.resolve_internal(info_ref)
                && let Ok(dict) = info_obj.as_dict()
            {
                self.info.push(dict.clone());
            }
        }

        Ok(())
    }

    fn find_startxref_simd(data: &[u8]) -> Option<usize> {
        let needle = b"startxref";
        if data.len() < needle.len() {
            return None;
        }

        let search_start = if data.len() > 1024 {
            data.len() - 1024
        } else {
            0
        };
        let hay = &data[search_start..];
        let mut found = None;
        if hay.len() >= needle.len() {
            for pos in 0..=hay.len() - needle.len() {
                if &hay[pos..pos + needle.len()] == needle {
                    found = Some(search_start + pos);
                }
            }
        }
        found
    }

    /// Find the startxref position by scanning backwards from end of file.
    fn find_startxref(&self) -> Result<usize> {
        let search = b"startxref";
        let data = self.data.as_slice();

        // Handle files too small to contain startxref
        if data.len() < search.len() {
            return Err(crate::error::PdfError::SyntaxError("PDF too small".into()));
        }
        let Some(i) = Self::find_startxref_simd(data) else {
            return Err(PdfError::NoValidXRef);
        };

        let rest = &data[i + search.len()..];
        let mut pos = 0;
        while pos < rest.len() && (rest[pos] == b' ' || rest[pos] == b'\n' || rest[pos] == b'\r') {
            pos += 1;
        }
        let mut num_end = pos;
        while num_end < rest.len() && rest[num_end].is_ascii_digit() {
            num_end += 1;
        }
        if num_end > pos {
            let num_str =
                std::str::from_utf8(&rest[pos..num_end]).map_err(|_| PdfError::NoValidXRef)?;
            return num_str.parse().map_err(|_| PdfError::NoValidXRef);
        }

        Err(PdfError::NoValidXRef)
    }

    /// Load xref tables starting from given position.
    fn load_xrefs(&mut self, mut pos: usize) -> Result<()> {
        let mut visited = std::collections::HashSet::new();

        while !visited.contains(&pos) {
            visited.insert(pos);

            let xref = self.load_xref_at(pos)?;

            // Check for XRefStm (hybrid-reference xref stream)
            let xref_stm = xref
                .trailer
                .get("XRefStm")
                .and_then(|p| p.as_int().ok())
                .map(|n| n as usize);

            // Check for Prev pointer to previous xref
            let prev = xref
                .trailer
                .get("Prev")
                .and_then(|p| p.as_int().ok())
                .map(|n| n as usize);

            self.xrefs.push(xref);

            if let Some(xref_stm_pos) = xref_stm
                && !visited.contains(&xref_stm_pos)
                && let Ok(xref_stm) = self.load_xref_stream(xref_stm_pos)
            {
                self.xrefs.push(xref_stm);
                visited.insert(xref_stm_pos);
            }

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
        let data = &self.data.as_slice()[pos..];

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
        let data = &self.data.as_slice()[pos..];

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
        let data = &self.data.as_slice()[pos + cursor..];
        let mut cursor = 0;
        while cursor < data.len()
            && (data[cursor] == b' ' || data[cursor] == b'\n' || data[cursor] == b'\r')
        {
            cursor += 1;
        }

        // Parse trailer dict
        if data[cursor..].starts_with(b"<<") {
            let mut parser = PDFParser::new(&data[cursor..]);
            if let Ok(trailer_obj) = parser.parse_object()
                && let Ok(dict) = trailer_obj.as_dict()
            {
                xref.trailer = dict.clone();
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

    fn stream_has_filters(stream: &crate::model::objects::PDFStream) -> bool {
        stream.get("Filter").is_some()
    }

    /// Decode a PDF stream with caller-provided scratch storage.
    ///
    /// The closure receives a slice valid for the duration of the call.
    pub fn with_decoded_stream<R>(
        &self,
        stream: &crate::model::objects::PDFStream,
        objid: u32,
        genno: u16,
        scratch: &mut Vec<u8>,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Result<R> {
        let needs_decrypt = !stream.rawdata_is_decrypted() && self.security_handler.is_some();
        let has_filters = Self::stream_has_filters(stream);

        if !needs_decrypt && !has_filters {
            return Ok(f(stream.get_rawdata()));
        }

        let mut data = stream.get_rawdata();

        if needs_decrypt {
            let handler = self
                .security_handler
                .as_ref()
                .expect("security handler checked");
            *scratch = handler.decrypt_stream(objid, genno, data, &stream.attrs);
            data = scratch.as_slice();
        }

        if !has_filters {
            return Ok(f(data));
        }

        let decoded = self.apply_filters(data, stream)?;
        *scratch = decoded;
        Ok(f(scratch.as_slice()))
    }

    /// Decode a PDF stream to shared bytes.
    ///
    /// Returns a zero-copy view when no decryption or filters are required.
    pub fn decode_stream_bytes_with_objid(
        &self,
        stream: &crate::model::objects::PDFStream,
        objid: u32,
        genno: u16,
    ) -> Result<Bytes> {
        let needs_decrypt = !stream.rawdata_is_decrypted() && self.security_handler.is_some();
        let has_filters = Self::stream_has_filters(stream);

        if !needs_decrypt && !has_filters {
            return Ok(stream.rawdata_bytes());
        }

        let mut scratch = Vec::new();
        self.with_decoded_stream(stream, objid, genno, &mut scratch, |data| {
            Bytes::copy_from_slice(data)
        })
    }

    /// Decode a PDF stream to shared bytes without explicit objid/genno.
    pub fn decode_stream_bytes(&self, stream: &crate::model::objects::PDFStream) -> Result<Bytes> {
        let objid = stream.objid.unwrap_or(0);
        let genno = stream.genno.unwrap_or(0) as u16;
        self.decode_stream_bytes_with_objid(stream, objid, genno)
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
        stream: &crate::model::objects::PDFStream,
        objid: u32,
        genno: u16,
    ) -> Result<Vec<u8>> {
        let mut raw = stream.get_rawdata().to_vec();

        // Decrypt stream data BEFORE decompressing (if encrypted), unless already done
        if !stream.rawdata_is_decrypted()
            && let Some(ref handler) = self.security_handler
        {
            raw = handler.decrypt_stream(objid, genno, &raw, &stream.attrs);
        }

        // Apply filters (decompression)
        self.apply_filters(&raw, stream)
    }

    /// Decode a PDF stream without explicit objid/genno.
    ///
    /// Uses the stream's embedded objid/genno if available.
    pub fn decode_stream(&self, stream: &crate::model::objects::PDFStream) -> Result<Vec<u8>> {
        let objid = stream.objid.unwrap_or(0);
        let genno = stream.genno.unwrap_or(0) as u16;
        self.decode_stream_with_objid(stream, objid, genno)
    }

    /// Apply decompression filters to stream data.
    fn apply_filters(
        &self,
        data: &[u8],
        stream: &crate::model::objects::PDFStream,
    ) -> Result<Vec<u8>> {
        let mut output = data.to_vec();

        // Check for Filter (resolve indirects)
        if let Some(filter) = stream.get("Filter") {
            let filter = match self.resolve_internal(filter) {
                Ok(PDFObject::Array(arr)) => {
                    let mut resolved = Vec::with_capacity(arr.len());
                    for item in arr.iter() {
                        resolved.push(self.resolve_internal(item).unwrap_or_else(|_| item.clone()));
                    }
                    PDFObject::Array(resolved)
                }
                Ok(obj) => obj,
                Err(_) => filter.clone(),
            };
            let filter_name = match &filter {
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
            let parms = match self.resolve_internal(parms) {
                Ok(PDFObject::Array(arr)) => {
                    let mut resolved = Vec::with_capacity(arr.len());
                    for item in arr.iter() {
                        resolved.push(self.resolve_internal(item).unwrap_or_else(|_| item.clone()));
                    }
                    PDFObject::Array(resolved)
                }
                Ok(obj) => obj,
                Err(_) => parms.clone(),
            };
            let parms_dict = match &parms {
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
    fn apply_png_predictor_impl(
        data: &[u8],
        columns: usize,
        colors: usize,
        bits_per_component: usize,
        use_simd: bool,
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
                    if use_simd && row_bytes >= PNG_SIMD_LANES {
                        type V = Simd<u8, { PNG_SIMD_LANES }>;
                        let (prefix, middle, suffix) = row_data.as_simd::<{ PNG_SIMD_LANES }>();
                        let mut offset = 0;
                        for &b in prefix {
                            current_row[offset] = b.wrapping_add(prev_row[offset]);
                            offset += 1;
                        }
                        for chunk in middle {
                            let prev = V::from_slice(&prev_row[offset..offset + PNG_SIMD_LANES]);
                            let sum = *chunk + prev;
                            let lanes = sum.to_array();
                            current_row[offset..offset + PNG_SIMD_LANES].copy_from_slice(&lanes);
                            offset += PNG_SIMD_LANES;
                        }
                        for &b in suffix {
                            current_row[offset] = b.wrapping_add(prev_row[offset]);
                            offset += 1;
                        }
                    } else {
                        for i in 0..row_bytes {
                            current_row[i] = row_data[i].wrapping_add(prev_row[i]);
                        }
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

    fn apply_png_predictor(
        data: &[u8],
        columns: usize,
        colors: usize,
        bits_per_component: usize,
    ) -> Result<Vec<u8>> {
        Self::apply_png_predictor_impl(data, columns, colors, bits_per_component, true)
    }

    #[cfg(test)]
    fn apply_png_predictor_scalar(
        data: &[u8],
        columns: usize,
        colors: usize,
        bits_per_component: usize,
    ) -> Result<Vec<u8>> {
        Self::apply_png_predictor_impl(data, columns, colors, bits_per_component, false)
    }

    /// Paeth predictor function used in PNG filtering.
    const fn paeth_predictor(left: u8, above: u8, upper_left: u8) -> u8 {
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

        for cap in re.captures_iter(self.data.as_slice()) {
            let objid = match std::str::from_utf8(&cap[1])
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(value) if value <= u32::MAX as u64 => value as u32,
                _ => continue,
            };
            let genno = match std::str::from_utf8(&cap[2])
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(value) if value <= u32::MAX as u64 => value as u32,
                _ => continue,
            };
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
            let data = &self.data.as_slice()[trailer_pos..];
            // Skip "trailer" and whitespace
            let mut skip = 7;
            while skip < data.len()
                && (data[skip] == b' ' || data[skip] == b'\n' || data[skip] == b'\r')
            {
                skip += 1;
            }
            if data[skip..].starts_with(b"<<") {
                let mut parser = PDFParser::new(&data[skip..]);
                if let Ok(trailer_obj) = parser.parse_object()
                    && let Ok(dict) = trailer_obj.as_dict()
                {
                    xref.trailer = dict.clone();
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
            .find(|&i| &self.data.as_slice()[i..i + search.len()] == search)
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
        Ok((*self.getobj_shared(objid)?).clone())
    }

    /// Get an object by ID without cloning the cached object.
    pub fn getobj_shared(&self, objid: u32) -> Result<Arc<PDFObject>> {
        if objid == 0 {
            return Err(PdfError::ObjectNotFound(0));
        }

        // Thread-local cycle detection to prevent infinite recursion.
        // Using thread-local instead of shared RwLock avoids race conditions
        // when multiple threads resolve the same object concurrently.
        thread_local! {
            static RESOLVING: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
        }

        struct ThreadLocalGuard {
            objid: u32,
        }

        impl Drop for ThreadLocalGuard {
            fn drop(&mut self) {
                RESOLVING.with(|set| {
                    set.borrow_mut().remove(&self.objid);
                });
            }
        }

        // Check for circular reference within this thread's call stack
        let is_circular = RESOLVING.with(|set| {
            let mut borrowed = set.borrow_mut();
            if borrowed.contains(&objid) {
                true
            } else {
                borrowed.insert(objid);
                false
            }
        });

        if is_circular {
            return Err(PdfError::SyntaxError(format!(
                "circular reference detected for obj {}",
                objid
            )));
        }

        let _guard = ThreadLocalGuard { objid };

        // Check cache first
        if let Ok(mut cache) = self.cache.lock()
            && let Some(obj) = cache.get(objid)
        {
            return Ok(obj);
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

                let obj = Arc::new(obj);
                if let Ok(mut cache) = self.cache.lock() {
                    cache.insert(objid, Arc::clone(&obj));
                }
                return Ok(obj);
            }
        }

        // Fallback: scan object streams directly when xrefs are incomplete.
        if !self.all_xrefs_are_fallback()
            && let Ok(Some(obj)) = self.find_obj_in_objstms(objid)
        {
            let obj = Arc::new(obj);
            if let Ok(mut cache) = self.cache.lock() {
                cache.insert(objid, Arc::clone(&obj));
            }
            return Ok(obj);
        }

        Err(PdfError::ObjectNotFound(objid))
    }

    /// Fallback scan for an object inside any ObjStm stream.
    fn find_obj_in_objstms(&self, objid: u32) -> Result<Option<PDFObject>> {
        use regex::bytes::Regex;

        if let Ok(index_guard) = self.objstm_index.read()
            && let Some(index) = index_guard.as_ref()
        {
            if let Some((stream_objid, idx)) = index.get(&objid).copied() {
                return Ok(Some(self.parse_object_from_stream(stream_objid, idx)?));
            }
            return Ok(None);
        }

        let re = Regex::new(r"(\d+)\s+(\d+)\s+obj\b").unwrap();
        let mut index: HashMap<u32, (u32, usize)> = HashMap::new();
        for cap in re.captures_iter(self.data.as_slice()) {
            let stream_objid: u32 = match std::str::from_utf8(&cap[1])
                .ok()
                .and_then(|s| s.parse().ok())
            {
                Some(v) => v,
                None => continue,
            };
            let genno: u16 = match std::str::from_utf8(&cap[2])
                .ok()
                .and_then(|s| s.parse().ok())
            {
                Some(v) => v,
                None => continue,
            };
            let pos = cap.get(0).map(|m| m.start()).unwrap_or(0);

            let obj = match self.parse_object_at(pos, stream_objid, true) {
                Ok(o) => o,
                Err(_) => continue,
            };
            let stream = match obj.as_stream() {
                Ok(s) => s,
                Err(_) => continue,
            };
            match stream.get("Type") {
                Some(PDFObject::Name(name)) if name == "ObjStm" => {}
                _ => continue,
            }

            let data = match self.decode_stream_with_objid(stream, stream_objid, genno) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let n = match stream.get("N").and_then(|v| v.as_int().ok()) {
                Some(n) => n as usize,
                None => continue,
            };
            let first = match stream.get("First").and_then(|v| v.as_int().ok()) {
                Some(f) => f as usize,
                None => continue,
            };

            if first > data.len() {
                continue;
            }

            let mut header_parser = PDFParser::new(&data[..first]);
            for i in 0..n {
                let obj_id = match header_parser.parse_object().and_then(|o| o.as_int()) {
                    Ok(id) => id as u32,
                    Err(_) => break,
                };
                let _offset = match header_parser.parse_object().and_then(|o| o.as_int()) {
                    Ok(off) => off as usize,
                    Err(_) => break,
                };
                index.entry(obj_id).or_insert((stream_objid, i));
            }
        }

        if let Ok(mut index_guard) = self.objstm_index.write() {
            *index_guard = Some(index.clone());
        }

        if let Some((stream_objid, idx)) = index.get(&objid).copied() {
            return Ok(Some(self.parse_object_from_stream(stream_objid, idx)?));
        }

        Ok(None)
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
        let stream_obj = self.getobj_shared(stream_objid)?;
        let stream = stream_obj.as_ref().as_stream()?;

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
        let mut cursor = offset;
        let mut data = &self.data.as_slice()[offset..];

        // Parse "objid genno obj"
        let (_objid, consumed1) = self.read_number(data)?;
        cursor += consumed1;
        data = &data[consumed1..];

        // Skip whitespace
        let mut skip = 0;
        while skip < data.len()
            && (data[skip] == b' ' || data[skip] == b'\n' || data[skip] == b'\r')
        {
            skip += 1;
        }
        cursor += skip;
        data = &data[skip..];

        let (_genno, consumed2) = self.read_number(data)?;
        cursor += consumed2;
        data = &data[consumed2..];

        // Skip whitespace
        let mut skip = 0;
        while skip < data.len()
            && (data[skip] == b' ' || data[skip] == b'\n' || data[skip] == b'\r')
        {
            skip += 1;
        }
        cursor += skip;
        data = &data[skip..];

        // Expect "obj"
        if !data.starts_with(b"obj") {
            return Err(PdfError::SyntaxError(format!(
                "expected 'obj' at offset {}, got {:?}",
                offset,
                String::from_utf8_lossy(&data[..std::cmp::min(10, data.len())])
            )));
        }
        cursor += 3;
        data = &data[3..];

        // Skip whitespace
        let mut skip = 0;
        while skip < data.len()
            && (data[skip] == b' ' || data[skip] == b'\n' || data[skip] == b'\r')
        {
            skip += 1;
        }
        cursor += skip;
        data = &data[skip..];

        // Parse the object
        let mut parser = PDFParser::new(data);
        let obj = parser.parse_object()?;
        let base_pos = parser.tell();

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

                // For critical streams (XRef/ObjStm), prefer endstream scan for robustness.
                let force_scan = matches!(
                    dict.get("Type"),
                    Some(PDFObject::Name(name)) if name == "XRef" || name == "ObjStm"
                );

                // Get length from dict (resolve indirect refs) unless fallback/force_scan
                let length: usize = if fallback || force_scan {
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
                let remaining_len = remaining.len();
                let stream_start_abs = cursor + base_pos + stream_start;

                let stream_data = if fallback || force_scan || length == 0 {
                    // Fallback mode or missing length: scan for endstream
                    if let Some(end_pos) = Self::find_endstream(&remaining[stream_start..]) {
                        let end = (stream_start_abs + end_pos).min(self.data.len());
                        self.data.as_bytes().slice(stream_start_abs..end)
                    } else {
                        self.data.as_bytes().slice(stream_start_abs..)
                    }
                } else if stream_start + length <= remaining_len {
                    // Trust declared /Length when it fits in the remaining buffer
                    self.data
                        .as_bytes()
                        .slice(stream_start_abs..stream_start_abs + length)
                } else {
                    // Length looks corrupted; fall back to endstream scan
                    if let Some(end_pos) = Self::find_endstream(&remaining[stream_start..]) {
                        let end = (stream_start_abs + end_pos).min(self.data.len());
                        self.data.as_bytes().slice(stream_start_abs..end)
                    } else {
                        self.data.as_bytes().slice(stream_start_abs..)
                    }
                };

                return Ok(PDFObject::Stream(Box::new(
                    crate::model::objects::PDFStream::new(dict.clone(), stream_data),
                )));
            }
        }

        Ok(obj)
    }

    fn find_endstream_simd(data: &[u8]) -> Option<usize> {
        let needle = b"endstream";
        if data.len() < needle.len() {
            return None;
        }
        for pos in 0..=data.len() - needle.len() {
            if &data[pos..pos + needle.len()] == needle {
                let mut end = pos;
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

    fn find_endstream(data: &[u8]) -> Option<usize> {
        Self::find_endstream_simd(data)
    }

    /// Get document catalog.
    pub const fn catalog(&self) -> &HashMap<String, PDFObject> {
        &self.catalog
    }

    /// Get document info dictionaries.
    pub const fn info(&self) -> &Vec<HashMap<String, PDFObject>> {
        &self.info
    }

    /// Check if the document is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.security_handler.is_some()
    }

    /// Resolve a reference to its actual object.
    pub fn resolve(&self, obj: &PDFObject) -> Result<PDFObject> {
        Ok((*self.resolve_shared(obj)?).clone())
    }

    /// Resolve a reference to its actual object without cloning.
    pub fn resolve_shared(&self, obj: &PDFObject) -> Result<Arc<PDFObject>> {
        self.resolve_internal_shared(obj)
    }

    /// Internal resolve that doesn't require mutable access.
    fn resolve_internal(&self, obj: &PDFObject) -> Result<PDFObject> {
        Ok((*self.resolve_shared(obj)?).clone())
    }

    fn resolve_internal_shared(&self, obj: &PDFObject) -> Result<Arc<PDFObject>> {
        let mut seen = std::collections::HashSet::new();
        let mut current = match obj {
            PDFObject::Ref(r) => {
                seen.insert(r.objid);
                self.getobj_shared(r.objid)?
            }
            _ => return Ok(Arc::new(obj.clone())),
        };
        loop {
            match current.as_ref() {
                PDFObject::Ref(r) => {
                    if !seen.insert(r.objid) {
                        return Err(PdfError::SyntaxError(format!(
                            "circular reference detected for obj {}",
                            r.objid
                        )));
                    }
                    current = self.getobj_shared(r.objid)?;
                }
                _ => return Ok(current),
            }
        }
    }

    /// Get the total page count from the Pages tree.
    fn get_page_count(&self) -> usize {
        if let Some(pages_ref) = self.catalog.get("Pages")
            && let Ok(pages) = self.resolve_internal(pages_ref)
            && let Ok(dict) = pages.as_dict()
            && let Some(count) = dict.get("Count")
            && let Ok(n) = count.as_int()
        {
            return n as usize;
        }
        0
    }

    /// Get the total number of pages in the document.
    pub fn page_count(&self) -> usize {
        self.get_page_count()
    }

    /// Get mediaboxes for all pages in the document.
    pub fn page_mediaboxes(&self) -> Result<Vec<[f64; 4]>> {
        self.page_index().mediaboxes(self)
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

    /// Get object IDs for each xref table.
    pub fn get_xref_objids(&self) -> Vec<Vec<u32>> {
        self.xrefs
            .iter()
            .map(|xref| {
                let mut ids: Vec<u32> = xref.get_objids().collect();
                ids.sort();
                ids
            })
            .collect()
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
        if let Some(names_ref) = self.catalog.get("Names")
            && let Ok(names) = self.resolve_internal(names_ref)
            && let Ok(names_dict) = names.as_dict()
            && let Some(dests_ref) = names_dict.get("Dests")
            && let Ok(dests) = self.resolve_internal(dests_ref)
            && let Some(result) = self.lookup_name_tree(&dests, name)?
        {
            return Ok(result);
        }

        // Try catalog Dests dict (PDF 1.1)
        if let Some(dests_ref) = self.catalog.get("Dests")
            && let Ok(dests) = self.resolve_internal(dests_ref)
            && let Ok(dests_dict) = dests.as_dict()
        {
            let name_str = String::from_utf8_lossy(name);
            if let Some(dest) = dests_dict.get(name_str.as_ref()) {
                return self.resolve_internal(dest);
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
        if let Some(names_arr) = dict.get("Names")
            && let Ok(arr) = self.resolve_internal(names_arr)?.as_array()
        {
            // Names array is pairs: [name1, value1, name2, value2, ...]
            let mut i = 0;
            while i + 1 < arr.len() {
                if let Ok(key) = arr[i].as_string()
                    && key == name
                {
                    return Ok(Some(self.resolve_internal(&arr[i + 1])?));
                }
                i += 2;
            }
        }

        // Check Kids array (intermediate node)
        if let Some(kids) = dict.get("Kids")
            && let Ok(kids_arr) = self.resolve_internal(kids)?.as_array()
        {
            for kid in kids_arr {
                if let Ok(kid_obj) = self.resolve_internal(kid) {
                    // Check Limits to optimize search
                    if let Ok(kid_dict) = kid_obj.as_dict()
                        && let Some(limits) = kid_dict.get("Limits")
                        && let Ok(limits_arr) = limits.as_array()
                        && limits_arr.len() >= 2
                    {
                        let min = limits_arr[0].as_string().unwrap_or(&[]);
                        let max = limits_arr[1].as_string().unwrap_or(&[]);
                        if name < min || name > max {
                            continue;
                        }
                    }
                    if let Some(result) = self.lookup_name_tree(&kid_obj, name)? {
                        return Ok(Some(result));
                    }
                }
            }
        }

        Ok(None)
    }
}

/// Page label iterator.
pub struct PageLabels<'a> {
    #[allow(dead_code)]
    doc: &'a PDFDocument,
    ranges: Vec<(i64, HashMap<String, PDFObject>)>,
    range_idx: usize,
    current_value: i64,
    current_page: i64,
    range_end: Option<i64>,
    style: Option<String>,
    prefix: String,
    page_count: usize,
}

impl<'a> PageLabels<'a> {
    fn new(doc: &'a PDFDocument, obj: PDFObject, page_count: usize) -> Result<Self> {
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
        doc: &PDFDocument,
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
        if let Some(end) = self.range_end
            && self.current_page >= end
        {
            self.init_range(self.range_idx + 1);
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
    use crate::pdfpage::{reset_page_create_count, take_page_create_count};

    fn build_minimal_pdf_with_pages(page_count: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");

        let mut offsets: Vec<usize> = Vec::new();
        let push_obj = |buf: &mut Vec<u8>, obj: String, offsets: &mut Vec<usize>| {
            offsets.push(buf.len());
            buf.extend_from_slice(obj.as_bytes());
        };

        push_obj(
            &mut out,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
            &mut offsets,
        );

        let kids: String = (0..page_count)
            .map(|i| format!("{} 0 R", 3 + i))
            .collect::<Vec<_>>()
            .join(" ");
        push_obj(
            &mut out,
            format!(
                "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
                kids, page_count
            ),
            &mut offsets,
        );

        for i in 0..page_count {
            let page_id = 3 + i;
            let contents_id = 3 + page_count + i;
            push_obj(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents {} 0 R >>\nendobj\n",
                    page_id, contents_id
                ),
                &mut offsets,
            );
        }

        for i in 0..page_count {
            let contents_id = 3 + page_count + i;
            push_obj(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n",
                    contents_id
                ),
                &mut offsets,
            );
        }

        let xref_pos = out.len();
        let obj_count = offsets.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", obj_count + 1).as_bytes(),
        );
        for offset in offsets {
            out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }
        out.extend_from_slice(b"trailer\n<< /Size ");
        out.extend_from_slice((obj_count + 1).to_string().as_bytes());
        out.extend_from_slice(b" /Root 1 0 R >>\nstartxref\n");
        out.extend_from_slice(xref_pos.to_string().as_bytes());
        out.extend_from_slice(b"\n%%EOF");

        out
    }

    #[test]
    fn test_get_page_cached_reuses_page() {
        let pdf = build_minimal_pdf_with_pages(2);
        let doc = PDFDocument::new(pdf, "").unwrap();
        reset_page_create_count(&doc);

        let _ = doc.get_page_cached(0).unwrap();
        assert_eq!(take_page_create_count(&doc), 1);

        let _ = doc.get_page_cached(0).unwrap();
        assert_eq!(take_page_create_count(&doc), 0);
    }

    /// Test that PDFDocument can be created from owned data and stored.
    /// This requires owned Bytes - will fail with borrowed reference design.
    #[test]
    fn test_pdfdocument_owns_data() {
        fn create_doc() -> PDFDocument {
            let data = std::fs::read("tests/fixtures/simple1.pdf").unwrap();
            PDFDocument::new(data, "").unwrap()
        }
        let _doc = create_doc();
    }

    /// Test that PDFDocument is Send + Sync (thread-safe).
    #[test]
    fn test_pdfdocument_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PDFDocument>();
    }

    #[test]
    fn test_pdfdocument_mmap_uses_shared_bytes() {
        use std::fs::File;

        let path = format!("{}/tests/fixtures/simple1.pdf", env!("CARGO_MANIFEST_DIR"));
        let file = File::open(path).unwrap();
        // Safety: the file handle remains open for the duration of the map.
        let mmap = unsafe { Mmap::map(&file) }.unwrap();
        let doc = PDFDocument::new_from_mmap(mmap, "").unwrap();
        match doc.data {
            PdfBytes::Shared(_) => {}
            _ => panic!("expected PdfBytes::Shared for mmap input"),
        }
    }

    #[test]
    fn test_stream_rawdata_is_slice_of_document_bytes() {
        use bytes::Bytes;

        let pdf = b"%PDF-1.4\n1 0 obj\n<< /Length 11 >>\nstream\nhello world\nendstream\nendobj\n";
        let bytes = Bytes::from_static(pdf);
        let base_ptr = bytes.as_ptr();
        let doc = PDFDocument::new_from_bytes(bytes.clone(), "").unwrap();
        let obj = doc.getobj(1).unwrap();
        let stream = obj.as_stream().unwrap();
        let raw = stream.get_rawdata();

        assert_eq!(raw, b"hello world");

        let needle = b"stream\n";
        let stream_pos = pdf
            .windows(needle.len())
            .position(|window| window == needle)
            .expect("stream marker not found");
        let stream_start = stream_pos + needle.len();
        // Safety: stream_start was computed within the bounds of `pdf`.
        let expected_ptr = unsafe { base_ptr.add(stream_start) };

        assert_eq!(raw.as_ptr(), expected_ptr);
    }

    #[test]
    fn test_decode_stream_bytes_no_filters_is_zero_copy() {
        use bytes::Bytes;

        let pdf = b"%PDF-1.4\n1 0 obj\n<< /Length 11 >>\nstream\nhello world\nendstream\nendobj\n";
        let bytes = Bytes::from_static(pdf);
        let base_ptr = bytes.as_ptr();
        let doc = PDFDocument::new_from_bytes(bytes.clone(), "").unwrap();
        let obj = doc.getobj(1).unwrap();
        let stream = obj.as_stream().unwrap();
        let decoded = doc.decode_stream_bytes(stream).unwrap();

        assert_eq!(&decoded[..], b"hello world");

        let needle = b"stream\n";
        let stream_pos = pdf
            .windows(needle.len())
            .position(|window| window == needle)
            .expect("stream marker not found");
        let stream_start = stream_pos + needle.len();
        // Safety: stream_start was computed within the bounds of `pdf`.
        let expected_ptr = unsafe { base_ptr.add(stream_start) };

        assert_eq!(decoded.as_ptr(), expected_ptr);
    }

    #[test]
    fn test_getobj_shared_cache_returns_same_arc() {
        use std::sync::Arc;

        let path = format!("{}/tests/fixtures/simple1.pdf", env!("CARGO_MANIFEST_DIR"));
        let data = std::fs::read(path).unwrap();
        let doc = PDFDocument::new(data, "").unwrap();
        let obj1 = doc.getobj_shared(1).unwrap();
        let obj2 = doc.getobj_shared(1).unwrap();
        assert!(Arc::ptr_eq(&obj1, &obj2));
    }

    #[test]
    fn test_getobj_shared_no_cache_returns_new_arc() {
        use std::sync::Arc;

        let path = format!("{}/tests/fixtures/simple1.pdf", env!("CARGO_MANIFEST_DIR"));
        let data = std::fs::read(path).unwrap();
        let doc = PDFDocument::new_with_cache(data, "", 0).unwrap();
        let obj1 = doc.getobj_shared(1).unwrap();
        let obj2 = doc.getobj_shared(1).unwrap();
        assert!(!Arc::ptr_eq(&obj1, &obj2));
    }

    #[test]
    fn test_object_cache_lru_evicts_oldest() {
        use std::sync::Arc;

        let path = format!("{}/tests/fixtures/simple1.pdf", env!("CARGO_MANIFEST_DIR"));
        let data = std::fs::read(path).unwrap();
        let doc = PDFDocument::new_with_cache(data, "", 1).unwrap();
        let obj1 = doc.getobj_shared(1).unwrap();
        let _obj2 = doc.getobj_shared(2).unwrap();
        let obj1_again = doc.getobj_shared(1).unwrap();
        assert!(!Arc::ptr_eq(&obj1, &obj1_again));
    }

    #[test]
    fn font_encoding_cache_reuses_entry() {
        use std::sync::Arc;

        let pdf = build_minimal_pdf_with_pages(1);
        let doc = PDFDocument::new(pdf, "").unwrap();
        let encoding = PDFObject::Name("WinAnsiEncoding".to_string());

        let a = doc.get_or_build_font_encoding(42, &encoding).unwrap();
        let b = doc.get_or_build_font_encoding(42, &encoding).unwrap();

        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn test_page_mediaboxes_does_not_create_pages() {
        let pdf = build_minimal_pdf_with_pages(3);
        let doc = PDFDocument::new(pdf, "").unwrap();
        crate::pdfpage::reset_page_create_count(&doc);

        let boxes = doc.page_mediaboxes().unwrap();
        assert_eq!(boxes.len(), 3);
        let created = crate::pdfpage::take_page_create_count(&doc);
        assert_eq!(created, 0);
    }

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

    #[test]
    fn find_endstream_simd_trims_whitespace() {
        let data = b"abc  \nendstream";
        let end = PDFDocument::find_endstream_simd(data).unwrap();
        assert_eq!(&data[..end], b"abc");
    }

    #[test]
    fn find_startxref_simd_matches_scalar() {
        let data = b"trailer\nstartxref\n123\n%%EOF";
        let pos = PDFDocument::find_startxref_simd(data).unwrap();
        assert_eq!(&data[pos..pos + 9], b"startxref");
    }

    #[test]
    fn find_startxref_returns_last_occurrence() {
        let data = b"startxref\n1\nstartxref\n2\n%%EOF";
        let pos = PDFDocument::find_startxref_simd(data).unwrap();
        assert_eq!(&data[pos..pos + 9], b"startxref");
        assert_eq!(&data[pos + 9..pos + 11], b"\n2");
    }

    #[test]
    fn png_predictor_up_simd_matches_scalar() {
        let data = [
            2, 1, 2, 3, 4, // row1 (Up)
            2, 4, 3, 2, 1, // row2 (Up)
        ];
        let scalar = PDFDocument::apply_png_predictor_scalar(&data, 4, 1, 8).unwrap();
        let simd = PDFDocument::apply_png_predictor(&data, 4, 1, 8).unwrap();
        assert_eq!(scalar, simd);
    }
}
