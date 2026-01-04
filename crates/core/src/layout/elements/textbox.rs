//! Text box types and text groups.

use std::hash::Hash;

use crate::utils::{HasBBox, INF_F64, Rect};

use super::component::LTComponent;
use super::textline::{Axis, LTTextLine, LTTextLineHorizontal, LTTextLineVertical};
use crate::layout::params::LAParams;

/// Trait for text box types.
pub trait LTTextBox {
    fn get_text(&self) -> String;
    fn get_writing_mode(&self) -> &'static str;
    fn index(&self) -> i32;
    fn set_index(&mut self, index: i32);
    fn is_empty(&self) -> bool;
    fn axis(&self) -> Axis;
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

    fn axis(&self) -> Axis {
        Axis::Horizontal
    }
}

impl_has_bbox_delegate!(LTTextBoxHorizontal, component);

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

    fn axis(&self) -> Axis {
        Axis::Vertical
    }
}

impl_has_bbox_delegate!(LTTextBoxVertical, component);

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

    /// Returns the axis (horizontal or vertical) of this text box.
    pub fn axis(&self) -> Axis {
        match self {
            TextBoxType::Horizontal(_) => Axis::Horizontal,
            TextBoxType::Vertical(_) => Axis::Vertical,
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

impl_has_bbox_delegate!(LTTextGroup, component);

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

impl_has_bbox_delegate!(LTTextGroupLRTB, tuple_method);

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

impl_has_bbox_delegate!(LTTextGroupTBRL, tuple_method);

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
