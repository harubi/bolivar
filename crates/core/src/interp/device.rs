//! PDF Device - output interface for PDF page interpretation.
//!
//! Port of pdfminer.six pdfdevice.py
//!
//! Devices translate the output of PDFPageInterpreter to various formats.
//! The base PDFDevice trait provides the interface, while concrete implementations
//! like PDFTextDevice and TagExtractor produce specific outputs.

use crate::pdfcolor::PDFColorSpace;
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::pdftypes::PDFStream;
use crate::psparser::PSLiteral;
use crate::utils::{Matrix, Point, Rect, enc, mult_matrix, translate_matrix};
use std::collections::HashMap;
use std::io::Write;

/// Sequence of text elements that can contain numbers (positioning) or bytes (character data).
pub type PDFTextSeq = Vec<PDFTextSeqItem>;

/// Individual item in a PDF text sequence.
#[derive(Debug, Clone)]
pub enum PDFTextSeqItem {
    /// Numeric adjustment (horizontal offset in text space)
    Number(f64),
    /// Character data bytes
    Bytes(Vec<u8>),
}

/// Stack type for PDF operations (matches Python's PDFStackT).
pub type PDFStackT = HashMap<String, PDFStackValue>;

/// Values that can appear on the PDF stack.
#[derive(Debug, Clone)]
pub enum PDFStackValue {
    Int(i64),
    Real(f64),
    Bool(bool),
    Name(String),
    String(Vec<u8>),
    Array(Vec<Self>),
    Dict(HashMap<String, Self>),
}

/// Path segment for graphics operations.
#[derive(Debug, Clone)]
pub enum PathSegment {
    /// Move to point (x, y)
    MoveTo(f64, f64),
    /// Line to point (x, y)
    LineTo(f64, f64),
    /// Cubic bezier curve (x1, y1, x2, y2, x3, y3)
    CurveTo(f64, f64, f64, f64, f64, f64),
    /// Close path
    ClosePath,
}

/// PDF Device trait - interface for rendering PDF page content.
///
/// Implementations translate the output of PDFPageInterpreter to the
/// desired output format (text extraction, rendering, etc.).
pub trait PDFDevice {
    /// Set the current transformation matrix.
    fn set_ctm(&mut self, ctm: Matrix);

    /// Get the current transformation matrix.
    fn ctm(&self) -> Option<Matrix>;

    /// Close the device and release resources.
    fn close(&mut self) {}

    /// Begin a marked content tag.
    fn begin_tag(&mut self, _tag: &PSLiteral, _props: Option<&PDFStackT>) {}

    /// End a marked content tag.
    fn end_tag(&mut self) {}

    /// Handle an inline marked content tag (no content).
    fn do_tag(&mut self, _tag: &PSLiteral, _props: Option<&PDFStackT>) {}

    /// Begin processing a page.
    fn begin_page(&mut self, _pageid: u32, _mediabox: Rect, _ctm: Matrix) {}

    /// End processing a page.
    fn end_page(&mut self, _pageid: u32) {}

    /// Begin a Form XObject (figure).
    fn begin_figure(&mut self, _name: &str, _bbox: Rect, _matrix: Matrix) {}

    /// End a Form XObject (figure).
    fn end_figure(&mut self, _name: &str) {}

    /// Paint a graphics path.
    fn paint_path(
        &mut self,
        _graphicstate: &PDFGraphicState,
        _stroke: bool,
        _fill: bool,
        _evenodd: bool,
        _path: &[PathSegment],
    ) {
    }

    /// Render an inline or XObject image.
    fn render_image(&mut self, _name: &str, _stream: &PDFStream) {}

