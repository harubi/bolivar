//! Table extraction functions for Python.
//!
//! Provides functions for extracting tables from PDF pages and converting
//! page objects to chars/edges for table extraction.

use bolivar_core::arena::PageArena;
use bolivar_core::high_level::{
    ExtractOptions, extract_pages_with_document as core_extract_pages_with_document,
    extract_pages_with_images_with_document as core_extract_pages_with_images_with_document,
    extract_tables_for_page_indexed as core_extract_tables_for_page_indexed,
    extract_tables_with_document_geometries as core_extract_tables_with_document_geometries,
    extract_text_with_document as core_extract_text_with_document,
};
use bolivar_core::pdfdocument::{DEFAULT_CACHE_CAPACITY, PDFDocument};
use bolivar_core::table::{CharObj, EdgeObj, Orientation, PageGeometry, TableSettings};
use memmap2::Mmap;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PySequence};
use std::fs::File;

use crate::document::{PdfInput, PyPDFDocument, PyPDFPage, pdf_input_from_py};
use crate::layout::{PyLTPage, ltpage_to_py};
use crate::params::{PyLAParams, parse_bbox, parse_page_geometries, parse_table_settings};

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
#[pyo3(signature = (doc, page, laparams=None))]
pub fn process_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page: &PyPDFPage,
    laparams: Option<&PyLAParams>,
) -> PyResult<PyLTPage> {
    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
    let pageid = page.pageid;
    let label = page.label.clone();
    let mediabox = page.mediabox.map(|b| [b.0, b.1, b.2, b.3]);
    let cropbox = page.cropbox.map(|b| [b.0, b.1, b.2, b.3]);
    let bleedbox = page.bleedbox.map(|b| [b.0, b.1, b.2, b.3]);
    let trimbox = page.trimbox.map(|b| [b.0, b.1, b.2, b.3]);
    let artbox = page.artbox.map(|b| [b.0, b.1, b.2, b.3]);
    let rotate = page.rotate;
    let resources = page.core_resources();
    let contents = page.core_contents();

    let ltpage = py.detach(|| {
        // Create resource manager
        let mut rsrcmgr = bolivar_core::pdfinterp::PDFResourceManager::with_caching(true);
        let mut arena = PageArena::new();

        // Create aggregator for this page
        let mut aggregator =
            bolivar_core::converter::PDFPageAggregator::new(la, pageid as i32, &mut arena);

        // Recreate the core PDFPage with contents
        // This is a workaround since we can't store references across Python calls
        let core_page = bolivar_core::pdfpage::PDFPage {
            pageid,
            attrs: std::collections::HashMap::new(),
            label,
            mediabox,
            cropbox,
            bleedbox,
            trimbox,
            artbox,
            rotate,
            annots: None,
            resources,
            contents,
            user_unit: 1.0,
        };

        // Create interpreter and process page
        let mut interpreter =
            bolivar_core::pdfinterp::PDFPageInterpreter::new(&mut rsrcmgr, &mut aggregator);
        interpreter.process_page(&core_page, Some(&doc.inner));

        // Get the result as an owned LTPage
        aggregator.get_result().clone()
    });

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
#[pyo3(signature = (doc, laparams=None))]
pub fn process_pages(
    py: Python<'_>,
    doc: &PyPDFDocument,
    laparams: Option<&PyLAParams>,
) -> PyResult<Vec<PyLTPage>> {
    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: la,
    };

    let pages = py
        .detach(|| core_extract_pages_with_document(&doc.inner, options))
        .map_err(|e| PyValueError::new_err(format!("Failed to process pages: {}", e)))?;

    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Extract tables from a document using per-page geometry.
#[pyfunction]
#[pyo3(signature = (doc, geometries, table_settings = None, laparams = None))]
pub fn extract_tables_from_document(
    py: Python<'_>,
    doc: &PyPDFDocument,
    geometries: &Bound<'_, PyAny>,
    table_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
) -> PyResult<Vec<Vec<Vec<Vec<Option<String>>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: la,
    };
    let geoms = parse_page_geometries(geometries)?;
    let tables = py
        .detach(|| {
            core_extract_tables_with_document_geometries(&doc.inner, options, &settings, &geoms)
        })
        .map_err(|e| PyValueError::new_err(format!("Failed to extract tables: {}", e)))?;

    Ok(tables)
}

