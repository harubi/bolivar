//! Main analyze() method for layout containers.
//!
//! Contains the primary entry point for layout analysis on LTLayoutContainer,
//! LTFigure, and LTPage.

use crate::utils::HasBBox;

use super::super::params::LAParams;
use super::super::types::{
    IndexAssigner, LTChar, LTFigure, LTItem, LTLayoutContainer, LTPage, LTTextBox, LTTextGroup,
    TextBoxType, TextLineType,
};
use super::clustering::{group_textboxes_exact, group_textboxes_exact_owned};
use super::grouping::{group_objects, group_objects_arena, group_textlines, group_textlines_arena};
use crate::layout::arena::LayoutArena;

impl LTLayoutContainer {
    /// Groups character objects into text lines.
    ///
    /// Delegates to module-level function for testability.
    pub fn group_objects(&self, laparams: &LAParams, objs: &[LTChar]) -> Vec<TextLineType> {
        group_objects(laparams, objs)
    }

    /// Groups text lines into text boxes.
    ///
    /// Delegates to module-level function for testability.
    pub fn group_textlines(
        &self,
        laparams: &LAParams,
        lines: Vec<TextLineType>,
    ) -> Vec<TextBoxType> {
        group_textlines(laparams, lines)
    }

    /// Groups text boxes using exact pdfminer-compatible algorithm.
    ///
    /// Delegates to module-level function for testability.
    pub fn group_textboxes_exact(
        &self,
        laparams: &LAParams,
        boxes: &[TextBoxType],
    ) -> Vec<LTTextGroup> {
        group_textboxes_exact(laparams, boxes)
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
        let mut source_items = std::mem::take(&mut self.items);
        let char_count = source_items.iter().filter(|item| item.is_char()).count();
        if char_count == 0 {
            self.items = source_items;
            return;
        }

        let mut otherobjs = Vec::with_capacity(source_items.len() - char_count);
        let mut arena = LayoutArena::with_char_capacity(char_count);

        for item in source_items.drain(..) {
            match item {
                LTItem::Char(ch) => {
                    arena.push_char(ch);
                }
                other => otherobjs.push(other),
            }
        }

        let line_ids = group_objects_arena(laparams, &mut arena);
        let (empty_ids, non_empty_ids): (Vec<_>, Vec<_>) = line_ids
            .iter()
            .copied()
            .partition(|id| arena.line_is_empty(*id));

        let box_ids = group_textlines_arena(laparams, &mut arena, &non_empty_ids);
        let (mut textboxes, empties) = arena.into_materialized(&box_ids, &empty_ids);

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
            // Hierarchical grouping (exact pdfminer-compatible)
            let textbox_count = textboxes.len();
            let mut groups = group_textboxes_exact_owned(laparams, textboxes);

            // Analyze and assign indices (analyze recursively sorts elements within groups)
            let mut assigner = IndexAssigner::new();
            for group in groups.iter_mut() {
                group.analyze(laparams);
                assigner.run(group);
            }

            // Extract textboxes with assigned indices from the groups
            textboxes = Vec::with_capacity(textbox_count);
            for group in &groups {
                group.append_textboxes(&mut textboxes);
            }

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
        source_items.reserve(textboxes.len() + otherobjs.len() + empties.len());
        source_items.extend(textboxes.into_iter().map(LTItem::TextBox));
        source_items.extend(otherobjs);
        source_items.extend(empties.into_iter().map(LTItem::TextLine));
        self.items = source_items;
    }
}

impl LTFigure {
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

impl LTPage {
    /// Performs layout analysis on the page.
    pub fn analyze(&mut self, laparams: &LAParams) {
        self.container.analyze(laparams);
    }
}
