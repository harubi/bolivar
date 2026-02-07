//! HTML and HOCR Converters - outputs HTML with positioning or hOCR format.
//!
//! Port of HTMLConverter and HOCRConverter from pdfminer.six converter.py

use regex::Regex;
use std::collections::HashMap;
use std::io::Write;

use crate::layout::{
    LAParams, LTChar, LTItem, LTPage, LTTextBox, LTTextGroup, TextBoxType, TextGroupElement,
    TextLineElement, TextLineType, reorder_text_per_line,
};
use crate::utils::{HasBBox, Rect, enc, make_compat_str};

// ============================================================================
// HTMLConverter
// ============================================================================

/// HTML Converter - outputs HTML with positioning.
///
/// Port of HTMLConverter from pdfminer.six converter.py
pub struct HTMLConverter<W: Write> {
    /// Output writer
    outfp: W,
    /// Output encoding
    codec: String,
    /// Current page number
    pageno: i32,
    /// Layout parameters
    #[allow(dead_code)]
    laparams: Option<LAParams>,
    /// Scale factor
    scale: f64,
    /// Font scale factor
    #[allow(dead_code)]
    fontscale: f64,
    /// Layout mode
    #[allow(dead_code)]
    layoutmode: String,
    /// Show page numbers
    #[allow(dead_code)]
    showpageno: bool,
    /// Page margin
    #[allow(dead_code)]
    pagemargin: i32,
    /// Rectangle colors for debug rendering
    rect_colors: HashMap<String, String>,
    /// Text colors for debug rendering
    text_colors: HashMap<String, String>,
    /// Y offset for positioning
    #[allow(dead_code)]
    yoffset: f64,
}

impl<W: Write> HTMLConverter<W> {
    /// Default rectangle colors.
    pub fn default_rect_colors() -> HashMap<String, String> {
        let mut colors = HashMap::new();
        colors.insert("curve".to_string(), "black".to_string());
        colors.insert("page".to_string(), "gray".to_string());
        colors
    }

    /// Default text colors.
    pub fn default_text_colors() -> HashMap<String, String> {
        let mut colors = HashMap::new();
        colors.insert("char".to_string(), "black".to_string());
        colors
    }

    /// Full debug colors for rectangles.
    pub fn debug_rect_colors() -> HashMap<String, String> {
        let mut colors = Self::default_rect_colors();
        colors.insert("figure".to_string(), "yellow".to_string());
        colors.insert("textline".to_string(), "magenta".to_string());
        colors.insert("textbox".to_string(), "cyan".to_string());
        colors.insert("textgroup".to_string(), "red".to_string());
        colors
    }

    /// Full debug colors for text.
    pub fn debug_text_colors() -> HashMap<String, String> {
        let mut colors = Self::default_text_colors();
        colors.insert("textbox".to_string(), "blue".to_string());
        colors
    }

