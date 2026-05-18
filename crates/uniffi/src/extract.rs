use bolivar_core::extract::ExtractOptions as CoreExtractOptions;
use bolivar_core::extract::{
    extract_pages_stream_from_doc, extract_tables_metadata_stream_from_doc_with_geometries,
};
use bolivar_core::layout::LAParams as CoreLAParams;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::table::{PageGeometry, TableSettings};
use std::sync::Arc;

use crate::document::NativePdfDocument;
use crate::error::{BolivarError, map_io_error_kind};
use crate::types::{
    ExtractOptions, LayoutPage, LayoutParams, Table, cache_capacity, layout_page_from_ltpage,
    page_geometry_from_pdf_page, page_number, table_from_core,
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

pub(crate) fn extract_password(options: &Option<ExtractOptions>) -> String {
    options
        .as_ref()
        .and_then(|value| value.password.clone())
        .unwrap_or_default()
}

pub(crate) fn extract_caching(options: &Option<ExtractOptions>) -> bool {
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

pub(crate) fn extract_tables_core(
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
) -> Result<Vec<Table>, BolivarError> {
    let selected_indices = bolivar_core::engine::select_pages(
        doc.page_tree_len(),
        options.page_numbers.clone(),
        options.maxpages,
    );
    let geometries: Vec<PageGeometry> = selected_indices
        .iter()
        .map(|&idx| {
            doc.get_page_cached(idx)
                .map(|pdf_page| page_geometry_from_pdf_page(pdf_page.as_ref()))
                .map_err(BolivarError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let stream = extract_tables_metadata_stream_from_doc_with_geometries(
        Arc::clone(&doc),
        options,
        TableSettings::default(),
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
    pdf_data: &[u8],
    options: &Option<ExtractOptions>,
) -> Result<Arc<PDFDocument>, BolivarError> {
    let password = extract_password(options);
    let caching = extract_caching(options);
    PDFDocument::new_with_cache(pdf_data, &password, cache_capacity(caching))
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
