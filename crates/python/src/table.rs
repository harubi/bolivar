//! Table extraction functions for Python.
//!
//! Provides functions for extracting tables from PDF pages and converting
//! page objects to chars/edges for table extraction.

use bolivar_core::arena::PageArena;
use bolivar_core::high_level::{
    ExtractOptions, extract_pages as core_extract_pages,
    extract_pages_with_document as core_extract_pages_with_document,
    extract_tables_for_pages as core_extract_tables_for_pages,
    extract_tables_with_document_geometries as core_extract_tables_with_document_geometries,
    extract_text as core_extract_text,
    extract_text_with_document as core_extract_text_with_document,
};
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::table::{
    BBox, CharObj, EdgeObj, Orientation, PageGeometry, WordObj, extract_table_from_objects,
    extract_tables_from_objects, extract_text_from_ltpage, extract_words_from_ltpage,
};
use bolivar_core::utils::HasBBox;
use memmap2::Mmap;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PySequence};
use std::fs::File;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use crate::convert::py_text_to_string;
use crate::document::{PdfInput, PyPDFDocument, PyPDFPage, pdf_input_from_py};
use crate::layout::{PyLTItem, PyLTPage, ltitem_to_py, ltpage_to_py};
use crate::params::{PyLAParams, parse_page_geometries, parse_table_settings, parse_text_settings};
use crate::stream::{async_runtime_poc, extract_pages_async};

#[cfg(test)]
pub static LAYOUT_CACHE_RELEASED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
pub fn reset_layout_cache_release_flag() {
    LAYOUT_CACHE_RELEASED.store(false, Ordering::SeqCst);
}

#[cfg(test)]
pub fn layout_cache_release_hit() -> bool {
    LAYOUT_CACHE_RELEASED.load(Ordering::SeqCst)
}

