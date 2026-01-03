//! Layout element types for PDF text extraction.
//!
//! Contains all LT* struct definitions:
//! - LTComponent: Base type for objects with bounding boxes
//! - LTAnno: Virtual characters (spaces, newlines) inserted during analysis
//! - LTChar: Actual characters with bounding boxes
//! - LTTextLine: A line of text (horizontal or vertical)
//! - LTTextBox: A group of text lines
//! - LTTextGroup: Hierarchical grouping of text boxes
//! - LTCurve, LTLine, LTRect: Graphical elements
//! - LTImage: Image container
//! - LTFigure: Figure container (embedded PDF forms)
//! - LTPage: Page container
//! - LTLayoutContainer: Container that performs layout analysis
//! - LTItem: Enum to represent any layout object

use std::hash::Hash;

use crate::utils::{
    HasBBox, INF_F64, MATRIX_IDENTITY, Matrix, Plane, Point, Rect, apply_matrix_rect,
};

use super::params::LAParams;

// ============================================================================
// Base Component
// ============================================================================

/// Base component with a bounding box.
#[derive(Debug, Clone, PartialEq)]
pub struct LTComponent {
    pub(crate) x0: f64,
    pub(crate) y0: f64,
    pub(crate) x1: f64,
    pub(crate) y1: f64,
}

impl LTComponent {
    pub fn new(bbox: Rect) -> Self {
        let (x0, y0, x1, y1) = bbox;
        Self { x0, y0, x1, y1 }
    }

    pub fn set_bbox(&mut self, bbox: Rect) {
        let (x0, y0, x1, y1) = bbox;
        self.x0 = x0;
        self.y0 = y0;
        self.x1 = x1;
        self.y1 = y1;
    }

    pub fn x0(&self) -> f64 {
        self.x0
    }

    pub fn y0(&self) -> f64 {
        self.y0
    }

    pub fn x1(&self) -> f64 {
        self.x1
    }

    pub fn y1(&self) -> f64 {
        self.y1
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn bbox(&self) -> Rect {
        (self.x0, self.y0, self.x1, self.y1)
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    /// Returns true if there is horizontal overlap with another component.
    pub fn is_hoverlap(&self, other: &LTComponent) -> bool {
        other.x0 <= self.x1 && self.x0 <= other.x1
    }

    /// Returns the horizontal distance to another component.
    /// Returns 0 if they overlap.
    pub fn hdistance(&self, other: &LTComponent) -> f64 {
        if self.is_hoverlap(other) {
            0.0
        } else {
            (self.x0 - other.x1).abs().min((self.x1 - other.x0).abs())
        }
    }

    /// Returns the amount of horizontal overlap with another component.
    pub fn hoverlap(&self, other: &LTComponent) -> f64 {
        if self.is_hoverlap(other) {
            (self.x0 - other.x1).abs().min((self.x1 - other.x0).abs())
        } else {
            0.0
        }
    }

    /// Returns true if there is vertical overlap with another component.
    pub fn is_voverlap(&self, other: &LTComponent) -> bool {
        other.y0 <= self.y1 && self.y0 <= other.y1
    }

    /// Returns the vertical distance to another component.
    /// Returns 0 if they overlap.
    pub fn vdistance(&self, other: &LTComponent) -> f64 {
        if self.is_voverlap(other) {
            0.0
        } else {
            (self.y0 - other.y1).abs().min((self.y1 - other.y0).abs())
        }
    }

    /// Returns the amount of vertical overlap with another component.
    pub fn voverlap(&self, other: &LTComponent) -> f64 {
        if self.is_voverlap(other) {
            (self.y0 - other.y1).abs().min((self.y1 - other.y0).abs())
        } else {
            0.0
        }
    }
}

impl HasBBox for LTComponent {
    fn x0(&self) -> f64 {
        self.x0
    }
    fn y0(&self) -> f64 {
        self.y0
    }
    fn x1(&self) -> f64 {
        self.x1
    }
    fn y1(&self) -> f64 {
        self.y1
    }
}

impl Eq for LTComponent {}

impl Hash for LTComponent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x0.to_bits().hash(state);
        self.y0.to_bits().hash(state);
        self.x1.to_bits().hash(state);
        self.y1.to_bits().hash(state);
    }
}

// ============================================================================
// Color Type
// ============================================================================

/// Optional color type for stroking/non-stroking colors.
pub type Color = Option<Vec<f64>>;

// ============================================================================
// LTAnno - Virtual Character
// ============================================================================

/// Virtual character inserted by layout analyzer (e.g., space, newline).
///
/// Unlike LTChar, LTAnno has no bounding box as it represents a character
/// inferred from the relationship between real characters.
#[derive(Debug, Clone, PartialEq)]
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

// ============================================================================
// LTChar - Actual Character
// ============================================================================

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
    /// Non-stroking (fill) color
    non_stroking_color: Color,
    /// Stroking color
    stroking_color: Color,
}

impl LTChar {
    pub fn new(bbox: Rect, text: &str, fontname: &str, size: f64, upright: bool, adv: f64) -> Self {
        Self::new_with_matrix(bbox, text, fontname, size, upright, adv, MATRIX_IDENTITY)
    }

    pub fn new_with_matrix(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        matrix: Matrix,
    ) -> Self {
        Self {
            component: LTComponent::new(bbox),
            text: text.to_string(),
            fontname: fontname.to_string(),
            size,
            upright,
            adv,
            matrix,
            mcid: None,
            tag: None,
            ncs: None,
            non_stroking_color: None,
            stroking_color: None,
        }
    }

    pub fn with_mcid(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        mcid: Option<i32>,
    ) -> Self {
        Self::with_mcid_matrix(
            bbox,
            text,
            fontname,
            size,
            upright,
            adv,
            MATRIX_IDENTITY,
            mcid,
        )
    }

    pub fn with_mcid_matrix(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        matrix: Matrix,
        mcid: Option<i32>,
    ) -> Self {
        Self {
            component: LTComponent::new(bbox),
            text: text.to_string(),
            fontname: fontname.to_string(),
            size,
            upright,
            adv,
            matrix,
            mcid,
            tag: None,
            ncs: None,
            non_stroking_color: None,
            stroking_color: None,
        }
    }