fn table_extract_options(laparams: Option<&PyLAParams>) -> ExtractOptions {
    ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: laparams.map(|p| p.clone().into()),
    }
}

fn dict_f64(dict: &Bound<'_, PyDict>, key: &str) -> Option<f64> {
    dict.get_item(key)
        .ok()
        .flatten()
        .and_then(|v| v.extract::<f64>().ok())
}

fn dict_bool(dict: &Bound<'_, PyDict>, key: &str) -> Option<bool> {
    dict.get_item(key)
        .ok()
        .flatten()
        .and_then(|v| v.extract::<bool>().ok())
}

fn dict_text(dict: &Bound<'_, PyDict>, key: &str) -> Option<String> {
    let value = dict.get_item(key).ok().flatten()?;
    if let Ok(s) = value.extract::<String>() {
        return Some(s);
    }
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Some(String::from_utf8_lossy(bytes.as_bytes()).to_string());
    }
    None
}

fn char_from_dict(
    arena: &mut PageArena,
    dict: &Bound<'_, PyDict>,
    default_doctop: f64,
) -> Option<CharObj> {
    let text = dict_text(dict, "text")?;
    let x0 = dict_f64(dict, "x0")?;
    let x1 = dict_f64(dict, "x1")?;
    let top = dict_f64(dict, "top")?;
    let bottom = dict_f64(dict, "bottom")?;
    let doctop = dict_f64(dict, "doctop").unwrap_or(default_doctop + top);
    let width = dict_f64(dict, "width").unwrap_or((x1 - x0).abs());
    let height = dict_f64(dict, "height").unwrap_or((bottom - top).abs());
    let size = dict_f64(dict, "size").unwrap_or(0.0);
    let upright = dict_bool(dict, "upright").unwrap_or(true);
    Some(CharObj {
        text: arena.intern(&text),
        x0,
        x1,
        top,
        bottom,
        doctop,
        width,
        height,
        size,
        upright,
    })
}

fn line_edge_from_dict(dict: &Bound<'_, PyDict>) -> Option<EdgeObj> {
    let x0 = dict_f64(dict, "x0")?;
    let x1 = dict_f64(dict, "x1")?;
    let top = dict_f64(dict, "top")?;
    let bottom = dict_f64(dict, "bottom")?;
    let width = dict_f64(dict, "width").unwrap_or((x1 - x0).abs());
    let height = dict_f64(dict, "height").unwrap_or((bottom - top).abs());
    let orientation = if (top - bottom).abs() < f64::EPSILON {
        Some(Orientation::Horizontal)
    } else {
        Some(Orientation::Vertical)
    };
    Some(EdgeObj {
        x0,
        x1,
        top,
        bottom,
        width,
        height,
        orientation,
        object_type: "line",
    })
}

fn rect_edges_from_dict(dict: &Bound<'_, PyDict>) -> Option<Vec<EdgeObj>> {
    let x0 = dict_f64(dict, "x0")?;
    let x1 = dict_f64(dict, "x1")?;
    let top = dict_f64(dict, "top")?;
    let bottom = dict_f64(dict, "bottom")?;
    let width = (x1 - x0).abs();
    let height = (bottom - top).abs();
    Some(vec![
        EdgeObj {
            x0,
            x1,
            top,
            bottom: top,
            width,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "rect_edge",
        },
        EdgeObj {
            x0,
            x1,
            top: bottom,
            bottom,
            width,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "rect_edge",
        },
        EdgeObj {
            x0,
            x1: x0,
            top,
            bottom,
            width: 0.0,
            height,
            orientation: Some(Orientation::Vertical),
            object_type: "rect_edge",
        },
        EdgeObj {
            x0: x1,
            x1,
            top,
            bottom,
            width: 0.0,
            height,
            orientation: Some(Orientation::Vertical),
            object_type: "rect_edge",
        },
    ])
}

