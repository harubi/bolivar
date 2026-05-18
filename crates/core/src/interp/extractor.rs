//! `TagExtractor` device — extracts structured content tags to XML.
//!
//! Port of `TagExtractor` from pdfminer.six `pdfdevice.py`. This device is
//! interpreter-side (it consumes the interpreter callbacks defined by
//! `PDFDevice`), so it lives next to the interpreter rather than in the
//! top-level `device/` directory.

use crate::device::{PDFDevice, PDFTextDevice};
use crate::interp::types::{PDFStackT, PDFTextSeq, PDFTextSeqItem};
use crate::pdfcolor::PDFColorSpace;
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::psparser::PSLiteral;
use crate::utils::{Matrix, Rect, enc};
use std::io::Write;

/// Tag Extractor - extracts structured content tags to XML.
///
/// Port of TagExtractor from pdfminer.six pdfdevice.py
pub struct TagExtractor<W: Write> {
    /// Output writer
    outfp: W,
    /// Output encoding (stored for future use with proper encoding support)
    #[allow(dead_code)]
    codec: String,
    /// Current page number
    pageno: u32,
    /// Stack of open tags
    stack: Vec<PSLiteral>,
    /// Current transformation matrix
    ctm: Option<Matrix>,
}

impl<W: Write> TagExtractor<W> {
    /// Create a new TagExtractor.
    pub fn new(outfp: W, codec: &str) -> Self {
        Self {
            outfp,
            codec: codec.to_string(),
            pageno: 0,
            stack: Vec::new(),
            ctm: None,
        }
    }

    /// Consume the extractor and return the inner writer.
    pub fn into_inner(self) -> W {
        self.outfp
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

    /// Flush output.
    pub fn flush(&mut self) {
        let _ = self.outfp.flush();
    }
}

fn rotation_from_ctm(ctm: Matrix) -> i32 {
    let (a, b, c, d, _, _) = ctm;
    let eps = 1e-9;
    let is_zero = |value: f64| value.abs() < eps;

    if is_zero(a) && is_zero(d) {
        if b < 0.0 && c > 0.0 {
            return 90;
        }
        if b > 0.0 && c < 0.0 {
            return 270;
        }
    }

    if is_zero(b) && is_zero(c) && a < 0.0 && d < 0.0 {
        return 180;
    }

    0
}

impl<W: Write> PDFDevice for TagExtractor<W> {
    fn set_ctm(&mut self, ctm: Matrix) {
        self.ctm = Some(ctm);
    }

    fn ctm(&self) -> Option<Matrix> {
        self.ctm
    }

    fn begin_page(&mut self, _pageid: u32, mediabox: Rect, ctm: Matrix) {
        let (x0, y0, x1, y1) = mediabox;
        let rotate = rotation_from_ctm(ctm);
        let output = format!(
            "<page id=\"{}\" bbox=\"{:.3},{:.3},{:.3},{:.3}\" rotate=\"{}\">",
            self.pageno, x0, y0, x1, y1, rotate
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

    fn render_string(
        &mut self,
        textstate: &mut PDFTextState,
        seq: &PDFTextSeq,
        ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) {
        <Self as PDFTextDevice>::render_string(self, textstate, seq, ncs, graphicstate);
    }
}

impl<W: Write> PDFTextDevice for TagExtractor<W> {
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
