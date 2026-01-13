//! Character types: LTChar and LTAnno.
//!
//! Use `LTChar::builder()` to construct characters with optional fields.

use crate::utils::{MATRIX_IDENTITY, Matrix, Rect};

use super::component::LTComponent;

/// Optional color type for stroking/non-stroking colors.
pub type Color = Option<Vec<f64>>;

/// Virtual character inserted by layout analyzer (e.g., space, newline).
///
/// Unlike LTChar, LTAnno has no bounding box as it represents a character
/// inferred from the relationship between real characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LTAnno {
    text: String,
}

impl LTAnno {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
        }
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }
}

/// Builder for LTChar with fluent API for optional fields.
///
/// # Example
/// ```ignore
/// let ch = LTChar::builder((0.0, 0.0, 10.0, 12.0), "A", "Helvetica", 12.0)
///     .matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
///     .mcid(Some(42))
///     .tag(Some("P".to_string()))
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct LTCharBuilder {
    bbox: Rect,
    text: String,
    fontname: String,
    size: f64,
    upright: bool,
    adv: f64,
    matrix: Matrix,
    mcid: Option<i32>,
    tag: Option<String>,
    ncs: Option<String>,
    scs: Option<String>,
    non_stroking_color: Color,
    stroking_color: Color,
}

impl LTCharBuilder {
    /// Creates a new builder with required fields.
    /// Optional fields default to: upright=true, adv=0.0, matrix=identity, others=None.
    pub fn new(bbox: Rect, text: &str, fontname: &str, size: f64) -> Self {
        Self {
            bbox,
            text: text.to_string(),
            fontname: fontname.to_string(),
            size,
            upright: true,
            adv: 0.0,
            matrix: MATRIX_IDENTITY,
            mcid: None,
            tag: None,
            ncs: None,
            scs: None,
            non_stroking_color: None,
            stroking_color: None,
        }
    }

    /// Sets whether the character is upright (default: true).
    pub const fn upright(mut self, upright: bool) -> Self {
        self.upright = upright;
        self
    }

    /// Sets the advance width (default: 0.0).
    pub const fn adv(mut self, adv: f64) -> Self {
        self.adv = adv;
        self
    }

    /// Sets the text rendering matrix (default: identity matrix).
    pub const fn matrix(mut self, matrix: Matrix) -> Self {
        self.matrix = matrix;
        self
    }

    /// Sets the Marked Content ID for tagged PDF accessibility.
    pub const fn mcid(mut self, mcid: Option<i32>) -> Self {
        self.mcid = mcid;
        self
    }

    /// Sets the Marked Content tag (e.g., "P", "Span", "H1").
    pub fn tag(mut self, tag: Option<String>) -> Self {
        self.tag = tag;
        self
    }

    /// Sets the non-stroking colorspace name (e.g., "DeviceRGB").
    pub fn ncs(mut self, ncs: Option<String>) -> Self {
        self.ncs = ncs;
        self
    }

    /// Sets the stroking colorspace name (e.g., "DeviceRGB").
    pub fn scs(mut self, scs: Option<String>) -> Self {
        self.scs = scs;
        self
    }

    /// Sets the non-stroking (fill) color.
    pub fn non_stroking_color(mut self, color: Color) -> Self {
        self.non_stroking_color = color;
        self
    }

    /// Sets the stroking color.
    pub fn stroking_color(mut self, color: Color) -> Self {
        self.stroking_color = color;
        self
    }

    /// Builds the LTChar instance.
    pub fn build(self) -> LTChar {
        LTChar {
            component: LTComponent::new(self.bbox),
            text: self.text,
            fontname: self.fontname,
            size: self.size,
            upright: self.upright,
            adv: self.adv,
            matrix: self.matrix,
            mcid: self.mcid,
            tag: self.tag,
            ncs: self.ncs,
            scs: self.scs,
            non_stroking_color: self.non_stroking_color,
            stroking_color: self.stroking_color,
        }
    }
}