fn curve_points_to_edges(points: &[(f64, f64)], object_type: &'static str) -> Vec<EdgeObj> {
    let mut edges = Vec::new();
    for pair in points.windows(2) {
        let p0 = pair[0];
        let p1 = pair[1];
        let x0 = p0.0.min(p1.0);
        let x1 = p0.0.max(p1.0);
        let top = p0.1.min(p1.1);
        let bottom = p0.1.max(p1.1);
        let orientation = if (p0.0 - p1.0).abs() < f64::EPSILON {
            Some(Orientation::Vertical)
        } else if (p0.1 - p1.1).abs() < f64::EPSILON {
            Some(Orientation::Horizontal)
        } else {
            None
        };
        edges.push(EdgeObj {
            x0,
            x1,
            top,
            bottom,
            width: (x1 - x0).abs(),
            height: (bottom - top).abs(),
            orientation,
            object_type,
        });
    }
    edges
}

fn points_from_obj(obj: &Bound<'_, PyAny>) -> Vec<(f64, f64)> {
    if let Ok(points) = obj.extract::<Vec<(f64, f64)>>() {
        return points;
    }
    let Ok(seq) = obj.cast::<PySequence>() else {
        return Vec::new();
    };
    let len = seq.len().unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..len {
        let item = match seq.get_item(i) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if let Ok(pt) = item.extract::<(f64, f64)>() {
            out.push(pt);
            continue;
        }
        let Ok(seg) = item.cast::<PySequence>() else {
            continue;
        };
        let seg_len = seg.len().unwrap_or(0);
        for j in 0..seg_len {
            let seg_item = match seg.get_item(j) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if j == 0 {
                if seg_item.extract::<String>().is_ok() || seg_item.cast::<PyBytes>().is_ok() {
                    continue;
                }
            }
            if let Ok(pt) = seg_item.extract::<(f64, f64)>() {
                out.push(pt);
            }
        }
    }
    out
}

fn curve_edges_from_dict(dict: &Bound<'_, PyDict>) -> Option<Vec<EdgeObj>> {
    if let Some(pts_obj) = dict.get_item("pts").ok().flatten() {
        let points = points_from_obj(&pts_obj);
        if points.len() >= 2 {
            return Some(curve_points_to_edges(&points, "curve_edge"));
        }
    }
    if let Some(path_obj) = dict.get_item("path").ok().flatten() {
        let points = points_from_obj(&path_obj);
        if points.len() >= 2 {
            return Some(curve_points_to_edges(&points, "curve_edge"));
        }
    }
    None
}

fn append_chars_from_list(
    list: &Bound<'_, PyAny>,
    initial_doctop: f64,
    arena: &mut PageArena,
    out: &mut Vec<CharObj>,
) -> PyResult<()> {
    let seq = list
        .cast::<PySequence>()
        .map_err(|_| PyValueError::new_err("char objects must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(dict) = item.cast::<PyDict>() {
            if let Some(obj) = char_from_dict(arena, &dict, initial_doctop) {
                out.push(obj);
            }
        }
    }
    Ok(())
}

fn append_line_edges(list: &Bound<'_, PyAny>, out: &mut Vec<EdgeObj>) -> PyResult<()> {
    let seq = list
        .cast::<PySequence>()
        .map_err(|_| PyValueError::new_err("line objects must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(dict) = item.cast::<PyDict>() {
            if let Some(edge) = line_edge_from_dict(&dict) {
                out.push(edge);
            }
        }
    }
    Ok(())
}

fn append_rect_edges(list: &Bound<'_, PyAny>, out: &mut Vec<EdgeObj>) -> PyResult<()> {
    let seq = list
        .cast::<PySequence>()
        .map_err(|_| PyValueError::new_err("rect objects must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(dict) = item.cast::<PyDict>() {
            if let Some(edges) = rect_edges_from_dict(&dict) {
                out.extend(edges);
            }
        }
    }
    Ok(())
}

fn append_curve_edges(list: &Bound<'_, PyAny>, out: &mut Vec<EdgeObj>) -> PyResult<()> {
    let seq = list
        .cast::<PySequence>()
        .map_err(|_| PyValueError::new_err("curve objects must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(dict) = item.cast::<PyDict>() {
            if let Some(edges) = curve_edges_from_dict(&dict) {
                out.extend(edges);
            }
        }
    }
    Ok(())
}

