//! Base PDF converter wrapper - common scaffolding shared by output converters.
//!
//! Port of `PDFConverter` from pdfminer.six `converter.py`.

use std::io::Write;

use crate::layout::LAParams;

/// Base PDF Converter - common functionality for output converters.
#[allow(dead_code)]
pub struct PDFConverter<W: Write> {
    /// Output writer
    outfp: W,
    /// Output encoding
    codec: String,
    /// Current page number
    pageno: i32,
    /// Layout parameters
    laparams: Option<LAParams>,
    /// Whether output is binary
    outfp_binary: bool,
}

impl<W: Write> PDFConverter<W> {
    /// Create a new converter.
    pub fn new(outfp: W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
        Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            outfp_binary: true, // Default to binary
        }
    }

    /// Check if a stream is binary.
    ///
    /// In Rust, we use type-based detection rather than runtime checks.
    /// This is a simplified version that always returns true for byte writers.
    pub const fn is_binary_stream<T>(_stream: &T) -> bool {
        true
    }

    /// Check if output is text (not binary).
    pub const fn is_text_stream<T>(_stream: &T) -> bool {
        false
    }
}