/// Actual character in text with bounding box.
#[derive(Debug, Clone, PartialEq)]
pub struct LTChar {
    component: LTComponent,
    text: String,
    fontname: String,
    size: f64,
    upright: bool,
    adv: f64,
    /// Text rendering matrix (Tm * CTM, with per-char translation)
    matrix: Matrix,
    /// Marked Content ID for tagged PDF accessibility
    mcid: Option<i32>,
    /// Marked Content tag (e.g., "P", "Span", "H1") for tagged PDF
    tag: Option<String>,
    /// Non-stroking colorspace name (e.g., "DeviceRGB")
    ncs: Option<String>,
    /// Stroking colorspace name (e.g., "DeviceRGB")
    scs: Option<String>,
    /// Non-stroking (fill) color
    non_stroking_color: Color,
    /// Stroking color
    stroking_color: Color,
}

impl LTChar {
    /// Creates a new builder for constructing LTChar instances.
    ///
    /// # Example
    /// ```ignore
    /// let ch = LTChar::builder((0.0, 0.0, 10.0, 12.0), "A", "Helvetica", 12.0)
    ///     .upright(true)
    ///     .adv(10.0)
    ///     .matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    ///     .build();
    /// ```
    pub fn builder(bbox: Rect, text: &str, fontname: &str, size: f64) -> LTCharBuilder {
        LTCharBuilder::new(bbox, text, fontname, size)
    }

    /// Creates a basic character with required fields.
    pub fn new(bbox: Rect, text: &str, fontname: &str, size: f64, upright: bool, adv: f64) -> Self {
        Self::builder(bbox, text, fontname, size)
            .upright(upright)
            .adv(adv)
            .build()
    }

    /// Creates a character with a custom transformation matrix.
    pub fn new_with_matrix(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        matrix: Matrix,
    ) -> Self {
        Self::builder(bbox, text, fontname, size)
            .upright(upright)
            .adv(adv)
            .matrix(matrix)
            .build()
    }

    /// Creates a character with Marked Content ID.
    pub fn with_mcid(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        mcid: Option<i32>,
    ) -> Self {
        Self::builder(bbox, text, fontname, size)
            .upright(upright)
            .adv(adv)
            .mcid(mcid)
            .build()
    }

    /// Creates a character with full color information.
    #[allow(clippy::too_many_arguments)]
    pub fn with_colors(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        mcid: Option<i32>,
        tag: Option<String>,
        non_stroking_color: Color,
        stroking_color: Color,
    ) -> Self {
        Self::builder(bbox, text, fontname, size)
            .upright(upright)
            .adv(adv)
            .mcid(mcid)
            .tag(tag)
            .non_stroking_color(non_stroking_color)
            .stroking_color(stroking_color)
            .build()
    }

    /// Creates a character with full color and matrix information.
    #[allow(clippy::too_many_arguments)]
    pub fn with_colors_matrix(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        matrix: Matrix,
        mcid: Option<i32>,
        tag: Option<String>,
        non_stroking_color: Color,
        stroking_color: Color,
    ) -> Self {
        Self::builder(bbox, text, fontname, size)
            .upright(upright)
            .adv(adv)
            .matrix(matrix)
            .mcid(mcid)
            .tag(tag)
            .non_stroking_color(non_stroking_color)
            .stroking_color(stroking_color)
            .build()
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn fontname(&self) -> &str {
        &self.fontname
    }

    pub const fn size(&self) -> f64 {
        self.size
    }

    pub const fn upright(&self) -> bool {
        self.upright
    }

    pub const fn adv(&self) -> f64 {
        self.adv
    }

    pub const fn matrix(&self) -> Matrix {
        self.matrix
    }

    pub const fn mcid(&self) -> Option<i32> {
        self.mcid
    }

    pub fn tag(&self) -> Option<String> {
        self.tag.clone()
    }

    pub fn ncs(&self) -> Option<String> {
        self.ncs.clone()
    }

    pub fn set_ncs(&mut self, ncs: Option<String>) {
        self.ncs = ncs;
    }

    pub fn scs(&self) -> Option<String> {
        self.scs.clone()
    }

    pub fn set_scs(&mut self, scs: Option<String>) {
        self.scs = scs;
    }

    pub const fn non_stroking_color(&self) -> &Color {
        &self.non_stroking_color
    }

    pub const fn stroking_color(&self) -> &Color {
        &self.stroking_color
    }
}

impl std::ops::Deref for LTChar {
    type Target = LTComponent;
    fn deref(&self) -> &Self::Target {
        &self.component
    }
}

impl std::ops::DerefMut for LTChar {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.component
    }
}

