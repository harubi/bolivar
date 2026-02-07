//! Text line types: LTTextLineHorizontal, LTTextLineVertical, TextLineType.
//!
//! The `Axis` enum allows runtime distinction between horizontal and vertical text.

use std::hash::Hash;

use crate::layout::bidi::reorder_text_per_line;
use crate::utils::{HasBBox, INF_F64, Plane, Rect};

use super::character::{LTAnno, LTChar};
use super::component::LTComponent;

/// Axis for text direction - horizontal (left-to-right) or vertical (top-to-bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    /// Horizontal text (left-to-right reading order)
    Horizontal,
    /// Vertical text (top-to-bottom reading order, typically right-to-left columns)
    Vertical,
}

/// Trait for text line types.
pub trait LTTextLine: HasBBox {
    fn word_margin(&self) -> f64;
    fn get_text(&self) -> String;
    fn is_empty(&self) -> bool;
    fn set_bbox(&mut self, bbox: Rect);
    fn axis(&self) -> Axis;
}

/// Element in a text line - either a character or annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum TextLineElement {
    Char(Box<LTChar>),
    Anno(LTAnno),
}

fn element_text(element: &TextLineElement) -> &str {
    match element {
        TextLineElement::Char(c) => c.get_text(),
        TextLineElement::Anno(a) => a.get_text(),
    }
}

fn collect_text_from_elements(elements: &[TextLineElement]) -> String {
    let mut total_len = 0;
    for e in elements {
        total_len += element_text(e).len();
    }

    let mut out = String::with_capacity(total_len);
    for e in elements {
        out.push_str(element_text(e));
    }
    out
}

fn collect_reordered_text(elements: &[TextLineElement]) -> String {
    reorder_text_per_line(&collect_text_from_elements(elements))
}

fn elements_are_blank_or_whitespace(elements: &[TextLineElement]) -> bool {
    let mut has_any = false;
    for e in elements {
        let s = element_text(e);
        if !s.is_empty() {
            has_any = true;
        }
        if s.chars().any(|c| !c.is_whitespace()) {
            return false;
        }
    }
    has_any
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

    pub const fn set_bbox(&mut self, bbox: Rect) {
        self.component.set_bbox(bbox);
    }

    pub const fn bbox(&self) -> Rect {
        self.component.bbox()
    }

    /// Finds neighboring horizontal text lines in the plane.
    pub fn find_neighbors<'a>(&self, plane: &'a Plane<Self>, ratio: f64) -> Vec<&'a Self> {
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

    fn is_left_aligned_with(&self, other: &Self, tolerance: f64) -> bool {
        (other.component.x0 - self.component.x0).abs() <= tolerance
    }

    fn is_right_aligned_with(&self, other: &Self, tolerance: f64) -> bool {
        (other.component.x1 - self.component.x1).abs() <= tolerance
    }

    fn is_centrally_aligned_with(&self, other: &Self, tolerance: f64) -> bool {
        let self_center = (self.component.x0 + self.component.x1) / 2.0;
        let other_center = (other.component.x0 + other.component.x1) / 2.0;
        (other_center - self_center).abs() <= tolerance
    }

    fn is_same_height_as(&self, other: &Self, tolerance: f64) -> bool {
        (other.component.height() - self.component.height()).abs() <= tolerance
    }

    /// Returns an iterator over elements in this text line.
    pub fn iter(&self) -> impl Iterator<Item = &TextLineElement> {
        self.elements.iter()
    }

    /// Add a text line element.
    pub fn add_element(&mut self, element: TextLineElement) {
        self.elements.push(element);
    }

    /// Performs analysis on the text line.
    ///
    /// Adds a newline annotation at the end of the text line.
    /// Matches Python layout.py:484-487.
    pub fn analyze(&mut self) {
        self.elements.push(TextLineElement::Anno(LTAnno::new("\n")));
    }
}

impl_has_bbox_delegate!(LTTextLineHorizontal, component);

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
        collect_reordered_text(&self.elements)
    }

    fn is_empty(&self) -> bool {
        if self.component.is_empty() {
            return true;
        }
        elements_are_blank_or_whitespace(&self.elements)
    }

    fn set_bbox(&mut self, bbox: Rect) {
        self.component.set_bbox(bbox);
    }

    fn axis(&self) -> Axis {
        Axis::Horizontal
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

    pub const fn set_bbox(&mut self, bbox: Rect) {
        self.component.set_bbox(bbox);
    }

    pub const fn bbox(&self) -> Rect {
        self.component.bbox()
    }

    /// Finds neighboring vertical text lines in the plane.
    pub fn find_neighbors<'a>(&self, plane: &'a Plane<Self>, ratio: f64) -> Vec<&'a Self> {
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

    fn is_lower_aligned_with(&self, other: &Self, tolerance: f64) -> bool {
        (other.component.y0 - self.component.y0).abs() <= tolerance
    }

    fn is_upper_aligned_with(&self, other: &Self, tolerance: f64) -> bool {
        (other.component.y1 - self.component.y1).abs() <= tolerance
    }

    fn is_centrally_aligned_with(&self, other: &Self, tolerance: f64) -> bool {
        let self_center = (self.component.y0 + self.component.y1) / 2.0;
        let other_center = (other.component.y0 + other.component.y1) / 2.0;
        (other_center - self_center).abs() <= tolerance
    }

    fn is_same_width_as(&self, other: &Self, tolerance: f64) -> bool {
        (other.component.width() - self.component.width()).abs() <= tolerance
    }

    /// Returns an iterator over elements in this text line.
    pub fn iter(&self) -> impl Iterator<Item = &TextLineElement> {
        self.elements.iter()
    }

    /// Add a text line element.
    pub fn add_element(&mut self, element: TextLineElement) {
        self.elements.push(element);
    }

    /// Performs analysis on the text line.
    ///
    /// Adds a newline annotation at the end of the text line.
    /// Matches Python layout.py:484-487.
    pub fn analyze(&mut self) {
        self.elements.push(TextLineElement::Anno(LTAnno::new("\n")));
    }
}