    /// Create a new HTML converter.
    pub fn new(outfp: W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
        let mut converter = Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            scale: 1.0,
            fontscale: 1.0,
            layoutmode: "normal".to_string(),
            showpageno: true,
            pagemargin: 50,
            rect_colors: Self::default_rect_colors(),
            text_colors: Self::default_text_colors(),
            yoffset: 50.0,
        };
        converter.write_header();
        converter
    }

    /// Create with debug mode.
    pub fn with_debug(
        outfp: W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        debug: i32,
    ) -> Self {
        let mut converter = Self::new(outfp, codec, pageno, laparams);
        if debug > 0 {
            converter.rect_colors = Self::debug_rect_colors();
            converter.text_colors = Self::debug_text_colors();
        }
        converter
    }

    /// Create with custom options.
    pub fn with_options(
        outfp: W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        scale: f64,
        fontscale: f64,
    ) -> Self {
        let mut converter = Self::new(outfp, codec, pageno, laparams);
        converter.scale = scale;
        converter.fontscale = fontscale;
        converter
    }

    /// Get scale factor.
    pub const fn scale(&self) -> f64 {
        self.scale
    }

    /// Get rectangle colors.
    pub const fn rect_colors(&self) -> &HashMap<String, String> {
        &self.rect_colors
    }

    /// Set layout mode.
    pub fn set_layoutmode(&mut self, layoutmode: &str) {
        self.layoutmode = layoutmode.to_string();
    }

    /// Set whether to show page numbers.
    pub const fn set_showpageno(&mut self, showpageno: bool) {
        self.showpageno = showpageno;
    }

    /// Set page margin.
    pub const fn set_pagemargin(&mut self, pagemargin: i32) {
        self.pagemargin = pagemargin;
    }

    /// Set scale factor.
    pub const fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// Set font scale factor.
    pub const fn set_fontscale(&mut self, fontscale: f64) {
        self.fontscale = fontscale;
    }

    /// Replace rectangle colors.
    pub fn set_rect_colors(&mut self, rect_colors: HashMap<String, String>) {
        self.rect_colors = rect_colors;
    }

    /// Replace text colors.
    pub fn set_text_colors(&mut self, text_colors: HashMap<String, String>) {
        self.text_colors = text_colors;
    }

    /// Write output.
    fn write(&mut self, text: &str) {
        let _ = self.outfp.write_all(text.as_bytes());
    }

    /// Flush output.
    pub fn flush(&mut self) {
        let _ = self.outfp.flush();
    }

    /// Write header.
    fn write_header(&mut self) {
        self.write("<html><head>\n");
        if !self.codec.is_empty() {
            let meta = format!(
                "<meta http-equiv=\"Content-Type\" content=\"text/html; charset={}\">\n",
                self.codec
            );
            self.write(&meta);
        } else {
            self.write("<meta http-equiv=\"Content-Type\" content=\"text/html\">\n");
        }
        self.write("</head><body>\n");
    }

    /// Write footer.
    fn write_footer(&mut self) {
        let mut page_links = Vec::new();
        for i in 1..self.pageno {
            page_links.push(format!("<a href=\"#{}\">{}</a>", i, i));
        }
        let links = page_links.join(", ");
        let footer = format!(
            "<div style=\"position:absolute; top:0px;\">Page: {}</div>\n",
            links
        );
        self.write(&footer);
        self.write("</body></html>\n");
    }

    /// Write text with HTML encoding.
    pub fn write_text(&mut self, text: &str) {
        self.write(&enc(text));
    }

    /// Place a rectangle with the specified color and border.
    ///
    /// Port of HTMLConverter.place_rect from pdfminer.six
    pub fn place_rect(&mut self, color: &str, borderwidth: i32, x: f64, y: f64, w: f64, h: f64) {
        if let Some(color2) = self.rect_colors.get(color) {
            let s = format!(
                "<span style=\"position:absolute; border: {} {}px solid; \
                 left:{}px; top:{}px; width:{}px; height:{}px;\"></span>\n",
                color2,
                borderwidth,
                (x * self.scale) as i32,
                ((self.yoffset - y) * self.scale) as i32,
                (w * self.scale) as i32,
                (h * self.scale) as i32
            );
            self.write(&s);
        }
    }

    /// Place a border around a component.
    ///
    /// Port of HTMLConverter.place_border from pdfminer.six
    pub fn place_border<T: HasBBox>(&mut self, color: &str, borderwidth: i32, item: &T) {
        self.place_rect(
            color,
            borderwidth,
            item.x0(),
            item.y1(),
            item.width(),
            item.height(),
        );
    }

    /// Place an image.
    ///
    /// Port of HTMLConverter.place_image from pdfminer.six
    pub fn place_image(&mut self, name: &str, borderwidth: i32, x: f64, y: f64, w: f64, h: f64) {
        let s = format!(
            "<img src=\"{}\" border=\"{}\" style=\"position:absolute; \
             left:{}px; top:{}px;\" width=\"{}\" height=\"{}\" />\n",
            enc(name),
            borderwidth,
            (x * self.scale) as i32,
            ((self.yoffset - y) * self.scale) as i32,
            (w * self.scale) as i32,
            (h * self.scale) as i32
        );
        self.write(&s);
    }

    /// Place text at a specific position with specified size.
    ///
    /// Port of HTMLConverter.place_text from pdfminer.six
    pub fn place_text(&mut self, color: &str, text: &str, x: f64, y: f64, size: f64) {
        if let Some(color2) = self.text_colors.get(color) {
            let s = format!(
                "<span style=\"position:absolute; color:{}; left:{}px; \
                 top:{}px; font-size:{}px;\">",
                color2,
                (x * self.scale) as i32,
                ((self.yoffset - y) * self.scale) as i32,
                (size * self.scale * self.fontscale) as i32
            );
            self.write(&s);
            self.write_text(text);
            self.write("</span>\n");
        }
    }

    /// Begin a div element with positioning.
    ///
    /// Port of HTMLConverter.begin_div from pdfminer.six
    #[allow(clippy::too_many_arguments)]
    pub fn begin_div(
        &mut self,
        color: &str,
        borderwidth: i32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        writing_mode: &str,
    ) {
        let s = format!(
            "<div style=\"position:absolute; border: {} {}px solid; \
             writing-mode:{}; left:{}px; top:{}px; width:{}px; height:{}px;\">",
            color,
            borderwidth,
            writing_mode,
            (x * self.scale) as i32,
            ((self.yoffset - y) * self.scale) as i32,
            (w * self.scale) as i32,
            (h * self.scale) as i32
        );
        self.write(&s);
    }

    /// End a div element.
    ///
    /// Port of HTMLConverter.end_div from pdfminer.six
    pub fn end_div(&mut self, _color: &str) {
        self.write("</div>");
    }

    /// Write text with font styling.
    ///
    /// Port of HTMLConverter.put_text from pdfminer.six
    pub fn put_text(&mut self, text: &str, fontname: &str, fontsize: f64) {
        // Remove subset tag from fontname (e.g., ABCDEF+Times -> Times)
        let fontname_without_subset = fontname.split('+').next_back().unwrap_or(fontname);
        let s = format!(
            "<span style=\"font-family: {}; font-size:{}px\">",
            fontname_without_subset,
            (fontsize * self.scale * self.fontscale) as i32
        );
        self.write(&s);
        self.write_text(text);
        self.write("</span>");
    }

    /// Write a newline (line break).
    ///
    /// Port of HTMLConverter.put_newline from pdfminer.six
    pub fn put_newline(&mut self) {
        self.write("<br>");
    }

    /// Receive and render a layout page.
    ///
    /// Port of HTMLConverter.receive_layout from pdfminer.six
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        self.yoffset += ltpage.y1();
        self.place_border("page", 1, &ltpage);

        if self.showpageno {
            let header = format!(
                "<div style=\"position:absolute; top:{}px;\">\
                 <a name=\"{}\">Page {}</a></div>\n",
                ((self.yoffset - ltpage.y1()) * self.scale) as i32,
                ltpage.pageid,
                ltpage.pageid
            );
            self.write(&header);
        }

        // Render page content
        for item in ltpage.iter() {
            self.render_item(item);
        }

        // Render text groups if present
        if let Some(groups) = ltpage.groups() {
            for group in groups {
                self.show_group(group);
            }
        }

        self.yoffset += self.pagemargin as f64;
        self.pageno += 1;
    }

    /// Show text group borders (for debugging).
    fn show_group(&mut self, group: &LTTextGroup) {
        self.place_border("textgroup", 1, group);
        for elem in group.iter() {
            if let TextGroupElement::Group(subgroup) = elem {
                self.show_group(subgroup);
            }
        }
    }

    /// Render a layout item to HTML.
    fn render_item(&mut self, item: &LTItem) {
        match item {
            LTItem::Curve(_) => {
                if let LTItem::Curve(c) = item {
                    self.place_border("curve", 1, c);
                }
            }
            LTItem::Figure(fig) => {
                self.begin_div(
                    "figure",
                    1,
                    fig.x0(),
                    fig.y1(),
                    fig.width(),
                    fig.height(),
                    "lr-tb",
                );
                for child in fig.iter() {
                    self.render_item(child);
                }
                self.end_div("figure");
            }
            LTItem::Image(img) => {
                self.place_image(&img.name, 1, img.x0(), img.y1(), img.width(), img.height());
            }
            LTItem::TextLine(tl) => {
                self.place_border("textline", 1, tl);
                // Render children would go here if we stored them
            }
            LTItem::TextBox(tb) => {
                let (bbox, wmode, index) = match tb {
                    TextBoxType::Horizontal(h) => (h.bbox(), h.get_writing_mode(), h.index()),
                    TextBoxType::Vertical(v) => (v.bbox(), v.get_writing_mode(), v.index()),
                };
                self.place_border("textbox", 1, tb);
                self.place_text("textbox", &format!("{}", index + 1), bbox.0, bbox.3, 20.0);

                // In layoutmode "exact", render each character
                // In normal mode, render text boxes with divs
                if self.layoutmode == "exact" {
                    // Exact mode - render individual items
                } else {
                    self.begin_div(
                        "textbox",
                        1,
                        bbox.0,
                        bbox.3,
                        bbox.2 - bbox.0,
                        bbox.3 - bbox.1,
                        wmode,
                    );
                    // Render textbox text content in normal/loose modes
                    let text = match tb {
                        TextBoxType::Horizontal(h) => h.get_text(),
                        TextBoxType::Vertical(v) => v.get_text(),
                    };
                    let text = reorder_text_per_line(&text);
                    self.write_text(&text);
                    self.end_div("textbox");
                }
            }
            LTItem::Char(c) => {
                if self.layoutmode == "exact" {
                    self.place_border("char", 1, c);
                    self.place_text("char", c.get_text(), c.x0(), c.y1(), c.size());
                } else {
                    let fontname = make_compat_str(c.fontname());
                    self.put_text(c.get_text(), &fontname, c.size());
                }
            }
            LTItem::Anno(a) => {
                self.write_text(a.get_text());
            }
            _ => {}
        }
    }

    /// Close the converter.
    pub fn close(&mut self) {
        self.write_footer();
    }
}

