//! PDF color space definitions.
//!
//! Port of pdfminer.six pdfcolor.py

use std::collections::HashMap;
use std::sync::LazyLock;

/// Represents a PDF color space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PDFColorSpace {
    /// Name of the color space (e.g., "DeviceRGB")
    pub name: String,
    /// Number of color components
    pub ncomponents: usize,
}

impl PDFColorSpace {
    /// Create a new color space.
    pub fn new(name: &str, ncomponents: usize) -> Self {
        Self {
            name: name.to_string(),
            ncomponents,
        }
    }
}

/// Predefined PDF color spaces.
///
/// Order matches pdfminer.six (DeviceGray is default/first).
pub static PREDEFINED_COLORSPACE: LazyLock<HashMap<&'static str, PDFColorSpace>> =
    LazyLock::new(|| {
        let entries = [
            ("DeviceGray", 1),
            ("CalRGB", 3),
            ("CalGray", 1),
            ("Lab", 3),
            ("DeviceRGB", 3),
            ("DeviceCMYK", 4),
            ("Separation", 1),
            ("Indexed", 1),
            ("Pattern", 1),
        ];

        let mut map = HashMap::with_capacity(entries.len());
        for (name, n) in entries {
            map.insert(name, PDFColorSpace::new(name, n));
        }
        map
    });

/// Inline image color space abbreviations.
pub static INLINE_COLORSPACE_ABBREV: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        HashMap::from([
            ("G", "DeviceGray"),
            ("RGB", "DeviceRGB"),
            ("CMYK", "DeviceCMYK"),
        ])
    });