impl_has_bbox_delegate!(LTTextLineVertical, component);

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
        collect_reordered_text(&self.elements)
    }

    fn is_empty(&self) -> bool {
        if self.component.is_empty() {
            return true;
        }
        elements_are_blank_or_whitespace(&self.elements)
    }

    fn set_bbox(&mut self, bbox: Rect) {
        self.component.set_bbox(bbox);
    }

    fn axis(&self) -> Axis {
        Axis::Vertical
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
            Self::Horizontal(l) => l.is_empty(),
            Self::Vertical(l) => l.is_empty(),
        }
    }

    pub fn bbox(&self) -> Rect {
        match self {
            Self::Horizontal(l) => l.bbox(),
            Self::Vertical(l) => l.bbox(),
        }
    }

    pub fn set_bbox(&mut self, bbox: Rect) {
        match self {
            Self::Horizontal(l) => l.set_bbox(bbox),
            Self::Vertical(l) => l.set_bbox(bbox),
        }
    }

    /// Returns the axis (horizontal or vertical) of this text line.
    pub const fn axis(&self) -> Axis {
        match self {
            Self::Horizontal(_) => Axis::Horizontal,
            Self::Vertical(_) => Axis::Vertical,
        }
    }

    /// Performs analysis on the text line.
    ///
    /// Adds a newline annotation at the end of the text line.
    /// Matches Python layout.py:484-487.
    pub fn analyze(&mut self) {
        match self {
            Self::Horizontal(l) => l.analyze(),
            Self::Vertical(l) => l.analyze(),
        }
    }
}

impl HasBBox for TextLineType {
    fn x0(&self) -> f64 {
        match self {
            Self::Horizontal(l) => l.x0(),
            Self::Vertical(l) => l.x0(),
        }
    }
    fn y0(&self) -> f64 {
        match self {
            Self::Horizontal(l) => l.y0(),
            Self::Vertical(l) => l.y0(),
        }
    }
    fn x1(&self) -> f64 {
        match self {
            Self::Horizontal(l) => l.x1(),
            Self::Vertical(l) => l.x1(),
        }
    }
    fn y1(&self) -> f64 {
        match self {
            Self::Horizontal(l) => l.y1(),
            Self::Vertical(l) => l.y1(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_horizontal_line() {
        let line = LTTextLineHorizontal::new(0.5);
        assert_eq!(line.axis(), Axis::Horizontal);
    }

    #[test]
    fn axis_vertical_line() {
        let line = LTTextLineVertical::new(0.5);
        assert_eq!(line.axis(), Axis::Vertical);
    }

    #[test]
    fn textlinetype_axis() {
        let h = TextLineType::Horizontal(LTTextLineHorizontal::new(0.5));
        let v = TextLineType::Vertical(LTTextLineVertical::new(0.5));

        assert_eq!(h.axis(), Axis::Horizontal);
        assert_eq!(v.axis(), Axis::Vertical);
    }

    #[test]
    fn horizontal_get_text_reorders_rtl_runs() {
        let mut line = LTTextLineHorizontal::new(0.1);
        line.add_element(TextLineElement::Char(Box::new(LTChar::new(
            (0.0, 0.0, 1.0, 1.0),
            "\u{05D0}",
            "F",
            10.0,
            true,
            1.0,
        ))));
        line.add_element(TextLineElement::Char(Box::new(LTChar::new(
            (1.0, 0.0, 2.0, 1.0),
            "\u{05D1}",
            "F",
            10.0,
            true,
            1.0,
        ))));
        line.add_element(TextLineElement::Char(Box::new(LTChar::new(
            (2.0, 0.0, 3.0, 1.0),
            "\u{05D2}",
            "F",
            10.0,
            true,
            1.0,
        ))));
        line.analyze();

        assert_eq!(line.get_text(), "\u{05D2}\u{05D1}\u{05D0}\n");
    }
}