/// Convert page objects (chars, lines, rects, curves) to CharObj and EdgeObj.
pub fn page_objects_to_chars_edges(
    _py: Python<'_>,
    page: &Bound<'_, PyAny>,
) -> PyResult<(Vec<CharObj>, Vec<EdgeObj>, PageGeometry)> {
    fn extract_bbox(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<(f64, f64, f64, f64)> {
        if let Ok(bbox) = obj.extract::<(f64, f64, f64, f64)>() {
            return Ok(bbox);
        }
        let seq = obj
            .downcast::<PySequence>()
            .map_err(|_| PyValueError::new_err(format!("{name} must be a 4-item sequence")))?;
        if seq.len().unwrap_or(0) != 4 {
            return Err(PyValueError::new_err(format!("{name} must have 4 items")));
        }
        let mut vals = [0.0; 4];
        for i in 0..4 {
            vals[i] = seq.get_item(i)?.extract::<f64>()?;
        }
        Ok((vals[0], vals[1], vals[2], vals[3]))
    }

    let bbox_obj = page.getattr("bbox")?;
    let bbox = extract_bbox(&bbox_obj, "bbox")?;
    let mediabox = match page.getattr("mediabox") {
        Ok(v) => extract_bbox(&v, "mediabox").unwrap_or(bbox),
        Err(_) => bbox,
    };
    let initial_doctop: f64 = page
        .getattr("initial_doctop")
        .ok()
        .and_then(|v| v.extract().ok())
        .unwrap_or(0.0);
    let is_original: bool = page
        .getattr("is_original")
        .ok()
        .and_then(|v| v.extract().ok())
        .unwrap_or(true);

    let geom = PageGeometry {
        page_bbox: bbox,
        mediabox,
        initial_doctop,
        force_crop: !is_original,
    };

    let mut chars: Vec<CharObj> = Vec::new();
    let chars_obj = page.getattr("chars")?;
    let chars_seq = chars_obj
        .downcast::<PySequence>()
        .map_err(|_| PyValueError::new_err("page.chars must be a sequence"))?;
    let chars_len = chars_seq.len().unwrap_or(0);
    for i in 0..chars_len {
        let item = chars_seq.get_item(i)?;
        let dict = item
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err("char must be a dict"))?;
        let text_obj = dict
            .get_item("text")?
            .ok_or_else(|| PyValueError::new_err("char missing text"))?;
        let text = py_text_to_string(&text_obj)?;
        let x0: f64 = dict
            .get_item("x0")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let x1: f64 = dict
            .get_item("x1")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let top: f64 = dict
            .get_item("top")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let bottom: f64 = dict
            .get_item("bottom")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let width = dict
            .get_item("width")?
            .and_then(|v| v.extract().ok())
            .unwrap_or((x1 - x0).abs());
        let height = dict
            .get_item("height")?
            .and_then(|v| v.extract().ok())
            .unwrap_or((bottom - top).abs());
        let size = dict
            .get_item("size")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(height);
        let doctop = dict
            .get_item("doctop")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(top);
        let upright = dict
            .get_item("upright")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(true);
        chars.push(CharObj {
            text,
            x0,
            x1,
            top,
            bottom,
            doctop,
            width,
            height,
            size,
            upright,
        });
    }

    let mut edges: Vec<EdgeObj> = Vec::new();

    let lines_obj = page.getattr("lines")?;
    let lines_seq = lines_obj
        .downcast::<PySequence>()
        .map_err(|_| PyValueError::new_err("page.lines must be a sequence"))?;
    let lines_len = lines_seq.len().unwrap_or(0);
    for i in 0..lines_len {
        let item = lines_seq.get_item(i)?;
        let dict = item
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err("line must be a dict"))?;
        let x0: f64 = dict
            .get_item("x0")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let x1: f64 = dict
            .get_item("x1")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let top: f64 = dict
            .get_item("top")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let bottom: f64 = dict
            .get_item("bottom")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let width = dict
            .get_item("width")?
            .and_then(|v| v.extract().ok())
            .unwrap_or((x1 - x0).abs());
        let height = dict
            .get_item("height")?
            .and_then(|v| v.extract().ok())
            .unwrap_or((bottom - top).abs());
        let orientation = if (top - bottom).abs() < f64::EPSILON {
            Some(Orientation::Horizontal)
        } else if (x0 - x1).abs() < f64::EPSILON {
            Some(Orientation::Vertical)
        } else {
            None
        };
        edges.push(EdgeObj {
            x0,
            x1,
            top,
            bottom,
            width,
            height,
            orientation,
            object_type: "line",
        });
    }

    let rects_obj = page.getattr("rects")?;
    let rects_seq = rects_obj
        .downcast::<PySequence>()
        .map_err(|_| PyValueError::new_err("page.rects must be a sequence"))?;
    let rects_len = rects_seq.len().unwrap_or(0);
    for i in 0..rects_len {
        let item = rects_seq.get_item(i)?;
        let dict = item
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err("rect must be a dict"))?;
        let x0: f64 = dict
            .get_item("x0")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let x1: f64 = dict
            .get_item("x1")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let top: f64 = dict
            .get_item("top")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let bottom: f64 = dict
            .get_item("bottom")?
            .and_then(|v| v.extract().ok())
            .unwrap_or(0.0);
        let bbox = BBox {
            x0,
            x1,
            top,
            bottom,
        };
        edges.extend(vec![
            EdgeObj {
                x0: bbox.x0,
                x1: bbox.x1,
                top: bbox.top,
                bottom: bbox.top,
                width: bbox.x1 - bbox.x0,
                height: 0.0,
                orientation: Some(Orientation::Horizontal),
                object_type: "rect_edge",
            },
            EdgeObj {
                x0: bbox.x0,
                x1: bbox.x1,
                top: bbox.bottom,
                bottom: bbox.bottom,
                width: bbox.x1 - bbox.x0,
                height: 0.0,
                orientation: Some(Orientation::Horizontal),
                object_type: "rect_edge",
            },
            EdgeObj {
                x0: bbox.x0,
                x1: bbox.x0,
                top: bbox.top,
                bottom: bbox.bottom,
                width: 0.0,
                height: bbox.bottom - bbox.top,
                orientation: Some(Orientation::Vertical),
                object_type: "rect_edge",
            },
            EdgeObj {
                x0: bbox.x1,
                x1: bbox.x1,
                top: bbox.top,
                bottom: bbox.bottom,
                width: 0.0,
                height: bbox.bottom - bbox.top,
                orientation: Some(Orientation::Vertical),
                object_type: "rect_edge",
            },
        ]);
    }

    let curves_obj = page.getattr("curves")?;
    let curves_seq = curves_obj
        .downcast::<PySequence>()
        .map_err(|_| PyValueError::new_err("page.curves must be a sequence"))?;
    let curves_len = curves_seq.len().unwrap_or(0);
    for i in 0..curves_len {
        let item = curves_seq.get_item(i)?;
        let dict = item
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err("curve must be a dict"))?;
        let pts_obj = dict.get_item("pts")?;
        if let Some(pts_obj) = pts_obj
            && let Ok(pts) = pts_obj.extract::<Vec<(f64, f64)>>()
        {
            for pair in pts.windows(2) {
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
                    object_type: "curve_edge",
                });
            }
        }
    }

    Ok((chars, edges, geom))
}

