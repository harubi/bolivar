//! Color operators.
//!
//! Handles: G, g, RG, rg, K, k, SC, SCN, sc, scn
//!
//! - G/g: DeviceGray (stroke/non-stroke)
//! - RG/rg: DeviceRGB (stroke/non-stroke)
//! - K/k: DeviceCMYK (stroke/non-stroke)
//! - SC/SCN/sc/scn: Set color in current color space
//!
//! Note: CS/cs (set color space) not yet implemented.

use crate::interp::device::PDFDevice;
use crate::interp::interpreter::PDFPageInterpreter;
use crate::pdfcolor::PREDEFINED_COLORSPACE;
use crate::pdfstate::Color;
use crate::psparser::PSToken;

#[allow(non_snake_case)]
impl<'a, D: PDFDevice> PDFPageInterpreter<'a, D> {
    /// Sets the gray level for stroking operations.
    ///
    /// PDF operator: `G`
    ///
    /// Sets the stroking color space to DeviceGray and the gray level
    /// to the specified value (0.0 = black, 1.0 = white).
    pub fn do_G(&mut self, gray: f64) {
        let cs = PREDEFINED_COLORSPACE
            .get("DeviceGray")
            .expect("DeviceGray must exist")
            .clone();
        self.graphicstate.scs = cs;
        self.graphicstate.scolor = Color::Gray(gray);
    }

    /// Sets the gray level for non-stroking operations.
    ///
    /// PDF operator: `g`
    ///
    /// Sets the non-stroking color space to DeviceGray and the gray level
    /// to the specified value (0.0 = black, 1.0 = white).
    pub fn do_g(&mut self, gray: f64) {
        let cs = PREDEFINED_COLORSPACE
            .get("DeviceGray")
            .expect("DeviceGray must exist")
            .clone();
        self.graphicstate.ncs = cs;
        self.graphicstate.ncolor = Color::Gray(gray);
    }

    /// Sets the RGB color for stroking operations.
    ///
    /// PDF operator: `RG`
    ///
    /// Sets the stroking color space to DeviceRGB and the color
    /// to the specified RGB values (each 0.0 to 1.0).
    pub fn do_RG(&mut self, r: f64, g: f64, b: f64) {
        let cs = PREDEFINED_COLORSPACE
            .get("DeviceRGB")
            .expect("DeviceRGB must exist")
            .clone();
        self.graphicstate.scs = cs;
        self.graphicstate.scolor = Color::Rgb(r, g, b);
    }

    /// Sets the RGB color for non-stroking operations.
    ///
    /// PDF operator: `rg`
    ///
    /// Sets the non-stroking color space to DeviceRGB and the color
    /// to the specified RGB values (each 0.0 to 1.0).
    pub fn do_rg(&mut self, r: f64, g: f64, b: f64) {
        let cs = PREDEFINED_COLORSPACE
            .get("DeviceRGB")
            .expect("DeviceRGB must exist")
            .clone();
        self.graphicstate.ncs = cs;
        self.graphicstate.ncolor = Color::Rgb(r, g, b);
    }

    /// Sets the CMYK color for stroking operations.
    ///
    /// PDF operator: `K`
    ///
    /// Sets the stroking color space to DeviceCMYK and the color
    /// to the specified CMYK values (each 0.0 to 1.0).
    pub fn do_K(&mut self, c: f64, m: f64, y: f64, k: f64) {
        let cs = PREDEFINED_COLORSPACE
            .get("DeviceCMYK")
            .expect("DeviceCMYK must exist")
            .clone();
        self.graphicstate.scs = cs;
        self.graphicstate.scolor = Color::Cmyk(c, m, y, k);
    }

    /// Sets the CMYK color for non-stroking operations.
    ///
    /// PDF operator: `k`
    ///
    /// Sets the non-stroking color space to DeviceCMYK and the color
    /// to the specified CMYK values (each 0.0 to 1.0).
    pub fn do_k(&mut self, c: f64, m: f64, y: f64, k: f64) {
        let cs = PREDEFINED_COLORSPACE
            .get("DeviceCMYK")
            .expect("DeviceCMYK must exist")
            .clone();
        self.graphicstate.ncs = cs;
        self.graphicstate.ncolor = Color::Cmyk(c, m, y, k);
    }

    /// Sets the color for stroking operations in the current color space.
    ///
    /// PDF operator: `SC` / `SCN`
    ///
    /// Handles Pattern color spaces per ISO 32000-1:2008 4.5.5 (PDF 1.7)
    /// and ISO 32000-2:2020 8.7.3 (PDF 2.0):
    /// - Colored patterns (PaintType=1): single operand (pattern name)
    /// - Uncolored patterns (PaintType=2): n+1 operands (colors + pattern name)
    pub fn do_SC(&mut self, args: &mut Vec<PSToken>) {
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

    /// Sets the color for non-stroking operations in the current color space.
    ///
    /// PDF operator: `sc` / `scn`
    ///
    /// Handles Pattern color spaces per ISO 32000-1:2008 4.5.5 (PDF 1.7)
    /// and ISO 32000-2:2020 8.7.3 (PDF 2.0):
    /// - Colored patterns (PaintType=1): single operand (pattern name)
    /// - Uncolored patterns (PaintType=2): n+1 operands (colors + pattern name)
    pub fn do_sc(&mut self, args: &mut Vec<PSToken>) {
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

    /// Parses color components from operand stack.
    ///
    /// Returns a Color based on the number of numeric components:
    /// - 1 component: Gray
    /// - 3 components: RGB
    /// - 4 components: CMYK
    pub(crate) fn parse_color_components(args: &[PSToken]) -> Option<Color> {
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
}