    pub fn with_marked_content(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        mcid: Option<i32>,
        tag: Option<String>,
    ) -> Self {
        Self::with_marked_content_matrix(
            bbox,
            text,
            fontname,
            size,
            upright,
            adv,
            MATRIX_IDENTITY,
            mcid,
            tag,
        )
    }

    pub fn with_marked_content_matrix(
        bbox: Rect,
        text: &str,
        fontname: &str,
        size: f64,
        upright: bool,
        adv: f64,
        matrix: Matrix,
        mcid: Option<i32>,
        tag: Option<String>,
    ) -> Self {
        Self {
            component: LTComponent::new(bbox),
            text: text.to_string(),
            fontname: fontname.to_string(),
            size,
            upright,
            adv,
            matrix,
            mcid,
            tag,
            ncs: None,
            non_stroking_color: None,
            stroking_color: None,
        }
    }

    /// Create a character with full color information.
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
        Self::with_colors_matrix(
            bbox,
            text,
            fontname,
            size,
            upright,
            adv,
            MATRIX_IDENTITY,
            mcid,
            tag,
            non_stroking_color,
            stroking_color,
        )
    }

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
        Self {
            component: LTComponent::new(bbox),
            text: text.to_string(),
            fontname: fontname.to_string(),
            size,
            upright,
            adv,
            matrix,
            mcid,
            tag,
            ncs: None,
            non_stroking_color,
            stroking_color,
        }
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn fontname(&self) -> &str {
        &self.fontname
    }

    pub fn size(&self) -> f64 {
        self.size
    }

    pub fn upright(&self) -> bool {
        self.upright
    }

    pub fn adv(&self) -> f64 {
        self.adv
    }

    pub fn matrix(&self) -> Matrix {
        self.matrix
    }

    pub fn mcid(&self) -> Option<i32> {
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

    pub fn non_stroking_color(&self) -> &Color {
        &self.non_stroking_color
    }

    pub fn stroking_color(&self) -> &Color {
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

impl HasBBox for LTChar {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

// ============================================================================
// LTCurve - Generic Bezier Curve
// ============================================================================

use crate::utils::get_bound;

/// A generic Bezier curve.
///
/// The `pts` field contains the control points of the curve.
/// `original_path` can store the original PDF path operations for reconstruction.
#[derive(Debug, Clone, PartialEq)]
pub struct LTCurve {
    component: LTComponent,
    /// Control points of the curve
    pub pts: Vec<Point>,
    /// Line width
    pub linewidth: f64,
    /// Whether the path is stroked
    pub stroke: bool,
    /// Whether the path is filled
    pub fill: bool,
    /// Whether to use even-odd fill rule
    pub evenodd: bool,
    /// Stroking color
    pub stroking_color: Color,
    /// Non-stroking (fill) color
    pub non_stroking_color: Color,
    /// Original path operations (for reconstruction)
    pub original_path: Option<Vec<(char, Vec<Point>)>>,
    /// Dashing style: (pattern, phase)
    pub dashing_style: Option<(Vec<f64>, f64)>,
    /// Marked Content ID for tagged PDF accessibility
    mcid: Option<i32>,
    /// Marked Content tag (e.g., "P", "Span", "H1") for tagged PDF
    tag: Option<String>,
}

impl LTCurve {
    pub fn new(
        linewidth: f64,
        pts: Vec<Point>,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        stroking_color: Color,
        non_stroking_color: Color,
    ) -> Self {
        let bbox = get_bound(pts.iter().copied());
        Self {
            component: LTComponent::new(bbox),
            pts,
            linewidth,
            stroke,
            fill,
            evenodd,
            stroking_color,
            non_stroking_color,
            original_path: None,
            dashing_style: None,
            mcid: None,
            tag: None,
        }
    }

    /// Create a curve with dashing style and original path.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dashing(
        linewidth: f64,
        pts: Vec<Point>,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        stroking_color: Color,
        non_stroking_color: Color,
        original_path: Option<Vec<(char, Vec<Point>)>>,
        dashing_style: Option<(Vec<f64>, f64)>,
    ) -> Self {
        let bbox = get_bound(pts.iter().copied());
        Self {
            component: LTComponent::new(bbox),
            pts,
            linewidth,
            stroke,
            fill,
            evenodd,
            stroking_color,
            non_stroking_color,
            original_path,
            dashing_style,
            mcid: None,
            tag: None,
        }
    }

    /// Returns the points as a comma-separated string.
    pub fn get_pts(&self) -> String {
        self.pts
            .iter()
            .map(|(x, y)| format!("{:.3},{:.3}", x, y))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn mcid(&self) -> Option<i32> {
        self.mcid
    }

    pub fn tag(&self) -> Option<String> {
        self.tag.clone()
    }

    pub fn set_marked_content(&mut self, mcid: Option<i32>, tag: Option<String>) {
        self.mcid = mcid;
        self.tag = tag;
    }
}

impl std::ops::Deref for LTCurve {
    type Target = LTComponent;
    fn deref(&self) -> &Self::Target {
        &self.component
    }
}

impl HasBBox for LTCurve {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

// ============================================================================
// LTLine - Single Straight Line
// ============================================================================

/// A single straight line.
///
/// Could be used for separating text or figures.
#[derive(Debug, Clone, PartialEq)]
pub struct LTLine {
    curve: LTCurve,
}

impl LTLine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        linewidth: f64,
        p0: Point,
        p1: Point,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        stroking_color: Color,
        non_stroking_color: Color,
    ) -> Self {
        Self {
            curve: LTCurve::new(
                linewidth,
                vec![p0, p1],
                stroke,
                fill,
                evenodd,
                stroking_color,
                non_stroking_color,
            ),
        }
    }

    /// Create a line with dashing style and original path.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dashing(
        linewidth: f64,
        p0: Point,
        p1: Point,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        stroking_color: Color,
        non_stroking_color: Color,
        original_path: Option<Vec<(char, Vec<Point>)>>,
        dashing_style: Option<(Vec<f64>, f64)>,
    ) -> Self {
        Self {
            curve: LTCurve::new_with_dashing(
                linewidth,
                vec![p0, p1],
                stroke,
                fill,
                evenodd,
                stroking_color,
                non_stroking_color,
                original_path,
                dashing_style,
            ),
        }
    }

    pub fn p0(&self) -> Point {
        self.curve.pts[0]
    }

    pub fn p1(&self) -> Point {
        self.curve.pts[1]
    }

    pub fn set_marked_content(&mut self, mcid: Option<i32>, tag: Option<String>) {
        self.curve.set_marked_content(mcid, tag);
    }
}

impl std::ops::Deref for LTLine {
    type Target = LTCurve;
    fn deref(&self) -> &Self::Target {
        &self.curve
    }
}

impl HasBBox for LTLine {
    fn x0(&self) -> f64 {
        self.curve.x0()
    }
    fn y0(&self) -> f64 {
        self.curve.y0()
    }
    fn x1(&self) -> f64 {
        self.curve.x1()
    }
    fn y1(&self) -> f64 {
        self.curve.y1()
    }
}

// ============================================================================
// LTRect - Rectangle
// ============================================================================

/// A rectangle.
///
/// Could be used for framing pictures or figures.
#[derive(Debug, Clone, PartialEq)]
pub struct LTRect {
    curve: LTCurve,
}

impl LTRect {
    pub fn new(
        linewidth: f64,
        bbox: Rect,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        stroking_color: Color,
        non_stroking_color: Color,
    ) -> Self {
        let (x0, y0, x1, y1) = bbox;
        let pts = vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        Self {
            curve: LTCurve::new(
                linewidth,
                pts,
                stroke,
                fill,
                evenodd,
                stroking_color,
                non_stroking_color,
            ),
        }
    }

    /// Create a rectangle with dashing style and original path.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dashing(
        linewidth: f64,
        bbox: Rect,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        stroking_color: Color,
        non_stroking_color: Color,
        original_path: Option<Vec<(char, Vec<Point>)>>,
        dashing_style: Option<(Vec<f64>, f64)>,
    ) -> Self {
        let (x0, y0, x1, y1) = bbox;
        let pts = vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        Self {
            curve: LTCurve::new_with_dashing(
                linewidth,
                pts,
                stroke,
                fill,
                evenodd,
                stroking_color,
                non_stroking_color,
                original_path,
                dashing_style,
            ),
        }
    }

    pub fn set_marked_content(&mut self, mcid: Option<i32>, tag: Option<String>) {
        self.curve.set_marked_content(mcid, tag);
    }
}

