//! XML Converter - outputs XML with full structure.
//!
//! Port of XMLConverter from pdfminer.six converter.py

use regex::Regex;
use std::io::Write;

use crate::layout::{
    LAParams, LTItem, LTPage, LTTextBox, LTTextGroup, TextBoxType, TextGroupElement,
    TextLineElement, TextLineType,
};
use crate::utils::{HasBBox, bbox2str, enc};

/// XML Converter - outputs XML with full structure.
///
/// Port of XMLConverter from pdfminer.six converter.py
pub struct XMLConverter<W: Write> {
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
}

impl<W: Write> XMLConverter<W> {
    /// Create a new XML converter.
    pub fn new(outfp: W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
        let mut converter = Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            stripcontrol: false,
            control_re: Regex::new(r"[\x00-\x08\x0b-\x0c\x0e-\x1f]").unwrap(),
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

    /// Set whether to strip control characters.
    pub const fn set_stripcontrol(&mut self, stripcontrol: bool) {
        self.stripcontrol = stripcontrol;
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
        if !self.codec.is_empty() {
            let decl = format!("<?xml version=\"1.0\" encoding=\"{}\" ?>\n", self.codec);
            self.write(&decl);
        } else {
            self.write("<?xml version=\"1.0\" ?>\n");
        }
        self.write("<pages>\n");
    }

    /// Write footer.
    fn write_footer(&mut self) {
        self.write("</pages>\n");
    }

    /// Write text with encoding and control character handling.
    pub fn write_text(&mut self, text: &str) {
        let text = if self.stripcontrol {
            self.control_re.replace_all(text, "").to_string()
        } else {
            text.to_string()
        };
        self.write(&enc(&text));
    }

    /// Receive and render a layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        let page_xml = format!(
            "<page id=\"{}\" bbox=\"{}\" rotate=\"{}\">\n",
            ltpage.pageid,
            bbox2str(ltpage.bbox()),
            ltpage.rotate as i32
        );
        self.write(&page_xml);

        // Render page content
        for item in ltpage.iter() {
            self.render_item(item);
        }

        // Render groups if present
        if let Some(groups) = ltpage.groups() {
            self.write("<layout>\n");
            for group in groups {
                self.show_group(group);
            }
            self.write("</layout>\n");
        }

        self.write("</page>\n");
    }

    /// Render a text group.
    fn show_group(&mut self, group: &LTTextGroup) {
        let group_xml = format!("<textgroup bbox=\"{}\">\n", bbox2str(group.bbox()));
        self.write(&group_xml);
        for elem in group.iter() {
            match elem {
                TextGroupElement::Box(tb) => {
                    let (id, bbox) = match tb {
                        TextBoxType::Horizontal(h) => (h.index(), h.bbox()),
                        TextBoxType::Vertical(v) => (v.index(), v.bbox()),
                    };
                    let tb_xml = format!("<textbox id=\"{}\" bbox=\"{}\" />\n", id, bbox2str(bbox));
                    self.write(&tb_xml);
                }
                TextGroupElement::Group(g) => {
                    self.show_group(g);
                }
            }
        }
        self.write("</textgroup>\n");
    }

    /// Render an item.
    fn render_item(&mut self, item: &LTItem) {
        match item {
            LTItem::Line(l) => {
                let xml = format!(
                    "<line linewidth=\"{}\" bbox=\"{}\" />\n",
                    l.linewidth as i32,
                    bbox2str(l.bbox())
                );
                self.write(&xml);
            }
            LTItem::Rect(r) => {
                let xml = format!(
                    "<rect linewidth=\"{}\" bbox=\"{}\" />\n",
                    r.linewidth as i32,
                    bbox2str(r.bbox())
                );
                self.write(&xml);
            }
            LTItem::Curve(c) => {
                let xml = format!(
                    "<curve linewidth=\"{}\" bbox=\"{}\" pts=\"{}\"/>\n",
                    c.linewidth as i32,
                    bbox2str(c.bbox()),
                    c.get_pts()
                );
                self.write(&xml);
            }
            LTItem::Figure(fig) => {
                let xml = format!(
                    "<figure name=\"{}\" bbox=\"{}\">\n",
                    fig.name,
                    bbox2str(fig.bbox())
                );
                self.write(&xml);
                for child in fig.iter() {
                    self.render_item(child);
                }
                self.write("</figure>\n");
            }
            LTItem::TextLine(tl) => {
                let bbox = match tl {
                    TextLineType::Horizontal(h) => h.bbox(),
                    TextLineType::Vertical(v) => v.bbox(),
                };
                let xml = format!("<textline bbox=\"{}\">\n", bbox2str(bbox));
                self.write(&xml);

                // Render children (characters and annotations)
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

                self.write("</textline>\n");
            }
            LTItem::TextBox(tb) => {
                let (id, bbox, wmode) = match tb {
                    TextBoxType::Horizontal(h) => (h.index(), h.bbox(), ""),
                    TextBoxType::Vertical(v) => (v.index(), v.bbox(), " wmode=\"vertical\""),
                };
                let xml = format!(
                    "<textbox id=\"{}\" bbox=\"{}\"{}>\n",
                    id,
                    bbox2str(bbox),
                    wmode
                );
                self.write(&xml);

                // Render children (text lines)
                match tb {
                    TextBoxType::Horizontal(h) => {
                        for line in h.iter() {
                            let line_xml =
                                format!("<textline bbox=\"{}\">\n", bbox2str(line.bbox()));
                            self.write(&line_xml);
                            for elem in line.iter() {
                                self.render_textline_element(elem);
                            }
                            self.write("</textline>\n");
                        }
                    }
                    TextBoxType::Vertical(v) => {
                        for line in v.iter() {
                            let line_xml =
                                format!("<textline bbox=\"{}\">\n", bbox2str(line.bbox()));
                            self.write(&line_xml);
                            for elem in line.iter() {
                                self.render_textline_element(elem);
                            }
                            self.write("</textline>\n");
                        }
                    }
                }

                self.write("</textbox>\n");
            }
            LTItem::Char(c) => {
                let xml = format!(
                    "<text font=\"{}\" bbox=\"{}\" size=\"{:.3}\">",
                    enc(c.fontname()),
                    bbox2str(c.bbox()),
                    c.size()
                );
                self.write(&xml);
                self.write_text(c.get_text());
                self.write("</text>\n");
            }
            LTItem::Anno(a) => {
                self.write("<text>");
                self.write_text(a.get_text());
                self.write("</text>\n");
            }
            LTItem::Image(img) => {
                let xml = format!(
                    "<image width=\"{}\" height=\"{}\" />\n",
                    img.width() as i32,
                    img.height() as i32
                );
                self.write(&xml);
            }
            _ => {}
        }
    }

    /// Render a text line element (char or annotation).
    fn render_textline_element(&mut self, elem: &TextLineElement) {
        match elem {
            TextLineElement::Char(c) => {
                let xml = format!(
                    "<text font=\"{}\" bbox=\"{}\" size=\"{:.3}\">",
                    enc(c.fontname()),
                    bbox2str(c.bbox()),
                    c.size()
                );
                self.write(&xml);
                self.write_text(c.get_text());
                self.write("</text>\n");
            }
            TextLineElement::Anno(a) => {
                self.write("<text>");
                self.write_text(a.get_text());
                self.write("</text>\n");
            }
        }
    }

    /// Close the converter.
    pub fn close(&mut self) {
        self.write_footer();
    }
}