    /// Render a text string.
    ///
    /// # Arguments
    /// * `textstate` - Current text state (font, size, spacing, etc.)
    /// * `seq` - Text sequence containing positioning and character data
    /// * `ncs` - Non-stroking color space
    /// * `graphicstate` - Current graphics state
    fn render_string(
        &mut self,
        _textstate: &mut PDFTextState,
        _seq: &PDFTextSeq,
        _ncs: &PDFColorSpace,
        _graphicstate: &PDFGraphicState,
    ) {
    }
}

/// Placeholder font trait for text device operations.
///
/// TODO: Replace with actual PDFFont when pdfinterp is implemented.
/// This provides the interface needed by render_string_horizontal/vertical.
pub trait PDFFontLike {
    /// Check if font is vertical writing mode.
    fn is_vertical(&self) -> bool;

    /// Check if font is multibyte (CID fonts).
    fn is_multibyte(&self) -> bool;

    /// Decode bytes to character IDs.
    fn decode(&self, data: &[u8]) -> Vec<u32>;

    /// Convert CID to Unicode character.
    /// Returns None if the CID has no Unicode mapping.
    fn to_unichr(&self, cid: u32) -> Option<char>;
}

/// PDF Text Device - base for text extraction devices.
///
/// Provides infrastructure for rendering text strings by iterating through
/// characters and tracking positions. Concrete implementations override
/// render_char to handle individual characters.
pub trait PDFTextDevice: PDFDevice {
    /// Render a text string by dispatching to horizontal or vertical rendering.
    ///
    /// This is the main entry point for text rendering. It computes the
    /// transformation matrix and delegates to render_string_horizontal or
    /// render_string_vertical based on font writing mode.
    ///
    /// TODO: Full implementation requires PDFFont from pdfinterp (Task 8).
    fn render_string(
        &mut self,
        textstate: &mut PDFTextState,
        seq: &PDFTextSeq,
        ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) {
        let ctm = match self.ctm() {
            Some(ctm) => ctm,
            None => return,
        };
        let matrix = mult_matrix(textstate.matrix, ctm);

        // TODO: When PDFFont is available, implement proper text rendering:
        // - Get font from textstate
        // - Calculate dxscale = 0.001 * fontsize * scaling
        // - Dispatch to render_string_horizontal or render_string_vertical
        // For now, this is a stub that does basic processing.

        let fontsize = textstate.fontsize;
        let scaling = textstate.scaling * 0.01;
        let charspace = textstate.charspace * scaling;
        let wordspace = textstate.wordspace * scaling;
        let rise = textstate.rise;
        let dxscale = 0.001 * fontsize * scaling;

        // Stub: Assume horizontal, no font available yet
        // When font is available, check font.is_vertical() and dispatch accordingly
        let _ = (
            matrix,
            charspace,
            wordspace,
            rise,
            dxscale,
            seq,
            ncs,
            graphicstate,
        );
    }

    /// Render a horizontal text string.
    ///
    /// Iterates through the text sequence, processing positioning adjustments
    /// and character data. Returns the final position after rendering.
    ///
    /// # Arguments
    /// * `seq` - Text sequence with numbers (positioning) and bytes (characters)
    /// * `matrix` - Transformation matrix for character rendering
    /// * `pos` - Starting position (x, y)
    /// * `font` - Font for decoding characters
    /// * `fontsize` - Font size in user units
    /// * `scaling` - Horizontal scaling factor
    /// * `charspace` - Character spacing
    /// * `wordspace` - Word spacing (applied to space character)
    /// * `rise` - Text rise (baseline offset)
    /// * `dxscale` - Positioning scale factor (0.001 * fontsize * scaling)
    /// * `ncs` - Non-stroking color space
    /// * `graphicstate` - Graphics state
    #[allow(clippy::too_many_arguments)]
    fn render_string_horizontal<F: PDFFontLike>(
        &mut self,
        seq: &PDFTextSeq,
        matrix: Matrix,
        pos: Point,
        font: &F,
        fontsize: f64,
        scaling: f64,
        charspace: f64,
        wordspace: f64,
        rise: f64,
        dxscale: f64,
        ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) -> Point {
        let (mut x, y) = pos;
        let mut needcharspace = false;

        for item in seq {
            match item {
                PDFTextSeqItem::Number(n) => {
                    x -= n * dxscale;
                    needcharspace = true;
                }
                PDFTextSeqItem::Bytes(data) => {
                    for cid in font.decode(data) {
                        if needcharspace {
                            x += charspace;
                        }
                        let char_matrix = translate_matrix(matrix, (x, y));
                        x += self.render_char(
                            char_matrix,
                            font,
                            fontsize,
                            scaling,
                            rise,
                            cid,
                            ncs,
                            graphicstate,
                        );
                        if cid == 32 && wordspace != 0.0 {
                            x += wordspace;
                        }
                        needcharspace = true;
                    }
                }
            }
        }
        (x, y)
    }

