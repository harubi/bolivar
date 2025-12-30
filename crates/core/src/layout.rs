//! Layout Analysis Module - port of pdfminer.six layout.py
//!
//! Provides layout analysis types for PDF text extraction including:
//! - LAParams: Layout analysis parameters
//! - LTComponent: Base type for objects with bounding boxes
//! - LTAnno: Virtual characters (spaces, newlines) inserted during analysis
//! - LTChar: Actual characters with bounding boxes
//! - LTTextLine: A line of text (horizontal or vertical)
//! - LTTextBox: A group of text lines
//! - LTTextGroup: Hierarchical grouping of text boxes
//! - LTLayoutContainer: Container that performs layout analysis
//! - LTCurve, LTLine, LTRect: Graphical elements
//! - LTImage: Image container
//! - LTFigure: Figure container (embedded PDF forms)
//! - LTPage: Page container

use std::collections::BinaryHeap;
use std::hash::Hash;

use crate::utils::{
    HasBBox, INF_F64, Matrix, Plane, Point, Rect, apply_matrix_rect, fsplit, get_bound, uniq,
};

/// Parameters for layout analysis.
///
/// Controls how characters are grouped into lines, words, and text boxes.
#[derive(Debug, Clone, PartialEq)]
pub struct LAParams {
    /// If two characters have more overlap than this they are considered to be
    /// on the same line. Specified relative to the minimum height of both characters.
    pub line_overlap: f64,

    /// If two characters are closer together than this margin they are considered
    /// part of the same line. Specified relative to the width of the character.
    pub char_margin: f64,

    /// If two lines are close together they are considered to be part of the
    /// same paragraph. Specified relative to the height of a line.
    pub line_margin: f64,

    /// If two characters on the same line are further apart than this margin then
    /// they are considered to be two separate words. Specified relative to the
    /// width of the character.
    pub word_margin: f64,

    /// Specifies how much horizontal and vertical position of text matters when
    /// determining order. Range: -1.0 (only horizontal) to +1.0 (only vertical).
    /// None disables advanced layout analysis.
    pub boxes_flow: Option<f64>,

    /// If vertical text should be considered during layout analysis.
    pub detect_vertical: bool,

    /// If layout analysis should be performed on text in figures.
    pub all_texts: bool,
}

impl Default for LAParams {
    fn default() -> Self {
        Self {
            line_overlap: 0.5,
            char_margin: 2.0,
            line_margin: 0.5,
            word_margin: 0.1,
            boxes_flow: Some(0.5),
            detect_vertical: false,
            all_texts: false,
        }
    }
}

impl LAParams {
    /// Creates new layout parameters with the specified values.
    ///
    /// # Panics
    /// Panics if boxes_flow is Some and not in range [-1.0, 1.0].
    pub fn new(
        line_overlap: f64,
        char_margin: f64,
        line_margin: f64,
        word_margin: f64,
        boxes_flow: Option<f64>,
        detect_vertical: bool,
        all_texts: bool,
    ) -> Self {
        if let Some(bf) = boxes_flow {
            assert!(
                (-1.0..=1.0).contains(&bf),
                "boxes_flow should be None, or a number between -1 and +1"
            );
        }

        Self {
            line_overlap,
            char_margin,
            line_margin,
            word_margin,
            boxes_flow,
            detect_vertical,
            all_texts,
        }
    }
}