impl std::ops::Deref for LTRect {
    type Target = LTCurve;
    fn deref(&self) -> &Self::Target {
        &self.curve
    }
}

impl HasBBox for LTRect {
    fn x0(&self) -> f64 {
        self.curve.x0()
    }
    fn y0(&self) -> f64 {
        self.curve.y0()
    }
    fn x1(&self) -> f64 {
        self.curve.x1()
    }
    fn y1(&self) -> f64 {
        self.curve.y1()
    }
}

// ============================================================================
// LTImage - Image Object
// ============================================================================

/// An image object.
///
/// Embedded images can be in JPEG, Bitmap, JBIG2, or other formats.
#[derive(Debug, Clone, PartialEq)]
pub struct LTImage {
    component: LTComponent,
    /// Image name/identifier
    pub name: String,
    /// Source dimensions (width, height)
    pub srcsize: (Option<i32>, Option<i32>),
    /// Whether this is an image mask
    pub imagemask: bool,
    /// Bits per component
    pub bits: i32,
    /// Color space name(s)
    pub colorspace: Vec<String>,
}

impl LTImage {
    pub fn new(
        name: &str,
        bbox: Rect,
        srcsize: (Option<i32>, Option<i32>),
        imagemask: bool,
        bits: i32,
        colorspace: Vec<String>,
    ) -> Self {
        Self {
            component: LTComponent::new(bbox),
            name: name.to_string(),
            srcsize,
            imagemask,
            bits,
            colorspace,
        }
    }
}

impl std::ops::Deref for LTImage {
    type Target = LTComponent;
    fn deref(&self) -> &Self::Target {
        &self.component
    }
}

impl HasBBox for LTImage {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

// ============================================================================
// Text Line Types
// ============================================================================

/// Trait for text line types.
pub trait LTTextLine: HasBBox {
    fn word_margin(&self) -> f64;
    fn get_text(&self) -> String;
    fn is_empty(&self) -> bool;
    fn set_bbox(&mut self, bbox: Rect);
}

/// Element in a text line - either a character or annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum TextLineElement {
    Char(LTChar),
    Anno(LTAnno),
}

/// Horizontal text line.
#[derive(Debug, Clone, PartialEq)]
pub struct LTTextLineHorizontal {
    pub(crate) component: LTComponent,
    pub(crate) word_margin: f64,
    pub(crate) x1_tracker: f64,
    pub(crate) elements: Vec<TextLineElement>,
}

impl LTTextLineHorizontal {
    pub fn new(word_margin: f64) -> Self {
        Self {
            component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
            word_margin,
            x1_tracker: INF_F64,
            elements: Vec::new(),
        }
    }

    pub fn set_bbox(&mut self, bbox: Rect) {
        self.component.set_bbox(bbox);
    }

    pub fn bbox(&self) -> Rect {
        self.component.bbox()
    }