    /// Render a vertical text string.
    ///
    /// Similar to render_string_horizontal but advances in the Y direction.
    /// Used for vertical writing mode fonts (CJK vertical text).
    #[allow(clippy::too_many_arguments)]
    fn render_string_vertical<F: PDFFontLike>(
        &mut self,
        seq: &PDFTextSeq,
        matrix: Matrix,
        pos: Point,
        font: &F,
        fontsize: f64,
        scaling: f64,
        charspace: f64,
        wordspace: f64,
        rise: f64,
        dxscale: f64,
        ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) -> Point {
        let (x, mut y) = pos;
        let mut needcharspace = false;

        for item in seq {
            match item {
                PDFTextSeqItem::Number(n) => {
                    y -= n * dxscale;
                    needcharspace = true;
                }
                PDFTextSeqItem::Bytes(data) => {
                    for cid in font.decode(data) {
                        if needcharspace {
                            y += charspace;
                        }
                        let char_matrix = translate_matrix(matrix, (x, y));
                        y += self.render_char(
                            char_matrix,
                            font,
                            fontsize,
                            scaling,
                            rise,
                            cid,
                            ncs,
                            graphicstate,
                        );
                        if cid == 32 && wordspace != 0.0 {
                            y += wordspace;
                        }
                        needcharspace = true;
                    }
                }
            }
        }
        (x, y)
    }

    /// Render a single character.
    ///
    /// Returns the character advancement (width for horizontal, height for vertical).
    ///
    /// # Arguments
    /// * `matrix` - Transformation matrix for character placement
    /// * `font` - Font for glyph lookup
    /// * `fontsize` - Font size
    /// * `scaling` - Horizontal scaling factor
    /// * `rise` - Text rise
    /// * `cid` - Character ID
    /// * `ncs` - Non-stroking color space
    /// * `graphicstate` - Graphics state
    #[allow(clippy::too_many_arguments)]
    fn render_char<F: PDFFontLike>(
        &mut self,
        _matrix: Matrix,
        _font: &F,
        _fontsize: f64,
        _scaling: f64,
        _rise: f64,
        _cid: u32,
        _ncs: &PDFColorSpace,
        _graphicstate: &PDFGraphicState,
    ) -> f64 {
        0.0
    }
}

/// Tag Extractor - extracts structured content tags to XML.
///
/// Port of TagExtractor from pdfminer.six pdfdevice.py
pub struct TagExtractor<'a, W: Write> {
    /// Output writer
    outfp: &'a mut W,
    /// Output encoding (stored for future use with proper encoding support)
    #[allow(dead_code)]
    codec: &'a str,
    /// Current page number
    pageno: u32,
    /// Stack of open tags
    stack: Vec<PSLiteral>,
    /// Current transformation matrix
    ctm: Option<Matrix>,
}

impl<'a, W: Write> TagExtractor<'a, W> {
    /// Create a new TagExtractor.
    pub const fn new(outfp: &'a mut W, codec: &'a str) -> Self {
        Self {
            outfp,
            codec,
            pageno: 0,
            stack: Vec::new(),
            ctm: None,
        }
    }

