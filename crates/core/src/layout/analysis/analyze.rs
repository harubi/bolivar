//! Main analyze() method for layout containers.
//!
//! Contains the primary entry point for layout analysis on LTLayoutContainer,
//! LTFigure, and LTPage.

use crate::utils::{HasBBox, fsplit};

use super::super::elements::{
    IndexAssigner, LTChar, LTFigure, LTItem, LTLayoutContainer, LTPage, LTTextBox, LTTextGroup,
    TextBoxType, TextLineType,
};
use super::super::params::LAParams;
use super::clustering::{
    group_textboxes, group_textboxes_exact, group_textboxes_exact_dual_heap,
    group_textboxes_exact_single_heap,
};
use super::grouping::{group_objects, group_textlines};

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

    /// Groups text boxes hierarchically using approximate algorithm.
    ///
    /// Delegates to module-level function for testability.
    pub fn group_textboxes(&self, laparams: &LAParams, boxes: &[TextBoxType]) -> Vec<LTTextGroup> {
        group_textboxes(laparams, boxes)
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

    /// Groups text boxes using dual-heap exact algorithm.
    ///
    /// Delegates to module-level function for testability.
    pub fn group_textboxes_exact_dual_heap(
        &self,
        laparams: &LAParams,
        boxes: &[TextBoxType],
    ) -> Vec<LTTextGroup> {
        group_textboxes_exact_dual_heap(laparams, boxes)
    }

    /// Groups text boxes using single-heap best-first algorithm.
    ///
    /// Delegates to module-level function for testability.
    pub fn group_textboxes_exact_single_heap(
        &self,
        laparams: &LAParams,
        boxes: &[TextBoxType],
    ) -> Vec<LTTextGroup> {
        group_textboxes_exact_single_heap(laparams, boxes)
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
        let textlines = group_objects(laparams, &chars);

        // Separate empty lines
        let (empties, textlines) = fsplit(|l| l.is_empty(), textlines);

        // Group lines into text boxes
        let mut textboxes = group_textlines(laparams, textlines);

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
            let mut groups = group_textboxes_exact(laparams, &textboxes);

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
