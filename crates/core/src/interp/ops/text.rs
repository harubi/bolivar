//! Text operators.
//!
//! Handles: BT, ET, Tc, Tw, Tz, TL, Tf, Tr, Ts, Td, TD, Tm, T*, Tj, TJ, ', "
//!
//! Text object:
//! - BT/ET: Begin/end text object
//!
//! Text state:
//! - Tc: Character spacing
//! - Tw: Word spacing
//! - Tz: Horizontal scaling
//! - TL: Leading
//! - Tf: Font and size
//! - Tr: Rendering mode
//! - Ts: Rise (baseline offset)
//!
//! Text positioning:
//! - Td/TD: Move to next line (TD also sets leading)
//! - Tm: Set text matrix directly
//! - T*: Move to next line using current leading
//!
//! Text showing:
//! - Tj: Show string
//! - TJ: Show with individual glyph positioning
//! - ': Move to next line and show
//! - ": Set spacing, move to next line, and show

use crate::interp::device::{PDFDevice, PDFTextSeq, PDFTextSeqItem};
use crate::interp::interpreter::PDFPageInterpreter;

#[allow(non_snake_case)]
impl<'a, D: PDFDevice> PDFPageInterpreter<'a, D> {
    // ========================================================================
    // Text Object Operators
    // ========================================================================

    /// BT - Begin text object.
    ///
    /// Initializes the text matrix (Tm) and text line matrix (Tlm) to identity.
    /// Text objects cannot be nested.
    pub const fn do_BT(&mut self) {
        self.textstate.reset();
    }

    /// ET - End text object.
    pub const fn do_ET(&mut self) {
        // No action needed - text state persists for subsequent text objects
    }

    // ========================================================================
    // Text State Operators
    // ========================================================================

    /// Tc - Set character spacing.
    ///
    /// Character spacing is used by Tj, TJ, and ' operators.
    pub const fn do_Tc(&mut self, charspace: f64) {
        self.textstate.charspace = charspace;
    }

    /// Tw - Set word spacing.
    ///
    /// Word spacing is used by Tj, TJ, and ' operators.
    pub const fn do_Tw(&mut self, wordspace: f64) {
        self.textstate.wordspace = wordspace;
    }

    /// Tz - Set horizontal scaling.
    ///
    /// Scaling is a percentage (100 = normal width).
    pub const fn do_Tz(&mut self, scaling: f64) {
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
    pub const fn do_Tr(&mut self, render: i32) {
        self.textstate.render = render;
    }

    /// Ts - Set text rise (superscript/subscript offset).
    pub const fn do_Ts(&mut self, rise: f64) {
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
        let e_new = tx.mul_add(a, ty * c) + e;
        let f_new = tx.mul_add(b, ty * d) + f;
        self.textstate.matrix = (a, b, c, d, e_new, f_new);
        self.textstate.linematrix = (0.0, 0.0);
    }

    /// TD - Move to start of next line and set leading.
    ///
    /// Same as Td but also sets text leading to ty.
    pub fn do_TD(&mut self, tx: f64, ty: f64) {
        let (a, b, c, d, e, f) = self.textstate.matrix;
        let e_new = tx.mul_add(a, ty * c) + e;
        let f_new = tx.mul_add(b, ty * d) + f;
        self.textstate.matrix = (a, b, c, d, e_new, f_new);
        self.textstate.leading = ty;
        self.textstate.linematrix = (0.0, 0.0);
    }

    /// Tm - Set text matrix and text line matrix.
    ///
    /// Directly sets the text matrix to the specified values.
    pub const fn do_Tm(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.textstate.matrix = (a, b, c, d, e, f);
        self.textstate.linematrix = (0.0, 0.0);
    }

    /// T* - Move to start of next text line.
    ///
    /// Uses current leading value. Equivalent to: 0 -leading Td
    pub const fn do_T_star(&mut self) {
        let (a, b, c, d, e, f) = self.textstate.matrix;
        let leading = self.textstate.leading;
        // T* equivalent to Td(0, leading), but note leading already stores -TL
        self.textstate.matrix = (a, b, c, d, leading.mul_add(c, e), leading.mul_add(d, f));
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
}
