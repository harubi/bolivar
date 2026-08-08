use bolivar_core::extract::ExtractOptions as CoreExtractOptions;
use bolivar_core::extract::{
    extract_layout_tables_metadata_stream_from_doc_with_geometries, extract_pages_stream_from_doc,
    extract_tables_metadata_stream_from_doc_with_geometries,
    extract_tables_stream_from_doc_with_geometries,
};
use bolivar_core::layout::LAParams as CoreLAParams;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::table::{ExplicitLine, PageGeometry, TableSettings};
use std::sync::Arc;

use crate::document::NativePdfDocument;
use crate::error::{BolivarError, map_io_error_kind};
use crate::types::{
    BoundingBox, ExtractOptions, LayoutPage, LayoutParams, PageTableRows, RawDocument, Table,
    TableOptions, cache_capacity, layout_page_from_ltpage, page_geometry_from_pdf_page,
    page_number, raw_page_from_parts, table_from_core, usize_to_u32,
};

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

pub(crate) fn core_extract_options(
    options: Option<ExtractOptions>,
) -> Result<CoreExtractOptions, BolivarError> {
    let options = options.unwrap_or_default();
    Ok(CoreExtractOptions {
        password: options.password.unwrap_or_default(),
        page_numbers: normalize_page_numbers(options.page_numbers)?,
        maxpages: normalize_max_pages(options.max_pages)?,
        caching: options.caching.unwrap_or(true),
        laparams: normalize_layout_params(options.layout_params)?,
        rotation: 0,
        bidi: options.bidi.unwrap_or(false),
    })
}

fn validate_input_path(path: &str) -> Result<(), BolivarError> {
    if path.trim().is_empty() || path.contains('\0') || path.contains("://") {
        return Err(BolivarError::InvalidPath);
    }
    Ok(())
}

pub(crate) fn read_pdf_bytes(path: String) -> Result<Vec<u8>, BolivarError> {
    validate_input_path(&path)?;
    std::fs::read(path).map_err(|err| map_io_error_kind(err.kind()))
}

pub(crate) fn extract_layout_pages_core(
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
) -> Result<Vec<LayoutPage>, BolivarError> {
    let pages_iter = extract_pages_stream_from_doc(doc, options).map_err(BolivarError::from)?;
    let mut pages = Vec::new();
    for page_result in pages_iter {
        let (_, page) = page_result.map_err(BolivarError::from)?;
        pages.push(layout_page_from_ltpage(&page));
    }
    Ok(pages)
}

pub(crate) fn extract_raw_document_core(
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
) -> Result<RawDocument, BolivarError> {
    let selected_indices = bolivar_core::engine::select_pages(
        doc.page_tree_len(),
        options.page_numbers.clone(),
        options.maxpages,
    );
    let mut geometries = Vec::with_capacity(selected_indices.len());
    for &page_index in &selected_indices {
        let page = doc
            .get_page_cached(page_index)
            .map_err(BolivarError::from)?;
        geometries.push(page_geometry_from_pdf_page(page.as_ref()));
    }

    let stream = extract_layout_tables_metadata_stream_from_doc_with_geometries(
        Arc::clone(&doc),
        options,
        TableSettings::default(),
        geometries,
    )
    .map_err(BolivarError::from)?;
    let mut pages = Vec::with_capacity(selected_indices.len());
    for item in stream {
        let (page_index, (layout_page, tables)) = item.map_err(BolivarError::from)?;
        let pdf_page = doc
            .get_page_cached(page_index)
            .map_err(BolivarError::from)?;
        pages.push(raw_page_from_parts(
            page_index,
            pdf_page.as_ref(),
            layout_page,
            tables,
        ));
    }

    Ok(RawDocument {
        declared_page_count: usize_to_u32(doc.page_count()),
        page_count: usize_to_u32(pages.len()),
        pages,
    })
}

