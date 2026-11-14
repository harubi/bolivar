//! Borrowed views of a page's text boxes and lines, read straight from a compact layout or
//! from owned items, so that callers walking every character need not materialize the page.

use crate::layout::arena::{BoxId, LayoutArena, LineId};
use std::borrow::Cow;

use crate::layout::bidi::{
    ReconstructedLine, collect_raw_text, line_output, reconstruct_element_refs, reorder_for_output,
};
use crate::utils::{HasBBox, Rect};

use super::textbox::TextBoxType;
use super::textline::{Axis, ElementRef, LTTextLine};

/// `f(&text)` as an owned string, reusing `text` itself when `f` borrows it unchanged.
fn owned_or(text: String, f: impl for<'t> FnOnce(&'t str) -> Cow<'t, str>) -> String {
    match f(&text) {
        Cow::Borrowed(_) => None,
        Cow::Owned(changed) => Some(changed),
    }
    .unwrap_or(text)
}

/// A text box of a page.
#[derive(Clone, Copy)]
pub enum BoxRef<'a> {
    Owned(&'a TextBoxType),
    Arena(&'a LayoutArena, BoxId),
}

impl<'a> BoxRef<'a> {
    pub fn bbox(self) -> Rect {
        match self {
            Self::Owned(text_box) => text_box.bbox(),
            Self::Arena(arena, id) => arena.box_bbox(id),
        }
    }

    /// `lr-tb` or `tb-rl`.
    pub fn writing_mode(self) -> &'static str {
        match self {
            Self::Owned(text_box) => text_box.writing_mode(),
            Self::Arena(arena, id) if arena.box_is_vertical(id) => "tb-rl",
            Self::Arena(..) => "lr-tb",
        }
    }

    /// The box's lines, in order.
    pub fn lines(self) -> impl Iterator<Item = LineRef<'a>> {
        let (owned, arena) = match self {
            Self::Owned(text_box) => (Some(text_box.lines()), None),
            Self::Arena(arena, id) => (None, Some((arena, arena.box_lines(id)))),
        };
        owned.into_iter().flatten().map(LineRef::Owned).chain(
            arena
                .into_iter()
                .flat_map(|(arena, ids)| ids.iter().map(move |&id| LineRef::Arena(arena, id))),
        )
    }
}

