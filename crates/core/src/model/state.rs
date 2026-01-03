//! PDF Graphics and Text State.
//!
//! Port of state-related types from pdfminer.six pdfinterp.py

use super::color::{PDFColorSpace, PREDEFINED_COLORSPACE};
use crate::pdffont::PDFCIDFont;
use crate::utils::{MATRIX_IDENTITY, Matrix, Point};
use std::sync::Arc;

/// Color value types used in PDF graphics state.
///
/// Corresponds to Python's Color union type:
/// - float for Greyscale
/// - (float, float, float) for R, G, B
/// - (float, float, float, float) for C, M, Y, K
/// - str for Pattern name (colored pattern, PaintType=1)
/// - (StandardColor, str) for (base_color, pattern_name) (uncolored pattern, PaintType=2)
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    /// Greyscale color (0.0 = black, 1.0 = white)
    Gray(f64),
    /// RGB color
    Rgb(f64, f64, f64),
    /// CMYK color
    Cmyk(f64, f64, f64, f64),
    /// Colored tiling pattern (PaintType=1) - just the pattern name
    PatternColored(String),
    /// Uncolored tiling pattern (PaintType=2) - base color + pattern name
    PatternUncolored(Box<Color>, String),
}

impl Default for Color {
    fn default() -> Self {
        Color::Gray(0.0)
    }
}

impl Color {
    /// Convert to a Vec<f64> for layout types.
    ///
    /// For pattern colors:
    /// - PatternColored: returns empty vec (no numeric components)
    /// - PatternUncolored: returns the base color's components
    pub fn to_vec(&self) -> Vec<f64> {
        match self {
            Color::Gray(g) => vec![*g],
            Color::Rgb(r, g, b) => vec![*r, *g, *b],
            Color::Cmyk(c, m, y, k) => vec![*c, *m, *y, *k],
            Color::PatternColored(_) => vec![], // No numeric components
            Color::PatternUncolored(base, _) => base.to_vec(),
        }
    }

    /// Get the pattern name if this is a pattern color.
    ///
    /// Returns `Some(&str)` for PatternColored and PatternUncolored,
    /// `None` for standard colors.
    pub fn pattern_name(&self) -> Option<&str> {
        match self {
            Color::PatternColored(name) => Some(name),
            Color::PatternUncolored(_, name) => Some(name),
            _ => None,
        }
    }

    /// Check if this color is a pattern color.
    pub fn is_pattern(&self) -> bool {
        matches!(
            self,
            Color::PatternColored(_) | Color::PatternUncolored(_, _)
        )
    }
}

/// PDF Text State - manages text positioning and rendering parameters.
///
/// Port of PDFTextState from pdfminer.six pdfinterp.py
#[derive(Debug, Clone)]
pub struct PDFTextState {
    /// Current font (None if not set)
    pub font: Option<Arc<PDFCIDFont>>,
    /// Current font resource name (e.g., "F1") for fallback
    pub fontname: Option<String>,
    /// Font size in user units
    pub fontsize: f64,
    /// Character spacing
    pub charspace: f64,
    /// Word spacing (applied to space character, CID 32)
    pub wordspace: f64,
    /// Horizontal scaling percentage (100 = normal)
    pub scaling: f64,
    /// Text leading (vertical distance for Td operations)
    pub leading: f64,
    /// Text rendering mode (0-7)
    pub render: i32,
    /// Text rise (superscript/subscript offset)
    pub rise: f64,
    /// Text matrix (Tm)
    pub matrix: Matrix,
    /// Line matrix - current position within text object
    pub linematrix: Point,
}

impl PDFTextState {
    /// Create a new text state with default values.
    pub fn new() -> Self {
        let mut state = Self {
            font: None,
            fontname: None,
            fontsize: 0.0,
            charspace: 0.0,
            wordspace: 0.0,
            scaling: 100.0,
            leading: 0.0,
            render: 0,
            rise: 0.0,
            matrix: MATRIX_IDENTITY,
            linematrix: (0.0, 0.0),
        };
        state.reset();
        state
    }

    /// Create a copy of this text state.
    pub fn copy(&self) -> Self {
        Self {
            font: self.font.clone(),
            fontname: self.fontname.clone(),
            fontsize: self.fontsize,
            charspace: self.charspace,
            wordspace: self.wordspace,
            scaling: self.scaling,
            leading: self.leading,
            render: self.render,
            rise: self.rise,
            matrix: self.matrix,
            linematrix: self.linematrix,
        }
    }

    /// Reset text matrix and line matrix to defaults.
    ///
    /// Called at the start of each text object (BT operator).
    pub fn reset(&mut self) {
        self.matrix = MATRIX_IDENTITY;
        self.linematrix = (0.0, 0.0);
    }
}

impl Default for PDFTextState {
    fn default() -> Self {
        Self::new()
    }
}

/// PDF Graphics State - manages graphics rendering parameters.
///
/// Port of PDFGraphicState from pdfminer.six pdfinterp.py
#[derive(Debug, Clone)]
pub struct PDFGraphicState {
    /// Line width for stroke operations
    pub linewidth: f64,
    /// Line cap style (0, 1, or 2)
    pub linecap: Option<i32>,
    /// Line join style (0, 1, or 2)
    pub linejoin: Option<i32>,
    /// Miter limit for line joins
    pub miterlimit: Option<f64>,
    /// Dash pattern: (array, phase)
    pub dash: Option<(Vec<f64>, f64)>,
    /// Rendering intent name
    pub intent: Option<String>,
    /// Flatness tolerance
    pub flatness: Option<f64>,

    /// Stroking color
    pub scolor: Color,
    /// Stroking color space
    pub scs: PDFColorSpace,

    /// Non-stroking (fill) color
    pub ncolor: Color,
    /// Non-stroking color space
    pub ncs: PDFColorSpace,
}

impl PDFGraphicState {
    /// Create new graphics state with default values.
    pub fn new() -> Self {
        let device_gray = PREDEFINED_COLORSPACE
            .get("DeviceGray")
            .expect("DeviceGray must exist")
            .clone();

        Self {
            linewidth: 0.0,
            linecap: None,
            linejoin: None,
            miterlimit: None,
            dash: None,
            intent: None,
            flatness: None,
            scolor: Color::Gray(0.0),
            scs: device_gray.clone(),
            ncolor: Color::Gray(0.0),
            ncs: device_gray,
        }
    }

    /// Create a copy of this graphics state.
    pub fn copy(&self) -> Self {
        Self {
            linewidth: self.linewidth,
            linecap: self.linecap,
            linejoin: self.linejoin,
            miterlimit: self.miterlimit,
            dash: self.dash.clone(),
            intent: self.intent.clone(),
            flatness: self.flatness,
            scolor: self.scolor.clone(),
            scs: self.scs.clone(),
            ncolor: self.ncolor.clone(),
            ncs: self.ncs.clone(),
        }
    }
}

impl Default for PDFGraphicState {
    fn default() -> Self {
        Self::new()
    }
}