pub(crate) fn extract_raw_page_core(
    doc: Arc<PDFDocument>,
    mut options: CoreExtractOptions,
    page_number: u32,
) -> Result<crate::types::RawPage, BolivarError> {
    if page_number == 0 || page_number as usize > doc.page_tree_len() {
        return Err(BolivarError::InvalidArgument);
    }
    options.page_numbers = Some(vec![page_number as usize - 1]);
    options.maxpages = 1;
    extract_raw_document_core(doc, options)?
        .pages
        .into_iter()
        .next()
        .ok_or(BolivarError::InvalidArgument)
}

fn table_settings_from_options(options: &TableOptions) -> Result<TableSettings, BolivarError> {
    let mut settings = TableSettings::default();

    if let Some(strategy) = &options.vertical_strategy {
        settings.vertical_strategy = strategy
            .parse()
            .map_err(|()| BolivarError::InvalidArgument)?;
    }
    if let Some(strategy) = &options.horizontal_strategy {
        settings.horizontal_strategy = strategy
            .parse()
            .map_err(|()| BolivarError::InvalidArgument)?;
    }

    // General tolerances fan into both axes; axis-specific values override.
    if let Some(v) = options.snap_tolerance {
        settings.snap_x_tolerance = v;
        settings.snap_y_tolerance = v;
    }
    if let Some(v) = options.snap_x_tolerance {
        settings.snap_x_tolerance = v;
    }
    if let Some(v) = options.snap_y_tolerance {
        settings.snap_y_tolerance = v;
    }
    if let Some(v) = options.join_tolerance {
        settings.join_x_tolerance = v;
        settings.join_y_tolerance = v;
    }
    if let Some(v) = options.join_x_tolerance {
        settings.join_x_tolerance = v;
    }
    if let Some(v) = options.join_y_tolerance {
        settings.join_y_tolerance = v;
    }
    if let Some(v) = options.intersection_tolerance {
        settings.intersection_x_tolerance = v;
        settings.intersection_y_tolerance = v;
    }
    if let Some(v) = options.intersection_x_tolerance {
        settings.intersection_x_tolerance = v;
    }
    if let Some(v) = options.intersection_y_tolerance {
        settings.intersection_y_tolerance = v;
    }

    for tolerance in [
        settings.snap_x_tolerance,
        settings.snap_y_tolerance,
        settings.join_x_tolerance,
        settings.join_y_tolerance,
        settings.intersection_x_tolerance,
        settings.intersection_y_tolerance,
    ] {
        if !tolerance.is_finite() || tolerance < 0.0 {
            return Err(BolivarError::InvalidArgument);
        }
    }

    if let Some(lines) = &options.explicit_vertical_lines {
        settings.explicit_vertical_lines = lines
            .iter()
            .map(|&coord| ExplicitLine::Coord(coord))
            .collect();
    }
    if let Some(lines) = &options.explicit_horizontal_lines {
        settings.explicit_horizontal_lines = lines
            .iter()
            .map(|&coord| ExplicitLine::Coord(coord))
            .collect();
    }

    Ok(settings)
}

/// Clamp a pdfplumber-space crop to the page region; degenerate or
/// out-of-bounds crops fall back to the uncropped page (mirrors the
/// decision-layer `_sanitize_bbox` the Python binding relies on).
fn cropped_geometry(geometry: PageGeometry, crop: &BoundingBox) -> PageGeometry {
    if !(crop.x0.is_finite() && crop.y0.is_finite() && crop.x1.is_finite() && crop.y1.is_finite()) {
        return geometry;
    }
    let (px0, py0, px1, py1) = geometry.page_bbox;
    let x0 = crop.x0.max(px0).min(px1);
    let y0 = crop.y0.max(py0).min(py1);
    let x1 = crop.x1.max(px0).min(px1);
    let y1 = crop.y1.max(py0).min(py1);
    if x1 <= x0 || y1 <= y0 {
        return geometry;
    }
    let clamped = (x0, y0, x1, y1);
    if clamped == geometry.page_bbox {
        return geometry;
    }
    PageGeometry {
        page_bbox: clamped,
        force_crop: true,
        ..geometry
    }
}

