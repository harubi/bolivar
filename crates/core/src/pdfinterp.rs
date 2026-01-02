//! PDF content stream interpreter.
//!
//! Port of pdfminer.six pdfinterp.py - PDFContentParser and PDFResourceManager classes.
//!
//! PDFContentParser parses PDF content streams containing operators
//! like BT, ET, Tm, Tj, as well as inline images (BI/ID/EI).
//!
//! PDFResourceManager facilitates reuse of shared resources such as fonts
//! and color spaces so that large objects are not allocated multiple times.

use crate::cmapdb::{CMap, CMapDB};
use crate::error::{PdfError, Result};
use crate::pdfcolor::{PDFColorSpace, PREDEFINED_COLORSPACE};
use crate::pdftypes::{PDFObject, PDFStream};
use crate::psparser::{Keyword, PSBaseParser, PSLiteral, PSToken};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// Token types produced by PDFContentParser.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentToken {
    /// An operand (number, string, array, dict, literal)
    Operand(PSToken),
    /// A keyword/operator (BT, ET, Tj, etc.)
    Keyword(Keyword),
    /// An inline image with dictionary and data
    InlineImage {
        dict: HashMap<String, PSToken>,
        data: Vec<u8>,
    },
}

/// Context frame for tracking array/dict/proc construction
#[derive(Debug)]
enum Context {
    Array(usize, Vec<PSToken>),
    Dict(usize, Vec<PSToken>),
    Proc(usize, Vec<PSToken>),
}

/// Parser for PDF content streams.
///
/// Content streams contain a sequence of operators and operands.
/// Operands precede their operator. Special handling is needed
/// for inline images (BI/ID/EI sequence).
pub struct PDFContentParser {
    /// Concatenated data from all streams
    data: Rc<[u8]>,
    /// Current position in data
    pos: usize,
    /// Pending tokens (for buffering during inline image handling)
    pending: VecDeque<(usize, ContentToken)>,
    /// Current operand stack
    operand_stack: Vec<(usize, PSToken)>,
    /// Context stack for nested arrays/dicts/procs
    context_stack: Vec<Context>,
    /// Whether we're collecting inline image dictionary
    in_inline_dict: bool,
    /// Base parser reused for tokenization
    base_parser: PSBaseParser<'static>,
}

impl PDFContentParser {
    /// Create a new content parser from one or more content streams.
    pub fn new(streams: Vec<Vec<u8>>) -> Self {
        // Concatenate all streams with space separators
        let mut data = Vec::new();
        for (i, stream) in streams.into_iter().enumerate() {
            if i > 0 && !data.is_empty() {
                data.push(b' ');
            }
            data.extend(stream);
        }

        let data: Rc<[u8]> = Rc::from(data.into_boxed_slice());
        let mut base_parser = PSBaseParser::new_shared(data.clone());
        base_parser.set_pos(0);

        Self {
            data,
            pos: 0,
            pending: VecDeque::new(),
            operand_stack: Vec::new(),
            context_stack: Vec::new(),
            in_inline_dict: false,
            base_parser,
        }
    }

    /// Get next token with its position.
    pub fn next_with_pos(&mut self) -> Option<(usize, ContentToken)> {
        // Return pending tokens first
        if let Some(tok) = self.pending.pop_front() {
            return Some(tok);
        }

        loop {
            // Parse next token from base parser
            let (rel_pos, token) = match self.base_parser.next_token() {
                Some(Ok(t)) => t,
                Some(Err(_)) => {
                    // Skip bad token and continue
                    self.pos += 1;
                    self.base_parser.set_pos(self.pos);
                    continue;
                }
                None => {
                    // No more tokens - flush remaining operands
                    if !self.operand_stack.is_empty() {
                        for (pos, op) in self.operand_stack.drain(..) {
                            self.pending.push_back((pos, ContentToken::Operand(op)));
                        }
                        if let Some(tok) = self.pending.pop_front() {
                            return Some(tok);
                        }
                    }
                    return None;
                }
            };

            let abs_pos = rel_pos;
            self.pos = self.base_parser.tell();
            self.base_parser.set_pos(self.pos);

            match &token {
                PSToken::Keyword(kw) => {
                    // Handle structure keywords
                    match kw {
                        Keyword::ArrayStart => {
                            self.context_stack.push(Context::Array(abs_pos, Vec::new()));
                            continue;
                        }
                        Keyword::ArrayEnd => {
                            if let Some(Context::Array(arr_pos, items)) = self.context_stack.pop() {
                                let array_token = PSToken::Array(items);
                                if self.context_stack.is_empty() {
                                    // Top-level array - push to operand stack
                                    self.operand_stack.push((arr_pos, array_token));
                                } else {
                                    // Nested - push to parent context
                                    self.push_to_context(array_token);
                                }
                            }
                            continue;
                        }
                        Keyword::DictStart => {
                            self.context_stack.push(Context::Dict(abs_pos, Vec::new()));
                            continue;
                        }
                        Keyword::DictEnd => {
                            if let Some(Context::Dict(dict_pos, items)) = self.context_stack.pop() {
                                let dict = Self::build_dict(items);
                                let dict_token = PSToken::Dict(dict);
                                if self.context_stack.is_empty() {
                                    self.operand_stack.push((dict_pos, dict_token));
                                } else {
                                    self.push_to_context(dict_token);
                                }
                            }
                            continue;
                        }
                        Keyword::BraceOpen => {
                            self.context_stack.push(Context::Proc(abs_pos, Vec::new()));
                            continue;
                        }
                        Keyword::BraceClose => {
                            if let Some(Context::Proc(proc_pos, items)) = self.context_stack.pop() {
                                let proc_token = PSToken::Array(items);
                                if self.context_stack.is_empty() {
                                    self.operand_stack.push((proc_pos, proc_token));
                                } else {
                                    self.push_to_context(proc_token);
                                }
                            }
                            continue;
                        }
                        Keyword::BI => {
                            self.in_inline_dict = true;
                            self.operand_stack.clear();
                            continue;
                        }
                        Keyword::ID if self.in_inline_dict => {
                            self.in_inline_dict = false;
                            let dict = self.build_inline_dict();
                            let eos = self.get_inline_eos(&dict);
                            let (img_data, consumed) = self.get_inline_data(&eos);
                            self.pos += consumed;
                            self.base_parser.set_pos(self.pos);
                            return Some((
                                abs_pos,
                                ContentToken::InlineImage {
                                    dict,
                                    data: img_data,
                                },
                            ));
                        }
                        Keyword::EI => {
                            // Already handled by ID processing
                            continue;
                        }
                        _ => {} // Fall through to normal keyword handling
                    }

                    // If we're inside inline dict, treat keyword as operand
                    if self.in_inline_dict {
                        self.operand_stack.push((abs_pos, token));
                        continue;
                    }

                    // If we're inside array/dict/proc context, push keyword as operand
                    if !self.context_stack.is_empty() {
                        self.push_to_context(token);
                        continue;
                    }

                    // Regular operator - flush operand stack and return keyword
                    for (pos, op) in self.operand_stack.drain(..) {
                        self.pending.push_back((pos, ContentToken::Operand(op)));
                    }
                    self.pending
                        .push_back((abs_pos, ContentToken::Keyword(kw.clone())));

                    if let Some(tok) = self.pending.pop_front() {
                        return Some(tok);
                    }
                }
                _ => {
                    if self.in_inline_dict {
                        self.operand_stack.push((abs_pos, token));
                    } else if !self.context_stack.is_empty() {
                        self.push_to_context(token);
                    } else {
                        self.operand_stack.push((abs_pos, token));
                    }
                    continue;
                }
            }
        }
    }

    /// Push a token to the current context (array/dict/proc)
    fn push_to_context(&mut self, token: PSToken) {
        if let Some(ctx) = self.context_stack.last_mut() {
            match ctx {
                Context::Array(_, items) => items.push(token),
                Context::Dict(_, items) => items.push(token),
                Context::Proc(_, items) => items.push(token),
            }
        }
    }

    /// Build dictionary from key-value pairs
    fn build_dict(items: Vec<PSToken>) -> HashMap<String, PSToken> {
        let mut dict = HashMap::new();
        let mut iter = items.into_iter();
        while let Some(key) = iter.next() {
            if let PSToken::Literal(name) = key {
                if let Some(value) = iter.next() {
                    dict.insert(name, value);
                }
            }
        }
        dict
    }

    /// Build dictionary from collected operands (for inline images)
    fn build_inline_dict(&self) -> HashMap<String, PSToken> {
        let mut dict = HashMap::new();
        let mut iter = self.operand_stack.iter();
        while let Some((_, key)) = iter.next() {
            if let PSToken::Literal(name) = key {
                if let Some((_, value)) = iter.next() {
                    dict.insert(name.clone(), value.clone());
                }
            }
        }
        dict
    }

