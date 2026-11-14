//! Text line types: LTTextLineHorizontal, LTTextLineVertical, TextLineType.
//!
//! The `Axis` enum allows runtime distinction between horizontal and vertical text.

use std::hash::Hash;

use crate::layout::bidi::{
    ReconstructedLine, collect_raw_text, line_text, reconstruct_textline_elements,
    reorder_text_for_output,
};
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
    /// The line's output text: bidi-reconstructed when [`Self::bidi`] is on,
    /// otherwise [`Self::text_without_bidi`].
    fn get_text(&self) -> String;
    fn is_empty(&self) -> bool;
    fn set_bbox(&mut self, bbox: Rect);
    fn axis(&self) -> Axis;
    /// The line's characters and annotations, in content order.
    fn elements(&self) -> &[TextLineElement];
    /// Whether output text is reconstructed with ICU bidi.
    fn bidi(&self) -> bool;
    /// Logical text and its mapping back to the source elements.
    fn reconstructed(&self) -> ReconstructedLine;

    /// Every element's text in content order, annotations included.
    fn raw_text(&self) -> String {
        collect_text_from_elements(self.elements())
    }

    /// The output text this line gives without bidi reconstruction.
    fn text_without_bidi(&self) -> String {
        reorder_text_for_output(&self.raw_text())
    }
}

/// Element in a text line - either a character or annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum TextLineElement {
    Char(Box<LTChar>),
    Anno(LTAnno),
}

/// A borrowed line element: a character or an annotation, from an owned line or a compact
/// layout alike.
#[derive(Clone, Copy, Debug)]
pub enum ElementRef<'a> {
    Char(&'a LTChar),
    Anno(&'a LTAnno),
}

impl<'a> ElementRef<'a> {
    /// The element's text.
    pub fn text(self) -> &'a str {
        match self {
            Self::Char(character) => character.get_text(),
            Self::Anno(annotation) => annotation.get_text(),
        }
    }

    /// The character, if this is one.
    pub fn as_char(self) -> Option<&'a LTChar> {
        match self {
            Self::Char(character) => Some(character),
            Self::Anno(_) => None,
        }
    }
}

impl<'a> From<&'a TextLineElement> for ElementRef<'a> {
    fn from(element: &'a TextLineElement) -> Self {
        match element {
            TextLineElement::Char(character) => Self::Char(character),
            TextLineElement::Anno(annotation) => Self::Anno(annotation),
        }
    }
}

fn collect_text_from_elements(elements: &[TextLineElement]) -> String {
    collect_raw_text(elements.iter().map(ElementRef::from))
}

fn text_from_elements(elements: &[TextLineElement], axis: Axis, bidi: bool) -> String {
    line_text(elements.iter().map(ElementRef::from), axis, bidi)
}

/// Whether a line's elements hold text and all of it is whitespace.
pub(crate) fn elements_are_blank_or_whitespace<'a>(
    elements: impl IntoIterator<Item = ElementRef<'a>>,
) -> bool {
    let mut has_any = false;
    for element in elements {
        let text = element.text();
        if !text.is_empty() {
            has_any = true;
        }
        if text.chars().any(|c| !c.is_whitespace()) {
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
    pub(crate) bidi: bool,
}

impl LTTextLineHorizontal {
    pub fn new(word_margin: f64) -> Self {
        Self {
            component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
            word_margin,
            x1_tracker: INF_F64,
            elements: Vec::new(),
            bidi: false,
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

    /// Return the source elements in this text line.
    pub fn elements(&self) -> &[TextLineElement] {
        &self.elements
    }

    /// Return logical text and its mapping to source layout elements.
    pub fn reconstructed(&self) -> ReconstructedLine {
        reconstruct_textline_elements(&self.elements, Axis::Horizontal)
    }

    /// Enable or disable ICU bidi reconstruction for this line.
    pub const fn set_bidi(&mut self, bidi: bool) {
        self.bidi = bidi;
    }

    /// Return whether ICU bidi reconstruction is enabled.
    pub const fn bidi(&self) -> bool {
        self.bidi
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

    // The trait answers with the inherent methods, so each has one body.
    fn elements(&self) -> &[TextLineElement] {
        LTTextLineHorizontal::elements(self)
    }

    fn bidi(&self) -> bool {
        LTTextLineHorizontal::bidi(self)
    }

    fn reconstructed(&self) -> ReconstructedLine {
        LTTextLineHorizontal::reconstructed(self)
    }

    fn get_text(&self) -> String {
        text_from_elements(&self.elements, Axis::Horizontal, self.bidi)
    }

    fn is_empty(&self) -> bool {
        if self.component.is_empty() {
            return true;
        }
        elements_are_blank_or_whitespace(self.elements.iter().map(ElementRef::from))
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
    pub(crate) bidi: bool,
}

impl LTTextLineVertical {
    pub fn new(word_margin: f64) -> Self {
        Self {
            component: LTComponent::new((INF_F64, INF_F64, -INF_F64, -INF_F64)),
            word_margin,
            y0_tracker: -INF_F64,
            elements: Vec::new(),
            bidi: false,
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

    /// Return the source elements in this text line.
    pub fn elements(&self) -> &[TextLineElement] {
        &self.elements
    }

    /// Return logical text and its mapping to source layout elements.
    pub fn reconstructed(&self) -> ReconstructedLine {
        reconstruct_textline_elements(&self.elements, Axis::Vertical)
    }

    /// Enable or disable ICU bidi reconstruction for this line.
    pub const fn set_bidi(&mut self, bidi: bool) {
        self.bidi = bidi;
    }

    /// Return whether ICU bidi reconstruction is enabled.
    pub const fn bidi(&self) -> bool {
        self.bidi
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

    // The trait answers with the inherent methods, so each has one body.
    fn elements(&self) -> &[TextLineElement] {
        LTTextLineVertical::elements(self)
    }

    fn bidi(&self) -> bool {
        LTTextLineVertical::bidi(self)
    }

    fn reconstructed(&self) -> ReconstructedLine {
        LTTextLineVertical::reconstructed(self)
    }

    fn get_text(&self) -> String {
        text_from_elements(&self.elements, Axis::Vertical, self.bidi)
    }

    fn is_empty(&self) -> bool {
        if self.component.is_empty() {
            return true;
        }
        elements_are_blank_or_whitespace(self.elements.iter().map(ElementRef::from))
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

    #[test]
    fn bidi_is_opt_in_for_layout_lines() {
        let visual = "Task Ref42 12:34:\u{fe94}\u{fec8}\u{fea3}\u{fefc}\u{fee3}**56:78:\u{fe96}\u{fed7}\u{feee}\u{fedf}\u{fe8d}";
        let mut line = LTTextLineHorizontal::new(0.1);
        for (index, ch) in visual.chars().enumerate() {
            line.add_element(TextLineElement::Char(Box::new(LTChar::new(
                (index as f64, 0.0, index as f64 + 1.0, 1.0),
                &ch.to_string(),
                "F",
                10.0,
                true,
                1.0,
            ))));
        }

        assert_eq!(line.get_text(), reorder_text_for_output(visual));
        line.set_bidi(true);
        assert_eq!(line.get_text(), "Task Ref42 12:34:الوقت:56:78**ملاحظة");
    }
}