    /// Finds neighboring horizontal text lines in the plane.
    pub fn find_neighbors<'a>(
        &self,
        plane: &'a Plane<LTTextLineHorizontal>,
        ratio: f64,
    ) -> Vec<&'a LTTextLineHorizontal> {
        let d = ratio * self.component.height();
        let search_bbox = (
            self.component.x0,
            self.component.y0 - d,
            self.component.x1,
            self.component.y1 + d,
        );
        let objs = plane.find(search_bbox);

        objs.into_iter()
            .filter(|obj| {
                self.is_same_height_as(obj, d)
                    && (self.is_left_aligned_with(obj, d)
                        || self.is_right_aligned_with(obj, d)
                        || self.is_centrally_aligned_with(obj, d))
            })
            .collect()
    }

    fn is_left_aligned_with(&self, other: &LTTextLineHorizontal, tolerance: f64) -> bool {
        (other.component.x0 - self.component.x0).abs() <= tolerance
    }

    fn is_right_aligned_with(&self, other: &LTTextLineHorizontal, tolerance: f64) -> bool {
        (other.component.x1 - self.component.x1).abs() <= tolerance
    }

    fn is_centrally_aligned_with(&self, other: &LTTextLineHorizontal, tolerance: f64) -> bool {
        let self_center = (self.component.x0 + self.component.x1) / 2.0;
        let other_center = (other.component.x0 + other.component.x1) / 2.0;
        (other_center - self_center).abs() <= tolerance
    }

    fn is_same_height_as(&self, other: &LTTextLineHorizontal, tolerance: f64) -> bool {
        (other.component.height() - self.component.height()).abs() <= tolerance
    }

    /// Returns an iterator over elements in this text line.
    pub fn iter(&self) -> impl Iterator<Item = &TextLineElement> {
        self.elements.iter()
    }

    /// Performs analysis on the text line.
    ///
    /// Adds a newline annotation at the end of the text line.
    /// Matches Python layout.py:484-487.
    pub fn analyze(&mut self) {
        self.elements.push(TextLineElement::Anno(LTAnno::new("\n")));
    }
}

impl HasBBox for LTTextLineHorizontal {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

impl Eq for LTTextLineHorizontal {}

impl Hash for LTTextLineHorizontal {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.component.hash(state);
    }
}

impl LTTextLine for LTTextLineHorizontal {
    fn word_margin(&self) -> f64 {
        self.word_margin
    }

    fn get_text(&self) -> String {
        self.elements
            .iter()
            .map(|e| match e {
                TextLineElement::Char(c) => c.get_text().to_string(),
                TextLineElement::Anno(a) => a.get_text().to_string(),
            })
            .collect()
    }

    fn is_empty(&self) -> bool {
        // Note: Python's str.isspace() returns False for empty string,
        // but Rust's all() on empty iterator returns true. Match Python behavior.
        let text = self.get_text();
        self.component.is_empty() || (!text.is_empty() && text.chars().all(|c| c.is_whitespace()))
    }

    fn set_bbox(&mut self, bbox: Rect) {
        self.component.set_bbox(bbox);
    }
}

/// Vertical text line.
#[derive(Debug, Clone, PartialEq)]
pub struct LTTextLineVertical {
    pub(crate) component: LTComponent,
    pub(crate) word_margin: f64,
    pub(crate) y0_tracker: f64,
    pub(crate) elements: Vec<TextLineElement>,
}

impl LTTextLineVertical {
    pub fn new(word_margin: f64) -> Self {
        Self {
            component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
            word_margin,
            y0_tracker: -INF_F64,
            elements: Vec::new(),
        }
    }

    pub fn set_bbox(&mut self, bbox: Rect) {
        self.component.set_bbox(bbox);
    }

    pub fn bbox(&self) -> Rect {
        self.component.bbox()
    }

    /// Finds neighboring vertical text lines in the plane.
    pub fn find_neighbors<'a>(
        &self,
        plane: &'a Plane<LTTextLineVertical>,
        ratio: f64,
    ) -> Vec<&'a LTTextLineVertical> {
        let d = ratio * self.component.width();
        let search_bbox = (
            self.component.x0 - d,
            self.component.y0,
            self.component.x1 + d,
            self.component.y1,
        );
        let objs = plane.find(search_bbox);

        objs.into_iter()
            .filter(|obj| {
                self.is_same_width_as(obj, d)
                    && (self.is_lower_aligned_with(obj, d)
                        || self.is_upper_aligned_with(obj, d)
                        || self.is_centrally_aligned_with(obj, d))
            })
            .collect()
    }

    fn is_lower_aligned_with(&self, other: &LTTextLineVertical, tolerance: f64) -> bool {
        (other.component.y0 - self.component.y0).abs() <= tolerance
    }

    fn is_upper_aligned_with(&self, other: &LTTextLineVertical, tolerance: f64) -> bool {
        (other.component.y1 - self.component.y1).abs() <= tolerance
    }

    fn is_centrally_aligned_with(&self, other: &LTTextLineVertical, tolerance: f64) -> bool {
        let self_center = (self.component.y0 + self.component.y1) / 2.0;
        let other_center = (other.component.y0 + other.component.y1) / 2.0;
        (other_center - self_center).abs() <= tolerance
    }

    fn is_same_width_as(&self, other: &LTTextLineVertical, tolerance: f64) -> bool {
        (other.component.width() - self.component.width()).abs() <= tolerance
    }

    /// Returns an iterator over elements in this text line.
    pub fn iter(&self) -> impl Iterator<Item = &TextLineElement> {
        self.elements.iter()
    }

    /// Performs analysis on the text line.
    ///
    /// Adds a newline annotation at the end of the text line.
    /// Matches Python layout.py:484-487.
    pub fn analyze(&mut self) {
        self.elements.push(TextLineElement::Anno(LTAnno::new("\n")));
    }
}

impl HasBBox for LTTextLineVertical {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

impl Eq for LTTextLineVertical {}

impl Hash for LTTextLineVertical {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.component.hash(state);
    }
}

impl LTTextLine for LTTextLineVertical {
    fn word_margin(&self) -> f64 {
        self.word_margin
    }

    fn get_text(&self) -> String {
        self.elements
            .iter()
            .map(|e| match e {
                TextLineElement::Char(c) => c.get_text().to_string(),
                TextLineElement::Anno(a) => a.get_text().to_string(),
            })
            .collect()
    }

    fn is_empty(&self) -> bool {
        // Note: Python's str.isspace() returns False for empty string,
        // but Rust's all() on empty iterator returns true. Match Python behavior.
        let text = self.get_text();
        self.component.is_empty() || (!text.is_empty() && text.chars().all(|c| c.is_whitespace()))
    }

    fn set_bbox(&mut self, bbox: Rect) {
        self.component.set_bbox(bbox);
    }
}

/// Enum to hold either horizontal or vertical text line.
#[derive(Debug, Clone)]
pub enum TextLineType {
    Horizontal(LTTextLineHorizontal),
    Vertical(LTTextLineVertical),
}

