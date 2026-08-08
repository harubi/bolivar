//! Table extraction functions for Python.
//!
//! Provides functions for extracting tables from PDF pages and converting
//! page objects to chars/edges for table extraction.

use bolivar_core::arena::PageArena;
use bolivar_core::error::{PdfError, Result as CoreResult};
use bolivar_core::extract::{
    ExtractOptions,
    extract_pages_with_images_with_document as core_extract_pages_with_images_with_document,
    extract_text_with_document as core_extract_text_with_document,
};
use bolivar_core::extract::{
    extract_pages_stream_from_doc as core_extract_pages_stream_from_doc,
    extract_tables_stream_from_doc_with_geometries as core_extract_tables_stream_from_doc_with_geometries,
};
use bolivar_core::layout::LTPage;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::convert::core_error_to_py;
use crate::document::{
    PyPDFDocument, PyPDFPage, build_extract_options, build_extract_options_with_rotation,
    open_document_from_input, open_document_from_path,
};
use crate::layout::{PyLTPage, ltpage_to_py};
use crate::params::{PyLAParams, parse_page_geometry, parse_table_settings};
use crate::table_compat::compat_lists_to_chars_edges;

/// Process a PDF page and return its layout.
///
/// Args:
///     doc: PDFDocument instance
///     page: PDFPage to process
///     laparams: Layout analysis parameters
///
/// Returns:
///     LTPage with layout analysis results
#[pyfunction]
#[pyo3(signature = (doc, page, laparams=None, rotation=0, bidi=false))]
pub fn process_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page: &PyPDFPage,
    laparams: Option<&PyLAParams>,
    rotation: i64,
    bidi: bool,
) -> PyResult<PyLTPage> {
    use bolivar_core::device::PDFPageAggregator;
    use bolivar_core::engine::{no_precheck, run_stream};
    use bolivar_core::extract::{aggregator_result, process_page as core_process_page};
    use bolivar_core::interp::PDFResourceManager;

    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
    let page_index = page.page_index;
    let result: CoreResult<LTPage> = py.detach(|| {
        let mut stream = run_stream(
            std::sync::Arc::clone(&doc.inner),
            Some(vec![page_index]),
            0,
            no_precheck::<LTPage>,
            move |arena, page_idx, page, doc| {
                let mut rsrcmgr = PDFResourceManager::with_caching(true);
                let mut aggregator = PDFPageAggregator::new(la.clone(), page_idx as i32 + 1, arena);
                core_process_page(
                    page,
                    &mut aggregator,
                    &mut rsrcmgr,
                    rotation,
                    doc,
                    aggregator_result,
                )
            },
        )?;
        let (_, mut page) = stream
            .next()
            .ok_or_else(|| PdfError::DecodeError("page index out of range".to_string()))??;
        page.set_bidi(bidi);
        Ok(page)
    });
    let ltpage = result.map_err(|e| core_error_to_py(py, "Failed to process page", e))?;

    Ok(ltpage_to_py(ltpage))
}

