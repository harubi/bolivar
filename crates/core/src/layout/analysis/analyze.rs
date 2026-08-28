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
use crate::layout::arena::{BoxId, CompactPageLayout, LayoutArena};

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
    #[hotpath::measure]
    pub fn analyze(&mut self, laparams: &LAParams) {
        if self.compact_layout.is_some() {
            return;
        }
        if self.container.groups.is_some() {
            self.container.analyze(laparams);
            return;
        }

        let source_items = std::mem::take(&mut self.container.items);
        let char_count = source_items.iter().filter(|item| item.is_char()).count();
        if char_count == 0 {
            self.container.items = source_items;
            return;
        }

        let mut other_items = Vec::with_capacity(source_items.len() - char_count);
        let mut arena = LayoutArena::with_char_capacity(char_count);
        for item in source_items {
            match item {
                LTItem::Char(character) => {
                    arena.push_char(character);
                }
                other => other_items.push(other),
            }
        }

        let line_ids = group_objects_arena(laparams, &mut arena);
        let (empty_lines, non_empty_lines): (Vec<_>, Vec<_>) = line_ids
            .iter()
            .copied()
            .partition(|id| arena.line_is_empty(*id));
        let box_ids = group_textlines_arena(laparams, &mut arena, &non_empty_lines);
        for id in &box_ids {
            arena.analyze_box(*id);
        }

        let (box_order, groups) = if laparams.boxes_flow.is_none() {
            let mut box_order = box_ids;
            box_order.sort_by_key(|id| simple_box_sort_key(&arena, *id));
            (box_order, None)
        } else {
            let proxies = box_ids.iter().map(|id| arena.box_proxy(*id)).collect();
            let mut groups = group_textboxes_exact_owned(laparams, proxies);
            let mut assigner = IndexAssigner::new();
            let mut box_order = Vec::with_capacity(box_ids.len());
            for group in &mut groups {
                group.analyze(laparams);
                assigner.run_with_assignment(group, &mut |source_index, index| {
                    let id = BoxId(
                        usize::try_from(source_index)
                            .expect("compact text box source index must be non-negative"),
                    );
                    arena.set_box_index(id, index);
                    box_order.push(id);
                });
            }
            (box_order, Some(groups))
        };

        let layout = CompactPageLayout::new(
            arena,
            box_order,
            empty_lines,
            other_items,
            groups.as_deref(),
        );
        self.install_compact_layout(layout);
    }
}

fn simple_box_sort_key(arena: &LayoutArena, id: BoxId) -> (i32, i64, i64) {
    let bbox = arena.box_bbox(id);
    if arena.box_is_vertical(id) {
        (0, (-bbox.2 * 1000.0) as i64, (-bbox.1 * 1000.0) as i64)
    } else {
        (1, (-bbox.1 * 1000.0) as i64, (bbox.0 * 1000.0) as i64)
    }
}

#[cfg(test)]
mod compact_page_tests {
    use super::*;

    #[test]
    fn analyzed_page_keeps_compact_arrays_until_items_are_requested() {
        let mut page = LTPage::new(1, (0.0, 0.0, 100.0, 100.0), 0.0);
        page.add(LTItem::Char(LTChar::new(
            (0.0, 0.0, 5.0, 10.0),
            "A",
            "F1",
            10.0,
            true,
            5.0,
        )));
        page.add(LTItem::Char(LTChar::new(
            (6.0, 0.0, 11.0, 10.0),
            "B",
            "F1",
            10.0,
            true,
            5.0,
        )));

        page.analyze(&LAParams::default());

        assert_eq!(page.compact_storage_counts(), Some((2, 1, 1)));
        assert_eq!(page.get_text(), "AB\n");
        assert!(!page.compact_items_are_materialized());
        assert_eq!(page.iter().count(), 1);
        assert_eq!(page.groups().map(Vec::len), Some(1));
    }

    #[test]
    fn adding_after_analysis_materializes_the_page_before_mutation() {
        let mut page = LTPage::new(1, (0.0, 0.0, 100.0, 100.0), 0.0);
        page.add(LTItem::Char(LTChar::new(
            (0.0, 0.0, 5.0, 10.0),
            "A",
            "F1",
            10.0,
            true,
            5.0,
        )));
        page.analyze(&LAParams::default());

        page.add(LTItem::Anno(crate::layout::types::LTAnno::new("x")));

        assert_eq!(page.compact_storage_counts(), None);
        assert_eq!(page.iter().count(), 2);
        assert_eq!(page.groups().map(Vec::len), Some(1));
    }

    #[test]
    fn analyzed_page_distinguishes_empty_groups_from_disabled_groups() {
        let mut page = LTPage::new(1, (0.0, 0.0, 100.0, 100.0), 0.0);
        page.add(LTItem::Char(LTChar::new(
            (0.0, 0.0, 5.0, 10.0),
            " ",
            "F1",
            10.0,
            true,
            5.0,
        )));

        page.analyze(&LAParams::default());

        assert_eq!(page.groups().map(Vec::len), Some(0));
    }

    #[test]
    fn reanalysis_after_mutation_keeps_legacy_group_state() {
        let mut page = LTPage::new(1, (0.0, 0.0, 100.0, 100.0), 0.0);
        page.add(LTItem::Char(LTChar::new(
            (0.0, 0.0, 5.0, 10.0),
            "A",
            "F1",
            10.0,
            true,
            5.0,
        )));
        page.analyze(&LAParams::default());
        page.add(LTItem::Char(LTChar::new(
            (20.0, 0.0, 25.0, 10.0),
            "B",
            "F1",
            10.0,
            true,
            5.0,
        )));
        let params = LAParams {
            boxes_flow: None,
            ..LAParams::default()
        };

        page.analyze(&params);

        assert_eq!(page.groups().map(Vec::len), Some(1));
    }
}