impl TextLineType {
    pub fn is_empty(&self) -> bool {
        match self {
            TextLineType::Horizontal(l) => l.is_empty(),
            TextLineType::Vertical(l) => l.is_empty(),
        }
    }

    pub fn bbox(&self) -> Rect {
        match self {
            TextLineType::Horizontal(l) => l.bbox(),
            TextLineType::Vertical(l) => l.bbox(),
        }
    }

    pub fn set_bbox(&mut self, bbox: Rect) {
        match self {
            TextLineType::Horizontal(l) => l.set_bbox(bbox),
            TextLineType::Vertical(l) => l.set_bbox(bbox),
        }
    }

    /// Performs analysis on the text line.
    ///
    /// Adds a newline annotation at the end of the text line.
    /// Matches Python layout.py:484-487.
    pub fn analyze(&mut self) {
        match self {
            TextLineType::Horizontal(l) => l.analyze(),
            TextLineType::Vertical(l) => l.analyze(),
        }
    }
}

impl HasBBox for TextLineType {
    fn x0(&self) -> f64 {
        match self {
            TextLineType::Horizontal(l) => l.x0(),
            TextLineType::Vertical(l) => l.x0(),
        }
    }
    fn y0(&self) -> f64 {
        match self {
            TextLineType::Horizontal(l) => l.y0(),
            TextLineType::Vertical(l) => l.y0(),
        }
    }
    fn x1(&self) -> f64 {
        match self {
            TextLineType::Horizontal(l) => l.x1(),
            TextLineType::Vertical(l) => l.x1(),
        }
    }
    fn y1(&self) -> f64 {
        match self {
            TextLineType::Horizontal(l) => l.y1(),
            TextLineType::Vertical(l) => l.y1(),
        }
    }
}

impl PartialEq for TextLineType {
    fn eq(&self, other: &Self) -> bool {
        self.bbox() == other.bbox()
    }
}

impl Eq for TextLineType {}

impl Hash for TextLineType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x0().to_bits().hash(state);
        self.y0().to_bits().hash(state);
        self.x1().to_bits().hash(state);
        self.y1().to_bits().hash(state);
    }
}

// ============================================================================
// Text Box Types
// ============================================================================

/// Trait for text box types.
pub trait LTTextBox {
    fn get_text(&self) -> String;
    fn get_writing_mode(&self) -> &'static str;
    fn index(&self) -> i32;
    fn set_index(&mut self, index: i32);
    fn is_empty(&self) -> bool;
}

/// Horizontal text box containing horizontal text lines.
#[derive(Debug, Clone)]
pub struct LTTextBoxHorizontal {
    component: LTComponent,
    lines: Vec<LTTextLineHorizontal>,
    index: i32,
}

impl LTTextBoxHorizontal {
    pub fn new() -> Self {
        Self {
            component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
            lines: Vec::new(),
            index: -1,
        }
    }

    pub fn add(&mut self, line: LTTextLineHorizontal) {
        // Expand bounding box
        let bbox = line.bbox();
        self.component.x0 = self.component.x0.min(bbox.0);
        self.component.y0 = self.component.y0.min(bbox.1);
        self.component.x1 = self.component.x1.max(bbox.2);
        self.component.y1 = self.component.y1.max(bbox.3);
        self.lines.push(line);
    }

    pub fn bbox(&self) -> Rect {
        self.component.bbox()
    }

    /// Returns an iterator over lines in this text box.
    pub fn iter(&self) -> impl Iterator<Item = &LTTextLineHorizontal> {
        self.lines.iter()
    }

