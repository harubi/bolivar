use bolivar_core::PdfError;
use bolivar_core::api::stream::extract_pages_stream_from_doc;
use bolivar_core::high_level::{
    ExtractOptions, extract_pages as core_extract_pages, extract_text as core_extract_text,
};
use bolivar_core::layout::{
    LTItem, LTPage, LTTextBox as CoreLTTextBox, LTTextLine, LTTextLineHorizontal,
    LTTextLineVertical, TextBoxType,
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
#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

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

#[cfg(test)]
static OFFLOAD_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
static OFFLOAD_THREADS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
#[cfg(test)]
static OFFLOAD_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
fn mark_offload_call() {
    OFFLOAD_CALLS.fetch_add(1, Ordering::SeqCst);
    let ids = OFFLOAD_THREADS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut guard) = ids.lock() {
        guard.push(format!("{:?}", std::thread::current().id()));
    }
}

#[cfg(not(test))]
fn mark_offload_call() {}

#[cfg(test)]
fn reset_offload_calls() {
    OFFLOAD_CALLS.store(0, Ordering::SeqCst);
    let ids = OFFLOAD_THREADS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut guard) = ids.lock() {
        guard.clear();
    }
}

#[cfg(test)]
fn offload_call_count() -> usize {
    OFFLOAD_CALLS.load(Ordering::SeqCst)
}

#[cfg(test)]
fn offload_thread_ids() -> Vec<String> {
    let ids = OFFLOAD_THREADS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(guard) = ids.lock() {
        return guard.clone();
    }
    Vec::new()
}

#[cfg(test)]
fn offload_test_guard() -> std::sync::MutexGuard<'static, ()> {
    OFFLOAD_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("offload test lock")
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