/// Process all PDF pages and return their layouts.
///
/// Args:
///     doc: PDFDocument instance
///     laparams: Layout analysis parameters
/// Returns:
///     List of LTPage objects
#[pyfunction]
#[pyo3(signature = (doc, laparams=None, bidi=false))]
pub fn process_pages(
    py: Python<'_>,
    doc: &PyPDFDocument,
    laparams: Option<&PyLAParams>,
    bidi: bool,
) -> PyResult<Vec<PyLTPage>> {
    let options = ExtractOptions {
        laparams: Some(laparams.map(|p| p.clone().into()).unwrap_or_default()),
        bidi,
        ..ExtractOptions::default()
    };

    let pages: Vec<LTPage> = py
        .detach(|| {
            core_extract_pages_stream_from_doc(std::sync::Arc::clone(&doc.inner), options)?
                .map(|r| r.map(|(_, p)| p))
                .collect::<CoreResult<Vec<_>>>()
        })
        .map_err(|e| core_error_to_py(py, "Failed to process pages", e))?;

    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Extract tables for a single indexed page.
#[pyfunction(name = "_extract_tables_for_page_indexed")]
#[pyo3(signature = (doc, page_index, geometry, table_settings = None, laparams = None, caching = true))]
pub fn extract_tables_for_page_indexed(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    geometry: &Bound<'_, PyAny>,
    table_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    caching: bool,
) -> PyResult<Vec<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let geom = parse_page_geometry(geometry)?;

    let result: CoreResult<Vec<Vec<Vec<Option<String>>>>> = py.detach(|| {
        let opts = ExtractOptions {
            page_numbers: Some(vec![page_index]),
            caching,
            laparams: laparams.map(|p| p.clone().into()),
            ..ExtractOptions::default()
        };
        let mut stream = core_extract_tables_stream_from_doc_with_geometries(
            std::sync::Arc::clone(&doc.inner),
            opts,
            settings,
            vec![geom],
        )?;
        let (_, tables) = stream
            .next()
            .ok_or_else(|| PdfError::DecodeError("page index out of range".to_string()))??;
        Ok(tables)
    });

    result.map_err(|e| PyValueError::new_err(format!("Failed to extract tables: {e}")))
}

/// Extract tables for compatibility-only filtered or cropped page objects.
#[pyfunction(name = "_extract_tables_for_compat_page")]
#[pyo3(signature = (chars, lines, rects, curves, geometry, table_settings = None))]
pub fn extract_tables_for_compat_page(
    py: Python<'_>,
    chars: &Bound<'_, PyAny>,
    lines: &Bound<'_, PyAny>,
    rects: &Bound<'_, PyAny>,
    curves: &Bound<'_, PyAny>,
    geometry: &Bound<'_, PyAny>,
    table_settings: Option<Py<PyAny>>,
) -> PyResult<Vec<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let geom = parse_page_geometry(geometry)?;
    if !geom.force_crop {
        return Err(PyValueError::new_err(
            "compat table helper requires cropped or filtered geometry",
        ));
    }
    let mut arena = PageArena::new();
    let (chars, edges) =
        compat_lists_to_chars_edges(chars, lines, rects, curves, &geom, &mut arena)?;
    Ok(bolivar_core::table::extract_tables_from_objects(
        chars, edges, &geom, &settings, &arena,
    ))
}

/// Repair a PDF and return the repaired bytes.
#[pyfunction]
pub fn repair_pdf(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let (bytes, _path) = crate::document::read_bytes_and_path(py, data)?;
    let repaired = py
        .detach(|| bolivar_core::document::repair::repair_bytes(&bytes))
        .map_err(|e| PyValueError::new_err(format!("repair failed: {e}")))?;
    Ok(PyBytes::new(py, &repaired).into_any().unbind())
}

/// Extract text from PDF bytes.
#[pyfunction]
#[pyo3(signature = (data, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, bidi = false))]
pub fn extract_text(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    bidi: bool,
) -> PyResult<String> {
    let options = build_extract_options(password, page_numbers, maxpages, caching, laparams, bidi);
    let doc = open_document_from_input(py, data, password, caching, true)?;
    let result = py.detach(|| core_extract_text_with_document(doc.as_ref(), options));
    result.map_err(|e| core_error_to_py(py, "Failed to extract text", e))
}

/// Extract text from a PDF file path using memory-mapped I/O.
#[pyfunction]
#[pyo3(signature = (path, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, bidi = false))]
pub fn extract_text_from_path(
    py: Python<'_>,
    path: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    bidi: bool,
) -> PyResult<String> {
    let doc = open_document_from_path(path, password, caching, true)?;
    let options = build_extract_options(password, page_numbers, maxpages, caching, laparams, bidi);

    let result = py.detach(|| core_extract_text_with_document(doc.as_ref(), options));
    result.map_err(|e| core_error_to_py(py, "Failed to extract text", e))
}

