//! Helper types shared by the interpreter (`PDFPageInterpreter`) and the
//! `PDFDevice` trait.
//!
//! These types form the vocabulary the interpreter uses to communicate with
//! its sink device: text sequences, marked-content property dictionaries,
//! stack values, path segments, and a placeholder font interface.
//!
//! Extracted from the original `interp/device.rs` so the `PDFDevice` trait can
//! live in its own top-level `device/` module without dragging interpreter
//! internals along.

use crate::pdftypes::PDFName;
use rustc_hash::FxHashMap;

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
pub type PDFStackT = FxHashMap<PDFName, PDFStackValue>;

/// Values that can appear on the PDF stack.
#[derive(Debug, Clone)]
pub enum PDFStackValue {
    Int(i64),
    Real(f64),
    Bool(bool),
    Name(PDFName),
    String(Vec<u8>),
    Array(Vec<Self>),
    Dict(FxHashMap<PDFName, Self>),
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
