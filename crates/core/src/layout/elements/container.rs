//! Container types: LTLayoutContainer, LTFigure, LTPage.

use crate::utils::{Matrix, Rect, apply_matrix_rect};

use super::component::LTComponent;
use super::item::LTItem;
use super::textbox::LTTextGroup;

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

impl_has_bbox_delegate!(LTLayoutContainer, component);

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

impl_has_bbox_delegate!(LTFigure, container, method);

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

impl_has_bbox_delegate!(LTPage, container, method);
