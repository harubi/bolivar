use bolivar_core::PdfError;
use bolivar_core::api::stream::extract_pages_stream_from_doc;
use bolivar_core::high_level::{
    ExtractOptions as CoreExtractOptions,
    extract_text_with_document as core_extract_text_with_document,
};
use bolivar_core::layout::{
    LAParams as CoreLAParams, LTItem, LTPage, LTTextBox as CoreLTTextBox, LTTextLine,
    LTTextLineHorizontal, LTTextLineVertical, TextBoxType,
};
use bolivar_core::layout::{TextLineElement, TextLineType};
use bolivar_core::pdfdocument::{DEFAULT_CACHE_CAPACITY, PDFDocument};
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::table::{
    BBox as CoreTableBBox, PageGeometry, TableCellMetadata as CoreTableCellMetadata,
    TableMetadata as CoreTableMetadata, TableSettings,
    extract_tables_with_metadata_from_ltpage as core_extract_tables_with_metadata_from_ltpage,
};
use bolivar_core::utils::HasBBox;
use std::io::ErrorKind;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum BolivarError {
    #[error("path is invalid")]
    InvalidPath,
    #[error("invalid argument")]
    InvalidArgument,
    #[error("io not found")]
    IoNotFound,
    #[error("io permission denied")]
    IoPermissionDenied,
    #[error("io error")]
    IoError,
    #[error("syntax error")]
    SyntaxError,
    #[error("encryption error")]
    EncryptionError,
    #[error("pdf error")]
    PdfError,
    #[error("decode error")]
    DecodeError,
    #[error("runtime error")]
    RuntimeError,
}

fn map_io_error_kind(kind: ErrorKind) -> BolivarError {
    match kind {
        ErrorKind::NotFound => BolivarError::IoNotFound,
        ErrorKind::PermissionDenied => BolivarError::IoPermissionDenied,
        _ => BolivarError::IoError,
    }
}

impl From<PdfError> for BolivarError {
    fn from(value: PdfError) -> Self {
        match value {
            PdfError::Io(err) => map_io_error_kind(err.kind()),
            PdfError::DecodeError(_) => Self::DecodeError,
            PdfError::SyntaxError(_) => Self::SyntaxError,
            PdfError::InvalidArgument(_) => Self::InvalidArgument,
            PdfError::EncryptionError(_) => Self::EncryptionError,
            _ => Self::PdfError,
        }
    }
}

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

fn normalize_page_numbers(
    page_numbers: Option<Vec<u32>>,
) -> Result<Option<Vec<usize>>, BolivarError> {
    let Some(page_numbers) = page_numbers else {
        return Ok(None);
    };

    let mut normalized = Vec::with_capacity(page_numbers.len());
    for page in page_numbers {
        if page == 0 {
            return Err(BolivarError::InvalidArgument);
        }
        let zero_based = page - 1;
        let index = usize::try_from(zero_based).map_err(|_| BolivarError::InvalidArgument)?;
        normalized.push(index);
    }

    Ok(Some(normalized))
}

fn normalize_max_pages(max_pages: Option<u32>) -> Result<usize, BolivarError> {
    let max_pages = max_pages.unwrap_or(0);
    usize::try_from(max_pages).map_err(|_| BolivarError::InvalidArgument)
}

fn extract_password(options: &Option<ExtractOptions>) -> String {
    options
        .as_ref()
        .and_then(|value| value.password.clone())
        .unwrap_or_default()
}

fn extract_caching(options: &Option<ExtractOptions>) -> bool {
    options
        .as_ref()
        .and_then(|value| value.caching)
        .unwrap_or(true)
}

fn normalize_layout_params(
    layout_params: Option<LayoutParams>,
) -> Result<Option<CoreLAParams>, BolivarError> {
    let Some(layout_params) = layout_params else {
        return Ok(None);
    };

    let defaults = CoreLAParams::default();
    let boxes_flow = layout_params.boxes_flow.or(defaults.boxes_flow);
    if let Some(flow) = boxes_flow
        && !(-1.0..=1.0).contains(&flow)
    {
        return Err(BolivarError::InvalidArgument);
    }

    Ok(Some(CoreLAParams {
        line_overlap: layout_params.line_overlap.unwrap_or(defaults.line_overlap),
        char_margin: layout_params.char_margin.unwrap_or(defaults.char_margin),
        line_margin: layout_params.line_margin.unwrap_or(defaults.line_margin),
        word_margin: layout_params.word_margin.unwrap_or(defaults.word_margin),
        boxes_flow,
        detect_vertical: layout_params
            .detect_vertical
            .unwrap_or(defaults.detect_vertical),
        all_texts: layout_params.all_texts.unwrap_or(defaults.all_texts),
    }))
}