fn extract_options(
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<ExtractOptions, BolivarError> {
    Ok(ExtractOptions {
        password: password.unwrap_or_default(),
        page_numbers: normalize_page_numbers(page_numbers)?,
        maxpages: normalize_max_pages(max_pages)?,
        caching: true,
        laparams: None,
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
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    let options = extract_options(password, page_numbers, max_pages)?;
    let pages_iter = core_extract_pages(&pdf_data, Some(options)).map_err(BolivarError::from)?;
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

fn selected_page_indices(doc: &PDFDocument, options: &ExtractOptions) -> Vec<usize> {
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
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<Table>, BolivarError> {
    let options = extract_options(password, page_numbers, max_pages)?;
    let doc = Arc::new(
        PDFDocument::new_with_cache(
            &pdf_data,
            &options.password,
            cache_capacity(options.caching),
        )
        .map_err(BolivarError::from)?,
    );
    let selected_indices = selected_page_indices(doc.as_ref(), &options);
    let mut pages =
        extract_pages_stream_from_doc(Arc::clone(&doc), options).map_err(BolivarError::from)?;

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

fn get_or_try_init_no_error_cache<T, E>(
    cell: &OnceLock<T>,
    init: impl FnOnce() -> Result<T, E>,
) -> Result<&T, E> {
    if let Some(value) = cell.get() {
        return Ok(value);
    }

    let value = init()?;
    let _ = cell.set(value);
    Ok(cell
        .get()
        .expect("OnceLock should contain a value after successful initialization"))
}

static ASYNC_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn build_async_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

fn get_async_runtime() -> Result<&'static tokio::runtime::Runtime, BolivarError> {
    get_or_try_init_no_error_cache(&ASYNC_RUNTIME, build_async_runtime)
        .map_err(|_| BolivarError::RuntimeError)
}

async fn offload_blocking<T, F>(job: F) -> Result<T, BolivarError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, BolivarError> + Send + 'static,
{
    let runtime = get_async_runtime()?;
    let join = runtime.spawn_blocking(move || {
        mark_offload_call();
        job()
    });
    join.await.map_err(|_| BolivarError::RuntimeError)?
}

pub fn extract_text_from_bytes(
    pdf_data: Vec<u8>,
    password: Option<String>,
) -> Result<String, BolivarError> {
    extract_text_from_bytes_with_page_range(pdf_data, password, None, None)
}

pub fn extract_text_from_bytes_with_page_range(
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<String, BolivarError> {
    let options = extract_options(password, page_numbers, max_pages)?;
    core_extract_text(&pdf_data, Some(options)).map_err(BolivarError::from)
}

pub fn extract_text_from_path(
    path: String,
    password: Option<String>,
) -> Result<String, BolivarError> {
    extract_text_from_path_with_page_range(path, password, None, None)
}

pub fn extract_text_from_path_with_page_range(
    path: String,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<String, BolivarError> {
    let pdf_data = read_pdf_bytes(path)?;
    extract_text_from_bytes_with_page_range(pdf_data, password, page_numbers, max_pages)
}

pub async fn extract_text_from_bytes_async(
    pdf_data: Vec<u8>,
    password: Option<String>,
) -> Result<String, BolivarError> {
    extract_text_from_bytes_with_page_range_async(pdf_data, password, None, None).await
}

pub async fn extract_text_from_bytes_with_page_range_async(
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<String, BolivarError> {
    offload_blocking(move || {
        extract_text_from_bytes_with_page_range(pdf_data, password, page_numbers, max_pages)
    })
    .await
}

pub async fn extract_text_from_path_async(
    path: String,
    password: Option<String>,
) -> Result<String, BolivarError> {
    extract_text_from_path_with_page_range_async(path, password, None, None).await
}

pub async fn extract_text_from_path_with_page_range_async(
    path: String,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<String, BolivarError> {
    offload_blocking(move || {
        extract_text_from_path_with_page_range(path, password, page_numbers, max_pages)
    })
    .await
}

pub fn extract_page_summaries_from_bytes(
    pdf_data: Vec<u8>,
    password: Option<String>,
) -> Result<Vec<PageSummary>, BolivarError> {
    extract_page_summaries_from_bytes_with_page_range(pdf_data, password, None, None)
}

pub fn extract_page_summaries_from_bytes_with_page_range(
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<PageSummary>, BolivarError> {
    Ok(
        extract_layout_pages_core(pdf_data, password, page_numbers, max_pages)?
            .into_iter()
            .map(summary_from_layout_page)
            .collect(),
    )
}

pub fn extract_page_summaries_from_path(
    path: String,
    password: Option<String>,
) -> Result<Vec<PageSummary>, BolivarError> {
    extract_page_summaries_from_path_with_page_range(path, password, None, None)
}

pub fn extract_page_summaries_from_path_with_page_range(
    path: String,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<PageSummary>, BolivarError> {
    let pdf_data = read_pdf_bytes(path)?;
    extract_page_summaries_from_bytes_with_page_range(pdf_data, password, page_numbers, max_pages)
}

pub async fn extract_page_summaries_from_bytes_async(
    pdf_data: Vec<u8>,
    password: Option<String>,
) -> Result<Vec<PageSummary>, BolivarError> {
    extract_page_summaries_from_bytes_with_page_range_async(pdf_data, password, None, None).await
}

pub async fn extract_page_summaries_from_bytes_with_page_range_async(
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<PageSummary>, BolivarError> {
    offload_blocking(move || {
        extract_page_summaries_from_bytes_with_page_range(
            pdf_data,
            password,
            page_numbers,
            max_pages,
        )
    })
    .await
}

pub async fn extract_page_summaries_from_path_async(
    path: String,
    password: Option<String>,
) -> Result<Vec<PageSummary>, BolivarError> {
    extract_page_summaries_from_path_with_page_range_async(path, password, None, None).await
}

pub async fn extract_page_summaries_from_path_with_page_range_async(
    path: String,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<PageSummary>, BolivarError> {
    offload_blocking(move || {
        extract_page_summaries_from_path_with_page_range(path, password, page_numbers, max_pages)
    })
    .await
}

pub fn extract_layout_pages_from_bytes(
    pdf_data: Vec<u8>,
    password: Option<String>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    extract_layout_pages_from_bytes_with_page_range(pdf_data, password, None, None)
}

pub fn extract_layout_pages_from_bytes_with_page_range(
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    extract_layout_pages_core(pdf_data, password, page_numbers, max_pages)
}

pub fn extract_layout_pages_from_path(
    path: String,
    password: Option<String>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    extract_layout_pages_from_path_with_page_range(path, password, None, None)
}

pub fn extract_layout_pages_from_path_with_page_range(
    path: String,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    let pdf_data = read_pdf_bytes(path)?;
    extract_layout_pages_from_bytes_with_page_range(pdf_data, password, page_numbers, max_pages)
}

pub async fn extract_layout_pages_from_bytes_async(
    pdf_data: Vec<u8>,
    password: Option<String>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    extract_layout_pages_from_bytes_with_page_range_async(pdf_data, password, None, None).await
}

pub async fn extract_layout_pages_from_bytes_with_page_range_async(
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    offload_blocking(move || {
        extract_layout_pages_from_bytes_with_page_range(pdf_data, password, page_numbers, max_pages)
    })
    .await
}

pub async fn extract_layout_pages_from_path_async(
    path: String,
    password: Option<String>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    extract_layout_pages_from_path_with_page_range_async(path, password, None, None).await
}

pub async fn extract_layout_pages_from_path_with_page_range_async(
    path: String,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<LayoutPage>, BolivarError> {
    offload_blocking(move || {
        extract_layout_pages_from_path_with_page_range(path, password, page_numbers, max_pages)
    })
    .await
}

pub fn extract_tables_from_bytes(
    pdf_data: Vec<u8>,
    password: Option<String>,
) -> Result<Vec<Table>, BolivarError> {
    extract_tables_from_bytes_with_page_range(pdf_data, password, None, None)
}

pub fn extract_tables_from_bytes_with_page_range(
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<Table>, BolivarError> {
    extract_tables_core(pdf_data, password, page_numbers, max_pages)
}

pub fn extract_tables_from_path(
    path: String,
    password: Option<String>,
) -> Result<Vec<Table>, BolivarError> {
    extract_tables_from_path_with_page_range(path, password, None, None)
}

pub fn extract_tables_from_path_with_page_range(
    path: String,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<Table>, BolivarError> {
    let pdf_data = read_pdf_bytes(path)?;
    extract_tables_from_bytes_with_page_range(pdf_data, password, page_numbers, max_pages)
}

pub async fn extract_tables_from_bytes_async(
    pdf_data: Vec<u8>,
    password: Option<String>,
) -> Result<Vec<Table>, BolivarError> {
    extract_tables_from_bytes_with_page_range_async(pdf_data, password, None, None).await
}

pub async fn extract_tables_from_bytes_with_page_range_async(
    pdf_data: Vec<u8>,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<Table>, BolivarError> {
    offload_blocking(move || {
        extract_tables_from_bytes_with_page_range(pdf_data, password, page_numbers, max_pages)
    })
    .await
}

pub async fn extract_tables_from_path_async(
    path: String,
    password: Option<String>,
) -> Result<Vec<Table>, BolivarError> {
    extract_tables_from_path_with_page_range_async(path, password, None, None).await
}

pub async fn extract_tables_from_path_with_page_range_async(
    path: String,
    password: Option<String>,
    page_numbers: Option<Vec<u32>>,
    max_pages: Option<u32>,
) -> Result<Vec<Table>, BolivarError> {
    offload_blocking(move || {
        extract_tables_from_path_with_page_range(path, password, page_numbers, max_pages)
    })
    .await
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
    fn async_text_uses_offload_helper() {
        let _guard = offload_test_guard();
        reset_offload_calls();
        let caller = format!("{:?}", std::thread::current().id());
        let pdf = common::build_minimal_pdf_with_pages(1);
        let _ = pollster::block_on(extract_text_from_bytes_async(pdf, None)).expect("text async");
        assert!(offload_call_count() > 0);
        let worker_ids = offload_thread_ids();
        assert!(worker_ids.iter().any(|id| id != &caller));
    }

    #[test]
    fn async_layout_uses_offload_helper() {
        let _guard = offload_test_guard();
        reset_offload_calls();
        let caller = format!("{:?}", std::thread::current().id());
        let pdf = common::build_minimal_pdf_with_pages(1);
        let _ = pollster::block_on(extract_layout_pages_from_bytes_async(pdf, None))
            .expect("layout async");
        assert!(offload_call_count() > 0);
        let worker_ids = offload_thread_ids();
        assert!(worker_ids.iter().any(|id| id != &caller));
    }

    #[test]
    fn async_summaries_uses_offload_helper() {
        let _guard = offload_test_guard();
        reset_offload_calls();
        let caller = format!("{:?}", std::thread::current().id());
        let pdf = common::build_minimal_pdf_with_pages(1);
        let _ = pollster::block_on(extract_page_summaries_from_bytes_async(pdf, None))
            .expect("summary async");
        assert!(offload_call_count() > 0);
        let worker_ids = offload_thread_ids();
        assert!(worker_ids.iter().any(|id| id != &caller));
    }

    #[test]
    fn async_tables_uses_offload_helper() {
        let _guard = offload_test_guard();
        reset_offload_calls();
        let caller = format!("{:?}", std::thread::current().id());
        let pdf = common::build_minimal_pdf_with_pages(1);
        let _ =
            pollster::block_on(extract_tables_from_bytes_async(pdf, None)).expect("tables async");
        assert!(offload_call_count() > 0);
        let worker_ids = offload_thread_ids();
        assert!(worker_ids.iter().any(|id| id != &caller));
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
        let options = ExtractOptions {
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
    fn runtime_cell_retries_after_init_failure() {
        let cell = OnceLock::new();
        let attempts = AtomicUsize::new(0);

        let first = get_or_try_init_no_error_cache(&cell, || {
            let call = attempts.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                return Err(());
            }
            Ok(7u32)
        });
        assert!(first.is_err());

        let second = get_or_try_init_no_error_cache(&cell, || {
            let call = attempts.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                return Err(());
            }
            Ok(7u32)
        })
        .expect("second init should succeed");

        assert_eq!(*second, 7);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
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
}
