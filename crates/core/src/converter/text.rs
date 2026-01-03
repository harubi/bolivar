//! Text Converter - outputs plain text.
//!
//! Port of TextConverter from pdfminer.six converter.py

use std::io::Write;

use crate::layout::{LAParams, LTItem, LTPage, LTTextBox, LTTextLine, TextBoxType, TextLineType};

/// Text Converter - outputs plain text.
///
/// Port of TextConverter from pdfminer.six converter.py
pub struct TextConverter<'a, W: Write> {
    /// Output writer
    outfp: &'a mut W,
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
}

impl<'a, W: Write> TextConverter<'a, W> {
    /// Create a new text converter.
    pub fn new(
        outfp: &'a mut W,
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
        }
    }

    /// Check if page numbers are shown.
    pub fn show_pageno(&self) -> bool {
        self.showpageno
    }

    /// Write text to output.
    pub fn write_text(&mut self, text: &str) {
        let _ = self.outfp.write_all(text.as_bytes());
    }

    /// Receive and render a layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        if self.showpageno {
            let header = format!("Page {}\n", ltpage.pageid);
            self.write_text(&header);
        }

        // Render page content
        self.render_item(&LTItem::Page(Box::new(ltpage)));

        // Form feed at end of page
        self.write_text("\x0c");
    }

    /// Recursively render an item.
    fn render_item(&mut self, item: &LTItem) {
        match item {
            LTItem::Page(page) => {
                for child in page.iter() {
                    self.render_item(child);
                }
            }
            LTItem::TextBox(tb) => {
                let text = match tb {
                    TextBoxType::Horizontal(h) => h.get_text(),
                    TextBoxType::Vertical(v) => v.get_text(),
                };
                self.write_text(&text);
                self.write_text("\n");
            }
            LTItem::TextLine(tl) => {
                let text = match tl {
                    TextLineType::Horizontal(h) => h.get_text(),
                    TextLineType::Vertical(v) => v.get_text(),
                };
                self.write_text(&text);
            }
            LTItem::Char(c) => {
                self.write_text(c.get_text());
            }
            LTItem::Anno(a) => {
                self.write_text(a.get_text());
            }
            LTItem::Figure(fig) => {
                for child in fig.iter() {
                    self.render_item(child);
                }
            }
            _ => {}
        }
    }
}
