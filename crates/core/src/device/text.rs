//! Text Converter - outputs plain text.
//!
//! Port of TextConverter from pdfminer.six converter.py

use std::io::Write;

use crate::layout::{
    LAParams, LTItem, LTPage, LTTextBox, LTTextLine, TextBoxType, TextLineType,
    normalize_arabic_presentation_forms,
};

/// Text Converter - outputs plain text.
///
/// Port of TextConverter from pdfminer.six converter.py
pub struct TextConverter<W: Write> {
    /// Output writer
    outfp: W,
    /// Output encoding
    #[allow(dead_code)]
    codec: String,
    /// Current page number
    #[allow(dead_code)]
    pageno: i32,
    /// Layout parameters
    #[allow(dead_code)]
    laparams: Option<LAParams>,
    /// Whether to show page numbers
    showpageno: bool,
    /// Whether to force ICU bidi reconstruction on received pages.
    bidi: bool,
}

impl<W: Write> TextConverter<W> {
    /// Create a new text converter.
    pub fn new(
        outfp: W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        showpageno: bool,
    ) -> Self {
        Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            showpageno,
            bidi: false,
        }
    }

    /// Set whether to show page numbers.
    pub const fn set_showpageno(&mut self, showpageno: bool) {
        self.showpageno = showpageno;
    }

    /// Check if page numbers are shown.
    pub const fn show_pageno(&self) -> bool {
        self.showpageno
    }

    /// Set whether to use ICU bidi reconstruction.
    pub const fn set_bidi(&mut self, bidi: bool) {
        self.bidi = bidi;
    }

    /// Write text to output.
    pub fn write_text(&mut self, text: &str) {
        let _ = self.outfp.write_all(text.as_bytes());
    }

    /// Flush output.
    pub fn flush(&mut self) {
        let _ = self.outfp.flush();
    }

    /// Receive and render a layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        let output = self.page_text(ltpage);
        self.write_text(&output);
    }

    /// The text this converter writes for a page, without writing it.
    pub fn page_text(&self, ltpage: LTPage) -> String {
        render_page_text(ltpage, self.showpageno, self.bidi)
    }
}

/// The text a [`TextConverter`] writes for a page: an optional `Page N` header, every item's
/// text, then a form feed. `bidi` reconstructs right-to-left text. Needs no writer, so workers
/// render pages on their own.
pub fn render_page_text(mut ltpage: LTPage, showpageno: bool, bidi: bool) -> String {
    use std::fmt::Write as _;
    if bidi {
        ltpage.set_bidi(true);
    }
    let mut output = String::new();
    if showpageno {
        let _ = writeln!(output, "Page {}", ltpage.pageid);
    }
    render_page(&ltpage, bidi, &mut output);
    // Form feed at end of page
    output.push('\x0c');
    output
}

/// Render a page's items into `output`, without materializing a compact layout.
fn render_page(page: &LTPage, bidi: bool, output: &mut String) {
    page.write_items(output, &mut |item, output| render_item(item, bidi, output));
}

/// Render one item into `output`.
fn render_item(item: &LTItem, bidi: bool, output: &mut String) {
    match item {
        LTItem::Page(page) => render_page(page, bidi, output),
        LTItem::TextBox(tb) => {
            output.push_str(&match tb {
                TextBoxType::Horizontal(h) => h.get_text(),
                TextBoxType::Vertical(v) => v.get_text(),
            });
            output.push('\n');
        }
        LTItem::TextLine(tl) => output.push_str(&match tl {
            TextLineType::Horizontal(h) => h.get_text(),
            TextLineType::Vertical(v) => v.get_text(),
        }),
        LTItem::Char(c) => push_glyph_text(c.get_text(), bidi, output),
        LTItem::Anno(a) => push_glyph_text(a.get_text(), bidi, output),
        LTItem::Figure(fig) => {
            for child in fig.iter() {
                render_item(child, bidi, output);
            }
        }
        _ => {}
    }
}

fn push_glyph_text(text: &str, bidi: bool, output: &mut String) {
    if bidi {
        output.push_str(&normalize_arabic_presentation_forms(text));
    } else {
        output.push_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::PDFDocument;
    use crate::extract::{ExtractOptions, extract_pages_stream_from_doc};
    use std::sync::Arc;

    /// Rendering from the compact layout writes what rendering the materialized items writes,
    /// on every page of every fixture, with bidi off and on.
    #[test]
    fn compact_rendering_matches_materialized_items() {
        let files = crate::test_support::fixture_pdfs();
        let mut pages = 0;
        for file in &files {
            let Ok(doc) = PDFDocument::open_path(file, &Default::default()) else {
                continue;
            };
            let Ok(stream) =
                extract_pages_stream_from_doc(Arc::new(doc), ExtractOptions::default())
            else {
                continue;
            };
            for (_, page) in stream.flatten() {
                for bidi in [false, true] {
                    let mut page = page.clone();
                    page.set_bidi(bidi);
                    let mut converter = TextConverter::new(Vec::new(), "utf-8", 1, None, false);
                    converter.set_bidi(bidi);
                    let mut expected = String::new();
                    for item in page.iter() {
                        render_item(item, bidi, &mut expected);
                    }
                    expected.push('\x0c');
                    converter.receive_layout(page);
                    let actual = String::from_utf8(converter.outfp).unwrap();
                    assert_eq!(actual, expected, "{} bidi={bidi}", file.display());
                }
                pages += 1;
            }
        }
        assert!(pages > 100, "rendered {pages} pages");
    }
}
