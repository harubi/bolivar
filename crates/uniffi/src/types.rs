use bolivar_core::layout::{
    LTItem, LTPage, LTTextBox as CoreLTTextBox, LTTextLine, LTTextLineHorizontal,
    LTTextLineVertical, TextBoxType,
};
use bolivar_core::layout::{TextLineElement, TextLineType};
use bolivar_core::pdfdocument::DEFAULT_CACHE_CAPACITY;
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::table::{
    BBox as CoreTableBBox, PageGeometry, TableCellMetadata as CoreTableCellMetadata,
    TableMetadata as CoreTableMetadata,
};
use bolivar_core::utils::HasBBox;

#[derive(Debug, Clone, PartialEq)]
pub struct BoundingBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageSummary {
    pub page_number: u32,
    pub text: String,
    pub bbox: BoundingBox,
    pub rotate: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutChar {
    pub text: String,
    pub bbox: BoundingBox,
    pub font_name: String,
    pub size: f64,
    pub upright: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutLine {
    pub bbox: BoundingBox,
    pub orientation: String,
    pub text: String,
    pub chars: Vec<LayoutChar>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTextBox {
    pub bbox: BoundingBox,
    pub writing_mode: String,
    pub text: String,
    pub lines: Vec<LayoutLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutPage {
    pub page_number: u32,
    pub bbox: BoundingBox,
    pub rotate: f64,
    pub text: String,
    pub text_boxes: Vec<LayoutTextBox>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub row_index: u32,
    pub column_index: u32,
    pub row_span: u32,
    pub column_span: u32,
    pub bbox: BoundingBox,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub page_number: u32,
    pub bbox: BoundingBox,
    pub row_count: u32,
    pub column_count: u32,
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutParams {
    pub line_overlap: Option<f64>,
    pub char_margin: Option<f64>,
    pub line_margin: Option<f64>,
    pub word_margin: Option<f64>,
    pub boxes_flow: Option<f64>,
    pub detect_vertical: Option<bool>,
    pub all_texts: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractOptions {
    pub password: Option<String>,
    pub page_numbers: Option<Vec<u32>>,
    pub max_pages: Option<u32>,
    pub caching: Option<bool>,
    pub layout_params: Option<LayoutParams>,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            password: None,
            page_numbers: None,
            max_pages: None,
            caching: Some(true),
            layout_params: None,
        }
    }
}

pub(crate) fn bbox_from_rect(rect: (f64, f64, f64, f64)) -> BoundingBox {
    BoundingBox {
        x0: rect.0,
        y0: rect.1,
        x1: rect.2,
        y1: rect.3,
    }
}

pub(crate) fn bbox_from_table_bbox_in_pdf_space(
    bbox: CoreTableBBox,
    geometry: &PageGeometry,
) -> BoundingBox {
    let page_top = geometry.mediabox.3;
    let raw_y0 = page_top - bbox.bottom;
    let raw_y1 = page_top - bbox.top;
    let (y0, y1) = if raw_y0 <= raw_y1 {
        (raw_y0, raw_y1)
    } else {
        (raw_y1, raw_y0)
    };
    BoundingBox {
        x0: bbox.x0,
        y0,
        x1: bbox.x1,
        y1,
    }
}

pub(crate) fn page_number(pageid: i32) -> u32 {
    match u32::try_from(pageid) {
        Ok(0) | Err(_) => 1,
        Ok(page_number) => page_number,
    }
}

pub(crate) fn usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn line_text_chars_from_horizontal(line: &LTTextLineHorizontal) -> (String, Vec<LayoutChar>) {
    let mut chars = Vec::new();
    for element in line.iter() {
        match element {
            TextLineElement::Char(ch) => {
                chars.push(LayoutChar {
                    text: ch.get_text().to_string(),
                    bbox: bbox_from_rect((ch.x0(), ch.y0(), ch.x1(), ch.y1())),
                    font_name: ch.fontname().to_string(),
                    size: ch.size(),
                    upright: ch.upright(),
                });
            }
            TextLineElement::Anno(_) => {}
        }
    }
    (line.get_text(), chars)
}

fn line_text_chars_from_vertical(line: &LTTextLineVertical) -> (String, Vec<LayoutChar>) {
    let mut chars = Vec::new();
    for element in line.iter() {
        match element {
            TextLineElement::Char(ch) => {
                chars.push(LayoutChar {
                    text: ch.get_text().to_string(),
                    bbox: bbox_from_rect((ch.x0(), ch.y0(), ch.x1(), ch.y1())),
                    font_name: ch.fontname().to_string(),
                    size: ch.size(),
                    upright: ch.upright(),
                });
            }
            TextLineElement::Anno(_) => {}
        }
    }
    (line.get_text(), chars)
}

pub(crate) fn layout_line_from_textline(textline: &TextLineType) -> LayoutLine {
    match textline {
        TextLineType::Horizontal(line) => {
            let (text, chars) = line_text_chars_from_horizontal(line);
            LayoutLine {
                bbox: bbox_from_rect(line.bbox()),
                orientation: "horizontal".to_string(),
                text,
                chars,
            }
        }
        TextLineType::Vertical(line) => {
            let (text, chars) = line_text_chars_from_vertical(line);
            LayoutLine {
                bbox: bbox_from_rect(line.bbox()),
                orientation: "vertical".to_string(),
                text,
                chars,
            }
        }
    }
}

fn layout_text_box_from_text_box_type(text_box: &TextBoxType) -> LayoutTextBox {
    match text_box {
        TextBoxType::Horizontal(b) => {
            let mut lines = Vec::new();
            for line in b.iter() {
                lines.push(layout_line_from_textline(&TextLineType::Horizontal(
                    line.clone(),
                )));
            }
            LayoutTextBox {
                bbox: bbox_from_rect(b.bbox()),
                writing_mode: "lr-tb".to_string(),
                text: b.get_text(),
                lines,
            }
        }
        TextBoxType::Vertical(b) => {
            let mut lines = Vec::new();
            for line in b.iter() {
                lines.push(layout_line_from_textline(&TextLineType::Vertical(
                    line.clone(),
                )));
            }
            LayoutTextBox {
                bbox: bbox_from_rect(b.bbox()),
                writing_mode: "tb-rl".to_string(),
                text: b.get_text(),
                lines,
            }
        }
    }
}

fn collect_layout_text_boxes(item: &LTItem, out: &mut Vec<LayoutTextBox>) {
    match item {
        LTItem::TextBox(text_box) => out.push(layout_text_box_from_text_box_type(text_box)),
        LTItem::Figure(figure) => {
            for child in figure.iter() {
                collect_layout_text_boxes(child, out);
            }
        }
        LTItem::Page(page) => {
            for child in page.iter() {
                collect_layout_text_boxes(child, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn layout_page_from_ltpage(page: &LTPage) -> LayoutPage {
    let mut text_boxes = Vec::new();
    for item in page.iter() {
        collect_layout_text_boxes(item, &mut text_boxes);
    }
    let text = text_boxes
        .iter()
        .map(|text_box| text_box.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    LayoutPage {
        page_number: page_number(page.pageid),
        bbox: bbox_from_rect(page.bbox()),
        rotate: page.rotate,
        text,
        text_boxes,
    }
}

fn table_cell_from_core(cell: CoreTableCellMetadata, geometry: &PageGeometry) -> TableCell {
    TableCell {
        row_index: usize_to_u32(cell.row_index),
        column_index: usize_to_u32(cell.column_index),
        row_span: usize_to_u32(cell.row_span),
        column_span: usize_to_u32(cell.column_span),
        bbox: bbox_from_table_bbox_in_pdf_space(cell.bbox, geometry),
        text: cell.text,
    }
}

pub(crate) fn table_from_core(
    page_number: u32,
    table: CoreTableMetadata,
    geometry: &PageGeometry,
) -> Table {
    Table {
        page_number,
        bbox: bbox_from_table_bbox_in_pdf_space(table.bbox, geometry),
        row_count: usize_to_u32(table.row_count),
        column_count: usize_to_u32(table.column_count),
        cells: table
            .cells
            .into_iter()
            .map(|cell| table_cell_from_core(cell, geometry))
            .collect(),
    }
}

fn normalize_rect_from_box(rect: [f64; 4]) -> (f64, f64, f64, f64) {
    let x0 = rect[0].min(rect[2]);
    let x1 = rect[0].max(rect[2]);
    let y0 = rect[1].min(rect[3]);
    let y1 = rect[1].max(rect[3]);
    (x0, y0, x1, y1)
}

pub(crate) fn page_geometry_from_pdf_page(page: &PDFPage) -> PageGeometry {
    let mediabox = normalize_rect_from_box(page.mediabox.unwrap_or([0.0, 0.0, 0.0, 0.0]));
    let page_bbox = normalize_rect_from_box(
        page.cropbox
            .unwrap_or([mediabox.0, mediabox.1, mediabox.2, mediabox.3]),
    );
    PageGeometry {
        page_bbox,
        mediabox,
        initial_doctop: 0.0,
        force_crop: page_bbox != mediabox,
    }
}

pub(crate) fn cache_capacity(caching: bool) -> usize {
    if caching { DEFAULT_CACHE_CAPACITY } else { 0 }
}

pub(crate) fn summary_from_layout_page(layout_page: LayoutPage) -> PageSummary {
    PageSummary {
        page_number: layout_page.page_number,
        text: layout_page.text,
        bbox: layout_page.bbox,
        rotate: layout_page.rotate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bolivar_core::layout::LTChar;
    use bolivar_core::pdftypes::PDFDict;

    #[test]
    fn page_geometry_uses_cropbox_and_mediabox_from_pdf_page() {
        let page = PDFPage {
            pageid: 1,
            attrs: PDFDict::default(),
            label: None,
            mediabox: Some([0.0, 0.0, 200.0, 200.0]),
            cropbox: Some([50.0, 50.0, 150.0, 150.0]),
            bleedbox: None,
            trimbox: None,
            artbox: None,
            rotate: 0,
            annots: None,
            resources: PDFDict::default(),
            contents: Vec::new(),
            user_unit: 1.0,
        };

        let geometry = page_geometry_from_pdf_page(&page);
        assert_eq!(geometry.mediabox, (0.0, 0.0, 200.0, 200.0));
        assert_eq!(geometry.page_bbox, (50.0, 50.0, 150.0, 150.0));
    }

    #[test]
    fn table_bbox_conversion_uses_layout_coordinate_convention() {
        let geometry = PageGeometry {
            page_bbox: (50.0, 50.0, 150.0, 150.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let table_bbox = CoreTableBBox {
            x0: 10.0,
            top: 20.0,
            x1: 40.0,
            bottom: 80.0,
        };
        let converted = bbox_from_table_bbox_in_pdf_space(table_bbox, &geometry);
        assert_eq!(converted.x0, 10.0);
        assert_eq!(converted.x1, 40.0);
        assert_eq!(converted.y0, 120.0);
        assert_eq!(converted.y1, 180.0);
    }

    #[test]
    fn table_bbox_conversion_normalizes_inverted_vertical_bounds() {
        let geometry = PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let malformed_bbox = CoreTableBBox {
            x0: 10.0,
            top: 120.0,
            x1: 20.0,
            bottom: 40.0,
        };

        let converted = bbox_from_table_bbox_in_pdf_space(malformed_bbox, &geometry);
        assert_eq!(converted.x0, 10.0);
        assert_eq!(converted.y0, 80.0);
        assert_eq!(converted.x1, 20.0);
        assert_eq!(converted.y1, 160.0);
    }

    #[test]
    fn layout_line_text_normalizes_arabic_presentation_forms() {
        let mut line = LTTextLineHorizontal::new(0.1);
        let visual = ["ﺏ", "ﺎ", "ﺴ", "ﺤ", "ﻟ", "ﺍ", " ", "ﻒ", "ﺸ", "ﻛ"];
        for (idx, glyph) in visual.into_iter().enumerate() {
            line.add_element(TextLineElement::Char(Box::new(LTChar::new(
                (idx as f64, 0.0, idx as f64 + 1.0, 1.0),
                glyph,
                "F",
                10.0,
                true,
                1.0,
            ))));
        }
        line.analyze();

        let layout_line = layout_line_from_textline(&TextLineType::Horizontal(line));
        assert_eq!(layout_line.text, "كشف الحساب\n");
    }

    #[test]
    fn page_number_clamps_non_positive_to_one() {
        assert_eq!(page_number(0), 1);
        assert_eq!(page_number(-5), 1);
    }
}