    /// Determine end-of-stream marker for inline image.
    fn get_inline_eos(&self, dict: &HashMap<String, PSToken>) -> Vec<u8> {
        let filter = dict.get("F").or_else(|| dict.get("Filter"));

        if let Some(PSToken::Literal(name)) = filter {
            if name == "A85" || name == "ASCII85Decode" {
                return b"~>".to_vec();
            }
        }

        if let Some(PSToken::Array(filters)) = filter {
            if let Some(PSToken::Literal(name)) = filters.first() {
                if name == "A85" || name == "ASCII85Decode" {
                    return b"~>".to_vec();
                }
            }
        }

        b"EI".to_vec()
    }

    /// Get inline image data by scanning for end marker.
    fn get_inline_data(&self, target: &[u8]) -> (Vec<u8>, usize) {
        let remaining = &self.data[self.pos..];

        // Skip leading whitespace after ID
        let mut start = 0;
        while start < remaining.len() && is_whitespace(remaining[start]) {
            start += 1;
        }

        let data_start = start;
        let mut i = data_start;

        while i < remaining.len() {
            if remaining[i..].starts_with(target) {
                let after = i + target.len();
                if after >= remaining.len() || is_whitespace(remaining[after]) {
                    let mut data = remaining[data_start..i].to_vec();
                    while data.last() == Some(&b'\r') || data.last() == Some(&b'\n') {
                        data.pop();
                    }
                    let consumed = after + if after < remaining.len() { 1 } else { 0 };
                    return (data, consumed);
                }
            }
            i += 1;
        }

        (remaining[data_start..].to_vec(), remaining.len())
    }
}

impl Iterator for PDFContentParser {
    type Item = ContentToken;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_with_pos().map(|(_, token)| token)
    }
}

/// Check if byte is PDF whitespace.
fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x00' | b'\x0c')
}

// ============================================================================
// PDFResourceManager
// ============================================================================

/// Unique identifier for a cached font.
pub type FontId = u64;

/// Repository of shared resources.
///
/// ResourceManager facilitates reuse of shared resources such as fonts
/// and images so that large objects are not allocated multiple times.
///
/// Port of pdfminer.six PDFResourceManager class.
pub struct PDFResourceManager {
    /// Whether caching is enabled
    caching: bool,
    /// Cached fonts: objid -> FontId
    cached_fonts: HashMap<u64, FontId>,
    /// Counter for generating unique font IDs
    next_font_id: FontId,
}

impl PDFResourceManager {
    /// Create a new PDFResourceManager with caching enabled.
    pub fn new() -> Self {
        Self::with_caching(true)
    }

    /// Create a new PDFResourceManager with specified caching behavior.
    pub fn with_caching(caching: bool) -> Self {
        Self {
            caching,
            cached_fonts: HashMap::new(),
            next_font_id: 1,
        }
    }

    /// Check if caching is enabled.
    pub fn caching_enabled(&self) -> bool {
        self.caching
    }

    /// Process a ProcSet array.
    ///
    /// In PDF, ProcSet defines which procedure sets are needed.
    /// This is largely obsolete and we just log/ignore like Python does.
    pub fn get_procset(&self, _procs: &[&str]) {
        // Matches Python behavior: essentially a no-op
        // Python iterates procs and checks for LITERAL_PDF/LITERAL_TEXT
        // but doesn't do anything meaningful with them
    }

    /// Get a predefined color space by name.
    ///
    /// Returns None if the color space is not in the predefined set.
    pub fn get_colorspace(&self, name: &str) -> Option<PDFColorSpace> {
        PREDEFINED_COLORSPACE.get(name).cloned()
    }

    /// Get or create a font from the specification.
    ///
    /// If objid is provided and caching is enabled, returns cached font
    /// if already loaded. Otherwise creates a new font entry.
    ///
    /// Returns a FontId that can be used to reference the font.
    pub fn get_font(&mut self, objid: Option<u64>, _spec: &HashMap<String, PDFObject>) -> FontId {
        // Check cache if objid provided and caching enabled
        if let Some(id) = objid {
            if self.caching {
                if let Some(&font_id) = self.cached_fonts.get(&id) {
                    return font_id;
                }
            }
        }

        // Create new font entry
        let font_id = self.next_font_id;
        self.next_font_id += 1;

        // Cache if objid provided and caching enabled
        if let Some(id) = objid {
            if self.caching {
                self.cached_fonts.insert(id, font_id);
            }
        }

        font_id
    }

    /// Get a CMap by name.
    ///
    /// If strict is true and the CMap is not found, returns an error.
    /// If strict is false and the CMap is not found, returns an empty CMap.
    ///
    /// Currently only handles Identity CMaps (Identity-H, Identity-V,
    /// DLIdent-H, DLIdent-V). Other CMaps will be loaded from embedded
    /// data in a future implementation.
    pub fn get_cmap(&self, cmapname: &str, strict: bool) -> Result<CMap> {
        // Check for identity CMaps first
        if CMapDB::is_identity_cmap(cmapname) || CMapDB::is_identity_cmap_byte(cmapname) {
            // For identity CMaps, return an empty CMap with appropriate vertical mode
            // The actual identity mapping is handled by IdentityCMap/IdentityCMapByte types
            let mut cmap = CMap::new();
            cmap.set_vertical(CMapDB::is_vertical(cmapname));
            cmap.attrs
                .insert("CMapName".to_string(), cmapname.to_string());
            return Ok(cmap);
        }

        // CMap not found - either error or return empty CMap
        if strict {
            Err(PdfError::CMapNotFound(cmapname.to_string()))
        } else {
            Ok(CMap::new())
        }
    }
}

impl Default for PDFResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// PDFPageInterpreter
// ============================================================================

use crate::pdfdevice::{
    PDFDevice, PDFStackT, PDFStackValue, PDFTextSeq, PDFTextSeqItem, PathSegment,
};
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::utils::{MATRIX_IDENTITY, Matrix, mult_matrix};

/// Saved graphics state for q/Q operators.
type SavedState = (Matrix, PDFTextState, PDFGraphicState);

/// PDF Page Interpreter - executes PDF content stream operators.
///
/// Port of PDFPageInterpreter from pdfminer.six pdfinterp.py
///
/// Reference: PDF Reference, Appendix A, Operator Summary
///
/// Note: Method names like `do_Q`, `do_S`, `do_B` intentionally use uppercase
/// to match PDF operator names from the spec (q/Q, s/S, b/B, etc.).
pub struct PDFPageInterpreter<'a, D: PDFDevice> {
    /// Resource manager for fonts, color spaces, etc.
    #[allow(dead_code)]
    rsrcmgr: &'a mut PDFResourceManager,
    /// Output device for rendering operations
    device: &'a mut D,
    /// Graphics state stack for q/Q operators
    gstack: Vec<SavedState>,
    /// Current transformation matrix
    ctm: Matrix,
    /// Current text state
    textstate: PDFTextState,
    /// Current graphics state
    graphicstate: PDFGraphicState,
    /// Current path being constructed
    curpath: Vec<PathSegment>,
    /// Current point for path operations (used by v operator)
    current_point: Option<(f64, f64)>,
    /// Font map: font name -> PDFCIDFont
    fontmap: HashMap<String, std::sync::Arc<crate::pdffont::PDFCIDFont>>,
    /// Current resources dictionary (for XObject lookup fallback)
    resources: HashMap<String, PDFObject>,
    /// XObject map: name -> stream
    xobjmap: HashMap<String, PDFStream>,
    /// Inline image counter
    inline_image_id: usize,
    /// Document reference for resolving XObject resources
    doc: Option<&'a crate::pdfdocument::PDFDocument>,
}

#[allow(non_snake_case)]
impl<'a, D: PDFDevice> PDFPageInterpreter<'a, D> {
    /// Create a new PDFPageInterpreter.
    pub fn new(rsrcmgr: &'a mut PDFResourceManager, device: &'a mut D) -> Self {
        Self {
            rsrcmgr,
            device,
            gstack: Vec::new(),
            ctm: MATRIX_IDENTITY,
            textstate: PDFTextState::new(),
            graphicstate: PDFGraphicState::new(),
            curpath: Vec::new(),
            current_point: None,
            fontmap: HashMap::new(),
            resources: HashMap::new(),
            xobjmap: HashMap::new(),
            inline_image_id: 0,
            doc: None,
        }
    }

    /// Initialize graphics state for rendering.
    ///
    /// Called at the start of page rendering.
    pub fn init_state(&mut self, ctm: Matrix) {
        self.gstack.clear();
        self.ctm = ctm;
        self.device.set_ctm(self.ctm);
        self.textstate = PDFTextState::new();
        self.graphicstate = PDFGraphicState::new();
        self.curpath.clear();
        self.current_point = None;
    }

    /// Get current transformation matrix.
    pub fn ctm(&self) -> Matrix {
        self.ctm
    }

    /// Get current graphics state (read-only).
    pub fn graphicstate(&self) -> &PDFGraphicState {
        &self.graphicstate
    }

    /// Get current text state (read-only).
    pub fn textstate(&self) -> &PDFTextState {
        &self.textstate
    }