// ============================================================================
// HOCRConverter
// ============================================================================

/// HOCR Converter - outputs hOCR format for OCR-compatible output.
///
/// Port of HOCRConverter from pdfminer.six converter.py
///
/// HOCR is a standard format for OCR output that includes bounding box
/// coordinates for each word/line. This converter extracts explicit text
/// information from PDFs that have it and generates hOCR representation
/// that can be used in conjunction with page images.
pub struct HOCRConverter<W: Write> {
    /// Output writer
    outfp: W,
    /// Output encoding
    codec: String,
    /// Current page number
    #[allow(dead_code)]
    pageno: i32,
    /// Layout parameters
    #[allow(dead_code)]
    laparams: Option<LAParams>,
    /// Strip control characters
    stripcontrol: bool,
    /// Regex for control characters
    control_re: Regex,
    /// Whether currently accumulating characters within a word
    within_chars: bool,
    /// Current page bounding box (for y-coordinate inversion)
    page_bbox: Rect,
    /// Working text buffer for current word
    working_text: String,
    /// Working bounding box for current word
    working_bbox: Rect,
    /// Working font name for current word
    working_font: String,
    /// Working font size for current word
    working_size: f64,
}

impl<W: Write> HOCRConverter<W> {
    /// Create a new HOCR converter.
    pub fn new(outfp: W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
        let mut converter = Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            stripcontrol: false,
            control_re: Regex::new(r"[\x00-\x08\x0b-\x0c\x0e-\x1f]").unwrap(),
            within_chars: false,
            page_bbox: (0.0, 0.0, 0.0, 0.0),
            working_text: String::new(),
            working_bbox: (0.0, 0.0, 0.0, 0.0),
            working_font: String::new(),
            working_size: 0.0,
        };
        converter.write_header();
        converter
    }

    /// Create with options.
    pub fn with_options(
        outfp: W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        stripcontrol: bool,
    ) -> Self {
        let mut converter = Self::new(outfp, codec, pageno, laparams);
        converter.stripcontrol = stripcontrol;
        converter
    }

    /// Write output.
    fn write(&mut self, text: &str) {
        let _ = self.outfp.write_all(text.as_bytes());
    }

    /// Flush output.
    pub fn flush(&mut self) {
        let _ = self.outfp.flush();
    }

    /// Convert PDF bbox to hOCR bbox string.
    ///
    /// PDF y-coordinates are inverted compared to hOCR coordinates.
    fn bbox_repr(&self, bbox: Rect) -> String {
        let (in_x0, in_y0, in_x1, in_y1) = bbox;
        // PDF y-coordinates are the other way round from hOCR coordinates
        let out_x0 = in_x0 as i32;
        let out_y0 = (self.page_bbox.3 - in_y1) as i32;
        let out_x1 = in_x1 as i32;
        let out_y1 = (self.page_bbox.3 - in_y0) as i32;
        format!("bbox {} {} {} {}", out_x0, out_y0, out_x1, out_y1)
    }

    /// Write header.
    fn write_header(&mut self) {
        if !self.codec.is_empty() {
            self.write(&format!(
                "<html xmlns='http://www.w3.org/1999/xhtml' xml:lang='en' lang='en' charset='{}'>\n",
                self.codec
            ));
        } else {
            self.write("<html xmlns='http://www.w3.org/1999/xhtml' xml:lang='en' lang='en'>\n");
        }
        self.write("<head>\n");
        self.write("<title></title>\n");
        self.write("<meta http-equiv='Content-Type' content='text/html;charset=utf-8' />\n");
        self.write("<meta name='ocr-system' content='bolivar' />\n");
        self.write(
            "  <meta name='ocr-capabilities' content='ocr_page ocr_block ocr_line ocrx_word'/>\n",
        );
        self.write("</head>\n");
        self.write("<body>\n");
    }

    /// Write footer.
    fn write_footer(&mut self) {
        self.write("<!-- comment in the following line to debug -->\n");
        self.write("<!--script src='https://unpkg.com/hocrjs'></script--></body></html>\n");
    }

    /// Write text with control character handling.
    pub fn write_text(&mut self, text: &str) {
        let text = if self.stripcontrol {
            self.control_re.replace_all(text, "").to_string()
        } else {
            text.to_string()
        };
        self.write(&text);
    }

    /// Write accumulated word span.
    fn write_word(&mut self) {
        if !self.working_text.is_empty() {
            let mut bold_and_italic_styles = String::new();
            if self.working_font.contains("Italic") {
                bold_and_italic_styles.push_str("font-style: italic; ");
            }
            if self.working_font.contains("Bold") {
                bold_and_italic_styles.push_str("font-weight: bold; ");
            }
            // Escape font name and text content to prevent XSS
            let escaped_font = enc(&self.working_font);
            let escaped_text = enc(self.working_text.trim());
            let output = format!(
                "<span style='font:\"{}\"; font-size:{}; {}' class='ocrx_word' title='{}; x_font {}; x_fsize {}'>{}</span>",
                escaped_font,
                self.working_size,
                bold_and_italic_styles,
                self.bbox_repr(self.working_bbox),
                escaped_font,
                self.working_size,
                escaped_text
            );
            self.write(&output);
        }
        self.within_chars = false;
    }

    /// Receive and render a layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        self.page_bbox = ltpage.bbox();

        // Write page div
        let page_output = format!(
            "<div class='ocr_page' id='{}' title='{}'>\n",
            ltpage.pageid,
            self.bbox_repr(ltpage.bbox())
        );
        self.write(&page_output);

        // Render page content
        for item in ltpage.iter() {
            self.render_item(item);
        }

        self.write("</div>\n");
        self.pageno += 1;
    }

    /// Render an item.
    fn render_item(&mut self, item: &LTItem) {
        // Check if we need to write accumulated word before processing annotations
        if self.within_chars
            && let LTItem::Anno(_) = item
        {
            self.write_word();
        }

        match item {
            LTItem::Page(page) => {
                for child in page.iter() {
                    self.render_item(child);
                }
            }
            LTItem::TextLine(tl) => {
                let bbox = match tl {
                    TextLineType::Horizontal(h) => h.bbox(),
                    TextLineType::Vertical(v) => v.bbox(),
                };
                let line_output =
                    format!("<span class='ocr_line' title='{}'>", self.bbox_repr(bbox));
                self.write(&line_output);

                // Render children
                match tl {
                    TextLineType::Horizontal(h) => {
                        for elem in h.iter() {
                            self.render_textline_element(elem);
                        }
                    }
                    TextLineType::Vertical(v) => {
                        for elem in v.iter() {
                            self.render_textline_element(elem);
                        }
                    }
                }

                self.write("</span>\n");
            }
            LTItem::TextBox(tb) => {
                let (id, bbox) = match tb {
                    TextBoxType::Horizontal(h) => (h.index(), h.bbox()),
                    TextBoxType::Vertical(v) => (v.index(), v.bbox()),
                };
                let block_output = format!(
                    "<div class='ocr_block' id='{}' title='{}'>\n",
                    id,
                    self.bbox_repr(bbox)
                );
                self.write(&block_output);

                // Render children (text lines)
                match tb {
                    TextBoxType::Horizontal(h) => {
                        for line in h.iter() {
                            let line_bbox = line.bbox();
                            let line_output = format!(
                                "<span class='ocr_line' title='{}'>",
                                self.bbox_repr(line_bbox)
                            );
                            self.write(&line_output);
                            for elem in line.iter() {
                                self.render_textline_element(elem);
                            }
                            self.write("</span>\n");
                        }
                    }
                    TextBoxType::Vertical(v) => {
                        for line in v.iter() {
                            let line_bbox = line.bbox();
                            let line_output = format!(
                                "<span class='ocr_line' title='{}'>",
                                self.bbox_repr(line_bbox)
                            );
                            self.write(&line_output);
                            for elem in line.iter() {
                                self.render_textline_element(elem);
                            }
                            self.write("</span>\n");
                        }
                    }
                }

                self.write("</div>\n");
            }
            LTItem::Char(c) => {
                self.process_char(c);
            }
            LTItem::Anno(a) => {
                // Annotations (whitespace) - write accumulated word if any
                if self.within_chars {
                    self.write_word();
                }
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

    /// Render a text line element (char or annotation).
    fn render_textline_element(&mut self, elem: &TextLineElement) {
        match elem {
            TextLineElement::Char(c) => {
                self.process_char(c);
            }
            TextLineElement::Anno(a) => {
                // Annotation ends current word
                if self.within_chars {
                    self.write_word();
                }
                self.write_text(a.get_text());
            }
        }
    }

    /// Process a character, accumulating into words.
    fn process_char(&mut self, c: &LTChar) {
        if !self.within_chars {
            // Start new word
            self.within_chars = true;
            self.working_text = c.get_text().to_string();
            self.working_bbox = c.bbox();
            self.working_font = c.fontname().to_string();
            self.working_size = c.size();
        } else if c.get_text().trim().is_empty() {
            // Whitespace character ends word
            self.write_word();
            self.write_text(c.get_text());
        } else {
            // Check if font/size/baseline changed - start new word if so
            let baseline_changed = (self.working_bbox.1 - c.bbox().1).abs() > 0.001;
            let font_changed = self.working_font != c.fontname();
            let size_changed = (self.working_size - c.size()).abs() > 0.001;

            if baseline_changed || font_changed || size_changed {
                self.write_word();
                self.within_chars = true;
                self.working_text = c.get_text().to_string();
                self.working_bbox = c.bbox();
                self.working_font = c.fontname().to_string();
                self.working_size = c.size();
            } else {
                // Continue accumulating word
                self.working_text.push_str(c.get_text());
                // Extend bbox to include this character
                self.working_bbox = (
                    self.working_bbox.0,
                    self.working_bbox.1,
                    c.bbox().2,
                    self.working_bbox.3,
                );
            }
        }
    }

    /// Close the converter.
    pub fn close(&mut self) {
        // Write any remaining word
        if self.within_chars {
            self.write_word();
        }
        self.write_footer();
    }
}
