//! Container types: LTLayoutContainer, LTFigure, LTPage.

use std::sync::OnceLock;

use crate::layout::arena::CompactPageLayout;
use crate::utils::{Matrix, Rect, apply_matrix_rect};

use super::component::LTComponent;
use super::item::LTItem;
use super::textbox::{LTTextGroup, TextBoxType};
use super::textline::LTTextLine;

fn append_item_text(item: &LTItem, output: &mut String, first_box: &mut bool) {
    match item {
        LTItem::TextBox(text_box) => {
            if !*first_box {
                output.push('\n');
            }
            *first_box = false;
            match text_box {
                TextBoxType::Horizontal(text_box) => {
                    for line in text_box.iter() {
                        output.push_str(&line.get_text());
                    }
                }
                TextBoxType::Vertical(text_box) => {
                    for line in text_box.iter() {
                        output.push_str(&line.get_text());
                    }
                }
            }
        }
        LTItem::Figure(figure) => {
            for child in figure.iter() {
                append_item_text(child, output, first_box);
            }
        }
        LTItem::Page(page) => page.append_text(output, first_box),
        _ => {}
    }
}

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
    pub const fn new(bbox: Rect) -> Self {
        Self {
            component: LTComponent::new(bbox),
            items: Vec::new(),
            groups: None,
        }
    }

    pub const fn bbox(&self) -> Rect {
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

    pub(crate) fn reserve_items(&mut self, additional: usize) {
        self.container.items.reserve(additional);
    }

    /// Returns an iterator over contained items.
    pub fn iter(&self) -> impl Iterator<Item = &LTItem> {
        self.container.iter()
    }

    pub(crate) fn set_bidi(&mut self, bidi: bool) {
        for item in &mut self.container.items {
            item.set_bidi(bidi);
        }
    }
}

impl_has_bbox_delegate!(LTFigure, container, method);

/// Represents an entire page.
///
/// Like any other LTLayoutContainer, an LTPage can be iterated to obtain child
/// objects like LTTextBox, LTFigure, LTImage, LTRect, LTCurve and LTLine.
#[derive(Debug)]
pub struct LTPage {
    pub(crate) container: LTLayoutContainer,
    pub(crate) compact_layout: Option<CompactPageLayout>,
    item_cache: OnceLock<Vec<LTItem>>,
    group_cache: OnceLock<Vec<LTTextGroup>>,
    /// Page identifier (usually 1-based page number)
    pub pageid: i32,
    /// Page rotation in degrees
    pub rotate: f64,
}

impl LTPage {
    pub fn new(pageid: i32, bbox: Rect, rotate: f64) -> Self {
        Self {
            container: LTLayoutContainer::new(bbox),
            compact_layout: None,
            item_cache: OnceLock::new(),
            group_cache: OnceLock::new(),
            pageid,
            rotate,
        }
    }

    pub fn bbox(&self) -> Rect {
        self.container.bbox()
    }

    /// Adds an item to the page.
    pub fn add(&mut self, item: LTItem) {
        self.materialize_compact_layout();
        self.container.add(item);
    }

    pub(crate) fn reserve_items(&mut self, additional: usize) {
        self.materialize_compact_layout();
        self.container.items.reserve(additional);
    }

    /// Returns an iterator over contained items.
    pub fn iter(&self) -> std::slice::Iter<'_, LTItem> {
        if let Some(layout) = &self.compact_layout {
            self.item_cache
                .get_or_init(|| layout.materialize_items())
                .iter()
        } else {
            self.container.items.iter()
        }
    }

    /// Returns page text without materializing compact layout objects.
    pub fn get_text(&self) -> String {
        let mut output = String::new();
        let mut first_box = true;
        self.append_text(&mut output, &mut first_box);
        output
    }

    fn append_text(&self, output: &mut String, first_box: &mut bool) {
        if let Some(layout) = &self.compact_layout {
            layout.append_text_boxes(output, first_box);
            for item in layout.other_items() {
                append_item_text(item, output, first_box);
            }
        } else {
            for item in &self.container.items {
                append_item_text(item, output, first_box);
            }
        }
    }

    /// Enable or disable ICU bidi reconstruction for all text on this page.
    pub fn set_bidi(&mut self, bidi: bool) {
        if let Some(layout) = &mut self.compact_layout {
            layout.set_bidi(bidi);
            self.item_cache.take();
            self.group_cache.take();
        } else {
            for item in &mut self.container.items {
                item.set_bidi(bidi);
            }
        }
    }

    /// Returns the text groups after analysis (if boxes_flow was enabled).
    pub fn groups(&self) -> Option<&Vec<LTTextGroup>> {
        if let Some(layout) = &self.compact_layout {
            layout
                .has_groups()
                .then(|| self.group_cache.get_or_init(|| layout.materialize_groups()))
        } else {
            self.container.groups.as_ref()
        }
    }

    pub(crate) fn install_compact_layout(&mut self, layout: CompactPageLayout) {
        self.container.items.clear();
        self.container.groups = None;
        self.compact_layout = Some(layout);
        self.item_cache.take();
        self.group_cache.take();
    }

    fn materialize_compact_layout(&mut self) {
        let Some(layout) = self.compact_layout.take() else {
            return;
        };
        self.container.items = self
            .item_cache
            .take()
            .unwrap_or_else(|| layout.materialize_items());
        self.container.groups = layout.has_groups().then(|| {
            self.group_cache
                .take()
                .unwrap_or_else(|| layout.materialize_groups())
        });
    }

    #[cfg(test)]
    pub(crate) fn compact_storage_counts(&self) -> Option<(usize, usize, usize)> {
        self.compact_layout
            .as_ref()
            .map(CompactPageLayout::storage_counts)
    }

    #[cfg(test)]
    pub(crate) fn compact_items_are_materialized(&self) -> bool {
        self.item_cache.get().is_some()
    }
}

impl Clone for LTPage {
    fn clone(&self) -> Self {
        Self {
            container: self.container.clone(),
            compact_layout: self.compact_layout.clone(),
            item_cache: OnceLock::new(),
            group_cache: OnceLock::new(),
            pageid: self.pageid,
            rotate: self.rotate,
        }
    }
}

impl_has_bbox_delegate!(LTPage, container, method);