    /// Get current text state (mutable).
    pub fn textstate_mut(&mut self) -> &mut PDFTextState {
        &mut self.textstate
    }

    /// Get current path (read-only).
    pub fn current_path(&self) -> &[PathSegment] {
        &self.curpath
    }

    /// Initialize resources from a page's resource dictionary.
    ///
    /// Builds the fontmap from Font resources, parsing ToUnicode streams.
    ///
    /// Port of PDFPageInterpreter.init_resources from pdfminer.six
    pub fn init_resources(
        &mut self,
        resources: &HashMap<String, PDFObject>,
        doc: Option<&'a crate::pdfdocument::PDFDocument>,
    ) {
        self.fontmap.clear();
        self.xobjmap.clear();
        self.resources = resources.clone();

        // Get Font dictionary (optional)
        let fonts = match resources.get("Font") {
            Some(PDFObject::Dict(d)) => Some(d.clone()),
            Some(PDFObject::Ref(r)) => {
                if let Some(doc) = doc {
                    match doc.resolve(&PDFObject::Ref(r.clone())) {
                        Ok(PDFObject::Dict(d)) => Some(d),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        // Process each font
        if let Some(fonts) = fonts {
            for (fontid, spec_obj) in fonts.iter() {
                let spec = match spec_obj {
                    PDFObject::Dict(d) => d.clone(),
                    PDFObject::Ref(r) => {
                        if let Some(doc) = doc {
                            if let Ok(PDFObject::Dict(d)) = doc.resolve(&PDFObject::Ref(r.clone()))
                            {
                                d
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };

                // Get font subtype
                let subtype = spec
                    .get("Subtype")
                    .and_then(|s| s.as_name().ok())
                    .unwrap_or("")
                    .to_string();

                // Handle Type0 fonts - merge ToUnicode and Encoding into descendant font spec
                let (final_spec, tounicode_data) = if subtype == "Type0" {
                    // Get descendant font spec
                    let descendant_spec = Self::get_descendant_font_spec(&spec, doc);
                    if let Some(mut dspec) = descendant_spec {
                        // Copy ToUnicode and Encoding from Type0 to descendant
                        if let Some(v) = spec.get("ToUnicode") {
                            dspec.insert("ToUnicode".to_string(), v.clone());
                        }
                        if let Some(v) = spec.get("Encoding") {
                            dspec.insert("Encoding".to_string(), v.clone());
                        }
                        let tounicode = Self::extract_tounicode(&dspec, doc);
                        (dspec, tounicode)
                    } else {
                        let tounicode = Self::extract_tounicode(&spec, doc);
                        (spec, tounicode)
                    }
                } else {
                    let tounicode = Self::extract_tounicode(&spec, doc);
                    (spec, tounicode)
                };

                // Resolve Encoding reference if present (needed for Type1 fonts with custom encodings)
                let mut final_spec = final_spec;
                if let Some(PDFObject::Ref(r)) = final_spec.get("Encoding").cloned() {
                    if let Some(doc) = doc {
                        if let Ok(resolved) = doc.resolve(&PDFObject::Ref(r)) {
                            final_spec.insert("Encoding".to_string(), resolved);
                        }
                    }
                }

                // Resolve Widths reference if present (needed for simple fonts)
                if let Some(PDFObject::Ref(r)) = final_spec.get("Widths").cloned() {
                    if let Some(doc) = doc {
                        if let Ok(resolved) = doc.resolve(&PDFObject::Ref(r)) {
                            final_spec.insert("Widths".to_string(), resolved);
                        }
                    }
                }

                // Resolve W (CID font widths) reference if present
                if let Some(PDFObject::Ref(r)) = final_spec.get("W").cloned() {
                    if let Some(doc) = doc {
                        if let Ok(resolved) = doc.resolve(&PDFObject::Ref(r)) {
                            final_spec.insert("W".to_string(), resolved);
                        }
                    }
                }

                // Resolve FontDescriptor reference if present (needed for accurate ascent/descent)
                if let Some(PDFObject::Ref(r)) = final_spec.get("FontDescriptor").cloned() {
                    if let Some(doc) = doc {
                        if let Ok(resolved) = doc.resolve(&PDFObject::Ref(r)) {
                            final_spec.insert("FontDescriptor".to_string(), resolved);
                        }
                    }
                }

                // Extract FontFile2 (TrueType font data) if available
                let ttf_data = Self::extract_fontfile2(&final_spec, doc);

                // Create font with ToUnicode data and TrueType font data
                let font = crate::pdffont::PDFCIDFont::new_with_ttf(
                    &final_spec,
                    tounicode_data.as_deref(),
                    ttf_data.as_deref(),
                    subtype == "Type0",
                    Some(fontid.clone()),
                );
                self.fontmap
                    .insert(fontid.clone(), std::sync::Arc::new(font));
            }
        }

        // Build XObject map (optional)
        let xobjects = match resources.get("XObject") {
            Some(PDFObject::Dict(d)) => Some(d.clone()),
            Some(PDFObject::Ref(r)) => {
                if let Some(doc) = doc {
                    match doc.resolve(&PDFObject::Ref(r.clone())) {
                        Ok(PDFObject::Dict(d)) => Some(d),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(xobjects) = xobjects {
            for (xobjid, xobj) in xobjects.iter() {
                let stream = match xobj {
                    PDFObject::Stream(s) => Some((**s).clone()),
                    PDFObject::Ref(r) => {
                        if let Some(doc) = doc {
                            match doc.resolve(&PDFObject::Ref(r.clone())) {
                                Ok(PDFObject::Stream(s)) => Some((*s).clone()),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(stream) = stream {
                    self.xobjmap.insert(xobjid.clone(), stream);
                }
            }
        }
    }

    /// Get the first descendant font spec from a Type0 font.
    fn get_descendant_font_spec(
        spec: &HashMap<String, PDFObject>,
        doc: Option<&crate::pdfdocument::PDFDocument>,
    ) -> Option<HashMap<String, PDFObject>> {
        let dfonts = spec.get("DescendantFonts")?;

        // Resolve if reference
        let dfonts_resolved = match dfonts {
            PDFObject::Array(arr) => arr.clone(),
            PDFObject::Ref(r) => {
                if let Some(doc) = doc {
                    if let Ok(PDFObject::Array(arr)) = doc.resolve(&PDFObject::Ref(r.clone())) {
                        arr
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        // Get first descendant font
        let first = dfonts_resolved.first()?;

        // Resolve font spec
        match first {
            PDFObject::Dict(d) => Some(d.clone()),
            PDFObject::Ref(r) => {
                if let Some(doc) = doc {
                    if let Ok(PDFObject::Dict(d)) = doc.resolve(&PDFObject::Ref(r.clone())) {
                        Some(d)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Extract ToUnicode stream data from font spec.
    fn extract_tounicode(
        spec: &HashMap<String, PDFObject>,
        doc: Option<&crate::pdfdocument::PDFDocument>,
    ) -> Option<Vec<u8>> {
        let tounicode = spec.get("ToUnicode")?;

        match tounicode {
            PDFObject::Stream(stream) => {
                // Decode the stream directly
                if let Some(doc) = doc {
                    doc.decode_stream(stream).ok()
                } else {
                    Some(stream.get_data().to_vec())
                }
            }
            PDFObject::Ref(r) => {
                // Resolve reference and decode
                if let Some(doc) = doc {
                    if let Ok(PDFObject::Stream(stream)) = doc.resolve(&PDFObject::Ref(r.clone())) {
                        doc.decode_stream(&stream).ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Extract FontFile2 (TrueType font) data from font spec.
    ///
    /// Follows the chain: spec["FontDescriptor"]["FontFile2"]
    fn extract_fontfile2(
        spec: &HashMap<String, PDFObject>,
        doc: Option<&crate::pdfdocument::PDFDocument>,
    ) -> Option<Vec<u8>> {
        // Get FontDescriptor
        let font_descriptor = spec.get("FontDescriptor")?;

        // Resolve FontDescriptor if it's a reference
        let fd_dict = match font_descriptor {
            PDFObject::Dict(d) => d.clone(),
            PDFObject::Ref(r) => {
                if let Some(doc) = doc {
                    if let Ok(PDFObject::Dict(d)) = doc.resolve(&PDFObject::Ref(r.clone())) {
                        d
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        // Get FontFile2
        let fontfile2 = fd_dict.get("FontFile2")?;

        // Resolve and decode the stream
        match fontfile2 {
            PDFObject::Stream(stream) => {
                if let Some(doc) = doc {
                    doc.decode_stream(stream).ok()
                } else {
                    Some(stream.get_data().to_vec())
                }
            }
            PDFObject::Ref(r) => {
                if let Some(doc) = doc {
                    if let Ok(PDFObject::Stream(stream)) = doc.resolve(&PDFObject::Ref(r.clone())) {
                        doc.decode_stream(&stream).ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Get current state tuple for saving.
    fn get_current_state(&self) -> SavedState {
        (self.ctm, self.textstate.copy(), self.graphicstate.copy())
    }

    /// Set current state from saved tuple.
    fn set_current_state(&mut self, state: SavedState) {
        let (ctm, textstate, graphicstate) = state;
        self.ctm = ctm;
        self.textstate = textstate;
        self.graphicstate = graphicstate;
        self.device.set_ctm(self.ctm);
    }

    // ========================================================================
    // Graphics State Operators
    // ========================================================================

    /// q - Save graphics state.
    pub fn do_q(&mut self) {
        self.gstack.push(self.get_current_state());
    }

    /// Q - Restore graphics state.
    pub fn do_Q(&mut self) {
        if let Some(state) = self.gstack.pop() {
            self.set_current_state(state);
        }
    }

    /// cm - Concatenate matrix to current transformation matrix.
    pub fn do_cm(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        let matrix = (a, b, c, d, e, f);
        self.ctm = mult_matrix(matrix, self.ctm);
        self.device.set_ctm(self.ctm);
    }

    /// w - Set line width.
    pub fn do_w(&mut self, linewidth: f64) {
        let scale = (self.ctm.0.powi(2) + self.ctm.1.powi(2)).sqrt();
        self.graphicstate.linewidth = linewidth * scale;
    }

    /// J - Set line cap style.
    pub fn do_J(&mut self, linecap: i32) {
        self.graphicstate.linecap = Some(linecap);
    }

    /// j - Set line join style.
    pub fn do_j(&mut self, linejoin: i32) {
        self.graphicstate.linejoin = Some(linejoin);
    }

    /// M - Set miter limit.
    pub fn do_M(&mut self, miterlimit: f64) {
        self.graphicstate.miterlimit = Some(miterlimit);
    }

    /// d - Set line dash pattern.
    pub fn do_d(&mut self, dash_array: Vec<f64>, phase: f64) {
        self.graphicstate.dash = Some((dash_array, phase));
    }

    /// ri - Set color rendering intent.
    pub fn do_ri(&mut self, intent: &str) {
        self.graphicstate.intent = Some(intent.to_string());
    }

    /// i - Set flatness tolerance.
    pub fn do_i(&mut self, flatness: f64) {
        self.graphicstate.flatness = Some(flatness);
    }

    /// gs - Set parameters from graphics state parameter dictionary.
    ///
    /// TODO: Implement ExtGState parameter lookup from resources.
    pub fn do_gs(&mut self, _name: &str) {
        // TODO: Look up ExtGState from resources and apply parameters
    }

    // ========================================================================
    // Color Operators
    // ========================================================================

    /// G - Set stroking color space to DeviceGray and set gray level.
    pub fn do_G(&mut self, gray: f64) {
        self.graphicstate.scolor = crate::pdfstate::Color::Gray(gray);
    }

    /// g - Set non-stroking color space to DeviceGray and set gray level.
    pub fn do_g(&mut self, gray: f64) {
        self.graphicstate.ncolor = crate::pdfstate::Color::Gray(gray);
    }

    /// RG - Set stroking color space to DeviceRGB and set color.
    pub fn do_RG(&mut self, r: f64, g: f64, b: f64) {
        self.graphicstate.scolor = crate::pdfstate::Color::Rgb(r, g, b);
    }

    /// rg - Set non-stroking color space to DeviceRGB and set color.
    pub fn do_rg(&mut self, r: f64, g: f64, b: f64) {
        self.graphicstate.ncolor = crate::pdfstate::Color::Rgb(r, g, b);
    }

    /// K - Set stroking color space to DeviceCMYK and set color.
    pub fn do_K(&mut self, c: f64, m: f64, y: f64, k: f64) {
        self.graphicstate.scolor = crate::pdfstate::Color::Cmyk(c, m, y, k);
    }

    /// k - Set non-stroking color space to DeviceCMYK and set color.
    pub fn do_k(&mut self, c: f64, m: f64, y: f64, k: f64) {
        self.graphicstate.ncolor = crate::pdfstate::Color::Cmyk(c, m, y, k);
    }

    /// SC/SCN - Set stroking color in current color space.
    ///
    /// Handles Pattern color spaces per ISO 32000-1:2008 4.5.5 (PDF 1.7)
    /// and ISO 32000-2:2020 8.7.3 (PDF 2.0):
    /// - Colored patterns (PaintType=1): single operand (pattern name)
    /// - Uncolored patterns (PaintType=2): n+1 operands (colors + pattern name)
    pub fn do_SC(&mut self, args: &mut Vec<PSToken>) {
        use crate::pdfstate::Color;

        // Check if current stroking colorspace is Pattern
        if self.graphicstate.scs.name == "Pattern" {
            // Pattern color space - last component should be pattern name
            if args.is_empty() {
                return;
            }

            // Check if last argument is a name (pattern name)
            let last_is_name = matches!(args.last(), Some(PSToken::Literal(_)));

            if last_is_name {
                let pattern_name = match args.pop() {
                    Some(PSToken::Literal(name)) => name,
                    _ => return,
                };

                if args.is_empty() {
                    // Colored tiling pattern (PaintType=1): just pattern name
                    self.graphicstate.scolor = Color::PatternColored(pattern_name);
                } else {
                    // Uncolored tiling pattern (PaintType=2): color components + pattern name
                    let base_color = Self::parse_color_components(args);
                    if let Some(base) = base_color {
                        self.graphicstate.scolor =
                            Color::PatternUncolored(Box::new(base), pattern_name);
                    }
                }
            }
        } else {
            // Standard color space - parse numeric components
            if let Some(color) = Self::parse_color_components(args) {
                self.graphicstate.scolor = color;
            }
        }
    }

    /// sc/scn - Set non-stroking color in current color space.
    ///
    /// Handles Pattern color spaces per ISO 32000-1:2008 4.5.5 (PDF 1.7)
    /// and ISO 32000-2:2020 8.7.3 (PDF 2.0):
    /// - Colored patterns (PaintType=1): single operand (pattern name)
    /// - Uncolored patterns (PaintType=2): n+1 operands (colors + pattern name)
    pub fn do_sc(&mut self, args: &mut Vec<PSToken>) {
        use crate::pdfstate::Color;

        // Check if current non-stroking colorspace is Pattern
        if self.graphicstate.ncs.name == "Pattern" {
            // Pattern color space - last component should be pattern name
            if args.is_empty() {
                return;
            }

            // Check if last argument is a name (pattern name)
            let last_is_name = matches!(args.last(), Some(PSToken::Literal(_)));

            if last_is_name {
                let pattern_name = match args.pop() {
                    Some(PSToken::Literal(name)) => name,
                    _ => return,
                };

                if args.is_empty() {
                    // Colored tiling pattern (PaintType=1): just pattern name
                    self.graphicstate.ncolor = Color::PatternColored(pattern_name);
                } else {
                    // Uncolored tiling pattern (PaintType=2): color components + pattern name
                    let base_color = Self::parse_color_components(args);
                    if let Some(base) = base_color {
                        self.graphicstate.ncolor =
                            Color::PatternUncolored(Box::new(base), pattern_name);
                    }
                }
            }
        } else {
            // Standard color space - parse numeric components
            if let Some(color) = Self::parse_color_components(args) {
                self.graphicstate.ncolor = color;
            }
        }
    }

    /// Parse color components from operand stack.
    ///
    /// Returns a Color based on the number of numeric components:
    /// - 1 component: Gray
    /// - 3 components: RGB
    /// - 4 components: CMYK
    fn parse_color_components(args: &[PSToken]) -> Option<crate::pdfstate::Color> {
        use crate::pdfstate::Color;

        let values: Vec<f64> = args
            .iter()
            .filter_map(|arg| match arg {
                PSToken::Real(n) => Some(*n),
                PSToken::Int(n) => Some(*n as f64),
                _ => None,
            })
            .collect();

        match values.len() {
            1 => Some(Color::Gray(values[0])),
            3 => Some(Color::Rgb(values[0], values[1], values[2])),
            4 => Some(Color::Cmyk(values[0], values[1], values[2], values[3])),
            _ => None,
        }
    }

    // ========================================================================
    // Path Construction Operators
    // ========================================================================

    /// m - Begin new subpath (moveto).
    pub fn do_m(&mut self, x: f64, y: f64) {
        self.curpath.push(PathSegment::MoveTo(x, y));
        self.current_point = Some((x, y));
    }

    /// l - Append straight line segment (lineto).
    pub fn do_l(&mut self, x: f64, y: f64) {
        self.curpath.push(PathSegment::LineTo(x, y));
        self.current_point = Some((x, y));
    }

    /// c - Append cubic Bezier curve (three control points).
    pub fn do_c(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.curpath
            .push(PathSegment::CurveTo(x1, y1, x2, y2, x3, y3));
        self.current_point = Some((x3, y3));
    }

    /// v - Append curved segment with initial point replicated.
    ///
    /// Uses current point as first control point.
    pub fn do_v(&mut self, x2: f64, y2: f64, x3: f64, y3: f64) {
        let (x1, y1) = self.current_point.unwrap_or((0.0, 0.0));
        self.curpath
            .push(PathSegment::CurveTo(x1, y1, x2, y2, x3, y3));
        self.current_point = Some((x3, y3));
    }

    /// y - Append curved segment with final point replicated.
    ///
    /// Uses endpoint as second control point.
    pub fn do_y(&mut self, x1: f64, y1: f64, x3: f64, y3: f64) {
        self.curpath
            .push(PathSegment::CurveTo(x1, y1, x3, y3, x3, y3));
        self.current_point = Some((x3, y3));
    }

    /// h - Close subpath.
    pub fn do_h(&mut self) {
        self.curpath.push(PathSegment::ClosePath);
    }

    /// re - Append rectangle to path.
    pub fn do_re(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.curpath.push(PathSegment::MoveTo(x, y));
        self.curpath.push(PathSegment::LineTo(x + w, y));
        self.curpath.push(PathSegment::LineTo(x + w, y + h));
        self.curpath.push(PathSegment::LineTo(x, y + h));
        self.curpath.push(PathSegment::ClosePath);
        self.current_point = Some((x, y));
    }

    // ========================================================================
    // Path Painting Operators
    // ========================================================================

    /// Helper to paint path and clear it.
    fn paint_path(&mut self, stroke: bool, fill: bool, evenodd: bool) {
        self.device
            .paint_path(&self.graphicstate, stroke, fill, evenodd, &self.curpath);
        self.curpath.clear();
        self.current_point = None;
    }

    /// S - Stroke path.
    pub fn do_S(&mut self) {
        self.paint_path(true, false, false);
    }

    /// s - Close and stroke path.
    pub fn do_s(&mut self) {
        self.do_h();
        self.do_S();
    }

    /// f - Fill path using nonzero winding number rule.
    pub fn do_f(&mut self) {
        self.paint_path(false, true, false);
    }

    /// F - Fill path using nonzero winding number rule (obsolete).
    pub fn do_F(&mut self) {
        self.do_f();
    }

    /// f* - Fill path using even-odd rule.
    pub fn do_f_star(&mut self) {
        self.paint_path(false, true, true);
    }

    /// B - Fill and stroke path using nonzero winding number rule.
    pub fn do_B(&mut self) {
        self.paint_path(true, true, false);
    }

    /// B* - Fill and stroke path using even-odd rule.
    pub fn do_B_star(&mut self) {
        self.paint_path(true, true, true);
    }

    /// b - Close, fill, and stroke path using nonzero winding number rule.
    pub fn do_b(&mut self) {
        self.do_h();
        self.do_B();
    }

    /// b* - Close, fill, and stroke path using even-odd rule.
    pub fn do_b_star(&mut self) {
        self.do_h();
        self.do_B_star();
    }

    /// n - End path without filling or stroking.
    pub fn do_n(&mut self) {
        self.curpath.clear();
        self.current_point = None;
    }

    // ========================================================================
    // Clipping Path Operators
    // ========================================================================

    /// W - Set clipping path using nonzero winding number rule.
    ///
    /// Note: In PDF, clipping is applied to subsequent operations.
    /// The path is not cleared - it can still be painted.
    pub fn do_W(&mut self) {
        // TODO: Implement actual clipping path handling
        // For now, this is a no-op that preserves the path
    }

    /// W* - Set clipping path using even-odd rule.
    pub fn do_W_star(&mut self) {
        // TODO: Implement actual clipping path handling
        // For now, this is a no-op that preserves the path
    }

    // ========================================================================
    // Text Object Operators
    // ========================================================================

    /// BT - Begin text object.
    ///
    /// Initializes the text matrix (Tm) and text line matrix (Tlm) to identity.
    /// Text objects cannot be nested.
    pub fn do_BT(&mut self) {
        self.textstate.reset();
    }

    /// ET - End text object.
    pub fn do_ET(&mut self) {
        // No action needed - text state persists for subsequent text objects
    }

    // ========================================================================
    // Text State Operators
    // ========================================================================

    /// Tc - Set character spacing.
    ///
    /// Character spacing is used by Tj, TJ, and ' operators.
    pub fn do_Tc(&mut self, charspace: f64) {
        self.textstate.charspace = charspace;
    }

    /// Tw - Set word spacing.
    ///
    /// Word spacing is used by Tj, TJ, and ' operators.
    pub fn do_Tw(&mut self, wordspace: f64) {
        self.textstate.wordspace = wordspace;
    }

    /// Tz - Set horizontal scaling.
    ///
    /// Scaling is a percentage (100 = normal width).
    pub fn do_Tz(&mut self, scaling: f64) {
        self.textstate.scaling = scaling;
    }

    /// TL - Set text leading.
    ///
    /// Text leading is used by T*, ', and " operators.
    /// The leading value is negated (stored as -leading) per PDF spec behavior.
    pub fn do_TL(&mut self, leading: f64) {
        self.textstate.leading = -leading;
    }

    /// Tf - Set text font and size.
    ///
    /// fontid is the name of a font resource in the Font subdictionary.
    /// fontsize is a scale factor in user space units.
    pub fn do_Tf(&mut self, fontid: &str, fontsize: f64) {
        // Look up font from fontmap
        if let Some(font) = self.fontmap.get(fontid) {
            self.textstate.font = Some(font.clone());
        }
        self.textstate.fontname = Some(fontid.to_string());
        self.textstate.fontsize = fontsize;
    }

    /// Tr - Set text rendering mode.
    ///
    /// Rendering modes: 0=fill, 1=stroke, 2=fill+stroke, 3=invisible,
    /// 4-7 add clipping to modes 0-3.
    pub fn do_Tr(&mut self, render: i32) {
        self.textstate.render = render;
    }

    /// Ts - Set text rise (superscript/subscript offset).
    pub fn do_Ts(&mut self, rise: f64) {
        self.textstate.rise = rise;
    }

    // ========================================================================
    // Text Positioning Operators
    // ========================================================================

    /// Td - Move to start of next line.
    ///
    /// Offset from start of current line by (tx, ty).
    /// Updates text matrix: e_new = tx*a + ty*c + e, f_new = tx*b + ty*d + f
    pub fn do_Td(&mut self, tx: f64, ty: f64) {
        let (a, b, c, d, e, f) = self.textstate.matrix;
        let e_new = tx * a + ty * c + e;
        let f_new = tx * b + ty * d + f;
        self.textstate.matrix = (a, b, c, d, e_new, f_new);
        self.textstate.linematrix = (0.0, 0.0);
    }

    /// TD - Move to start of next line and set leading.
    ///
    /// Same as Td but also sets text leading to ty.
    pub fn do_TD(&mut self, tx: f64, ty: f64) {
        let (a, b, c, d, e, f) = self.textstate.matrix;
        let e_new = tx * a + ty * c + e;
        let f_new = tx * b + ty * d + f;
        self.textstate.matrix = (a, b, c, d, e_new, f_new);
        self.textstate.leading = ty;
        self.textstate.linematrix = (0.0, 0.0);
    }

    /// Tm - Set text matrix and text line matrix.
    ///
    /// Directly sets the text matrix to the specified values.
    pub fn do_Tm(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.textstate.matrix = (a, b, c, d, e, f);
        self.textstate.linematrix = (0.0, 0.0);
    }

    /// T* - Move to start of next text line.
    ///
    /// Uses current leading value. Equivalent to: 0 -leading Td
    pub fn do_T_star(&mut self) {
        let (a, b, c, d, e, f) = self.textstate.matrix;
        let leading = self.textstate.leading;
        // T* equivalent to Td(0, leading), but note leading already stores -TL
        self.textstate.matrix = (a, b, c, d, leading * c + e, leading * d + f);
        self.textstate.linematrix = (0.0, 0.0);
    }

    // ========================================================================
    // Text Showing Operators
    // ========================================================================

    /// TJ - Show text, allowing individual glyph positioning.
    ///
    /// The seq parameter contains a mix of strings (to show) and numbers
    /// (horizontal adjustments in thousandths of text space).
    pub fn do_TJ(&mut self, seq: PDFTextSeq) {
        // Pass to device for rendering
        self.device.render_string(
            &mut self.textstate,
            &seq,
            &self.graphicstate.ncs,
            &self.graphicstate,
        );
    }

    /// Tj - Show text string.
    ///
    /// Wraps the string in a single-element sequence and calls do_TJ.
    pub fn do_Tj(&mut self, s: Vec<u8>) {
        self.do_TJ(vec![PDFTextSeqItem::Bytes(s)]);
    }

    /// ' (quote) - Move to next line and show text.
    ///
    /// Equivalent to: T* (string) Tj
    pub fn do_quote(&mut self, s: Vec<u8>) {
        self.do_T_star();
        self.do_TJ(vec![PDFTextSeqItem::Bytes(s)]);
    }

    /// " (doublequote) - Set word and character spacing, move to next line, and show text.
    ///
    /// Equivalent to: aw Tw ac Tc (string) '
    pub fn do_doublequote(&mut self, aw: f64, ac: f64, s: Vec<u8>) {
        self.do_Tw(aw);
        self.do_Tc(ac);
        self.do_quote(s);
    }

    // ========================================================================
    // XObject Operators
    // ========================================================================

    /// Do - Invoke named XObject (images or form XObjects).
    pub fn do_Do(&mut self, xobjid: String) {
        if std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1") {
            eprintln!("Do XObject: {}", xobjid);
        }
        let xobj = match self.xobjmap.get(&xobjid) {
            Some(xobj) => xobj,
            None => return,
        };

        let subtype = xobj
            .get("Subtype")
            .and_then(|obj| obj.as_name().ok())
            .unwrap_or("");

        if subtype == "Form" && xobj.get("BBox").is_some() {
            if std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1") {
                eprintln!("Do Form XObject: {}", xobjid);
            }
            let bbox = match Self::parse_bbox(xobj.get("BBox")) {
                Some(b) => b,
                None => return,
            };
            let matrix = Self::parse_matrix(xobj.get("Matrix"));

            let resources = xobj
                .get("Resources")
                .and_then(|r| self.resolve_resources(r))
                .unwrap_or_else(|| self.resources.clone());

            let data = if let Some(doc) = self.doc {
                doc.decode_stream(xobj).ok()
            } else {
                Some(xobj.get_data().to_vec())
            };
            if data.is_none()
                && std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1")
            {
                eprintln!("Do Form XObject: {} decode failed", xobjid);
            }
            let Some(data) = data else {
                return;
            };
            if std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1") {
                eprintln!("Do Form XObject: {} data len {}", xobjid, data.len());
            }

            let saved = self.snapshot_state();
            self.device.begin_figure(&xobjid, bbox, matrix);

            let form_ctm = mult_matrix(matrix, self.ctm);
            self.render_contents(resources, vec![data], form_ctm);

            self.device.end_figure(&xobjid);
            self.restore_state(saved);
        } else if subtype == "Image"
            && xobj.get("Width").is_some()
            && xobj.get("Height").is_some()
        {
            self.device
                .begin_figure(&xobjid, (0.0, 0.0, 1.0, 1.0), MATRIX_IDENTITY);
            self.device.render_image(&xobjid, xobj);
            self.device.end_figure(&xobjid);
        }
    }

    /// Render content streams with specific resources and CTM.
    fn render_contents(
        &mut self,
        resources: HashMap<String, PDFObject>,
        streams: Vec<Vec<u8>>,
        ctm: Matrix,
    ) {
        self.init_resources(&resources, self.doc);
        self.init_state(ctm);
        self.execute(&streams);
    }

    fn snapshot_state(&mut self) -> InterpreterState {
        InterpreterState {
            gstack: std::mem::take(&mut self.gstack),
            ctm: self.ctm,
            textstate: self.textstate.clone(),
            graphicstate: self.graphicstate.clone(),
            curpath: std::mem::take(&mut self.curpath),
            current_point: self.current_point,
            fontmap: std::mem::take(&mut self.fontmap),
            resources: std::mem::take(&mut self.resources),
            xobjmap: std::mem::take(&mut self.xobjmap),
        }
    }

    fn restore_state(&mut self, state: InterpreterState) {
        self.gstack = state.gstack;
        self.ctm = state.ctm;
        self.device.set_ctm(self.ctm);
        self.textstate = state.textstate;
        self.graphicstate = state.graphicstate;
        self.curpath = state.curpath;
        self.current_point = state.current_point;
        self.fontmap = state.fontmap;
        self.resources = state.resources;
        self.xobjmap = state.xobjmap;
    }

    fn resolve_resources(&self, obj: &PDFObject) -> Option<HashMap<String, PDFObject>> {
        match obj {
            PDFObject::Dict(d) => Some(d.clone()),
            PDFObject::Ref(r) => {
                if let Some(doc) = self.doc {
                    match doc.resolve(&PDFObject::Ref(r.clone())) {
                        Ok(PDFObject::Dict(d)) => Some(d),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn parse_bbox(obj: Option<&PDFObject>) -> Option<(f64, f64, f64, f64)> {
        let arr = obj?.as_array().ok()?;
        if arr.len() < 4 {
            return None;
        }
        let x0 = arr[0].as_num().ok()?;
        let y0 = arr[1].as_num().ok()?;
        let x1 = arr[2].as_num().ok()?;
        let y1 = arr[3].as_num().ok()?;
        Some((x0, y0, x1, y1))
    }

    fn parse_matrix(obj: Option<&PDFObject>) -> Matrix {
        let arr = match obj.and_then(|o| o.as_array().ok()) {
            Some(arr) if arr.len() >= 6 => arr,
            _ => return MATRIX_IDENTITY,
        };
        let a = arr[0].as_num().ok();
        let b = arr[1].as_num().ok();
        let c = arr[2].as_num().ok();
        let d = arr[3].as_num().ok();
        let e = arr[4].as_num().ok();
        let f = arr[5].as_num().ok();
        match (a, b, c, d, e, f) {
            (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) => (a, b, c, d, e, f),
            _ => MATRIX_IDENTITY,
        }
    }

    // ========================================================================
    // Marked Content Operators
    // ========================================================================

    /// BMC - Begin Marked Content
    pub fn do_BMC(&mut self, tag: &PSLiteral) {
        self.device.begin_tag(tag, None);
    }

    /// BDC - Begin Marked Content with property dictionary
    pub fn do_BDC(&mut self, tag: &PSLiteral, props: &PDFStackT) {
        self.device.begin_tag(tag, Some(props));
    }

    /// EMC - End Marked Content
    pub fn do_EMC(&mut self) {
        self.device.end_tag();
    }

    // ========================================================================
    // Page Processing
    // ========================================================================

    /// Process a PDF page.
    ///
    /// This is the main entry point for page interpretation.
    /// Sets up the CTM based on page rotation, then renders the content streams.
    ///
    /// Port of PDFPageInterpreter.process_page from pdfminer.six
    pub fn process_page(
        &mut self,
        page: &crate::pdfpage::PDFPage,
        doc: Option<&'a crate::pdfdocument::PDFDocument>,
    ) {
        self.doc = doc;
        let mediabox = page.mediabox.unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let (x0, y0, x1, y1) = (mediabox[0], mediabox[1], mediabox[2], mediabox[3]);

        // Calculate CTM based on page rotation
        let mut ctm = match page.rotate {
            90 => (0.0, -1.0, 1.0, 0.0, -y0, x1),
            180 => (-1.0, 0.0, 0.0, -1.0, x1, y1),
            270 => (0.0, 1.0, -1.0, 0.0, y1, -x0),
            _ => (1.0, 0.0, 0.0, 1.0, -x0, -y0),
        };

        // Apply UserUnit scaling (PDF 1.6 feature)
        let user_unit = page.user_unit;
        if user_unit != 1.0 {
            ctm = mult_matrix((user_unit, 0.0, 0.0, user_unit, 0.0, 0.0), ctm);
        }

        // Begin page on device
        let bbox = (x0, y0, x1, y1);
        self.device.begin_page(page.pageid, bbox, ctm);

        // Initialize resources (builds fontmap)
        self.init_resources(&page.resources, doc);

        // Initialize state and execute content streams
        self.init_state(ctm);
        self.execute(&page.contents);

        // End page on device
        self.device.end_page(page.pageid);
    }

    /// Execute content streams.
    ///
    /// Parses the content streams and dispatches operators to do_* methods.
    ///
    /// Port of PDFPageInterpreter.execute from pdfminer.six
    pub fn execute(&mut self, streams: &[Vec<u8>]) {
        if streams.is_empty() {
            return;
        }

        let parser = PDFContentParser::new(streams.to_vec());
        let mut operand_stack: Vec<PSToken> = Vec::new();

        for token in parser {
            match token {
                ContentToken::Operand(op) => {
                    operand_stack.push(op);
                }
                ContentToken::Keyword(name) => {
                    self.dispatch_operator(&name, &mut operand_stack);
                    operand_stack.clear();
                }
                ContentToken::InlineImage { dict, data } => {
                    let mut attrs = HashMap::new();
                    for (key, value) in dict {
                        let obj = match value {
                            PSToken::Int(n) => PDFObject::Int(n as i64),
                            PSToken::Real(n) => PDFObject::Real(n),
                            PSToken::Bool(b) => PDFObject::Bool(b),
                            PSToken::Literal(name) => PDFObject::Name(name),
                            PSToken::String(s) => PDFObject::String(s),
                            PSToken::Array(arr) => {
                                let mut vals = Vec::new();
                                for item in arr {
                                    match item {
                                        PSToken::Int(n) => vals.push(PDFObject::Int(n as i64)),
                                        PSToken::Real(n) => vals.push(PDFObject::Real(n)),
                                        PSToken::Bool(b) => vals.push(PDFObject::Bool(b)),
                                        PSToken::Literal(name) => vals.push(PDFObject::Name(name)),
                                        PSToken::String(s) => vals.push(PDFObject::String(s)),
                                        _ => {}
                                    }
                                }
                                PDFObject::Array(vals)
                            }
                            PSToken::Dict(d) => {
                                let mut map = HashMap::new();
                                for (k, v) in d {
                                    let vobj = match v {
                                        PSToken::Int(n) => PDFObject::Int(n as i64),
                                        PSToken::Real(n) => PDFObject::Real(n),
                                        PSToken::Bool(b) => PDFObject::Bool(b),
                                        PSToken::Literal(name) => PDFObject::Name(name),
                                        PSToken::String(s) => PDFObject::String(s),
                                        _ => PDFObject::Null,
                                    };
                                    map.insert(k, vobj);
                                }
                                PDFObject::Dict(map)
                            }
                            _ => PDFObject::Null,
                        };
                        let key = match key.as_str() {
                            "BPC" => "BitsPerComponent",
                            "CS" => "ColorSpace",
                            "W" => "Width",
                            "H" => "Height",
                            "IM" => "ImageMask",
                            "DP" => "DecodeParms",
                            "F" => "Filter",
                            _ => key.as_str(),
                        }
                        .to_string();
                        attrs.insert(key, obj);
                    }
                    let stream = PDFStream::new(attrs, data);
                    let name = format!("inline{}", self.inline_image_id);
                    self.inline_image_id += 1;
                    self.device
                        .begin_figure(&name, (0.0, 0.0, 1.0, 1.0), MATRIX_IDENTITY);
                    self.device.render_image(&name, &stream);
                    self.device.end_figure(&name);
                    operand_stack.clear();
                }
            }
        }
    }

    fn pstoken_to_stackvalue(token: &PSToken) -> Option<PDFStackValue> {
        match token {
            PSToken::Int(n) => Some(PDFStackValue::Int(*n)),
            PSToken::Real(n) => Some(PDFStackValue::Real(*n)),
            PSToken::Bool(b) => Some(PDFStackValue::Bool(*b)),
            PSToken::Literal(name) => Some(PDFStackValue::Name(name.clone())),
            PSToken::String(s) => Some(PDFStackValue::String(s.clone())),
            PSToken::Array(arr) => {
                let values = arr
                    .iter()
                    .filter_map(Self::pstoken_to_stackvalue)
                    .collect();
                Some(PDFStackValue::Array(values))
            }
            PSToken::Dict(map) => {
                let mut values = HashMap::new();
                for (key, val) in map.iter() {
                    if let Some(v) = Self::pstoken_to_stackvalue(val) {
                        values.insert(key.clone(), v);
                    }
                }
                Some(PDFStackValue::Dict(values))
            }
            PSToken::Keyword(_) => None,
        }
    }

    fn pdfobject_to_stackvalue(&self, obj: &PDFObject) -> Option<PDFStackValue> {
        match obj {
            PDFObject::Int(n) => Some(PDFStackValue::Int(*n)),
            PDFObject::Real(n) => Some(PDFStackValue::Real(*n)),
            PDFObject::Bool(b) => Some(PDFStackValue::Bool(*b)),
            PDFObject::Name(name) => Some(PDFStackValue::Name(name.clone())),
            PDFObject::String(s) => Some(PDFStackValue::String(s.clone())),
            PDFObject::Array(arr) => {
                let values = arr
                    .iter()
                    .filter_map(|item| self.pdfobject_to_stackvalue(item))
                    .collect();
                Some(PDFStackValue::Array(values))
            }
            PDFObject::Dict(map) => {
                let mut values = HashMap::new();
                for (key, val) in map.iter() {
                    if let Some(v) = self.pdfobject_to_stackvalue(val) {
                        values.insert(key.clone(), v);
                    }
                }
                Some(PDFStackValue::Dict(values))
            }
            PDFObject::Ref(r) => {
                if let Some(doc) = self.doc {
                    if let Ok(resolved) = doc.resolve(&PDFObject::Ref(r.clone())) {
                        return self.pdfobject_to_stackvalue(&resolved);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn properties_dict(&self) -> Option<HashMap<String, PDFObject>> {
        match self.resources.get("Properties") {
            Some(PDFObject::Dict(d)) => Some(d.clone()),
            Some(PDFObject::Ref(r)) => {
                if let Some(doc) = self.doc {
                    match doc.resolve(&PDFObject::Ref(r.clone())) {
                        Ok(PDFObject::Dict(d)) => Some(d),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn props_from_token(&self, token: Option<PSToken>) -> PDFStackT {
        let mut props = HashMap::new();
        match token {
            Some(PSToken::Dict(map)) => {
                for (key, val) in map {
                    if let Some(v) = Self::pstoken_to_stackvalue(&val) {
                        props.insert(key, v);
                    }
                }
            }
            Some(PSToken::Literal(name)) => {
                if let Some(dict) = self.properties_dict() {
                    if let Some(obj) = dict.get(&name) {
                        if let Some(PDFStackValue::Dict(map)) =
                            self.pdfobject_to_stackvalue(obj)
                        {
                            props = map;
                        }
                    }
                }
            }
            _ => {}
        }
        props
    }

    /// Dispatch an operator to the appropriate do_* method.
    fn dispatch_operator(&mut self, op: &Keyword, args: &mut Vec<PSToken>) {
        match op {
            // Graphics state operators
            Keyword::Qq => self.do_q(),
            Keyword::Q => self.do_Q(),
            Keyword::Cm => {
                if let Some((a, b, c, d, e, f)) = Self::pop_matrix(args) {
                    self.do_cm(a, b, c, d, e, f);
                }
            }
            Keyword::Ww => {
                if let Some(w) = Self::pop_number(args) {
                    self.do_w(w);
                }
            }
            Keyword::J => {
                if let Some(n) = Self::pop_int(args) {
                    self.do_J(n);
                }
            }
            Keyword::Jj => {
                if let Some(n) = Self::pop_int(args) {
                    self.do_j(n);
                }
            }
            Keyword::M => {
                if let Some(m) = Self::pop_number(args) {
                    self.do_M(m);
                }
            }
            Keyword::D => {
                // dash pattern: [array] phase
                if args.len() >= 2 {
                    let phase = Self::pop_number(args).unwrap_or(0.0);
                    let arr = Self::pop_array(args).unwrap_or_default();
                    self.do_d(arr, phase);
                }
            }
            Keyword::Ri => {
                if let Some(intent) = Self::pop_name(args) {
                    self.do_ri(&intent);
                }
            }
            Keyword::I => {
                if let Some(f) = Self::pop_number(args) {
                    self.do_i(f);
                }
            }
            Keyword::Gs => {
                if let Some(name) = Self::pop_name(args) {
                    self.do_gs(&name);
                }
            }
            Keyword::Do => {
                if let Some(name) = Self::pop_name(args) {
                    self.do_Do(name);
                }
            }

            // Marked content operators
            Keyword::BMC => {
                if let Some(name) = Self::pop_name(args) {
                    let tag = PSLiteral::new(&name);
                    self.do_BMC(&tag);
                }
            }
            Keyword::BDC => {
                // BDC takes tag and properties dict
                let props_token = args.pop();
                if let Some(name) = Self::pop_name(args) {
                    let tag = PSLiteral::new(&name);
                    let props_map = self.props_from_token(props_token);
                    self.do_BDC(&tag, &props_map);
                }
            }
            Keyword::EMC => {
                self.do_EMC();
            }

            // Path construction operators
            Keyword::Mm => {
                if let Some((x, y)) = Self::pop_point(args) {
                    self.do_m(x, y);
                }
            }
            Keyword::L => {
                if let Some((x, y)) = Self::pop_point(args) {
                    self.do_l(x, y);
                }
            }
            Keyword::C => {
                if args.len() >= 6 {
                    let y3 = Self::pop_number(args).unwrap_or(0.0);
                    let x3 = Self::pop_number(args).unwrap_or(0.0);
                    let y2 = Self::pop_number(args).unwrap_or(0.0);
                    let x2 = Self::pop_number(args).unwrap_or(0.0);
                    let y1 = Self::pop_number(args).unwrap_or(0.0);
                    let x1 = Self::pop_number(args).unwrap_or(0.0);
                    self.do_c(x1, y1, x2, y2, x3, y3);
                }
            }
            Keyword::V => {
                if args.len() >= 4 {
                    let y3 = Self::pop_number(args).unwrap_or(0.0);
                    let x3 = Self::pop_number(args).unwrap_or(0.0);
                    let y2 = Self::pop_number(args).unwrap_or(0.0);
                    let x2 = Self::pop_number(args).unwrap_or(0.0);
                    self.do_v(x2, y2, x3, y3);
                }
            }
            Keyword::Y => {
                if args.len() >= 4 {
                    let y3 = Self::pop_number(args).unwrap_or(0.0);
                    let x3 = Self::pop_number(args).unwrap_or(0.0);
                    let y1 = Self::pop_number(args).unwrap_or(0.0);
                    let x1 = Self::pop_number(args).unwrap_or(0.0);
                    self.do_y(x1, y1, x3, y3);
                }
            }
            Keyword::H => self.do_h(),
            Keyword::Re => {
                if args.len() >= 4 {
                    let h = Self::pop_number(args).unwrap_or(0.0);
                    let w = Self::pop_number(args).unwrap_or(0.0);
                    let y = Self::pop_number(args).unwrap_or(0.0);
                    let x = Self::pop_number(args).unwrap_or(0.0);
                    self.do_re(x, y, w, h);
                }
            }

            // Path painting operators
            Keyword::S => self.do_S(),
            Keyword::Ss => self.do_s(),
            Keyword::Ff | Keyword::F => self.do_f(),
            Keyword::FStar => self.do_f_star(),
            Keyword::B => self.do_B(),
            Keyword::BStar => self.do_B_star(),
            Keyword::Bb => self.do_b(),
            Keyword::BbStar => self.do_b_star(),
            Keyword::N => self.do_n(),

            // Color operators
            Keyword::G => {
                if let Some(g) = Self::pop_number(args) {
                    self.do_G(g);
                }
            }
            Keyword::Gg => {
                if let Some(g) = Self::pop_number(args) {
                    self.do_g(g);
                }
            }
            Keyword::RG => {
                if args.len() >= 3 {
                    let b = Self::pop_number(args).unwrap_or(0.0);
                    let g = Self::pop_number(args).unwrap_or(0.0);
                    let r = Self::pop_number(args).unwrap_or(0.0);
                    self.do_RG(r, g, b);
                }
            }
            Keyword::Rg => {
                if args.len() >= 3 {
                    let b = Self::pop_number(args).unwrap_or(0.0);
                    let g = Self::pop_number(args).unwrap_or(0.0);
                    let r = Self::pop_number(args).unwrap_or(0.0);
                    self.do_rg(r, g, b);
                }
            }
            Keyword::K => {
                if args.len() >= 4 {
                    let k = Self::pop_number(args).unwrap_or(0.0);
                    let y = Self::pop_number(args).unwrap_or(0.0);
                    let m = Self::pop_number(args).unwrap_or(0.0);
                    let c = Self::pop_number(args).unwrap_or(0.0);
                    self.do_K(c, m, y, k);
                }
            }
            Keyword::Kk => {
                if args.len() >= 4 {
                    let k = Self::pop_number(args).unwrap_or(0.0);
                    let y = Self::pop_number(args).unwrap_or(0.0);
                    let m = Self::pop_number(args).unwrap_or(0.0);
                    let c = Self::pop_number(args).unwrap_or(0.0);
                    self.do_k(c, m, y, k);
                }
            }
            Keyword::SC | Keyword::SCN => {
                // SC/SCN - set stroking color in current color space
                self.do_SC(args);
            }
            Keyword::Sc | Keyword::Scn => {
                // sc/scn - set non-stroking color in current color space
                self.do_sc(args);
            }

            // Clipping operators
            Keyword::WClip => self.do_W(),
            Keyword::WStar => self.do_W_star(),

            // Text object operators
            Keyword::BT => self.do_BT(),
            Keyword::ET => self.do_ET(),

            // Text state operators
            Keyword::Tc => {
                if let Some(cs) = Self::pop_number(args) {
                    self.do_Tc(cs);
                }
            }
            Keyword::Tw => {
                if let Some(ws) = Self::pop_number(args) {
                    self.do_Tw(ws);
                }
            }
            Keyword::Tz => {
                if let Some(s) = Self::pop_number(args) {
                    self.do_Tz(s);
                }
            }
            Keyword::TL => {
                if let Some(l) = Self::pop_number(args) {
                    self.do_TL(l);
                }
            }
            Keyword::Tf => {
                if args.len() >= 2 {
                    let size = Self::pop_number(args).unwrap_or(12.0);
                    let fontid = Self::pop_name(args).unwrap_or_default();
                    self.do_Tf(&fontid, size);
                }
            }
            Keyword::Tr => {
                if let Some(r) = Self::pop_int(args) {
                    self.do_Tr(r);
                }
            }
            Keyword::Ts => {
                if let Some(r) = Self::pop_number(args) {
                    self.do_Ts(r);
                }
            }

            // Text positioning operators
            Keyword::Td => {
                if let Some((tx, ty)) = Self::pop_point(args) {
                    self.do_Td(tx, ty);
                }
            }
            Keyword::TD => {
                if let Some((tx, ty)) = Self::pop_point(args) {
                    self.do_TD(tx, ty);
                }
            }
            Keyword::Tm => {
                if let Some((a, b, c, d, e, f)) = Self::pop_matrix(args) {
                    self.do_Tm(a, b, c, d, e, f);
                }
            }
            Keyword::TStar => self.do_T_star(),

            // Text showing operators
            Keyword::Tj => {
                if let Some(s) = Self::pop_string(args) {
                    self.do_Tj(s);
                }
            }
            Keyword::TJ => {
                if let Some(seq) = Self::pop_text_seq(args) {
                    self.do_TJ(seq);
                }
            }
            Keyword::Quote => {
                if let Some(s) = Self::pop_string(args) {
                    self.do_quote(s);
                }
            }
            Keyword::DoubleQuote => {
                if args.len() >= 3 {
                    let s = Self::pop_string(args).unwrap_or_default();
                    let ac = Self::pop_number(args).unwrap_or(0.0);
                    let aw = Self::pop_number(args).unwrap_or(0.0);
                    self.do_doublequote(aw, ac, s);
                }
            }

            // Unknown operator - ignore
            _ => {}
        }
    }

    // Helper functions to pop values from operand stack

    fn pop_number(args: &mut Vec<PSToken>) -> Option<f64> {
        args.pop().and_then(|t| match t {
            PSToken::Int(n) => Some(n as f64),
            PSToken::Real(n) => Some(n),
            _ => None,
        })
    }

    fn pop_int(args: &mut Vec<PSToken>) -> Option<i32> {
        args.pop().and_then(|t| match t {
            PSToken::Int(n) => Some(n as i32),
            PSToken::Real(n) => Some(n as i32),
            _ => None,
        })
    }

    fn pop_string(args: &mut Vec<PSToken>) -> Option<Vec<u8>> {
        args.pop().and_then(|t| match t {
            PSToken::String(s) => Some(s),
            _ => None,
        })
    }

    fn pop_name(args: &mut Vec<PSToken>) -> Option<String> {
        args.pop().and_then(|t| match t {
            PSToken::Literal(s) => Some(s),
            PSToken::Keyword(k) => std::str::from_utf8(k.as_bytes()).ok().map(String::from),
            _ => None,
        })
    }

    fn pop_array(args: &mut Vec<PSToken>) -> Option<Vec<f64>> {
        args.pop().and_then(|t| match t {
            PSToken::Array(arr) => Some(
                arr.iter()
                    .filter_map(|x| match x {
                        PSToken::Int(n) => Some(*n as f64),
                        PSToken::Real(n) => Some(*n),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
    }

    fn pop_point(args: &mut Vec<PSToken>) -> Option<(f64, f64)> {
        if args.len() >= 2 {
            let y = Self::pop_number(args)?;
            let x = Self::pop_number(args)?;
            Some((x, y))
        } else {
            None
        }
    }

    fn pop_matrix(args: &mut Vec<PSToken>) -> Option<(f64, f64, f64, f64, f64, f64)> {
        if args.len() >= 6 {
            let f = Self::pop_number(args)?;
            let e = Self::pop_number(args)?;
            let d = Self::pop_number(args)?;
            let c = Self::pop_number(args)?;
            let b = Self::pop_number(args)?;
            let a = Self::pop_number(args)?;
            Some((a, b, c, d, e, f))
        } else {
            None
        }
    }

    fn pop_text_seq(args: &mut Vec<PSToken>) -> Option<PDFTextSeq> {
        args.pop().and_then(|t| match t {
            PSToken::Array(arr) => {
                let seq: PDFTextSeq = arr
                    .into_iter()
                    .filter_map(|item| match item {
                        PSToken::Int(n) => Some(PDFTextSeqItem::Number(n as f64)),
                        PSToken::Real(n) => Some(PDFTextSeqItem::Number(n)),
                        PSToken::String(s) => Some(PDFTextSeqItem::Bytes(s)),
                        _ => None,
                    })
                    .collect();
                Some(seq)
            }
            _ => None,
        })
    }
}

struct InterpreterState {
    gstack: Vec<SavedState>,
    ctm: Matrix,
    textstate: PDFTextState,
    graphicstate: PDFGraphicState,
    curpath: Vec<PathSegment>,
    current_point: Option<(f64, f64)>,
    fontmap: HashMap<String, std::sync::Arc<crate::pdffont::PDFCIDFont>>,
    resources: HashMap<String, PDFObject>,
    xobjmap: HashMap<String, PDFStream>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_parse() {
        let stream = b"BT ET";
        let parser = PDFContentParser::new(vec![stream.to_vec()]);

        let tokens: Vec<_> = parser.collect();
        assert_eq!(tokens.len(), 2);
    }
}