pub(crate) fn extract_tables_core(
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
) -> Result<Vec<Table>, BolivarError> {
    extract_tables_with_core(doc, options, None)
}

fn resolved_geometries(
    doc: &Arc<PDFDocument>,
    selected_indices: &[usize],
    table_options: &TableOptions,
) -> Result<Vec<PageGeometry>, BolivarError> {
    selected_indices
        .iter()
        .map(|&idx| {
            doc.get_page_cached(idx)
                .map(|pdf_page| {
                    let geometry = page_geometry_from_pdf_page(pdf_page.as_ref());
                    let crop = if idx == 0 {
                        table_options
                            .first_page_crop
                            .as_ref()
                            .or(table_options.crop.as_ref())
                    } else {
                        table_options.crop.as_ref()
                    };
                    match crop {
                        Some(c) => cropped_geometry(geometry, c),
                        None => geometry,
                    }
                })
                .map_err(BolivarError::from)
        })
        .collect()
}

fn prepared_extraction(
    doc: &Arc<PDFDocument>,
    mut options: CoreExtractOptions,
    table_options: Option<TableOptions>,
) -> Result<
    (
        CoreExtractOptions,
        TableSettings,
        Vec<usize>,
        Vec<PageGeometry>,
    ),
    BolivarError,
> {
    let table_options = table_options.unwrap_or_default();
    let settings = table_settings_from_options(&table_options)?;
    if let Some(max_pages) = table_options.max_pages {
        options.maxpages = usize::try_from(max_pages).map_err(|_| BolivarError::InvalidArgument)?;
    }
    let selected_indices = bolivar_core::engine::select_pages(
        doc.page_tree_len(),
        options.page_numbers.clone(),
        options.maxpages,
    );
    let geometries = resolved_geometries(doc, &selected_indices, &table_options)?;
    Ok((options, settings, selected_indices, geometries))
}

pub(crate) fn extract_table_rows_with_core(
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
    table_options: Option<TableOptions>,
) -> Result<Vec<PageTableRows>, BolivarError> {
    let (options, settings, selected_indices, geometries) =
        prepared_extraction(&doc, options, table_options)?;

    let stream = extract_tables_stream_from_doc_with_geometries(
        Arc::clone(&doc),
        options,
        settings,
        geometries,
    )
    .map_err(BolivarError::from)?;

    let mut pages = Vec::new();
    for (i, item) in stream.enumerate() {
        let (_page_idx, tables) = item.map_err(BolivarError::from)?;
        pages.push(PageTableRows {
            page_number: page_number((selected_indices[i] + 1) as i32),
            tables,
        });
    }
    Ok(pages)
}

pub(crate) fn extract_tables_with_core(
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
    table_options: Option<TableOptions>,
) -> Result<Vec<Table>, BolivarError> {
    let (options, settings, selected_indices, geometries) =
        prepared_extraction(&doc, options, table_options)?;

    let stream = extract_tables_metadata_stream_from_doc_with_geometries(
        Arc::clone(&doc),
        options,
        settings,
        geometries.clone(),
    )
    .map_err(BolivarError::from)?;

    let mut tables = Vec::new();
    for (i, item) in stream.enumerate() {
        let (_page_idx, page_tables) = item.map_err(BolivarError::from)?;
        let geometry = &geometries[i];
        let page_num = page_number((selected_indices[i] + 1) as i32);
        for meta in page_tables {
            tables.push(table_from_core(page_num, meta, geometry));
        }
    }
    Ok(tables)
}

pub(crate) fn open_pdf_document(
    pdf_data: Vec<u8>,
    options: &CoreExtractOptions,
) -> Result<Arc<PDFDocument>, BolivarError> {
    PDFDocument::new_from_vec_with_cache(
        pdf_data,
        &options.password,
        cache_capacity(options.caching),
    )
    .map(Arc::new)
    .map_err(BolivarError::from)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_extract_options_validates_boxes_flow_range() {
        let options = ExtractOptions {
            password: None,
            page_numbers: None,
            max_pages: None,
            caching: None,
            bidi: None,
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