fn word_obj_to_py(py: Python<'_>, w: &WordObj) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    d.set_item("text", &w.text)?;
    d.set_item("x0", w.x0)?;
    d.set_item("x1", w.x1)?;
    d.set_item("top", w.top)?;
    d.set_item("bottom", w.bottom)?;
    d.set_item("doctop", w.doctop)?;
    d.set_item("width", w.width)?;
    d.set_item("height", w.height)?;
    d.set_item("upright", if w.upright { 1 } else { 0 })?;
    d.set_item(
        "direction",
        match w.direction {
            bolivar_core::table::TextDir::Ttb => "ttb",
            bolivar_core::table::TextDir::Btt => "btt",
            bolivar_core::table::TextDir::Ltr => "ltr",
            bolivar_core::table::TextDir::Rtl => "rtl",
        },
    )?;
    Ok(d.into_any().unbind())
}

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
    let resources = page.resources.clone();
    let contents = page.contents.clone();

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

    // Convert to Python types (preserve layout tree)
    let items: Vec<PyLTItem> = ltpage.iter().map(ltitem_to_py).collect();

    Ok(PyLTPage {
        pageid: ltpage.pageid,
        rotate: ltpage.rotate,
        bbox: (ltpage.x0(), ltpage.y0(), ltpage.x1(), ltpage.y1()),
        items,
    })
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

    Ok(pages.iter().map(ltpage_to_py).collect())
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

/// Extract tables from specific pages using per-page geometry.
#[pyfunction]
#[pyo3(signature = (doc, page_numbers, geometries, table_settings = None, laparams = None))]
pub fn extract_tables_from_document_pages(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_numbers: &Bound<'_, PyAny>,
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
    let seq = page_numbers
        .downcast::<PySequence>()
        .map_err(|_| PyValueError::new_err("page_numbers must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    let mut pages = Vec::with_capacity(len as usize);
    for idx in 0..len {
        pages.push(seq.get_item(idx)?.extract::<usize>()?);
    }
    let geoms = parse_page_geometries(geometries)?;
    let tables = py
        .detach(|| core_extract_tables_for_pages(&doc.inner, &pages, &geoms, options, &settings))
        .map_err(|e| PyValueError::new_err(format!("Failed to extract tables: {}", e)))?;

    Ok(tables)
}

/// Extract tables from a page using Rust table extraction.
#[pyfunction]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, table_settings = None, laparams = None, force_crop = false))]
pub fn extract_tables_from_page(
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
    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: la,
    };
    let geom = PageGeometry {
        page_bbox,
        mediabox,
        initial_doctop,
        force_crop,
    };
    let page_numbers = vec![page_index];
    let geoms = vec![geom];
    let tables: Result<_, String> = py.detach(|| {
        let tables =
            core_extract_tables_for_pages(&doc.inner, &page_numbers, &geoms, options, &settings)
                .map_err(|e| format!("Failed to extract tables: {}", e))?;
        tables
            .get(0)
            .cloned()
            .ok_or_else(|| "page_index out of range".to_string())
    });
    tables.map_err(|e| PyValueError::new_err(e))
}

/// Extract a single table from a page using Rust table extraction.
#[pyfunction]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, table_settings = None, laparams = None, force_crop = false))]
pub fn extract_table_from_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    table_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    force_crop: bool,
) -> PyResult<Option<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: la,
    };
    let geom = PageGeometry {
        page_bbox,
        mediabox,
        initial_doctop,
        force_crop,
    };
    let page_numbers = vec![page_index];
    let geoms = vec![geom];
    let table: Result<_, String> = py.detach(|| {
        let tables =
            core_extract_tables_for_pages(&doc.inner, &page_numbers, &geoms, options, &settings)
                .map_err(|e| format!("Failed to extract tables: {}", e))?;
        let page_tables = tables
            .get(0)
            .cloned()
            .ok_or_else(|| "page_index out of range".to_string())?;
        if page_tables.is_empty() {
            return Ok(None);
        }
        let mut best = 0usize;
        for (idx, table) in page_tables.iter().enumerate().skip(1) {
            if table.iter().map(|row| row.len()).sum::<usize>()
                > page_tables[best].iter().map(|row| row.len()).sum::<usize>()
            {
                best = idx;
            }
        }
        Ok(Some(page_tables[best].clone()))
    });
    table.map_err(|e| PyValueError::new_err(e))
}

