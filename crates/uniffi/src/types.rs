use bolivar_core::layout::{
    LTItem, LTPage, LTTextBox as CoreLTTextBox, LTTextLine as CoreLTTextLine, LTTextLineHorizontal,
    LTTextLineVertical, TextBoxType,
};
use bolivar_core::layout::{ReconstructedLine, TextLineElement, TextLineType};
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
pub struct RawTableBoundingBox {
    pub x0: f64,
    pub top: f64,
    pub x1: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawCharacter {
    pub text: String,
    pub bbox: BoundingBox,
    pub font_name: String,
    pub size: f64,
    pub upright: bool,
    pub advance: f64,
    pub matrix: Vec<f64>,
    pub marked_content_id: Option<i32>,
    pub tag: Option<String>,
    pub non_stroking_color_space: Option<String>,
    pub stroking_color_space: Option<String>,
    pub non_stroking_color: Option<Vec<f64>>,
    pub stroking_color: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawTextLine {
    pub bbox: BoundingBox,
    pub orientation: String,
    pub raw_text: String,
    pub text: String,
    pub characters: Vec<RawCharacter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawTextBox {
    pub bbox: BoundingBox,
    pub writing_mode: String,
    pub text: String,
    pub lines: Vec<RawTextLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawTableCell {
    pub row_index: u32,
    pub column_index: u32,
    pub row_span: u32,
    pub column_span: u32,
    pub bbox: RawTableBoundingBox,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawTable {
    pub bbox: RawTableBoundingBox,
    pub row_count: u32,
    pub column_count: u32,
    pub cells: Vec<RawTableCell>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawPageBoxes {
    pub media: Option<Vec<f64>>,
    pub crop: Option<Vec<f64>>,
    pub bleed: Option<Vec<f64>>,
    pub trim: Option<Vec<f64>>,
    pub art: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawPage {
    pub page_index: u32,
    pub page_number: u32,
    pub object_id: u32,
    pub label: Option<String>,
    pub rotation: i64,
    pub user_unit: f64,
    pub boxes: RawPageBoxes,
    pub layout_bbox: BoundingBox,
    pub text: String,
    pub text_boxes: Vec<RawTextBox>,
    pub tables: Vec<RawTable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawDocument {
    pub declared_page_count: u32,
    pub page_count: u32,
    pub pages: Vec<RawPage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetadataEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfVersion {
    pub header: Option<String>,
    pub catalog: Option<String>,
    pub effective: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfPermissions {
    pub printable: bool,
    pub modifiable: bool,
    pub extractable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawDocumentMetadata {
    pub document_info: Vec<MetadataEntry>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date_raw: Option<String>,
    pub creation_date_iso: Option<String>,
    pub modification_date_raw: Option<String>,
    pub modification_date_iso: Option<String>,
    pub version: PdfVersion,
    pub file_size_bytes: u64,
    pub page_count: u32,
    pub encrypted: bool,
    pub permissions: PdfPermissions,
    pub linearized: bool,
    pub tagged: bool,
    pub user_properties: bool,
    pub suspects: bool,
    pub form: String,
    pub has_javascript: bool,
    pub has_metadata_stream: bool,
    pub xmp_metadata: Option<String>,
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
    pub bidi: Option<bool>,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            password: None,
            page_numbers: None,
            max_pages: None,
            caching: Some(true),
            layout_params: None,
            bidi: Some(false),
        }
    }
}

/// Raw table rows for one page, exactly as the pdfplumber-compatible rows
/// pipeline emits them (None = empty cell). This is the same core path the
/// Python binding's `_extract_tables_stream` uses.
#[derive(Debug, Clone, PartialEq)]
pub struct PageTableRows {
    pub page_number: u32,
    pub tables: Vec<Vec<Vec<Option<String>>>>,
}

/// Table extraction tuning mirroring the pdfplumber-compatible settings the
/// Python binding accepts. General tolerances fan out into both axes; the
/// axis-specific values override them. Crops are pdfplumber-space page
/// regions; `first_page_crop` wins over `crop` on page one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableOptions {
    pub vertical_strategy: Option<String>,
    pub horizontal_strategy: Option<String>,
    pub snap_tolerance: Option<f64>,
    pub snap_x_tolerance: Option<f64>,
    pub snap_y_tolerance: Option<f64>,
    pub join_tolerance: Option<f64>,
    pub join_x_tolerance: Option<f64>,
    pub join_y_tolerance: Option<f64>,
    pub intersection_tolerance: Option<f64>,
    pub intersection_x_tolerance: Option<f64>,
    pub intersection_y_tolerance: Option<f64>,
    pub explicit_vertical_lines: Option<Vec<f64>>,
    pub explicit_horizontal_lines: Option<Vec<f64>>,
    pub crop: Option<BoundingBox>,
    pub first_page_crop: Option<BoundingBox>,
    pub max_pages: Option<u32>,
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

fn layout_chars(
    elements: &[&TextLineElement],
    reconstructed: &ReconstructedLine,
) -> Vec<LayoutChar> {
    let mut chars = Vec::new();
    for span in &reconstructed.spans {
        if let Some(TextLineElement::Char(ch)) = elements.get(span.source_index).copied() {
            chars.push(LayoutChar {
                text: span.text.clone(),
                bbox: bbox_from_rect((ch.x0(), ch.y0(), ch.x1(), ch.y1())),
                font_name: ch.fontname().to_string(),
                size: ch.size(),
                upright: ch.upright(),
            });
        }
    }
    chars
}

fn line_text_chars_from_horizontal(line: &LTTextLineHorizontal) -> (String, Vec<LayoutChar>) {
    if !line.bidi() {
        return (line.get_text(), layout_chars_from_source(line.iter()));
    }
    let elements = line.iter().collect::<Vec<_>>();
    let reconstructed = line.reconstructed();
    (
        reconstructed.text.clone(),
        layout_chars(&elements, &reconstructed),
    )
}

fn line_text_chars_from_vertical(line: &LTTextLineVertical) -> (String, Vec<LayoutChar>) {
    if !line.bidi() {
        return (line.get_text(), layout_chars_from_source(line.iter()));
    }
    let elements = line.iter().collect::<Vec<_>>();
    let reconstructed = line.reconstructed();
    (
        reconstructed.text.clone(),
        layout_chars(&elements, &reconstructed),
    )
}

fn layout_chars_from_source<'a>(
    elements: impl Iterator<Item = &'a TextLineElement>,
) -> Vec<LayoutChar> {
    elements
        .filter_map(|element| match element {
            TextLineElement::Char(character) => Some(LayoutChar {
                text: character.get_text().to_owned(),
                bbox: bbox_from_rect(character.bbox()),
                font_name: character.fontname().to_owned(),
                size: character.size(),
                upright: character.upright(),
            }),
            TextLineElement::Anno(_) => None,
        })
        .collect()
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

pub(crate) fn raw_character_from_ltchar(character: &bolivar_core::layout::LTChar) -> RawCharacter {
    let matrix = character.matrix();
    RawCharacter {
        text: character.get_text().to_owned(),
        bbox: bbox_from_rect(character.bbox()),
        font_name: character.fontname().to_owned(),
        size: character.size(),
        upright: character.upright(),
        advance: character.adv(),
        matrix: vec![matrix.0, matrix.1, matrix.2, matrix.3, matrix.4, matrix.5],
        marked_content_id: character.mcid(),
        tag: character.tag(),
        non_stroking_color_space: character.ncs(),
        stroking_color_space: character.scs(),
        non_stroking_color: character.non_stroking_color().clone(),
        stroking_color: character.stroking_color().clone(),
    }
}

fn raw_characters<'a>(elements: impl Iterator<Item = &'a TextLineElement>) -> Vec<RawCharacter> {
    elements
        .filter_map(|element| match element {
            TextLineElement::Char(character) => Some(raw_character_from_ltchar(character)),
            TextLineElement::Anno(_) => None,
        })
        .collect()
}

fn raw_text_line_from_horizontal(line: &LTTextLineHorizontal) -> RawTextLine {
    RawTextLine {
        bbox: bbox_from_rect(line.bbox()),
        orientation: "horizontal".to_owned(),
        raw_text: source_text(line.iter()),
        text: line.get_text(),
        characters: raw_characters(line.iter()),
    }
}

fn raw_text_line_from_vertical(line: &LTTextLineVertical) -> RawTextLine {
    RawTextLine {
        bbox: bbox_from_rect(line.bbox()),
        orientation: "vertical".to_owned(),
        raw_text: source_text(line.iter()),
        text: line.get_text(),
        characters: raw_characters(line.iter()),
    }
}

fn source_text<'a>(elements: impl Iterator<Item = &'a TextLineElement>) -> String {
    elements
        .map(|element| match element {
            TextLineElement::Char(character) => character.get_text(),
            TextLineElement::Anno(annotation) => annotation.get_text(),
        })
        .collect()
}

fn raw_text_box_from_text_box_type(text_box: &TextBoxType) -> RawTextBox {
    match text_box {
        TextBoxType::Horizontal(text_box) => RawTextBox {
            bbox: bbox_from_rect(text_box.bbox()),
            writing_mode: text_box.get_writing_mode().to_owned(),
            text: text_box.get_text(),
            lines: text_box.iter().map(raw_text_line_from_horizontal).collect(),
        },
        TextBoxType::Vertical(text_box) => RawTextBox {
            bbox: bbox_from_rect(text_box.bbox()),
            writing_mode: text_box.get_writing_mode().to_owned(),
            text: text_box.get_text(),
            lines: text_box.iter().map(raw_text_line_from_vertical).collect(),
        },
    }
}

fn collect_raw_text_boxes(item: &LTItem, output: &mut Vec<RawTextBox>) {
    match item {
        LTItem::TextBox(text_box) => output.push(raw_text_box_from_text_box_type(text_box)),
        LTItem::Figure(figure) => {
            for child in figure.iter() {
                collect_raw_text_boxes(child, output);
            }
        }
        LTItem::Page(page) => {
            for child in page.iter() {
                collect_raw_text_boxes(child, output);
            }
        }
        _ => {}
    }
}

fn raw_text_boxes(page: &LTPage) -> Vec<RawTextBox> {
    let mut output = Vec::new();
    for item in page.iter() {
        collect_raw_text_boxes(item, &mut output);
    }
    output
}

fn raw_table_bbox(bbox: CoreTableBBox) -> RawTableBoundingBox {
    RawTableBoundingBox {
        x0: bbox.x0,
        top: bbox.top,
        x1: bbox.x1,
        bottom: bbox.bottom,
    }
}

fn raw_table_from_core(table: CoreTableMetadata) -> RawTable {
    RawTable {
        bbox: raw_table_bbox(table.bbox),
        row_count: usize_to_u32(table.row_count),
        column_count: usize_to_u32(table.column_count),
        cells: table
            .cells
            .into_iter()
            .map(|cell| RawTableCell {
                row_index: usize_to_u32(cell.row_index),
                column_index: usize_to_u32(cell.column_index),
                row_span: usize_to_u32(cell.row_span),
                column_span: usize_to_u32(cell.column_span),
                bbox: raw_table_bbox(cell.bbox),
                text: cell.text,
            })
            .collect(),
    }
}

fn raw_page_boxes(page: &PDFPage) -> RawPageBoxes {
    RawPageBoxes {
        media: page.mediabox.map(Vec::from),
        crop: page.cropbox.map(Vec::from),
        bleed: page.bleedbox.map(Vec::from),
        trim: page.trimbox.map(Vec::from),
        art: page.artbox.map(Vec::from),
    }
}

pub(crate) fn raw_page_from_parts(
    page_index: usize,
    pdf_page: &PDFPage,
    layout_page: LTPage,
    tables: Vec<CoreTableMetadata>,
) -> RawPage {
    let layout_bbox = bbox_from_rect(layout_page.bbox());
    let text_boxes = raw_text_boxes(&layout_page);
    let text = text_boxes
        .iter()
        .map(|text_box| text_box.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    RawPage {
        page_index: usize_to_u32(page_index),
        page_number: usize_to_u32(page_index.saturating_add(1)),
        object_id: pdf_page.pageid,
        label: pdf_page.label.clone(),
        rotation: pdf_page.rotate,
        user_unit: pdf_page.user_unit,
        boxes: raw_page_boxes(pdf_page),
        layout_bbox,
        text,
        text_boxes,
        tables: tables.into_iter().map(raw_table_from_core).collect(),
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
    // Layout space is post-rotation (pdfplumber semantics): 90/270 pages swap
    // their box axes so crops and pdf-space conversions line up with the
    // coordinates layout emits.
    let rotated = |b: (f64, f64, f64, f64)| -> (f64, f64, f64, f64) {
        match page.rotate.rem_euclid(360) {
            90 | 270 => (b.1, b.0, b.3, b.2),
            _ => b,
        }
    };
    let raw_mediabox = normalize_rect_from_box(page.mediabox.unwrap_or([0.0, 0.0, 0.0, 0.0]));
    let raw_page_bbox = normalize_rect_from_box(page.cropbox.unwrap_or([
        raw_mediabox.0,
        raw_mediabox.1,
        raw_mediabox.2,
        raw_mediabox.3,
    ]));
    let mediabox = rotated(raw_mediabox);
    let page_bbox = rotated(raw_page_bbox);
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
    fn layout_line_text_uses_icu_mapping_when_enabled() {
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
        line.set_bidi(true);

        let raw_line = raw_text_line_from_horizontal(&line);
        let layout_line = layout_line_from_textline(&TextLineType::Horizontal(line));
        assert_eq!(layout_line.text, "كشف الحساب\n");
        assert_eq!(
            layout_line
                .chars
                .iter()
                .map(|character| character.text.as_str())
                .collect::<String>(),
            "كشف الحساب"
        );
        assert_eq!(raw_line.raw_text, "ﺏﺎﺴﺤﻟﺍ ﻒﺸﻛ\n");
        assert_eq!(raw_line.text, "كشف الحساب\n");
        assert_eq!(raw_line.characters[0].text, "ﺏ");
    }

    #[test]
    fn page_number_clamps_non_positive_to_one() {
        assert_eq!(page_number(0), 1);
        assert_eq!(page_number(-5), 1);
    }

    #[test]
    fn raw_character_preserves_matrix_marked_content_and_colors() {
        let character = LTChar::builder((1.0, 2.0, 3.0, 4.0), "A", "Helvetica", 12.0)
            .upright(true)
            .adv(7.5)
            .matrix((1.0, 0.1, 0.2, 1.0, 20.0, 30.0))
            .mcid(Some(17))
            .tag(Some("Span".to_owned()))
            .ncs(Some("DeviceRGB".to_owned()))
            .scs(Some("DeviceGray".to_owned()))
            .non_stroking_color(Some(vec![0.1, 0.2, 0.3]))
            .stroking_color(Some(vec![0.4]))
            .build();

        let raw = raw_character_from_ltchar(&character);

        assert_eq!(raw.text, "A");
        assert_eq!(raw.advance, 7.5);
        assert_eq!(raw.matrix, vec![1.0, 0.1, 0.2, 1.0, 20.0, 30.0]);
        assert_eq!(raw.marked_content_id, Some(17));
        assert_eq!(raw.tag.as_deref(), Some("Span"));
        assert_eq!(raw.non_stroking_color_space.as_deref(), Some("DeviceRGB"));
        assert_eq!(raw.stroking_color_space.as_deref(), Some("DeviceGray"));
        assert_eq!(raw.non_stroking_color, Some(vec![0.1, 0.2, 0.3]));
        assert_eq!(raw.stroking_color, Some(vec![0.4]));
    }

    #[test]
    fn raw_page_preserves_pdf_identity_boxes_and_table_coordinates() {
        let page = PDFPage {
            pageid: 42,
            attrs: PDFDict::default(),
            label: Some("iv".to_owned()),
            mediabox: Some([0.0, 0.0, 200.0, 300.0]),
            cropbox: Some([10.0, 20.0, 190.0, 280.0]),
            bleedbox: Some([5.0, 6.0, 195.0, 294.0]),
            trimbox: Some([7.0, 8.0, 193.0, 292.0]),
            artbox: Some([9.0, 10.0, 191.0, 290.0]),
            rotate: 90,
            annots: None,
            resources: PDFDict::default(),
            contents: Vec::new(),
            user_unit: 2.0,
        };
        let layout_page = LTPage::new(1, (0.0, 0.0, 300.0, 200.0), 90.0);
        let table = CoreTableMetadata {
            bbox: CoreTableBBox {
                x0: 11.0,
                top: 22.0,
                x1: 99.0,
                bottom: 88.0,
            },
            row_count: 1,
            column_count: 1,
            cells: vec![CoreTableCellMetadata {
                row_index: 0,
                column_index: 0,
                row_span: 1,
                column_span: 1,
                bbox: CoreTableBBox {
                    x0: 11.0,
                    top: 22.0,
                    x1: 99.0,
                    bottom: 88.0,
                },
                text: "value".to_owned(),
            }],
        };

        let raw = raw_page_from_parts(3, &page, layout_page, vec![table]);

        assert_eq!(raw.page_index, 3);
        assert_eq!(raw.page_number, 4);
        assert_eq!(raw.object_id, 42);
        assert_eq!(raw.label.as_deref(), Some("iv"));
        assert_eq!(raw.rotation, 90);
        assert_eq!(raw.user_unit, 2.0);
        assert_eq!(raw.boxes.media, Some(vec![0.0, 0.0, 200.0, 300.0]));
        assert_eq!(raw.boxes.crop, Some(vec![10.0, 20.0, 190.0, 280.0]));
        assert_eq!(raw.tables[0].bbox.top, 22.0);
        assert_eq!(raw.tables[0].bbox.bottom, 88.0);
        assert_eq!(raw.tables[0].cells[0].bbox.top, 22.0);
    }
}