    /// Get current page number.
    pub const fn pageno(&self) -> u32 {
        self.pageno
    }

    /// Increment page number.
    pub const fn increment_pageno(&mut self) {
        self.pageno += 1;
    }

    /// Write text to output.
    pub fn write(&mut self, s: &str) {
        let _ = self.outfp.write_all(s.as_bytes());
    }

    fn write_bytes(&mut self, s: &str) {
        // In Python this encodes to self.codec; for simplicity we use UTF-8
        let _ = self.outfp.write_all(s.as_bytes());
    }
}

impl<'a, W: Write> PDFDevice for TagExtractor<'a, W> {
    fn set_ctm(&mut self, ctm: Matrix) {
        self.ctm = Some(ctm);
    }

    fn ctm(&self) -> Option<Matrix> {
        self.ctm
    }

    fn begin_page(&mut self, pageid: u32, mediabox: Rect, _ctm: Matrix) {
        let (x0, y0, x1, y1) = mediabox;
        let output = format!(
            "<page id=\"{}\" bbox=\"{:.3},{:.3},{:.3},{:.3}\" rotate=\"0\">",
            pageid, x0, y0, x1, y1
        );
        self.write_bytes(&output);
    }

    fn end_page(&mut self, _pageid: u32) {
        self.write_bytes("</page>\n");
        self.pageno += 1;
    }

    fn begin_tag(&mut self, tag: &PSLiteral, props: Option<&PDFStackT>) {
        let mut s = String::new();
        if let Some(props) = props {
            let mut sorted_keys: Vec<_> = props.keys().collect();
            sorted_keys.sort();
            for k in sorted_keys {
                if let Some(v) = props.get(k) {
                    let v_str = format!("{:?}", v);
                    s.push_str(&format!(" {}=\"{}\"", enc(k), enc(&v_str)));
                }
            }
        }
        let out_s = format!("<{}{}>", enc(tag.name()), s);
        self.write_bytes(&out_s);
        self.stack.push(tag.clone());
    }

    fn end_tag(&mut self) {
        if let Some(tag) = self.stack.pop() {
            let out_s = format!("</{}>", enc(tag.name()));
            self.write_bytes(&out_s);
        }
    }

    fn do_tag(&mut self, tag: &PSLiteral, props: Option<&PDFStackT>) {
        self.begin_tag(tag, props);
        self.stack.pop();
    }
}

impl<'a, W: Write> PDFTextDevice for TagExtractor<'a, W> {
    /// Render a text string by extracting Unicode text and writing to output.
    ///
    /// Unlike the base PDFTextDevice which tracks positions, TagExtractor
    /// only extracts the text content for structured output.
    ///
    /// TODO: Full implementation requires PDFFont from pdfinterp (Task 8).
    /// When font is available:
    /// 1. Get font from textstate
    /// 2. Iterate through seq, skip non-bytes items
    /// 3. For each bytes item, decode to CIDs via font.decode()
    /// 4. For each CID, convert to Unicode via font.to_unichr()
    /// 5. Write the collected text to output
    fn render_string(
        &mut self,
        textstate: &mut PDFTextState,
        seq: &PDFTextSeq,
        _ncs: &PDFColorSpace,
        _graphicstate: &PDFGraphicState,
    ) {
        // Extract raw bytes as ASCII where possible
        let _ = textstate; // silence unused warning
        for item in seq {
            if let PDFTextSeqItem::Bytes(data) = item {
                // Basic ASCII extraction without proper font decoding
                let text: String = data
                    .iter()
                    .filter_map(|&b| {
                        if (0x20..0x7f).contains(&b) {
                            Some(b as char)
                        } else {
                            None
                        }
                    })
                    .collect();
                if !text.is_empty() {
                    self.write_bytes(&enc(&text));
                }
            }
        }
    }
}