impl_has_bbox_delegate!(LTChar, component);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_matches_new_constructor() {
        let via_new = LTChar::new((0.0, 0.0, 10.0, 12.0), "A", "Helvetica", 12.0, true, 10.0);
        let via_builder = LTChar::builder((0.0, 0.0, 10.0, 12.0), "A", "Helvetica", 12.0)
            .upright(true)
            .adv(10.0)
            .build();

        assert_eq!(via_new.get_text(), via_builder.get_text());
        assert_eq!(via_new.fontname(), via_builder.fontname());
        assert_eq!(via_new.size(), via_builder.size());
        assert_eq!(via_new.upright(), via_builder.upright());
        assert_eq!(via_new.adv(), via_builder.adv());
        assert_eq!(via_new.matrix(), via_builder.matrix());
        assert_eq!(via_new.mcid(), via_builder.mcid());
        assert_eq!(via_new.tag(), via_builder.tag());
        assert_eq!(via_new.ncs(), via_builder.ncs());
        assert_eq!(via_new.scs(), via_builder.scs());
        assert_eq!(via_new.bbox(), via_builder.bbox());
    }

    #[test]
    fn builder_matches_with_colors_matrix() {
        let colors = Some(vec![1.0, 0.0, 0.0]);
        let matrix = (2.0, 0.0, 0.0, 2.0, 10.0, 20.0);

        let via_constructor = LTChar::with_colors_matrix(
            (0.0, 0.0, 10.0, 12.0),
            "B",
            "Times",
            14.0,
            false,
            8.0,
            matrix,
            Some(42),
            Some("P".to_string()),
            colors.clone(),
            Some(vec![0.0, 1.0, 0.0]),
        );

        let via_builder = LTChar::builder((0.0, 0.0, 10.0, 12.0), "B", "Times", 14.0)
            .upright(false)
            .adv(8.0)
            .matrix(matrix)
            .mcid(Some(42))
            .tag(Some("P".to_string()))
            .non_stroking_color(colors)
            .stroking_color(Some(vec![0.0, 1.0, 0.0]))
            .build();

        assert_eq!(via_constructor.get_text(), via_builder.get_text());
        assert_eq!(via_constructor.fontname(), via_builder.fontname());
        assert_eq!(via_constructor.size(), via_builder.size());
        assert_eq!(via_constructor.upright(), via_builder.upright());
        assert_eq!(via_constructor.adv(), via_builder.adv());
        assert_eq!(via_constructor.matrix(), via_builder.matrix());
        assert_eq!(via_constructor.mcid(), via_builder.mcid());
        assert_eq!(via_constructor.tag(), via_builder.tag());
        assert_eq!(
            via_constructor.non_stroking_color(),
            via_builder.non_stroking_color()
        );
        assert_eq!(
            via_constructor.stroking_color(),
            via_builder.stroking_color()
        );
    }

    #[test]
    fn builder_defaults_are_correct() {
        // Test that builder defaults match documented behavior
        let ch = LTChar::builder((0.0, 0.0, 10.0, 12.0), "X", "Font", 10.0).build();

        assert!(ch.upright()); // Default: true
        assert_eq!(ch.adv(), 0.0); // Default: 0.0
        assert_eq!(ch.matrix(), MATRIX_IDENTITY); // Default: identity
        assert_eq!(ch.mcid(), None);
        assert_eq!(ch.tag(), None);
        assert_eq!(ch.ncs(), None);
        assert_eq!(ch.scs(), None);
        assert_eq!(ch.non_stroking_color(), &None);
        assert_eq!(ch.stroking_color(), &None);
    }
}