/// Extract tables from a filtered/cropped pdfplumber Page using Rust table extraction.
#[pyfunction]
#[pyo3(signature = (page, table_settings = None))]
pub fn extract_tables_from_page_filtered(
    py: Python<'_>,
    page: &Bound<'_, PyAny>,
    table_settings: Option<Py<PyAny>>,
) -> PyResult<Vec<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let (chars, edges, geom) = page_objects_to_chars_edges(py, page)?;
    py.detach(|| Ok(extract_tables_from_objects(chars, edges, &geom, &settings)))
}

/// Extract a single table from a filtered/cropped pdfplumber Page using Rust table extraction.
#[pyfunction]
#[pyo3(signature = (page, table_settings = None))]
pub fn extract_table_from_page_filtered(
    py: Python<'_>,
    page: &Bound<'_, PyAny>,
    table_settings: Option<Py<PyAny>>,
) -> PyResult<Option<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let (chars, edges, geom) = page_objects_to_chars_edges(py, page)?;
    py.detach(|| Ok(extract_table_from_objects(chars, edges, &geom, &settings)))
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

/// Extract words from a page using Rust text extraction.
#[pyfunction]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, text_settings = None, laparams = None, force_crop = false))]
pub fn extract_words_from_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    text_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    force_crop: bool,
) -> PyResult<Vec<Py<PyAny>>> {
    let settings = parse_text_settings(py, text_settings)?;
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
    let ltpage = pages
        .get(page_index)
        .ok_or_else(|| PyValueError::new_err("page_index out of range"))?;
    let geom = PageGeometry {
        page_bbox,
        mediabox,
        initial_doctop,
        force_crop,
    };
    let words = py.detach(|| extract_words_from_ltpage(ltpage, &geom, settings));
    let mut out = Vec::with_capacity(words.len());
    for w in &words {
        out.push(word_obj_to_py(py, w)?);
    }
    Ok(out)
}

/// Extract text from a page using Rust text extraction.
#[pyfunction]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, text_settings = None, laparams = None, force_crop = false))]
pub fn extract_text_from_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    text_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    force_crop: bool,
) -> PyResult<String> {
    let settings = parse_text_settings(py, text_settings)?;
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
    let ltpage = pages
        .get(page_index)
        .ok_or_else(|| PyValueError::new_err("page_index out of range"))?;
    let geom = PageGeometry {
        page_bbox,
        mediabox,
        initial_doctop,
        force_crop,
    };
    Ok(py.detach(|| extract_text_from_ltpage(ltpage, &geom, settings)))
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
    let result = match pdf_input_from_py(data)? {
        PdfInput::Shared(bytes) => {
            let doc = PDFDocument::new_from_bytes(bytes, password)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.detach(|| core_extract_text_with_document(&doc, options))
        }
        PdfInput::Owned(bytes) => py.detach(|| core_extract_text(&bytes, Some(options))),
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
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
    let doc = PDFDocument::new_from_mmap(mmap, password)
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
    let pages = match pdf_input_from_py(data)? {
        PdfInput::Shared(bytes) => {
            let doc = PDFDocument::new_from_bytes(bytes, password)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.detach(|| core_extract_pages_with_document(&doc, options))
        }
        PdfInput::Owned(bytes) => py.detach(|| {
            let iter = core_extract_pages(&bytes, Some(options))?;
            iter.collect::<bolivar_core::error::Result<Vec<_>>>()
        }),
    }
    .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {}", e)))?;
    Ok(pages.iter().map(ltpage_to_py).collect())
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
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
    let doc = PDFDocument::new_from_mmap(mmap, password)
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
    Ok(pages.iter().map(ltpage_to_py).collect())
}

/// Register the table module functions with the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract_text, m)?)?;
    m.add_function(wrap_pyfunction!(extract_text_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(async_runtime_poc, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_async, m)?)?;
    m.add_function(wrap_pyfunction!(process_page, m)?)?;
    m.add_function(wrap_pyfunction!(process_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_from_document, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_from_document_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_from_page, m)?)?;
    m.add_function(wrap_pyfunction!(extract_table_from_page, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_from_page_filtered, m)?)?;
    m.add_function(wrap_pyfunction!(extract_table_from_page_filtered, m)?)?;
    m.add_function(wrap_pyfunction!(repair_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(extract_words_from_page, m)?)?;
    m.add_function(wrap_pyfunction!(extract_text_from_page, m)?)?;
    Ok(())
}