/// A text line of a page.
#[derive(Clone, Copy)]
pub enum LineRef<'a> {
    Owned(&'a dyn LTTextLine),
    Arena(&'a LayoutArena, LineId),
}

impl<'a> LineRef<'a> {
    pub fn bbox(self) -> Rect {
        match self {
            Self::Owned(line) => line.bbox(),
            Self::Arena(arena, id) => arena.line_bbox(id),
        }
    }

    pub fn axis(self) -> Axis {
        match self {
            Self::Owned(line) => line.axis(),
            Self::Arena(arena, id) => arena.line_axis(id),
        }
    }

    /// Whether the line reconstructs bidi text.
    pub fn bidi(self) -> bool {
        match self {
            Self::Owned(line) => line.bidi(),
            Self::Arena(arena, id) => arena.line_bidi(id),
        }
    }

    /// The output text, as [`LTTextLine::get_text`] gives it.
    pub fn text(self) -> String {
        match self {
            Self::Owned(line) => line.get_text(),
            Self::Arena(arena, id) => arena.line_text(id),
        }
    }

    /// The element texts in content order, as [`LTTextLine::raw_text`] gives them.
    pub fn raw_text(self) -> String {
        match self {
            Self::Owned(line) => line.raw_text(),
            Self::Arena(arena, id) => collect_raw_text(arena.line_elements(id)),
        }
    }

    /// The output text without bidi reconstruction.
    pub fn text_without_bidi(self) -> String {
        owned_or(self.raw_text(), reorder_for_output)
    }

    /// The output text, as [`Self::text`] gives it, from this line's [`Self::raw_text`]:
    /// borrowed from `raw` when the output is the raw text unchanged, as for most lines. Callers
    /// that need both texts collect the raw text once.
    pub fn output_for<'r>(self, raw: &'r str) -> Cow<'r, str> {
        line_output(raw, self.elements(), self.axis(), self.bidi())
    }

    /// The bidi reconstruction, with each span's source element.
    pub fn reconstructed(self) -> ReconstructedLine {
        match self {
            Self::Owned(line) => line.reconstructed(),
            Self::Arena(arena, id) => {
                reconstruct_element_refs(arena.line_elements(id), self.axis())
            }
        }
    }

    /// The line's elements, in content order.
    pub fn elements(self) -> impl Iterator<Item = ElementRef<'a>> + Clone {
        let (owned, arena) = match self {
            Self::Owned(line) => (Some(line.elements()), None),
            Self::Arena(arena, id) => (None, Some(arena.line_elements(id))),
        };
        owned
            .into_iter()
            .flatten()
            .map(ElementRef::from)
            .chain(arena.into_iter().flatten())
    }

    /// The `index`th element.
    pub fn element(self, index: usize) -> Option<ElementRef<'a>> {
        match self {
            Self::Owned(line) => line.elements().get(index).map(ElementRef::from),
            Self::Arena(arena, id) => arena.line_element(id, index),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::document::PDFDocument;
    use crate::extract::{ExtractOptions, extract_pages_stream_from_doc};
    use crate::layout::LAParams;

    use super::*;

    /// The text boxes of the materialized items, depth first through figures: the reference.
    fn materialized_boxes(page: &crate::layout::LTPage) -> Vec<BoxRef<'_>> {
        fn walk<'a>(item: &'a crate::layout::LTItem, out: &mut Vec<BoxRef<'a>>) {
            match item {
                crate::layout::LTItem::TextBox(text_box) => out.push(BoxRef::Owned(text_box)),
                crate::layout::LTItem::Figure(figure) => figure.iter().for_each(|c| walk(c, out)),
                crate::layout::LTItem::Page(page) => page.iter().for_each(|c| walk(c, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        page.iter().for_each(|item| walk(item, &mut out));
        out
    }

    fn line_facts(line: LineRef<'_>) -> String {
        let elements: Vec<String> = line.elements().map(|e| format!("{e:?}")).collect();
        let indexed: Vec<String> = (0..elements.len())
            .map(|i| format!("{:?}", line.element(i)))
            .collect();
        format!(
            "{:?}|{:?}|{}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}",
            line.bbox(),
            line.axis(),
            line.bidi(),
            line.text(),
            line.raw_text(),
            line.text_without_bidi(),
            line.reconstructed(),
            elements,
            indexed,
            line.element(elements.len()),
        )
    }

    fn page_facts(boxes: &[BoxRef<'_>]) -> Vec<String> {
        boxes
            .iter()
            .map(|b| {
                let lines: Vec<String> = b.lines().map(line_facts).collect();
                format!("{:?}|{}|{lines:?}", b.bbox(), b.writing_mode())
            })
            .collect()
    }

    /// The borrowed text layer reads, on every fixture page, exactly what the materialized
    /// text boxes hold: boxes, lines, texts, bidi reconstructions and every element.
    #[test]
    fn text_layer_matches_materialized_text_boxes() {
        let files = crate::test_support::fixture_pdfs();
        let mut pages = 0;
        for file in &files {
            for all_texts in [false, true] {
                let Ok(doc) = PDFDocument::open_path(file, &Default::default()) else {
                    continue;
                };
                let options = ExtractOptions {
                    laparams: Some(LAParams {
                        all_texts,
                        ..LAParams::default()
                    }),
                    ..ExtractOptions::default()
                };
                let Ok(stream) = extract_pages_stream_from_doc(Arc::new(doc), options) else {
                    continue;
                };
                for (_, page) in stream.flatten() {
                    for bidi in [false, true] {
                        let mut page = page.clone();
                        page.set_bidi(bidi);
                        let borrowed = page_facts(&page.text_layer());
                        let owned = materialized_boxes(&page);
                        assert_eq!(
                            borrowed,
                            page_facts(&owned),
                            "{} bidi={bidi}",
                            file.display()
                        );
                    }
                    pages += 1;
                }
            }
        }
        assert!(pages > 200, "checked {pages} pages");
    }
}