fn core_extract_options(
    options: Option<ExtractOptions>,
) -> Result<CoreExtractOptions, BolivarError> {
    let options = options.unwrap_or_default();
    Ok(CoreExtractOptions {
        password: options.password.unwrap_or_default(),
        page_numbers: normalize_page_numbers(options.page_numbers)?,
        maxpages: normalize_max_pages(options.max_pages)?,
        caching: options.caching.unwrap_or(true),
        laparams: normalize_layout_params(options.layout_params)?,
    })
}

fn validate_input_path(path: &str) -> Result<(), BolivarError> {
    if path.trim().is_empty() || path.contains('\0') || path.contains("://") {
        return Err(BolivarError::InvalidPath);
    }
    Ok(())
}

fn read_pdf_bytes(path: String) -> Result<Vec<u8>, BolivarError> {
    validate_input_path(&path)?;
    std::fs::read(path).map_err(|err| map_io_error_kind(err.kind()))
}

fn bbox_from_rect(rect: (f64, f64, f64, f64)) -> BoundingBox {
    BoundingBox {
        x0: rect.0,
        y0: rect.1,
        x1: rect.2,
        y1: rect.3,
    }
}

fn bbox_from_table_bbox_in_pdf_space(bbox: CoreTableBBox, geometry: &PageGeometry) -> BoundingBox {
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

fn page_number(pageid: i32) -> u32 {
    match u32::try_from(pageid) {
        Ok(0) | Err(_) => 1,
        Ok(page_number) => page_number,
    }
}

fn usize_to_u32(value: usize) -> u32 {
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

fn layout_line_from_textline(textline: &TextLineType) -> LayoutLine {
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

fn layout_page_from_ltpage(page: &LTPage) -> LayoutPage {
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

fn extract_layout_pages_core(
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
) -> Result<Vec<LayoutPage>, BolivarError> {
    let pages_iter = extract_pages_stream_from_doc(doc, options).map_err(BolivarError::from)?;
    let mut pages = Vec::new();
    for page_result in pages_iter {
        pages.push(layout_page_from_ltpage(
            &page_result.map_err(BolivarError::from)?,
        ));
    }
    Ok(pages)
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

fn table_from_core(page_number: u32, table: CoreTableMetadata, geometry: &PageGeometry) -> Table {
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

fn page_geometry_from_pdf_page(page: &PDFPage) -> PageGeometry {
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

fn cache_capacity(caching: bool) -> usize {
    if caching { DEFAULT_CACHE_CAPACITY } else { 0 }
}

fn selected_page_indices(doc: &PDFDocument, options: &CoreExtractOptions) -> Vec<usize> {
    let mut selected_indices = Vec::new();
    let mut selected = 0usize;
    for page_idx in 0..doc.page_tree_len() {
        if let Some(ref nums) = options.page_numbers
            && !nums.contains(&page_idx)
        {
            continue;
        }
        if options.maxpages > 0 && selected >= options.maxpages {
            break;
        }
        selected_indices.push(page_idx);
        selected += 1;
    }
    selected_indices
}

fn extract_tables_core(
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
) -> Result<Vec<Table>, BolivarError> {
    let selected_indices = selected_page_indices(doc.as_ref(), &options);
    let mut pages = extract_pages_stream_from_doc(Arc::clone(&doc), options.clone())
        .map_err(BolivarError::from)?;

    let settings = TableSettings::default();

    let mut tables = Vec::new();
    for page_idx in selected_indices {
        let page = pages
            .next()
            .ok_or(BolivarError::RuntimeError)?
            .map_err(BolivarError::from)?;
        let pdf_page = doc.get_page_cached(page_idx).map_err(BolivarError::from)?;
        let page_num = page_number(page.pageid);
        let geometry = page_geometry_from_pdf_page(pdf_page.as_ref());
        let page_tables =
            core_extract_tables_with_metadata_from_ltpage(&page, &geometry, &settings);
        tables.extend(
            page_tables
                .into_iter()
                .map(|table| table_from_core(page_num, table, &geometry)),
        );
    }
    if pages.next().is_some() {
        return Err(BolivarError::RuntimeError);
    }

    Ok(tables)
}

fn summary_from_layout_page(layout_page: LayoutPage) -> PageSummary {
    PageSummary {
        page_number: layout_page.page_number,
        text: layout_page.text,
        bbox: layout_page.bbox,
        rotate: layout_page.rotate,
    }
}

fn open_pdf_document(
    pdf_data: &[u8],
    options: &Option<ExtractOptions>,
) -> Result<Arc<PDFDocument>, BolivarError> {
    let password = extract_password(options);
    let caching = extract_caching(options);
    PDFDocument::new_with_cache(pdf_data, &password, cache_capacity(caching))
        .map(Arc::new)
        .map_err(BolivarError::from)
}

pub struct NativePdfDocument {
    doc: Arc<PDFDocument>,
    options: Option<ExtractOptions>,
}

impl std::fmt::Debug for NativePdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePdfDocument").finish_non_exhaustive()
    }
}

impl NativePdfDocument {
    pub fn from_path(path: String, options: Option<ExtractOptions>) -> Result<Self, BolivarError> {
        let pdf_data = read_pdf_bytes(path)?;
        Self::from_bytes(pdf_data, options)
    }

    pub fn from_bytes(
        pdf_data: Vec<u8>,
        options: Option<ExtractOptions>,
    ) -> Result<Self, BolivarError> {
        let doc = open_pdf_document(&pdf_data, &options)?;
        Ok(Self { doc, options })
    }

    fn core_options(&self) -> Result<CoreExtractOptions, BolivarError> {
        core_extract_options(self.options.clone())
    }

    pub fn extract_text(&self) -> Result<String, BolivarError> {
        let options = self.core_options()?;
        core_extract_text_with_document(self.doc.as_ref(), options).map_err(BolivarError::from)
    }

    pub fn extract_page_summaries(&self) -> Result<Vec<PageSummary>, BolivarError> {
        Ok(self
            .extract_layout_pages()?
            .into_iter()
            .map(summary_from_layout_page)
            .collect())
    }

    pub fn extract_layout_pages(&self) -> Result<Vec<LayoutPage>, BolivarError> {
        let options = self.core_options()?;
        extract_layout_pages_core(Arc::clone(&self.doc), options)
    }

    pub fn extract_tables(&self) -> Result<Vec<Table>, BolivarError> {
        let options = self.core_options()?;
        extract_tables_core(Arc::clone(&self.doc), options)
    }
}

pub fn quick_extract_text(
    path: String,
    options: Option<ExtractOptions>,
) -> Result<String, BolivarError> {
    let doc = NativePdfDocument::from_path(path, options)?;
    doc.extract_text()
}

pub fn quick_extract_text_from_bytes(
    pdf_data: Vec<u8>,
    options: Option<ExtractOptions>,
) -> Result<String, BolivarError> {
    let doc = NativePdfDocument::from_bytes(pdf_data, options)?;
    doc.extract_text()
}

uniffi::include_scaffolding!("bolivar");

#[cfg(test)]
mod tests {
    use super::*;
    use bolivar_core::layout::{LTChar, LTTextLineHorizontal, TextLineElement, TextLineType};
    use bolivar_core::pdfpage::PDFPage;
    use std::collections::HashMap;
    mod common {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/common/mod.rs"));
    }

    #[test]
    fn page_geometry_uses_cropbox_and_mediabox_from_pdf_page() {
        let page = PDFPage {
            pageid: 1,
            attrs: HashMap::new(),
            label: None,
            mediabox: Some([0.0, 0.0, 200.0, 200.0]),
            cropbox: Some([50.0, 50.0, 150.0, 150.0]),
            bleedbox: None,
            trimbox: None,
            artbox: None,
            rotate: 0,
            annots: None,
            resources: HashMap::new(),
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
    fn selected_page_indices_respect_page_numbers_and_max_pages() {
        let pdf = common::build_minimal_pdf_with_pages(5);
        let doc = PDFDocument::new_with_cache(&pdf, "", DEFAULT_CACHE_CAPACITY).expect("doc");
        let options = CoreExtractOptions {
            password: String::new(),
            page_numbers: Some(vec![0, 2, 4]),
            maxpages: 2,
            caching: true,
            laparams: None,
        };

        let selected = selected_page_indices(&doc, &options);
        assert_eq!(selected, vec![0, 2]);
    }

    #[test]
    fn page_number_clamps_non_positive_to_one() {
        assert_eq!(page_number(0), 1);
        assert_eq!(page_number(-5), 1);
    }

    #[test]
    fn maps_pdf_error_kinds_without_collapsing_all_to_pdf_error() {
        assert!(matches!(
            BolivarError::from(PdfError::SyntaxError("bad".to_string())),
            BolivarError::SyntaxError
        ));
        assert!(matches!(
            BolivarError::from(PdfError::InvalidArgument("bad".to_string())),
            BolivarError::InvalidArgument
        ));
        assert!(matches!(
            BolivarError::from(PdfError::EncryptionError("bad".to_string())),
            BolivarError::EncryptionError
        ));
    }

    #[test]
    fn core_extract_options_validates_boxes_flow_range() {
        let options = ExtractOptions {
            password: None,
            page_numbers: None,
            max_pages: None,
            caching: None,
            layout_params: Some(LayoutParams {
                line_overlap: None,
                char_margin: None,
                line_margin: None,
                word_margin: None,
                boxes_flow: Some(1.2),
                detect_vertical: None,
                all_texts: None,
            }),
        };
        let err = core_extract_options(Some(options)).expect_err("out-of-range boxes_flow");
        assert!(matches!(err, BolivarError::InvalidArgument));
    }
}