fn objects_to_chars_edges(
    objects: &Bound<'_, PyDict>,
    geom: &PageGeometry,
    arena: &mut PageArena,
) -> PyResult<(Vec<CharObj>, Vec<EdgeObj>)> {
    let mut chars = Vec::new();
    let mut edges = Vec::new();
    if let Some(list) = objects.get_item("char")? {
        append_chars_from_list(&list, geom.initial_doctop, arena, &mut chars)?;
    }
    if let Some(list) = objects.get_item("line")? {
        append_line_edges(&list, &mut edges)?;
    }
    if let Some(list) = objects.get_item("rect")? {
        append_rect_edges(&list, &mut edges)?;
    }
    if let Some(list) = objects.get_item("curve")? {
        append_curve_edges(&list, &mut edges)?;
    }
    Ok((chars, edges))
}

fn extract_tables_core_impl(
    doc: &PDFDocument,
    page_index: usize,
    geom: &PageGeometry,
    options: &ExtractOptions,
    settings: &TableSettings,
) -> Result<Vec<Vec<Vec<Option<String>>>>, String> {
    core_extract_tables_for_page_indexed(doc, page_index, geom, options.clone(), settings)
        .map_err(|e| format!("Failed to extract tables: {}", e))
}

fn extract_tables_core_internal(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    table_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    force_crop: bool,
) -> PyResult<Vec<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let options = table_extract_options(laparams);
    let geom = PageGeometry {
        page_bbox,
        mediabox,
        initial_doctop,
        force_crop,
    };
    let tables =
        py.detach(|| extract_tables_core_impl(&doc.inner, page_index, &geom, &options, &settings));
    tables.map_err(PyValueError::new_err)
}

/// Extract tables from a page using Rust table extraction.
#[pyfunction(name = "_extract_tables_core")]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, table_settings = None, laparams = None, force_crop = false))]
pub fn extract_tables_core(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    table_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    force_crop: bool,
) -> PyResult<Vec<Vec<Vec<Option<String>>>>> {
    extract_tables_core_internal(
        py,
        doc,
        page_index,
        page_bbox,
        mediabox,
        initial_doctop,
        table_settings,
        laparams,
        force_crop,
    )
}

/// Extract tables from page objects.
#[pyfunction(name = "_extract_tables_from_page_objects")]
#[pyo3(signature = (objects, page_bbox, mediabox, initial_doctop = 0.0, table_settings = None, force_crop = false))]
pub fn extract_tables_from_page_objects(
    py: Python<'_>,
    objects: &Bound<'_, PyAny>,
    page_bbox: &Bound<'_, PyAny>,
    mediabox: &Bound<'_, PyAny>,
    initial_doctop: f64,
    table_settings: Option<Py<PyAny>>,
    force_crop: bool,
) -> PyResult<Vec<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let page_bbox = parse_bbox(page_bbox, "page_bbox")?;
    let mediabox = parse_bbox(mediabox, "mediabox")?;
    let geom = PageGeometry {
        page_bbox,
        mediabox,
        initial_doctop,
        force_crop,
    };
    let dict = objects
        .cast::<PyDict>()
        .map_err(|_| PyValueError::new_err("page objects must be a dict"))?;
    let mut arena = PageArena::new();
    let (chars, edges) = objects_to_chars_edges(dict, &geom, &mut arena)?;
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
#[pyo3(signature = (data, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None))]
pub fn extract_text(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
) -> PyResult<String> {
    let options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };
    let cache_capacity = if caching { DEFAULT_CACHE_CAPACITY } else { 0 };
    let result = match pdf_input_from_py(data)? {
        PdfInput::Shared(bytes) => {
            let doc = PDFDocument::new_from_bytes_with_cache(bytes, password, cache_capacity)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.detach(|| core_extract_text_with_document(&doc, options))
        }
        PdfInput::Owned(bytes) => {
            let doc = PDFDocument::new_with_cache(bytes, password, cache_capacity)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.detach(|| core_extract_text_with_document(&doc, options))
        }
    };
    result.map_err(|e| PyValueError::new_err(format!("Failed to extract text: {}", e)))
}

/// Extract text from a PDF file path using memory-mapped I/O.
#[pyfunction]
#[pyo3(signature = (path, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None))]
pub fn extract_text_from_path(
    py: Python<'_>,
    path: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
) -> PyResult<String> {
    let file = File::open(path)
        .map_err(|e| PyValueError::new_err(format!("Failed to open PDF: {}", e)))?;
    // Safety: the file handle remains open for the duration of the map.
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
    let cache_capacity = if caching { DEFAULT_CACHE_CAPACITY } else { 0 };
    let doc = PDFDocument::new_from_mmap_with_cache(mmap, password, cache_capacity)
        .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;

    let options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };

    let result = py.detach(|| core_extract_text_with_document(&doc, options));
    result.map_err(|e| PyValueError::new_err(format!("Failed to extract text: {}", e)))
}