    /// Analyze this text box: sort lines by y-position (top to bottom).
    /// Matches Python's LTTextBoxHorizontal.analyze() which sorts by -obj.y1.
    pub fn analyze(&mut self) {
        // Sort lines by descending y1 (top-to-bottom reading order)
        // Note: lines are already analyzed (newlines added) during group_objects()
        self.lines.sort_by(|a, b| {
            let y1_a = a.y1();
            let y1_b = b.y1();
            // Descending order: higher y1 comes first
            y1_b.partial_cmp(&y1_a).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

impl Default for LTTextBoxHorizontal {
    fn default() -> Self {
        Self::new()
    }
}

impl LTTextBox for LTTextBoxHorizontal {
    fn get_text(&self) -> String {
        self.lines.iter().map(|l| l.get_text()).collect()
    }

    fn get_writing_mode(&self) -> &'static str {
        "lr-tb"
    }

    fn index(&self) -> i32 {
        self.index
    }

    fn set_index(&mut self, index: i32) {
        self.index = index;
    }

    fn is_empty(&self) -> bool {
        // Note: Python's str.isspace() returns False for empty string,
        // but Rust's all() on empty iterator returns true. Match Python behavior.
        let text = self.get_text();
        self.component.is_empty() || (!text.is_empty() && text.chars().all(|c| c.is_whitespace()))
    }
}

impl HasBBox for LTTextBoxHorizontal {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

/// Vertical text box containing vertical text lines.
#[derive(Debug, Clone)]
pub struct LTTextBoxVertical {
    component: LTComponent,
    lines: Vec<LTTextLineVertical>,
    index: i32,
}

impl LTTextBoxVertical {
    pub fn new() -> Self {
        Self {
            component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
            lines: Vec::new(),
            index: -1,
        }
    }

    pub fn add(&mut self, line: LTTextLineVertical) {
        // Expand bounding box
        let bbox = line.bbox();
        self.component.x0 = self.component.x0.min(bbox.0);
        self.component.y0 = self.component.y0.min(bbox.1);
        self.component.x1 = self.component.x1.max(bbox.2);
        self.component.y1 = self.component.y1.max(bbox.3);
        self.lines.push(line);
    }

    pub fn bbox(&self) -> Rect {
        self.component.bbox()
    }

    /// Returns an iterator over lines in this text box.
    pub fn iter(&self) -> impl Iterator<Item = &LTTextLineVertical> {
        self.lines.iter()
    }

    /// Analyze this text box: sort lines by x-position (right to left).
    /// Matches Python's LTTextBoxVertical.analyze() which sorts by -obj.x1.
    pub fn analyze(&mut self) {
        // Sort lines by descending x1 (right-to-left reading order for vertical text)
        // Note: lines are already analyzed (newlines added) during group_objects()
        self.lines.sort_by(|a, b| {
            let x1_a = a.x1();
            let x1_b = b.x1();
            // Descending order: higher x1 comes first
            x1_b.partial_cmp(&x1_a).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

impl Default for LTTextBoxVertical {
    fn default() -> Self {
        Self::new()
    }
}

impl LTTextBox for LTTextBoxVertical {
    fn get_text(&self) -> String {
        self.lines.iter().map(|l| l.get_text()).collect()
    }

    fn get_writing_mode(&self) -> &'static str {
        "tb-rl"
    }

    fn index(&self) -> i32 {
        self.index
    }

    fn set_index(&mut self, index: i32) {
        self.index = index;
    }

    fn is_empty(&self) -> bool {
        // Note: Python's str.isspace() returns False for empty string,
        // but Rust's all() on empty iterator returns true. Match Python behavior.
        let text = self.get_text();
        self.component.is_empty() || (!text.is_empty() && text.chars().all(|c| c.is_whitespace()))
    }
}

impl HasBBox for LTTextBoxVertical {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

/// Enum to hold either horizontal or vertical text box.
#[derive(Debug, Clone)]
pub enum TextBoxType {
    Horizontal(LTTextBoxHorizontal),
    Vertical(LTTextBoxVertical),
}

impl TextBoxType {
    pub fn is_empty(&self) -> bool {
        match self {
            TextBoxType::Horizontal(b) => b.is_empty(),
            TextBoxType::Vertical(b) => b.is_empty(),
        }
    }
}

impl HasBBox for TextBoxType {
    fn x0(&self) -> f64 {
        match self {
            TextBoxType::Horizontal(b) => b.x0(),
            TextBoxType::Vertical(b) => b.x0(),
        }
    }
    fn y0(&self) -> f64 {
        match self {
            TextBoxType::Horizontal(b) => b.y0(),
            TextBoxType::Vertical(b) => b.y0(),
        }
    }
    fn x1(&self) -> f64 {
        match self {
            TextBoxType::Horizontal(b) => b.x1(),
            TextBoxType::Vertical(b) => b.x1(),
        }
    }
    fn y1(&self) -> f64 {
        match self {
            TextBoxType::Horizontal(b) => b.y1(),
            TextBoxType::Vertical(b) => b.y1(),
        }
    }
}

// ============================================================================
// Text Groups - hierarchical grouping of text boxes
// ============================================================================

/// Element that can be part of a text group (either a text box or another group).
#[derive(Debug, Clone)]
pub enum TextGroupElement {
    Box(TextBoxType),
    Group(Box<LTTextGroup>),
}

impl TextGroupElement {
    pub fn is_vertical(&self) -> bool {
        match self {
            TextGroupElement::Box(TextBoxType::Vertical(_)) => true,
            TextGroupElement::Group(g) => g.is_vertical(),
            _ => false,
        }
    }
}

impl HasBBox for TextGroupElement {
    fn x0(&self) -> f64 {
        match self {
            TextGroupElement::Box(b) => b.x0(),
            TextGroupElement::Group(g) => g.x0(),
        }
    }
    fn y0(&self) -> f64 {
        match self {
            TextGroupElement::Box(b) => b.y0(),
            TextGroupElement::Group(g) => g.y0(),
        }
    }
    fn x1(&self) -> f64 {
        match self {
            TextGroupElement::Box(b) => b.x1(),
            TextGroupElement::Group(g) => g.x1(),
        }
    }
    fn y1(&self) -> f64 {
        match self {
            TextGroupElement::Box(b) => b.y1(),
            TextGroupElement::Group(g) => g.y1(),
        }
    }
}

impl PartialEq for TextGroupElement {
    fn eq(&self, other: &Self) -> bool {
        self.bbox() == other.bbox()
    }
}

impl Eq for TextGroupElement {}

impl Hash for TextGroupElement {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x0().to_bits().hash(state);
        self.y0().to_bits().hash(state);
        self.x1().to_bits().hash(state);
        self.y1().to_bits().hash(state);
    }
}

/// A hierarchical group of text boxes.
#[derive(Debug, Clone)]
pub struct LTTextGroup {
    component: LTComponent,
    pub(crate) elements: Vec<TextGroupElement>,
    /// Whether this group contains vertical text
    vertical: bool,
}

impl LTTextGroup {
    pub fn new(objs: Vec<TextGroupElement>, vertical: bool) -> Self {
        let mut x0 = INF_F64;
        let mut y0 = INF_F64;
        let mut x1 = -INF_F64;
        let mut y1 = -INF_F64;

        for obj in &objs {
            x0 = x0.min(obj.x0());
            y0 = y0.min(obj.y0());
            x1 = x1.max(obj.x1());
            y1 = y1.max(obj.y1());
        }

        Self {
            component: LTComponent::new((x0, y0, x1, y1)),
            elements: objs,
            vertical,
        }
    }

    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    pub fn elements(&self) -> &[TextGroupElement] {
        &self.elements
    }

    pub fn iter(&self) -> impl Iterator<Item = &TextGroupElement> {
        self.elements.iter()
    }

    /// Recursively collects all textboxes from this group and nested groups.
    pub fn collect_textboxes(&self) -> Vec<TextBoxType> {
        let mut result = Vec::new();
        for elem in &self.elements {
            match elem {
                TextGroupElement::Box(tb) => result.push(tb.clone()),
                TextGroupElement::Group(g) => result.extend(g.collect_textboxes()),
            }
        }
        result
    }

    /// Recursively analyzes and sorts elements within this group and nested groups.
    /// Matches Python's LTContainer.analyze() which calls analyze on ALL children.
    pub fn analyze(&mut self, laparams: &LAParams) {
        // First, recursively analyze ALL child elements (groups AND textboxes)
        // Python's LTContainer.analyze() calls analyze on every child object
        for elem in &mut self.elements {
            match elem {
                TextGroupElement::Group(g) => g.analyze(laparams),
                TextGroupElement::Box(tb) => match tb {
                    TextBoxType::Horizontal(h) => h.analyze(),
                    TextBoxType::Vertical(v) => v.analyze(),
                },
            }
        }

        // Then sort elements at this level
        let boxes_flow = laparams.boxes_flow.unwrap_or(0.5);
        if self.vertical {
            // Vertical text: reorder from top-right to bottom-left
            self.elements.sort_by(|a, b| {
                let key_a = -(1.0 + boxes_flow) * (a.x0() + a.x1()) - (1.0 - boxes_flow) * a.y1();
                let key_b = -(1.0 + boxes_flow) * (b.x0() + b.x1()) - (1.0 - boxes_flow) * b.y1();
                key_a
                    .partial_cmp(&key_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            // Horizontal text: reorder from top-left to bottom-right
            self.elements.sort_by(|a, b| {
                let key_a = (1.0 - boxes_flow) * a.x0() - (1.0 + boxes_flow) * (a.y0() + a.y1());
                let key_b = (1.0 - boxes_flow) * b.x0() - (1.0 + boxes_flow) * (b.y0() + b.y1());
                key_a
                    .partial_cmp(&key_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
}

impl HasBBox for LTTextGroup {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

impl PartialEq for LTTextGroup {
    fn eq(&self, other: &Self) -> bool {
        self.bbox() == other.bbox()
    }
}

impl Eq for LTTextGroup {}

impl Hash for LTTextGroup {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x0().to_bits().hash(state);
        self.y0().to_bits().hash(state);
        self.x1().to_bits().hash(state);
        self.y1().to_bits().hash(state);
    }
}

/// Horizontal left-to-right, top-to-bottom text group.
#[derive(Debug, Clone)]
pub struct LTTextGroupLRTB(LTTextGroup);

impl LTTextGroupLRTB {
    pub fn new(objs: Vec<TextGroupElement>) -> Self {
        Self(LTTextGroup::new(objs, false))
    }

    /// Sorts elements from top-left to bottom-right based on boxes_flow.
    pub fn analyze(&mut self, boxes_flow: f64) {
        self.0.elements.sort_by(|a, b| {
            let key_a = (1.0 - boxes_flow) * a.x0() - (1.0 + boxes_flow) * (a.y0() + a.y1());
            let key_b = (1.0 - boxes_flow) * b.x0() - (1.0 + boxes_flow) * (b.y0() + b.y1());
            key_a
                .partial_cmp(&key_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

impl std::ops::Deref for LTTextGroupLRTB {
    type Target = LTTextGroup;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl HasBBox for LTTextGroupLRTB {
    fn x0(&self) -> f64 {
        self.0.x0()
    }
    fn y0(&self) -> f64 {
        self.0.y0()
    }
    fn x1(&self) -> f64 {
        self.0.x1()
    }
    fn y1(&self) -> f64 {
        self.0.y1()
    }
}

/// Vertical top-to-bottom, right-to-left text group.
#[derive(Debug, Clone)]
pub struct LTTextGroupTBRL(LTTextGroup);

impl LTTextGroupTBRL {
    pub fn new(objs: Vec<TextGroupElement>) -> Self {
        Self(LTTextGroup::new(objs, true))
    }

    /// Sorts elements from top-right to bottom-left based on boxes_flow.
    pub fn analyze(&mut self, boxes_flow: f64) {
        self.0.elements.sort_by(|a, b| {
            let key_a = -(1.0 + boxes_flow) * (a.x0() + a.x1()) - (1.0 - boxes_flow) * a.y1();
            let key_b = -(1.0 + boxes_flow) * (b.x0() + b.x1()) - (1.0 - boxes_flow) * b.y1();
            key_a
                .partial_cmp(&key_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

impl std::ops::Deref for LTTextGroupTBRL {
    type Target = LTTextGroup;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl HasBBox for LTTextGroupTBRL {
    fn x0(&self) -> f64 {
        self.0.x0()
    }
    fn y0(&self) -> f64 {
        self.0.y0()
    }
    fn x1(&self) -> f64 {
        self.0.x1()
    }
    fn y1(&self) -> f64 {
        self.0.y1()
    }
}

// ============================================================================
// Index Assigner - assigns indices to text boxes in reading order
// ============================================================================

/// Assigns sequential indices to text boxes in a text group hierarchy.
pub struct IndexAssigner {
    index: i32,
}

impl IndexAssigner {
    pub fn new() -> Self {
        Self { index: 0 }
    }

    /// Recursively assigns indices to text boxes in the group.
    pub fn run(&mut self, group: &mut LTTextGroup) {
        for elem in &mut group.elements {
            match elem {
                TextGroupElement::Box(TextBoxType::Horizontal(b)) => {
                    b.set_index(self.index);
                    self.index += 1;
                }
                TextGroupElement::Box(TextBoxType::Vertical(b)) => {
                    b.set_index(self.index);
                    self.index += 1;
                }
                TextGroupElement::Group(g) => {
                    self.run(g);
                }
            }
        }
    }
}

impl Default for IndexAssigner {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// LTItem - enum to represent any layout object
// ============================================================================

/// Represents any item that can appear in a layout container.
#[derive(Debug, Clone)]
pub enum LTItem {
    Char(LTChar),
    Anno(LTAnno),
    Curve(LTCurve),
    Line(LTLine),
    Rect(LTRect),
    Image(LTImage),
    TextLine(TextLineType),
    TextBox(TextBoxType),
    Figure(Box<LTFigure>),
    Page(Box<LTPage>),
}

impl LTItem {
    pub fn is_char(&self) -> bool {
        matches!(self, LTItem::Char(_))
    }
}

impl HasBBox for LTItem {
    fn x0(&self) -> f64 {
        match self {
            LTItem::Char(c) => c.x0(),
            LTItem::Anno(_) => 0.0,
            LTItem::Curve(c) => c.x0(),
            LTItem::Line(l) => l.x0(),
            LTItem::Rect(r) => r.x0(),
            LTItem::Image(i) => i.x0(),
            LTItem::TextLine(l) => l.x0(),
            LTItem::TextBox(b) => b.x0(),
            LTItem::Figure(f) => f.x0(),
            LTItem::Page(p) => p.x0(),
        }
    }
    fn y0(&self) -> f64 {
        match self {
            LTItem::Char(c) => c.y0(),
            LTItem::Anno(_) => 0.0,
            LTItem::Curve(c) => c.y0(),
            LTItem::Line(l) => l.y0(),
            LTItem::Rect(r) => r.y0(),
            LTItem::Image(i) => i.y0(),
            LTItem::TextLine(l) => l.y0(),
            LTItem::TextBox(b) => b.y0(),
            LTItem::Figure(f) => f.y0(),
            LTItem::Page(p) => p.y0(),
        }
    }
    fn x1(&self) -> f64 {
        match self {
            LTItem::Char(c) => c.x1(),
            LTItem::Anno(_) => 0.0,
            LTItem::Curve(c) => c.x1(),
            LTItem::Line(l) => l.x1(),
            LTItem::Rect(r) => r.x1(),
            LTItem::Image(i) => i.x1(),
            LTItem::TextLine(l) => l.x1(),
            LTItem::TextBox(b) => b.x1(),
            LTItem::Figure(f) => f.x1(),
            LTItem::Page(p) => p.x1(),
        }
    }
    fn y1(&self) -> f64 {
        match self {
            LTItem::Char(c) => c.y1(),
            LTItem::Anno(_) => 0.0,
            LTItem::Curve(c) => c.y1(),
            LTItem::Line(l) => l.y1(),
            LTItem::Rect(r) => r.y1(),
            LTItem::Image(i) => i.y1(),
            LTItem::TextLine(l) => l.y1(),
            LTItem::TextBox(b) => b.y1(),
            LTItem::Figure(f) => f.y1(),
            LTItem::Page(p) => p.y1(),
        }
    }
}

// ============================================================================
// LTLayoutContainer - Container that performs layout analysis
// ============================================================================

/// Layout container that performs layout analysis on contained objects.
#[derive(Debug, Clone)]
pub struct LTLayoutContainer {
    pub(crate) component: LTComponent,
    /// Contained layout items
    pub(crate) items: Vec<LTItem>,
    /// Text groups after analysis (if boxes_flow is enabled)
    pub groups: Option<Vec<LTTextGroup>>,
}

impl LTLayoutContainer {
    pub fn new(bbox: Rect) -> Self {
        Self {
            component: LTComponent::new(bbox),
            items: Vec::new(),
            groups: None,
        }
    }

    pub fn bbox(&self) -> Rect {
        self.component.bbox()
    }

    /// Adds an item to the container.
    pub fn add(&mut self, item: LTItem) {
        self.items.push(item);
    }

    /// Returns an iterator over contained items.
    pub fn iter(&self) -> impl Iterator<Item = &LTItem> {
        self.items.iter()
    }
}

impl HasBBox for LTLayoutContainer {
    fn x0(&self) -> f64 {
        self.component.x0
    }
    fn y0(&self) -> f64 {
        self.component.y0
    }
    fn x1(&self) -> f64 {
        self.component.x1
    }
    fn y1(&self) -> f64 {
        self.component.y1
    }
}

// ============================================================================
// LTFigure - represents an area used by PDF Form objects
// ============================================================================

/// Represents an area used by PDF Form objects.
///
/// PDF Forms can be used to present figures or pictures by embedding yet
/// another PDF document within a page. Note that LTFigure objects can appear
/// recursively.
#[derive(Debug, Clone)]
pub struct LTFigure {
    pub(crate) container: LTLayoutContainer,
    /// Name/identifier of the figure
    pub name: String,
    /// Transformation matrix
    pub matrix: Matrix,
}

impl LTFigure {
    pub fn new(name: &str, bbox: Rect, matrix: Matrix) -> Self {
        let (x, y, w, h) = bbox;
        let rect = (x, y, x + w, y + h);
        let transformed_bbox = apply_matrix_rect(matrix, rect);
        Self {
            container: LTLayoutContainer::new(transformed_bbox),
            name: name.to_string(),
            matrix,
        }
    }

    /// Adds an item to the figure.
    pub fn add(&mut self, item: LTItem) {
        self.container.add(item);
    }

    /// Returns an iterator over contained items.
    pub fn iter(&self) -> impl Iterator<Item = &LTItem> {
        self.container.iter()
    }
}

impl HasBBox for LTFigure {
    fn x0(&self) -> f64 {
        self.container.x0()
    }
    fn y0(&self) -> f64 {
        self.container.y0()
    }
    fn x1(&self) -> f64 {
        self.container.x1()
    }
    fn y1(&self) -> f64 {
        self.container.y1()
    }
}

// ============================================================================
// LTPage - represents an entire page
// ============================================================================

/// Represents an entire page.
///
/// Like any other LTLayoutContainer, an LTPage can be iterated to obtain child
/// objects like LTTextBox, LTFigure, LTImage, LTRect, LTCurve and LTLine.
#[derive(Debug, Clone)]
pub struct LTPage {
    pub(crate) container: LTLayoutContainer,
    /// Page identifier (usually 1-based page number)
    pub pageid: i32,
    /// Page rotation in degrees
    pub rotate: f64,
}

impl LTPage {
    pub fn new(pageid: i32, bbox: Rect, rotate: f64) -> Self {
        Self {
            container: LTLayoutContainer::new(bbox),
            pageid,
            rotate,
        }
    }

    pub fn bbox(&self) -> Rect {
        self.container.bbox()
    }

    /// Adds an item to the page.
    pub fn add(&mut self, item: LTItem) {
        self.container.add(item);
    }

    /// Returns an iterator over contained items.
    pub fn iter(&self) -> impl Iterator<Item = &LTItem> {
        self.container.iter()
    }

    /// Returns the text groups after analysis (if boxes_flow was enabled).
    pub fn groups(&self) -> Option<&Vec<LTTextGroup>> {
        self.container.groups.as_ref()
    }
}

impl HasBBox for LTPage {
    fn x0(&self) -> f64 {
        self.container.x0()
    }
    fn y0(&self) -> f64 {
        self.container.y0()
    }
    fn x1(&self) -> f64 {
        self.container.x1()
    }
    fn y1(&self) -> f64 {
        self.container.y1()
    }
}