/// Extract pages (layout) from PDF bytes.
#[pyfunction]
#[pyo3(signature = (data, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, rotation = 0, bidi = false))]
pub fn extract_pages(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    rotation: i64,
    bidi: bool,
) -> PyResult<Vec<PyLTPage>> {
    let options = build_extract_options_with_rotation(
        password,
        page_numbers,
        maxpages,
        caching,
        laparams,
        rotation,
        bidi,
    );
    let doc = open_document_from_input(py, data, password, caching, true)?;
    let pages: Vec<LTPage> = py
        .detach(|| {
            core_extract_pages_stream_from_doc(std::sync::Arc::clone(&doc), options)?
                .map(|r| r.map(|(_, p)| p))
                .collect::<CoreResult<Vec<_>>>()
        })
        .map_err(|e| core_error_to_py(py, "Failed to extract pages", e))?;
    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Extract pages (layout) from PDF bytes while exporting images.
#[pyfunction]
#[pyo3(signature = (data, output_dir, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, rotation = 0, bidi = false))]
pub fn extract_pages_with_images(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    output_dir: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    rotation: i64,
    bidi: bool,
) -> PyResult<Vec<PyLTPage>> {
    let options = build_extract_options_with_rotation(
        password,
        page_numbers,
        maxpages,
        caching,
        laparams,
        rotation,
        bidi,
    );
    let doc = open_document_from_input(py, data, password, caching, true)?;
    let pages = py
        .detach(|| {
            core_extract_pages_with_images_with_document(
                std::sync::Arc::clone(&doc),
                options,
                output_dir,
            )
        })
        .map_err(|e| core_error_to_py(py, "Failed to extract pages", e))?;
    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Extract pages (layout) from a PDF file path using memory-mapped I/O.
#[pyfunction]
#[pyo3(signature = (path, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, rotation = 0, bidi = false))]
pub fn extract_pages_from_path(
    py: Python<'_>,
    path: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    rotation: i64,
    bidi: bool,
) -> PyResult<Vec<PyLTPage>> {
    let doc = open_document_from_path(path, password, caching, true)?;
    let options = build_extract_options_with_rotation(
        password,
        page_numbers,
        maxpages,
        caching,
        laparams,
        rotation,
        bidi,
    );

    let pages: Vec<LTPage> = py
        .detach(|| {
            core_extract_pages_stream_from_doc(std::sync::Arc::clone(&doc), options)?
                .map(|r| r.map(|(_, p)| p))
                .collect::<CoreResult<Vec<_>>>()
        })
        .map_err(|e| core_error_to_py(py, "Failed to extract pages", e))?;
    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Extract pages (layout) from a PDF file path while exporting images.
#[pyfunction]
#[pyo3(signature = (path, output_dir, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, rotation = 0, bidi = false))]
pub fn extract_pages_with_images_from_path(
    py: Python<'_>,
    path: &str,
    output_dir: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    rotation: i64,
    bidi: bool,
) -> PyResult<Vec<PyLTPage>> {
    let doc = open_document_from_path(path, password, caching, true)?;
    let options = build_extract_options_with_rotation(
        password,
        page_numbers,
        maxpages,
        caching,
        laparams,
        rotation,
        bidi,
    );

    let pages = py
        .detach(|| {
            core_extract_pages_with_images_with_document(
                std::sync::Arc::clone(&doc),
                options,
                output_dir,
            )
        })
        .map_err(|e| core_error_to_py(py, "Failed to extract pages", e))?;
    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Register the table module functions with the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract_text, m)?)?;
    m.add_function(wrap_pyfunction!(extract_text_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_with_images, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_with_images_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(process_page, m)?)?;
    m.add_function(wrap_pyfunction!(process_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_for_page_indexed, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_for_compat_page, m)?)?;
    m.add_function(wrap_pyfunction!(repair_pdf, m)?)?;
    Ok(())
}