/// Base component with a bounding box.
#[derive(Debug, Clone, PartialEq)]
pub struct LTComponent {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
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

/// Actual character in text with bounding box.
#[derive(Debug, Clone, PartialEq)]
pub struct LTChar {
    component: LTComponent,
    text: String,
    fontname: String,
    size: f64,
    upright: bool,
    adv: f64,
    /// Marked Content ID for tagged PDF accessibility
    mcid: Option<i32>,
    /// Marked Content tag (e.g., "P", "Span", "H1") for tagged PDF
    tag: Option<String>,
}

impl LTChar {
    pub fn new(bbox: Rect, text: &str, fontname: &str, size: f64, upright: bool, adv: f64) -> Self {
        Self {
            component: LTComponent::new(bbox),
            text: text.to_string(),
            fontname: fontname.to_string(),
            size,
            upright,
            adv,
            mcid: None,
            tag: None,
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
        Self {
            component: LTComponent::new(bbox),
            text: text.to_string(),
            fontname: fontname.to_string(),
            size,
            upright,
            adv,
            mcid,
            tag: None,
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
        Self {
            component: LTComponent::new(bbox),
            text: text.to_string(),
            fontname: fontname.to_string(),
            size,
            upright,
            adv,
            mcid,
            tag,
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

    pub fn mcid(&self) -> Option<i32> {
        self.mcid
    }

    pub fn tag(&self) -> Option<String> {
        self.tag.clone()
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

/// Optional color type for stroking/non-stroking colors.
pub type Color = Option<Vec<f64>>;

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

/// Trait for text line types.
pub trait LTTextLine: HasBBox {
    fn word_margin(&self) -> f64;
    fn get_text(&self) -> String;
    fn is_empty(&self) -> bool;
    fn set_bbox(&mut self, bbox: Rect);
}

/// Horizontal text line.
#[derive(Debug, Clone, PartialEq)]
pub struct LTTextLineHorizontal {
    component: LTComponent,
    word_margin: f64,
    x1_tracker: f64,
    elements: Vec<TextLineElement>,
}

/// Element in a text line - either a character or annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum TextLineElement {
    Char(LTChar),
    Anno(LTAnno),
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
    component: LTComponent,
    word_margin: f64,
    y0_tracker: f64,
    elements: Vec<TextLineElement>,
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
    elements: Vec<TextGroupElement>,
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
// Layout Item - enum to represent any layout object
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

/// Layout container that performs layout analysis on contained objects.
#[derive(Debug, Clone)]
pub struct LTLayoutContainer {
    component: LTComponent,
    /// Contained layout items
    items: Vec<LTItem>,
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

    /// Groups character objects into text lines.
    ///
    /// This is the core character-to-line grouping algorithm from pdfminer.
    /// It groups LTChar objects based on horizontal/vertical alignment and proximity.
    ///
    /// # Algorithm (Python lines 702-777)
    /// - For each pair of consecutive characters, check if they are:
    ///   - horizontally aligned (halign): on same line, close enough horizontally
    ///   - vertically aligned (valign): on same column, close enough vertically
    /// - Group characters into horizontal or vertical text lines accordingly
    pub fn group_objects(&self, laparams: &LAParams, objs: &[LTChar]) -> Vec<TextLineType> {
        let mut result = Vec::new();
        if objs.is_empty() {
            return result;
        }

        let mut obj_iter = objs.iter().peekable();
        let mut current_line: Option<TextLineType> = None;

        // Get first object
        let mut obj0 = match obj_iter.next() {
            Some(o) => o,
            None => return result,
        };

        for obj1 in obj_iter {
            // Check horizontal alignment:
            //   +------+ - - -
            //   | obj0 | - - +------+   -
            //   |      |     | obj1 |   | (line_overlap)
            //   +------+ - - |      |   -
            //          - - - +------+
            //          |<--->|
            //        (char_margin)
            let halign = obj0.is_voverlap(obj1)
                && obj0.height().min(obj1.height()) * laparams.line_overlap < obj0.voverlap(obj1)
                && obj0.hdistance(obj1) < obj0.width().max(obj1.width()) * laparams.char_margin;

            // Check vertical alignment:
            //   +------+
            //   | obj0 |
            //   |      |
            //   +------+ - - -
            //     |    |     | (char_margin)
            //     +------+ - -
            //     | obj1 |
            //     |      |
            //     +------+
            //     |<-->|
            //   (line_overlap)
            let valign = laparams.detect_vertical
                && obj0.is_hoverlap(obj1)
                && obj0.width().min(obj1.width()) * laparams.line_overlap < obj0.hoverlap(obj1)
                && obj0.vdistance(obj1) < obj0.height().max(obj1.height()) * laparams.char_margin;

            match &mut current_line {
                Some(TextLineType::Horizontal(line)) if halign => {
                    // Continue horizontal line
                    Self::add_char_to_horizontal_line(line, obj1.clone(), laparams.word_margin);
                }
                Some(TextLineType::Vertical(line)) if valign => {
                    // Continue vertical line
                    Self::add_char_to_vertical_line(line, obj1.clone(), laparams.word_margin);
                }
                Some(line) => {
                    // End current line (obj0 was already added to it)
                    line.analyze();
                    result.push(line.clone());
                    current_line = None;
                    // Don't create single-char line from obj0 - it's already in current_line
                    // Just continue to next iteration where obj1 becomes obj0
                }
                None => {
                    if valign && !halign {
                        // Start new vertical line
                        let mut line = LTTextLineVertical::new(laparams.word_margin);
                        Self::add_char_to_vertical_line(
                            &mut line,
                            obj0.clone(),
                            laparams.word_margin,
                        );
                        Self::add_char_to_vertical_line(
                            &mut line,
                            obj1.clone(),
                            laparams.word_margin,
                        );
                        current_line = Some(TextLineType::Vertical(line));
                    } else if halign && !valign {
                        // Start new horizontal line
                        let mut line = LTTextLineHorizontal::new(laparams.word_margin);
                        Self::add_char_to_horizontal_line(
                            &mut line,
                            obj0.clone(),
                            laparams.word_margin,
                        );
                        Self::add_char_to_horizontal_line(
                            &mut line,
                            obj1.clone(),
                            laparams.word_margin,
                        );
                        current_line = Some(TextLineType::Horizontal(line));
                    } else {
                        // Neither aligned - output single-char line
                        let mut line = LTTextLineHorizontal::new(laparams.word_margin);
                        Self::add_char_to_horizontal_line(
                            &mut line,
                            obj0.clone(),
                            laparams.word_margin,
                        );
                        line.analyze();
                        result.push(TextLineType::Horizontal(line));
                    }
                }
            }

            obj0 = obj1;
        }

        // Handle remaining line or last character
        match current_line {
            Some(mut line) => {
                line.analyze();
                result.push(line);
            }
            None => {
                // Last character wasn't part of a line
                let mut line = LTTextLineHorizontal::new(laparams.word_margin);
                Self::add_char_to_horizontal_line(&mut line, obj0.clone(), laparams.word_margin);
                line.analyze();
                result.push(TextLineType::Horizontal(line));
            }
        }

        result
    }

    /// Helper to add a character to a horizontal line, inserting word spaces as needed.
    fn add_char_to_horizontal_line(line: &mut LTTextLineHorizontal, ch: LTChar, word_margin: f64) {
        let margin = word_margin * ch.width().max(ch.height());
        if line.x1_tracker < ch.x0() - margin && line.x1_tracker != INF_F64 {
            line.elements.push(TextLineElement::Anno(LTAnno::new(" ")));
        }
        line.x1_tracker = ch.x1();

        // Expand bounding box
        line.component.x0 = line.component.x0.min(ch.x0());
        line.component.y0 = line.component.y0.min(ch.y0());
        line.component.x1 = line.component.x1.max(ch.x1());
        line.component.y1 = line.component.y1.max(ch.y1());

        line.elements.push(TextLineElement::Char(ch));
    }

    /// Helper to add a character to a vertical line, inserting word spaces as needed.
    fn add_char_to_vertical_line(line: &mut LTTextLineVertical, ch: LTChar, word_margin: f64) {
        let margin = word_margin * ch.width().max(ch.height());
        if ch.y1() + margin < line.y0_tracker && line.y0_tracker != -INF_F64 {
            line.elements.push(TextLineElement::Anno(LTAnno::new(" ")));
        }
        line.y0_tracker = ch.y0();

        // Expand bounding box
        line.component.x0 = line.component.x0.min(ch.x0());
        line.component.y0 = line.component.y0.min(ch.y0());
        line.component.x1 = line.component.x1.max(ch.x1());
        line.component.y1 = line.component.y1.max(ch.y1());

        line.elements.push(TextLineElement::Char(ch));
    }

    /// Groups text lines into text boxes based on neighbor relationships.
    pub fn group_textlines(
        &self,
        laparams: &LAParams,
        lines: Vec<TextLineType>,
    ) -> Vec<TextBoxType> {
        if lines.is_empty() {
            return Vec::new();
        }

        // Compute bounding box that covers all lines (may be outside container bbox)
        let mut min_x0 = INF_F64;
        let mut min_y0 = INF_F64;
        let mut max_x1 = -INF_F64;
        let mut max_y1 = -INF_F64;

        for line in &lines {
            min_x0 = min_x0.min(line.x0());
            min_y0 = min_y0.min(line.y0());
            max_x1 = max_x1.max(line.x1());
            max_y1 = max_y1.max(line.y1());
        }

        // Create plane with expanded bbox
        let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
        let mut plane: Plane<TextLineType> = Plane::new(plane_bbox, 1);

        // Add lines to plane (keep original lines with elements intact)
        for line in &lines {
            plane.add(line.clone());
        }
        let line_types = lines;

        // Group lines into boxes - MUST match Python's exact logic:
        // Python: boxes: Dict[LTTextLine, LTTextBox] = {}
        // Each line maps to its current box. When merging, ALL lines from
        // existing boxes are added to the new box.

        // line_to_box_id: maps line_index -> box_id (which box contains this line)
        // box_contents: maps box_id -> Vec<line_index> (lines in each box)
        let mut line_to_box_id: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let mut box_contents: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        let mut next_box_id: usize = 0;

        for (i, line) in line_types.iter().enumerate() {
            // Use different search strategy for horizontal vs vertical text
            let (d, search_bbox) = match line {
                TextLineType::Horizontal(_) => {
                    let d = laparams.line_margin * line.height();
                    (d, (line.x0(), line.y0() - d, line.x1(), line.y1() + d))
                }
                TextLineType::Vertical(_) => {
                    let d = laparams.line_margin * line.width();
                    (d, (line.x0() - d, line.y0(), line.x1() + d, line.y1()))
                }
            };
            // Use find_with_indices to get (seq_index, neighbor) pairs
            // Since we added lines to plane in order, seq_index == line_types index
            let neighbors = plane.find_with_indices(search_bbox);

            // Start with current line
            let mut members: Vec<usize> = vec![i];

            for (j, neighbor) in neighbors {
                // Python uses NON-STRICT comparison (<= tolerance)
                // See layout.py:543-560 - _is_left_aligned_with, _is_same_height_as, etc.
                let is_aligned = match (line, neighbor) {
                    (TextLineType::Horizontal(l1), TextLineType::Horizontal(l2)) => {
                        let tolerance = d;
                        let height_diff = (l2.height() - l1.height()).abs();
                        let same_height = height_diff <= tolerance; // Python: <=
                        let left_diff = (l2.x0() - l1.x0()).abs();
                        let left_aligned = left_diff <= tolerance; // Python: <=
                        let right_diff = (l2.x1() - l1.x1()).abs();
                        let right_aligned = right_diff <= tolerance; // Python: <=
                        let center1 = (l1.x0() + l1.x1()) / 2.0;
                        let center2 = (l2.x0() + l2.x1()) / 2.0;
                        let center_diff = (center2 - center1).abs();
                        let centrally_aligned = center_diff <= tolerance; // Python: <=
                        same_height && (left_aligned || right_aligned || centrally_aligned)
                    }
                    (TextLineType::Vertical(l1), TextLineType::Vertical(l2)) => {
                        let tolerance = d;
                        let same_width = (l2.width() - l1.width()).abs() <= tolerance; // Python: <=
                        let lower_aligned = (l2.y0() - l1.y0()).abs() <= tolerance; // Python: <=
                        let upper_aligned = (l2.y1() - l1.y1()).abs() <= tolerance; // Python: <=
                        let center1 = (l1.y0() + l1.y1()) / 2.0;
                        let center2 = (l2.y0() + l2.y1()) / 2.0;
                        let centrally_aligned = (center2 - center1).abs() <= tolerance; // Python: <=
                        same_width && (lower_aligned || upper_aligned || centrally_aligned)
                    }
                    _ => false,
                };

                if is_aligned {
                    // j is the direct index from plane, no need to search by bbox!
                    // Add neighbor to members
                    members.push(j);
                    // CRITICAL: If neighbor is already in a box, merge ALL lines from that box
                    // This matches Python's: members.extend(boxes.pop(obj1))
                    if let Some(&existing_box_id) = line_to_box_id.get(&j) {
                        if let Some(existing_members) = box_contents.remove(&existing_box_id) {
                            members.extend(existing_members);
                        }
                    }
                }
            }

            // Create new box with all members (matching Python: box = LTTextBox(); for obj in uniq(members): box.add(obj); boxes[obj] = box)
            let box_id = next_box_id;
            next_box_id += 1;

            let unique_members: Vec<usize> = uniq(members);
            for &m in &unique_members {
                line_to_box_id.insert(m, box_id);
            }
            box_contents.insert(box_id, unique_members);
        }

        // CRITICAL: Python iterates through original 'lines' in order and yields boxes
        // as their first line is encountered. We must do the same - NOT iterate the HashMap!
        let mut result: Vec<TextBoxType> = Vec::new();
        let mut done: std::collections::HashSet<usize> = std::collections::HashSet::new();

        // Iterate through lines in ORIGINAL ORDER (like Python's "for line in lines:")
        for (i, _line) in line_types.iter().enumerate() {
            // Look up which box this line belongs to
            let box_id = match line_to_box_id.get(&i) {
                Some(&id) => id,
                None => continue,
            };

            // Skip if we've already processed this box
            if done.contains(&box_id) {
                continue;
            }
            done.insert(box_id);

            // Get all members of this box
            let member_indices = match box_contents.get(&box_id) {
                Some(members) => members,
                None => continue,
            };

            let unique_members: Vec<usize> = uniq(member_indices.clone());

            // Determine box type from first line in group
            if unique_members.is_empty() {
                continue;
            }
            let first_line = &line_types[unique_members[0]];
            let is_vertical = matches!(first_line, TextLineType::Vertical(_));

            if is_vertical {
                let mut textbox = LTTextBoxVertical::new();
                for idx in unique_members {
                    if let TextLineType::Vertical(line) = &line_types[idx] {
                        textbox.add(line.clone());
                    }
                }
                if !textbox.is_empty() {
                    result.push(TextBoxType::Vertical(textbox));
                }
            } else {
                let mut textbox = LTTextBoxHorizontal::new();
                for idx in unique_members {
                    if let TextLineType::Horizontal(line) = &line_types[idx] {
                        textbox.add(line.clone());
                    }
                }
                if !textbox.is_empty() {
                    result.push(TextBoxType::Horizontal(textbox));
                }
            }
        }

        result
    }

    /// Groups text boxes hierarchically based on spatial proximity.
    ///
    /// Uses a distance function to find the closest pairs of text boxes
    /// and merges them into groups. Uses a heap for efficient access.
    pub fn group_textboxes(&self, _laparams: &LAParams, boxes: &[TextBoxType]) -> Vec<LTTextGroup> {
        if boxes.is_empty() {
            return Vec::new();
        }

        // Distance function: area of bounding rectangle minus areas of both boxes
        fn dist(obj1: &TextGroupElement, obj2: &TextGroupElement) -> f64 {
            let x0 = obj1.x0().min(obj2.x0());
            let y0 = obj1.y0().min(obj2.y0());
            let x1 = obj1.x1().max(obj2.x1());
            let y1 = obj1.y1().max(obj2.y1());
            (x1 - x0) * (y1 - y0) - obj1.width() * obj1.height() - obj2.width() * obj2.height()
        }

        // Heap entry: (distance, skip_isany, id1, id2, elements)
        // Python uses tuple: (skip_isany, d, id1, id2, obj1, obj2)
        #[derive(Clone)]
        struct HeapEntry {
            dist: f64, // Actual distance (not negated)
            skip_isany: bool,
            id1: usize,
            id2: usize,
            elem1: TextGroupElement,
            elem2: TextGroupElement,
        }

        impl PartialEq for HeapEntry {
            fn eq(&self, other: &Self) -> bool {
                self.dist == other.dist
            }
        }
        impl Eq for HeapEntry {}
        impl PartialOrd for HeapEntry {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for HeapEntry {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                // BinaryHeap is max-heap, so larger values pop first.
                // Python's heapq is min-heap with tuple: (skip_isany, d, id1, id2)
                //
                // Python priorities (min-heap, smaller pops first):
                // 1. skip_isany: False < True → False pops first
                // 2. dist: smaller distance pops first
                // 3. id1: smaller id pops first
                // 4. id2: smaller id pops first
                //
                // For max-heap, we reverse all comparisons:
                // 1. skip_isany: False > True (so False pops first)
                // 2. dist: smaller dist needs to be "greater" → compare other.dist to self.dist
                // 3. id1: smaller id needs to be "greater" → compare other.id1 to self.id1
                // 4. id2: smaller id needs to be "greater" → compare other.id2 to self.id2

                // First compare skip_isany (False should pop first)
                match other.skip_isany.cmp(&self.skip_isany) {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord, // True > False in reverse, so False pops first
                }

                // Then compare distance (smaller pops first)
                // Use total_cmp for consistent f64 ordering
                match other.dist.total_cmp(&self.dist) {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord, // larger other.dist = self is "greater", pops first
                }

                // Tie-break by id1 (smaller pops first)
                match other.id1.cmp(&self.id1) {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }

                // Tie-break by id2 (smaller pops first)
                other.id2.cmp(&self.id2)
            }
        }

        // Build plane with all boxes
        let mut min_x0 = INF_F64;
        let mut min_y0 = INF_F64;
        let mut max_x1 = -INF_F64;
        let mut max_y1 = -INF_F64;
        for b in boxes {
            min_x0 = min_x0.min(b.x0());
            min_y0 = min_y0.min(b.y0());
            max_x1 = max_x1.max(b.x1());
            max_y1 = max_y1.max(b.y1());
        }
        let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
        let mut plane: Plane<TextGroupElement> = Plane::new(plane_bbox, 1);

        // Convert boxes to elements and compute initial distances
        let mut elements: Vec<TextGroupElement> = boxes
            .iter()
            .map(|b| TextGroupElement::Box(b.clone()))
            .collect();

        // Add elements to plane with extend() to build RTree for fast neighbor queries
        plane.extend(elements.iter().cloned());
        // Now plane.seq[i] == elements[i], so seq_index == element_id

        let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();
        let mut next_id = elements.len();

        // Initialize heap with k-nearest neighbor distances
        // K=20 captures ~96% of merges based on empirical measurement
        // (see examples/measure_neighbors.rs for analysis)
        const K_NEIGHBORS: usize = 20;
        for (i, elem) in elements.iter().enumerate() {
            for (j, neighbor) in plane.neighbors(elem.bbox(), K_NEIGHBORS) {
                if j > i {
                    // Avoid duplicate pairs (i,j) and (j,i)
                    let d = dist(elem, neighbor);
                    heap.push(HeapEntry {
                        dist: d,
                        skip_isany: false,
                        id1: i,
                        id2: j,
                        elem1: elem.clone(),
                        elem2: neighbor.clone(),
                    });
                }
            }
        }

        let mut done: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut result_elements: Vec<TextGroupElement> = Vec::new();

        while let Some(entry) = heap.pop() {
            // Skip if either object is already merged
            // With proper id1/id2 from iter_with_indices, this check is sufficient
            if done.contains(&entry.id1) || done.contains(&entry.id2) {
                continue;
            }

            // Check if there are any other objects between these two
            // Use find_with_indices and compare by index (not bbox) for correctness
            if !entry.skip_isany {
                let x0 = entry.elem1.x0().min(entry.elem2.x0());
                let y0 = entry.elem1.y0().min(entry.elem2.y0());
                let x1 = entry.elem1.x1().max(entry.elem2.x1());
                let y1 = entry.elem1.y1().max(entry.elem2.y1());
                let between: Vec<_> = plane
                    .find_with_indices((x0, y0, x1, y1))
                    .into_iter()
                    .filter(|(idx, _)| !done.contains(idx))
                    .collect();

                // Check if there's any element between that isn't id1 or id2
                let has_other = between
                    .iter()
                    .any(|(idx, _)| *idx != entry.id1 && *idx != entry.id2);

                if has_other {
                    // Re-add with skip_isany=true
                    let mut new_entry = entry.clone();
                    new_entry.skip_isany = true;
                    heap.push(new_entry);
                    continue;
                }
            }

            // Merge the two elements into a group
            done.insert(entry.id1);
            // Only insert id2 if it's a real ID, not the usize::MAX placeholder
            if entry.id2 != usize::MAX {
                done.insert(entry.id2);
            }

            // Tombstone pattern: elements are tracked in `done` set instead of
            // being removed from plane. Query results are filtered against `done`.

            let is_vertical = entry.elem1.is_vertical() || entry.elem2.is_vertical();
            let group =
                LTTextGroup::new(vec![entry.elem1.clone(), entry.elem2.clone()], is_vertical);
            let group_elem = TextGroupElement::Group(Box::new(group));

            // Add distances to all remaining elements using iter_with_indices
            // The seq_index from Plane is the element ID for proper tie-breaking
            // Filter against done (tombstone pattern)
            for (other_id, other) in plane
                .iter_with_indices()
                .filter(|(id, _)| !done.contains(id))
            {
                let d = dist(&group_elem, other);
                heap.push(HeapEntry {
                    dist: d,
                    skip_isany: false,
                    id1: next_id,
                    id2: other_id,
                    elem1: group_elem.clone(),
                    elem2: other.clone(),
                });
            }

            // Add the new group to plane - next_id will be its seq_index
            plane.add(group_elem.clone());
            elements.push(group_elem);
            next_id += 1;
        }

        // Collect remaining elements as groups (filter against done - tombstone pattern)
        for (id, elem) in plane.iter_with_indices() {
            if !done.contains(&id) {
                result_elements.push(elem.clone());
            }
        }

        // Convert to LTTextGroup
        result_elements
            .into_iter()
            .map(|e| match e {
                TextGroupElement::Group(g) => *g,
                TextGroupElement::Box(b) => LTTextGroup::new(vec![TextGroupElement::Box(b)], false),
            })
            .collect()
    }

    /// Performs layout analysis on the container's items.
    ///
    /// This is the main entry point for layout analysis. It:
    /// 1. Separates text characters from other objects
    /// 2. Groups characters into text lines
    /// 3. Groups text lines into text boxes
    /// 4. Optionally groups text boxes hierarchically (if boxes_flow is set)
    /// 5. Assigns reading order indices to text boxes
    pub fn analyze(&mut self, laparams: &LAParams) {
        // Separate text objects from other objects
        let (textobjs, otherobjs): (Vec<_>, Vec<_>) =
            self.items.iter().cloned().partition(|obj| obj.is_char());

        if textobjs.is_empty() {
            return;
        }

        // Extract LTChar objects
        let chars: Vec<LTChar> = textobjs
            .into_iter()
            .filter_map(|item| match item {
                LTItem::Char(c) => Some(c),
                _ => None,
            })
            .collect();

        // Group characters into text lines
        let textlines = self.group_objects(laparams, &chars);

        // Separate empty lines
        let (empties, textlines): (Vec<_>, Vec<_>) =
            fsplit(|l: &TextLineType| l.is_empty(), textlines);

        // Group lines into text boxes
        let mut textboxes = self.group_textlines(laparams, textlines);

        if laparams.boxes_flow.is_none() {
            // Analyze each textbox (sorts internal lines)
            // Python: for textbox in textboxes: textbox.analyze(laparams)
            for tb in &mut textboxes {
                match tb {
                    TextBoxType::Horizontal(h) => h.analyze(),
                    TextBoxType::Vertical(v) => v.analyze(),
                }
            }

            // Simple sorting without hierarchical grouping
            textboxes.sort_by(|a, b| {
                let key_a = match a {
                    TextBoxType::Vertical(v) => {
                        (0, (-v.x1() * 1000.0) as i64, (-v.y0() * 1000.0) as i64)
                    }
                    TextBoxType::Horizontal(h) => {
                        (1, (-h.y0() * 1000.0) as i64, (h.x0() * 1000.0) as i64)
                    }
                };
                let key_b = match b {
                    TextBoxType::Vertical(v) => {
                        (0, (-v.x1() * 1000.0) as i64, (-v.y0() * 1000.0) as i64)
                    }
                    TextBoxType::Horizontal(h) => {
                        (1, (-h.y0() * 1000.0) as i64, (h.x0() * 1000.0) as i64)
                    }
                };
                key_a.cmp(&key_b)
            });
        } else {
            // Hierarchical grouping
            let mut groups = self.group_textboxes(laparams, &textboxes);

            // CRITICAL FIX: Sort groups to match Python's order!
            // The Rust group_textboxes() returns groups in a different order than Python
            // due to differences in how the plane iterates final elements. This causes
            // IndexAssigner to traverse groups in the wrong order, producing incorrect output.
            // Solution: Sort groups by spatial position using the same formula as analyze().
            // This ensures ROOT-level groups are in the correct order (e.g., "8" before "150,00").
            let boxes_flow = laparams.boxes_flow.unwrap_or(0.5);
            groups.sort_by(|a, b| {
                let key_a = (1.0 - boxes_flow) * a.x0() - (1.0 + boxes_flow) * (a.y0() + a.y1());
                let key_b = (1.0 - boxes_flow) * b.x0() - (1.0 + boxes_flow) * (b.y0() + b.y1());
                key_a
                    .partial_cmp(&key_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Analyze and assign indices (analyze recursively sorts elements within groups)
            let mut assigner = IndexAssigner::new();
            for group in groups.iter_mut() {
                group.analyze(laparams);
                assigner.run(group);
            }

            // Extract textboxes with assigned indices from the groups
            textboxes = groups.iter().flat_map(|g| g.collect_textboxes()).collect();

            self.groups = Some(groups);

            // Sort textboxes by their assigned index
            textboxes.sort_by(|a, b| {
                let idx_a = match a {
                    TextBoxType::Horizontal(h) => h.index(),
                    TextBoxType::Vertical(v) => v.index(),
                };
                let idx_b = match b {
                    TextBoxType::Horizontal(h) => h.index(),
                    TextBoxType::Vertical(v) => v.index(),
                };
                idx_a.cmp(&idx_b)
            });
        }

        // Rebuild items list: textboxes + other objects + empty lines
        self.items.clear();
        for tb in textboxes {
            self.items.push(LTItem::TextBox(tb));
        }
        for other in otherobjs {
            self.items.push(other);
        }
        for empty in empties {
            self.items.push(LTItem::TextLine(empty));
        }
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
    container: LTLayoutContainer,
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

    /// Performs layout analysis on the figure.
    ///
    /// Only performs analysis if all_texts is enabled in laparams.
    pub fn analyze(&mut self, laparams: &LAParams) {
        if !laparams.all_texts {
            return;
        }
        self.container.analyze(laparams);
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
    container: LTLayoutContainer,
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

    /// Performs layout analysis on the page.
    pub fn analyze(&mut self, laparams: &LAParams) {
        self.container.analyze(laparams);
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