/// Extract pages (layout) from PDF bytes.
#[pyfunction]
#[pyo3(signature = (data, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None))]
pub fn extract_pages(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
) -> PyResult<Vec<PyLTPage>> {
    let options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };
    let cache_capacity = if caching { DEFAULT_CACHE_CAPACITY } else { 0 };
    let pages = match pdf_input_from_py(data)? {
        PdfInput::Shared(bytes) => {
            let doc = PDFDocument::new_from_bytes_with_cache(bytes, password, cache_capacity)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.detach(|| core_extract_pages_with_document(&doc, options))
        }
        PdfInput::Owned(bytes) => {
            let doc = PDFDocument::new_with_cache(bytes, password, cache_capacity)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.detach(|| core_extract_pages_with_document(&doc, options))
        }
    }
    .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {}", e)))?;
    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Extract pages (layout) from PDF bytes while exporting images.
#[pyfunction]
#[pyo3(signature = (data, output_dir, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None))]
pub fn extract_pages_with_images(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    output_dir: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
) -> PyResult<Vec<PyLTPage>> {
    let options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };
    let cache_capacity = if caching { DEFAULT_CACHE_CAPACITY } else { 0 };
    let pages = match pdf_input_from_py(data)? {
        PdfInput::Shared(bytes) => {
            let doc = PDFDocument::new_from_bytes_with_cache(bytes, password, cache_capacity)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.detach(|| core_extract_pages_with_images_with_document(&doc, options, output_dir))
        }
        PdfInput::Owned(bytes) => {
            let doc = PDFDocument::new_with_cache(bytes, password, cache_capacity)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.detach(|| core_extract_pages_with_images_with_document(&doc, options, output_dir))
        }
    }
    .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {}", e)))?;
    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Extract pages (layout) from a PDF file path using memory-mapped I/O.
#[pyfunction]
#[pyo3(signature = (path, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None))]
pub fn extract_pages_from_path(
    py: Python<'_>,
    path: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
) -> PyResult<Vec<PyLTPage>> {
    let file = File::open(path)
        .map_err(|e| PyValueError::new_err(format!("Failed to open PDF: {}", e)))?;
    // Safety: the file handle remains open for the duration of the map.
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
    let cache_capacity = if caching { DEFAULT_CACHE_CAPACITY } else { 0 };
    let doc = PDFDocument::new_from_mmap_with_cache(mmap, password, cache_capacity)
        .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;

    let mut options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };
    // Match pdfminer.high_level.extract_pages default behavior.
    if options.laparams.is_none() {
        options.laparams = Some(bolivar_core::layout::LAParams::default());
    }

    let pages = py
        .detach(|| core_extract_pages_with_document(&doc, options))
        .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {}", e)))?;
    Ok(pages.into_iter().map(ltpage_to_py).collect())
}

/// Extract pages (layout) from a PDF file path while exporting images.
#[pyfunction]
#[pyo3(signature = (path, output_dir, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None))]
pub fn extract_pages_with_images_from_path(
    py: Python<'_>,
    path: &str,
    output_dir: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
) -> PyResult<Vec<PyLTPage>> {
    let file = File::open(path)
        .map_err(|e| PyValueError::new_err(format!("Failed to open PDF: {}", e)))?;
    // Safety: the file handle remains open for the duration of the map.
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
    let cache_capacity = if caching { DEFAULT_CACHE_CAPACITY } else { 0 };
    let doc = PDFDocument::new_from_mmap_with_cache(mmap, password, cache_capacity)
        .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;

    let mut options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };
    if options.laparams.is_none() {
        options.laparams = Some(bolivar_core::layout::LAParams::default());
    }

    let pages = py
        .detach(|| core_extract_pages_with_images_with_document(&doc, options, output_dir))
        .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {}", e)))?;
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
    m.add_function(wrap_pyfunction!(extract_tables_from_document, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_core, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_from_page_objects, m)?)?;
    m.add_function(wrap_pyfunction!(repair_pdf, m)?)?;
    Ok(())
}
