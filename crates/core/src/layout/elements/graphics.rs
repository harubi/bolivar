//! Graphical elements: LTCurve, LTLine, LTRect, LTImage.

use crate::utils::{Point, Rect, get_bound};

use super::character::Color;
use super::component::LTComponent;

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

impl_has_bbox_delegate!(LTCurve, component);

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

impl_has_bbox_delegate!(LTLine, curve, method);

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

impl_has_bbox_delegate!(LTRect, curve, method);

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

impl_has_bbox_delegate!(LTImage, component);
