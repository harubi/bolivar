//! Python bindings for bolivar PDF library
//!
//! This crate provides PyO3 bindings to expose bolivar's PDF parsing
//! functionality to Python, with a pdfminer.six-compatible API.

use bolivar_core::high_level::{
    ExtractOptions, extract_pages as core_extract_pages,
    extract_pages_with_document as core_extract_pages_with_document,
    extract_text as core_extract_text,
    extract_text_with_document as core_extract_text_with_document,
};
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdftypes::PDFObject;
use bolivar_core::table::{
    BBox, EdgeObj, ExplicitLine, Orientation, PageGeometry, TableSettings, TextDir, TextSettings,
    WordObj, extract_table_from_ltpage, extract_tables_from_ltpage, extract_text_from_ltpage,
    extract_words_from_ltpage,
};
use bolivar_core::utils::HasBBox;
use memmap2::Mmap;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySequence, PyType};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::sync::{Arc, Mutex};

/// Convert a PDFObject to a string representation for Python.
fn pdf_object_to_string(obj: &PDFObject) -> String {
    match obj {
        PDFObject::Null => "null".to_string(),
        PDFObject::Bool(b) => b.to_string(),
        PDFObject::Int(i) => i.to_string(),
        PDFObject::Real(r) => r.to_string(),
        PDFObject::Name(n) => n.clone(),
        PDFObject::String(s) => {
            // Try to decode as UTF-8, fall back to lossy
            String::from_utf8(s.clone()).unwrap_or_else(|_| String::from_utf8_lossy(s).to_string())
        }
        PDFObject::Array(arr) => {
            let items: Vec<String> = arr.iter().map(pdf_object_to_string).collect();
            format!("[{}]", items.join(", "))
        }
        PDFObject::Dict(dict) => {
            let items: Vec<String> = dict
                .iter()
                .map(|(k, v)| format!("{}: {}", k, pdf_object_to_string(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        PDFObject::Stream(_) => "<stream>".to_string(),
        PDFObject::Ref(objref) => format!("{} {} R", objref.objid, objref.genno),
    }
}

/// Convert a PDFObject to a Python object, resolving references.
/// Uses visited set to prevent infinite loops on circular references.
fn name_to_psliteral(py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
    let psparser = py.import("pdfminer.psparser")?;
    let cls = psparser.getattr("PSLiteral")?;
    let obj = cls.call1((PyBytes::new(py, name.as_bytes()),))?;
    Ok(obj.into_any().unbind())
}

fn pdf_object_to_py_internal(
    py: Python<'_>,
    obj: &PDFObject,
    doc: &PDFDocument,
    visited: &mut HashSet<(u32, u32)>,
    name_as_psliteral: bool,
    resolve_refs: bool,
    py_doc: Option<&Py<PyAny>>,
) -> PyResult<Py<PyAny>> {
    match obj {
        PDFObject::Null => Ok(py.None()),
        PDFObject::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any().unbind()),
        PDFObject::Int(i) => Ok(i.into_pyobject(py)?.to_owned().into_any().unbind()),
        PDFObject::Real(f) => Ok(f.into_pyobject(py)?.to_owned().into_any().unbind()),
        PDFObject::Name(n) => {
            if name_as_psliteral {
                name_to_psliteral(py, n)
            } else {
                Ok(n.clone().into_pyobject(py)?.to_owned().into_any().unbind())
            }
        }
        PDFObject::String(s) => {
            // Return as bytes like pdfminer.six - caller handles decoding
            Ok(PyBytes::new(py, s).into_any().unbind())
        }
        PDFObject::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(pdf_object_to_py_internal(
                    py,
                    item,
                    doc,
                    visited,
                    name_as_psliteral,
                    resolve_refs,
                    py_doc,
                )?)?;
            }
            Ok(list.into_any().unbind())
        }
        PDFObject::Dict(dict) => {
            let py_dict = PyDict::new(py);
            for (k, v) in dict.iter() {
                py_dict.set_item(
                    k,
                    pdf_object_to_py_internal(
                        py,
                        v,
                        doc,
                        visited,
                        name_as_psliteral,
                        resolve_refs,
                        py_doc,
                    )?,
                )?;
            }
            Ok(py_dict.into_any().unbind())
        }
        PDFObject::Stream(stream) => {
            // Return attrs part only for annotations
            let py_dict = PyDict::new(py);
            for (k, v) in stream.attrs.iter() {
                py_dict.set_item(
                    k,
                    pdf_object_to_py_internal(
                        py,
                        v,
                        doc,
                        visited,
                        name_as_psliteral,
                        resolve_refs,
                        py_doc,
                    )?,
                )?;
            }
            Ok(py_dict.into_any().unbind())
        }
        PDFObject::Ref(objref) => {
            if !resolve_refs {
                let pdftypes = py.import("pdfminer.pdftypes")?;
                let cls = pdftypes.getattr("PDFObjRef")?;
                let doc_obj = if let Some(doc_obj) = py_doc {
                    doc_obj.clone_ref(py).into_any()
                } else {
                    py.None()
                };
                let obj = cls.call1((doc_obj, objref.objid))?;
                return Ok(obj.into_any().unbind());
            }
            let id = (objref.objid, objref.genno);
            // Cycle detection
            if !visited.insert(id) {
                return Ok(py.None()); // Break cycle
            }
            match doc.getobj(objref.objid) {
                Ok(resolved) => pdf_object_to_py_internal(
                    py,
                    &resolved,
                    doc,
                    visited,
                    name_as_psliteral,
                    resolve_refs,
                    py_doc,
                ),
                Err(_) => Ok(py.None()),
            }
        }
    }
}

/// Convert a PDFObject to a Python object, resolving references.
/// Uses visited set to prevent infinite loops on circular references.
fn pdf_object_to_py(
    py: Python<'_>,
    obj: &PDFObject,
    doc: &PDFDocument,
    visited: &mut HashSet<(u32, u32)>,
) -> PyResult<Py<PyAny>> {
    pdf_object_to_py_internal(py, obj, doc, visited, false, true, None)
}

fn parse_text_dir(value: &str) -> Result<TextDir, PyErr> {
    match value {
        "ttb" => Ok(TextDir::Ttb),
        "btt" => Ok(TextDir::Btt),
        "ltr" => Ok(TextDir::Ltr),
        "rtl" => Ok(TextDir::Rtl),
        _ => Err(PyValueError::new_err(format!(
            "Invalid text direction: {}",
            value
        ))),
    }
}

fn apply_text_settings_from_dict(
    settings: &mut TextSettings,
    dict: &Bound<'_, PyDict>,
) -> PyResult<()> {
    let mut tolerance: Option<f64> = None;
    let mut x_set = false;
    let mut y_set = false;

    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        match key.as_str() {
            "x_tolerance" => {
                settings.x_tolerance = v.extract()?;
                x_set = true;
            }
            "y_tolerance" => {
                settings.y_tolerance = v.extract()?;
                y_set = true;
            }
            "tolerance" => {
                tolerance = Some(v.extract()?);
            }
            "x_tolerance_ratio" => settings.x_tolerance_ratio = Some(v.extract()?),
            "y_tolerance_ratio" => settings.y_tolerance_ratio = Some(v.extract()?),
            "keep_blank_chars" => settings.keep_blank_chars = v.extract()?,
            "use_text_flow" => settings.use_text_flow = v.extract()?,
            "vertical_ttb" => settings.vertical_ttb = v.extract()?,
            "horizontal_ltr" => settings.horizontal_ltr = v.extract()?,
            "line_dir" => settings.line_dir = parse_text_dir(&v.extract::<String>()?)?,
            "char_dir" => settings.char_dir = parse_text_dir(&v.extract::<String>()?)?,
            "line_dir_rotated" => {
                let val: String = v.extract()?;
                settings.line_dir_rotated = Some(parse_text_dir(&val)?);
            }
            "char_dir_rotated" => {
                let val: String = v.extract()?;
                settings.char_dir_rotated = Some(parse_text_dir(&val)?);
            }
            "split_at_punctuation" => settings.split_at_punctuation = v.extract()?,
            "expand_ligatures" => settings.expand_ligatures = v.extract()?,
            "layout" => settings.layout = v.extract()?,
            _ => {}
        }
    }

    if let Some(tol) = tolerance {
        if !x_set {
            settings.x_tolerance = tol;
        }
        if !y_set {
            settings.y_tolerance = tol;
        }
    }

    Ok(())
}

fn parse_text_settings(py: Python<'_>, text_settings: Option<Py<PyAny>>) -> PyResult<TextSettings> {
    let mut settings = TextSettings::default();
    let Some(obj) = text_settings else {
        return Ok(settings);
    };
    let obj = obj.bind(py);
    if obj.is_none() {
        return Ok(settings);
    }
    let dict = obj
        .downcast::<PyDict>()
        .map_err(|_| PyValueError::new_err("text_settings must be a dict when provided"))?;
    apply_text_settings_from_dict(&mut settings, dict)?;
    Ok(settings)
}

fn parse_table_settings(
    py: Python<'_>,
    table_settings: Option<Py<PyAny>>,
) -> PyResult<TableSettings> {
    let mut settings = TableSettings::default();
    let Some(obj) = table_settings else {
        return Ok(settings);
    };
    let obj = obj.bind(py);
    if obj.is_none() {
        return Ok(settings);
    }
    let dict = obj
        .downcast::<PyDict>()
        .map_err(|_| PyValueError::new_err("table_settings must be a dict when provided"))?;

    let mut text_settings = settings.text_settings.clone();

    fn parse_explicit_lines(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Vec<ExplicitLine>> {
        if obj.is_none() {
            return Ok(Vec::new());
        }
        let seq = obj
            .downcast::<PySequence>()
            .map_err(|_| PyValueError::new_err("explicit lines must be a list/tuple"))?;
        let mut out = Vec::new();
        let len = seq.len().unwrap_or(0);
        for i in 0..len {
            let item = seq.get_item(i)?;
            if let Ok(val) = item.extract::<f64>() {
                out.push(ExplicitLine::Coord(val));
                continue;
            }
            if let Ok(dict) = item.downcast::<PyDict>() {
                if let Some(pts_obj) = dict.get_item("pts")? {
                    let pts: Vec<(f64, f64)> = pts_obj.extract()?;
                    out.push(ExplicitLine::Curve(pts));
                    continue;
                }
                let obj_type: Option<String> = dict
                    .get_item("object_type")?
                    .and_then(|v| v.extract().ok());
                let x0: Option<f64> = dict.get_item("x0")?.and_then(|v| v.extract().ok());
                let x1: Option<f64> = dict.get_item("x1")?.and_then(|v| v.extract().ok());
                let top: Option<f64> = dict.get_item("top")?.and_then(|v| v.extract().ok());
                let bottom: Option<f64> = dict.get_item("bottom")?.and_then(|v| v.extract().ok());
                if let (Some(x0), Some(x1), Some(top), Some(bottom)) = (x0, x1, top, bottom) {
                    if obj_type.as_deref() == Some("rect") {
                        out.push(ExplicitLine::Rect(BBox {
                            x0,
                            x1,
                            top,
                            bottom,
                        }));
                        continue;
                    }
                    let width = dict
                        .get_item("width")?
                        .and_then(|v| v.extract().ok())
                        .unwrap_or(x1 - x0);
                    let height = dict
                        .get_item("height")?
                        .and_then(|v| v.extract().ok())
                        .unwrap_or(bottom - top);
                    let orientation = dict
                        .get_item("orientation")?
                        .and_then(|v| v.extract::<String>().ok())
                        .and_then(|o| match o.as_str() {
                            "v" => Some(Orientation::Vertical),
                            "h" => Some(Orientation::Horizontal),
                            _ => None,
                        })
                        .or_else(|| {
                            if (x0 - x1).abs() < 1e-9 {
                                Some(Orientation::Vertical)
                            } else if (top - bottom).abs() < 1e-9 {
                                Some(Orientation::Horizontal)
                            } else {
                                None
                            }
                        });
                    out.push(ExplicitLine::Edge(EdgeObj {
                        x0,
                        x1,
                        top,
                        bottom,
                        width,
                        height,
                        orientation,
                        object_type: "explicit_edge",
                    }));
                    continue;
                }
            }
        }
        Ok(out)
    }

    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        if let Some(stripped) = key.strip_prefix("text_") {
            let tmp = PyDict::new(py);
            tmp.set_item(stripped, v)?;
            apply_text_settings_from_dict(&mut text_settings, &tmp)?;
            continue;
        }

        match key.as_str() {
            "vertical_strategy" => settings.vertical_strategy = v.extract()?,
            "horizontal_strategy" => settings.horizontal_strategy = v.extract()?,
            "snap_tolerance" => {
                let val: f64 = v.extract()?;
                settings.snap_x_tolerance = val;
                settings.snap_y_tolerance = val;
            }
            "snap_x_tolerance" => settings.snap_x_tolerance = v.extract()?,
            "snap_y_tolerance" => settings.snap_y_tolerance = v.extract()?,
            "join_tolerance" => {
                let val: f64 = v.extract()?;
                settings.join_x_tolerance = val;
                settings.join_y_tolerance = val;
            }
            "join_x_tolerance" => settings.join_x_tolerance = v.extract()?,
            "join_y_tolerance" => settings.join_y_tolerance = v.extract()?,
            "edge_min_length" => settings.edge_min_length = v.extract()?,
            "edge_min_length_prefilter" => settings.edge_min_length_prefilter = v.extract()?,
            "min_words_vertical" => settings.min_words_vertical = v.extract()?,
            "min_words_horizontal" => settings.min_words_horizontal = v.extract()?,
            "intersection_tolerance" => {
                let val: f64 = v.extract()?;
                settings.intersection_x_tolerance = val;
                settings.intersection_y_tolerance = val;
            }
            "intersection_x_tolerance" => settings.intersection_x_tolerance = v.extract()?,
            "intersection_y_tolerance" => settings.intersection_y_tolerance = v.extract()?,
            "text_settings" => {
                if !v.is_none() {
                    let ts_dict = v.downcast::<PyDict>().map_err(|_| {
                        PyValueError::new_err("text_settings must be a dict when provided")
                    })?;
                    apply_text_settings_from_dict(&mut text_settings, ts_dict)?;
                }
            }
            "explicit_vertical_lines" => {
                settings.explicit_vertical_lines = parse_explicit_lines(py, &v)?;
            }
            "explicit_horizontal_lines" => {
                settings.explicit_horizontal_lines = parse_explicit_lines(py, &v)?;
            }
            _ => {}
        }
    }

    settings.text_settings = text_settings;
    Ok(settings)
}

/// Layout analysis parameters.
///
/// Controls how characters are grouped into lines, words, and text boxes.
#[pyclass(name = "LAParams")]
#[derive(Debug, Clone)]
pub struct PyLAParams {
    #[pyo3(get, set)]
    pub line_overlap: f64,
    #[pyo3(get, set)]
    pub char_margin: f64,
    #[pyo3(get, set)]
    pub line_margin: f64,
    #[pyo3(get, set)]
    pub word_margin: f64,
    #[pyo3(get, set)]
    pub boxes_flow: Option<f64>,
    #[pyo3(get, set)]
    pub detect_vertical: bool,
    #[pyo3(get, set)]
    pub all_texts: bool,
}

#[pymethods]
impl PyLAParams {
    #[new]
    #[pyo3(signature = (
        line_overlap = 0.5,
        char_margin = 2.0,
        line_margin = 0.5,
        word_margin = 0.1,
        boxes_flow = Some(0.5),
        detect_vertical = false,
        all_texts = false
    ))]
    fn new(
        line_overlap: f64,
        char_margin: f64,
        line_margin: f64,
        word_margin: f64,
        boxes_flow: Option<f64>,
        detect_vertical: bool,
        all_texts: bool,
    ) -> Self {
        Self {
            line_overlap,
            char_margin,
            line_margin,
            word_margin,
            boxes_flow,
            detect_vertical,
            all_texts,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "LAParams(line_overlap={}, char_margin={}, line_margin={}, word_margin={}, boxes_flow={:?}, detect_vertical={}, all_texts={})",
            self.line_overlap,
            self.char_margin,
            self.line_margin,
            self.word_margin,
            self.boxes_flow,
            self.detect_vertical,
            self.all_texts
        )
    }
}

impl From<PyLAParams> for bolivar_core::layout::LAParams {
    fn from(py: PyLAParams) -> Self {
        bolivar_core::layout::LAParams::new(
            py.line_overlap,
            py.char_margin,
            py.line_margin,
            py.word_margin,
            py.boxes_flow,
            py.detect_vertical,
            py.all_texts,
        )
    }
}

impl From<bolivar_core::layout::LAParams> for PyLAParams {
    fn from(la: bolivar_core::layout::LAParams) -> Self {
        Self {
            line_overlap: la.line_overlap,
            char_margin: la.char_margin,
            line_margin: la.line_margin,
            word_margin: la.word_margin,
            boxes_flow: la.boxes_flow,
            detect_vertical: la.detect_vertical,
            all_texts: la.all_texts,
        }
    }
}

/// PDF Document - main entry point for PDF parsing.
///
/// Creates a document from PDF bytes and provides access to pages.
#[pyclass(name = "PDFDocument")]
pub struct PyPDFDocument {
    /// The underlying Rust PDFDocument (owns the data via Arc)
    inner: PDFDocument,
    /// Cache resolved objects for faster PDFObjRef resolution
    resolved_cache: Mutex<HashMap<u32, Py<PyAny>>>,
}

#[pymethods]
impl PyPDFDocument {
    /// Create a new PDFDocument from PDF bytes.
    ///
    /// Args:
    ///     data: Raw PDF file contents as bytes
    ///     password: Optional password for encrypted PDFs (default: empty)
    ///
    /// Returns:
    ///     PDFDocument instance
    ///
    /// Raises:
    ///     ValueError: If the PDF cannot be parsed
    #[new]
    #[pyo3(signature = (data, password = ""))]
    fn new(data: &[u8], password: &str) -> PyResult<Self> {
        let doc = PDFDocument::new(data, password)
            .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
        Ok(Self {
            inner: doc,
            resolved_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Create a new PDFDocument from a file path using memory-mapped I/O.
    ///
    /// Args:
    ///     path: Path to PDF file
    ///     password: Optional password for encrypted PDFs (default: empty)
    ///
    /// Returns:
    ///     PDFDocument instance
    ///
    /// Raises:
    ///     ValueError: If the PDF cannot be parsed
    #[classmethod]
    #[pyo3(signature = (path, password = ""))]
    fn from_path(_cls: &Bound<'_, PyType>, path: &str, password: &str) -> PyResult<Self> {
        let file = File::open(path)
            .map_err(|e| PyValueError::new_err(format!("Failed to open PDF: {}", e)))?;
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
        let doc = PDFDocument::new_from_mmap(Arc::new(mmap), password)
            .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
        Ok(Self {
            inner: doc,
            resolved_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Get an iterator over pages in the document.
    ///
    /// Returns:
    ///     List of PDFPage objects
    fn get_pages(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Vec<PyPDFPage>> {
        let mut pages = Vec::new();
        let py_doc = unsafe { Py::<PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        for (idx, page_result) in
            bolivar_core::pdfpage::PDFPage::create_pages(&slf.inner).enumerate()
        {
            let page = page_result
                .map_err(|e| PyValueError::new_err(format!("Failed to get page {}: {}", idx, e)))?;
            pages.push(PyPDFPage::from_core(py, page, &slf.inner, Some(&py_doc))?);
        }
        Ok(pages)
    }

    /// Get document info dictionaries.
    ///
    /// Returns:
    ///     List of dictionaries containing document metadata (Producer, Creator, etc.)
    #[getter]
    fn info(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        let py_doc = unsafe { Py::<PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        for dict in slf.inner.info().iter() {
            let py_dict = PyDict::new(py);
            for (k, v) in dict.iter() {
                let mut visited = HashSet::new();
                let py_val = pdf_object_to_py_internal(
                    py,
                    v,
                    &slf.inner,
                    &mut visited,
                    true,
                    true,
                    Some(&py_doc),
                )?;
                py_dict.set_item(k, py_val)?;
            }
            out.push(py_dict.into_any().unbind());
        }
        Ok(out)
    }

    /// Get xref trailers as raw PDF objects (references preserved).
    #[getter]
    fn xrefs(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        let py_doc = unsafe { Py::<PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        for (_fallback, trailer) in slf.inner.get_trailers() {
            let py_dict = PyDict::new(py);
            for (k, v) in trailer.iter() {
                let mut visited = HashSet::new();
                let py_val = pdf_object_to_py_internal(
                    py,
                    v,
                    &slf.inner,
                    &mut visited,
                    true,
                    false,
                    Some(&py_doc),
                )?;
                py_dict.set_item(k, py_val)?;
            }
            out.push(py_dict.into_any().unbind());
        }
        Ok(out)
    }

    /// Get document catalog dictionary.
    #[getter]
    fn catalog(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let py_dict = PyDict::new(py);
        let catalog = slf.inner.catalog().clone();
        let py_doc = unsafe { Py::<PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        for (k, v) in catalog.iter() {
            let mut visited = HashSet::new();
            let py_val = pdf_object_to_py_internal(
                py,
                v,
                &slf.inner,
                &mut visited,
                true,
                false,
                Some(&py_doc),
            )?;
            py_dict.set_item(k, py_val)?;
        }
        Ok(py_dict.into_any().unbind())
    }

    /// Resolve an indirect object by ID.
    fn getobj(slf: PyRef<'_, Self>, py: Python<'_>, objid: u32) -> PyResult<Py<PyAny>> {
        if let Ok(cache) = slf.resolved_cache.lock() {
            if let Some(obj) = cache.get(&objid) {
                return Ok(obj.clone_ref(py));
            }
        }
        let obj = slf
            .inner
            .getobj(objid)
            .map_err(|e| PyValueError::new_err(format!("Failed to resolve object: {}", e)))?;
        let mut visited = HashSet::new();
        let py_doc = unsafe { Py::<PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        let py_obj = pdf_object_to_py_internal(
            py,
            &obj,
            &slf.inner,
            &mut visited,
            true,
            false,
            Some(&py_doc),
        )?;
        if let Ok(mut cache) = slf.resolved_cache.lock() {
            cache.insert(objid, py_obj.clone_ref(py));
        }
        Ok(py_obj)
    }
}

/// PDF Page - represents a single page in a PDF document.
#[pyclass(name = "PDFPage")]
pub struct PyPDFPage {
    /// Page object ID
    #[pyo3(get)]
    pub pageid: u32,
    /// Media box (physical page size) as (x0, y0, x1, y1)
    #[pyo3(get)]
    pub mediabox: Option<(f64, f64, f64, f64)>,
    /// Crop box (if different from mediabox)
    #[pyo3(get)]
    pub cropbox: Option<(f64, f64, f64, f64)>,
    /// Bleed box (printing bleed area)
    #[pyo3(get)]
    pub bleedbox: Option<(f64, f64, f64, f64)>,
    /// Trim box (finished page size)
    #[pyo3(get)]
    pub trimbox: Option<(f64, f64, f64, f64)>,
    /// Art box (meaningful content area)
    #[pyo3(get)]
    pub artbox: Option<(f64, f64, f64, f64)>,
    /// Page rotation in degrees
    #[pyo3(get)]
    pub rotate: i64,
    /// Page label (logical page number)
    #[pyo3(get)]
    pub label: Option<String>,
    /// Internal: decoded page contents (for processing)
    contents: Vec<Vec<u8>>,
    /// Internal: page resources (serialized for later use)
    resources: std::collections::HashMap<String, bolivar_core::pdftypes::PDFObject>,
    /// Page annotations (resolved to Python objects)
    annots_list: Py<PyAny>,
    /// Full page attributes dict (resolved)
    attrs_dict: Py<PyAny>,
    /// Page resources dict (resolved)
    resources_dict: Py<PyAny>,
}

impl PyPDFPage {
    /// Create from core PDFPage, resolving annotations
    fn from_core(
        py: Python<'_>,
        page: bolivar_core::pdfpage::PDFPage,
        doc: &PDFDocument,
        py_doc: Option<&Py<PyAny>>,
    ) -> PyResult<Self> {
        // Extract and resolve annotations
        let annots_list: Py<PyAny> = if let Some(ref annots_obj) = page.annots {
            // Resolve if it's a reference
            let resolved = match annots_obj {
                PDFObject::Ref(objref) => doc
                    .getobj(objref.objid)
                    .unwrap_or_else(|_| annots_obj.clone()),
                _ => annots_obj.clone(),
            };

            // Convert array elements
            if let PDFObject::Array(arr) = resolved {
                let list = PyList::empty(py);
                for item in arr {
                    // Fresh visited set per annotation to prevent cross-contamination
                    let mut visited = HashSet::new();
                    let py_item = pdf_object_to_py(py, &item, doc, &mut visited)?;
                    if py_item.bind(py).is_none() {
                        continue;
                    }
                    list.append(py_item)?;
                }
                list.into_any().unbind()
            } else {
                PyList::empty(py).into_any().unbind()
            }
        } else {
            PyList::empty(py).into_any().unbind()
        };

        let attrs_dict = PyDict::new(py);
        for (k, v) in page.attrs.iter() {
            let mut visited = HashSet::new();
            let py_val =
                pdf_object_to_py_internal(py, v, doc, &mut visited, true, false, py_doc)?;
            attrs_dict.set_item(k, py_val)?;
        }

        let resources_dict = PyDict::new(py);
        for (k, v) in page.resources.iter() {
            let mut visited = HashSet::new();
            let py_val =
                pdf_object_to_py_internal(py, v, doc, &mut visited, true, false, py_doc)?;
            resources_dict.set_item(k, py_val)?;
        }

        Ok(Self {
            pageid: page.pageid,
            mediabox: page.mediabox.map(|b| (b[0], b[1], b[2], b[3])),
            cropbox: page.cropbox.map(|b| (b[0], b[1], b[2], b[3])),
            bleedbox: page.bleedbox.map(|b| (b[0], b[1], b[2], b[3])),
            trimbox: page.trimbox.map(|b| (b[0], b[1], b[2], b[3])),
            artbox: page.artbox.map(|b| (b[0], b[1], b[2], b[3])),
            rotate: page.rotate,
            label: page.label.clone(),
            contents: page.contents.clone(),
            resources: page.resources.clone(),
            annots_list,
            attrs_dict: attrs_dict.into_any().unbind(),
            resources_dict: resources_dict.into_any().unbind(),
        })
    }
}

#[pymethods]
impl PyPDFPage {
    fn __repr__(&self) -> String {
        format!(
            "PDFPage(pageid={}, mediabox={:?}, rotate={})",
            self.pageid, self.mediabox, self.rotate
        )
    }

    /// Get page annotations as list of dicts
    #[getter]
    fn annots(&self, py: Python<'_>) -> Py<PyAny> {
        self.annots_list.clone_ref(py)
    }

    /// Get page attributes dict (resolved)
    #[getter]
    fn attrs(&self, py: Python<'_>) -> Py<PyAny> {
        self.attrs_dict.clone_ref(py)
    }

    /// Get page resources dict (resolved)
    #[getter]
    fn resources(&self, py: Python<'_>) -> Py<PyAny> {
        self.resources_dict.clone_ref(py)
    }
}

/// Layout page - result of processing a PDF page.
#[pyclass(name = "LTPage")]
#[derive(Clone)]
pub struct PyLTPage {
    /// Page identifier (1-based page number)
    #[pyo3(get)]
    pub pageid: i32,
    /// Page rotation in degrees
    #[pyo3(get)]
    pub rotate: f64,
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
    /// Layout items on this page
    items: Vec<PyLTItem>,
}

#[pymethods]
impl PyLTPage {
    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTPage(pageid={}, bbox={:?})", self.pageid, self.bbox)
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Iterator over LTPage items
#[pyclass]
pub struct PyLTPageIter {
    items: Vec<PyLTItem>,
    index: usize,
}

#[pymethods]
impl PyLTPageIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<PyLTItem> {
        if slf.index < slf.items.len() {
            let item = slf.items[slf.index].clone();
            slf.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

/// Layout item - can be a character, line, box, etc.
#[derive(Clone)]
pub enum PyLTItem {
    Char(PyLTChar),
    Rect(PyLTRect),
    Line(PyLTLine),
    Curve(PyLTCurve),
    Anno(PyLTAnno),
    TextLineH(PyLTTextLineHorizontal),
    TextLineV(PyLTTextLineVertical),
    TextBoxH(PyLTTextBoxHorizontal),
    TextBoxV(PyLTTextBoxVertical),
    Image(PyLTImage),
    Figure(PyLTFigure),
    Page(PyLTPage),
}

impl<'py> IntoPyObject<'py> for PyLTItem {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            PyLTItem::Char(c) => Ok(Bound::new(py, c)?.into_any()),
            PyLTItem::Rect(r) => Ok(Bound::new(py, r)?.into_any()),
            PyLTItem::Line(l) => Ok(Bound::new(py, l)?.into_any()),
            PyLTItem::Curve(c) => Ok(Bound::new(py, c)?.into_any()),
            PyLTItem::Anno(a) => Ok(Bound::new(py, a)?.into_any()),
            PyLTItem::TextLineH(l) => Ok(Bound::new(py, l)?.into_any()),
            PyLTItem::TextLineV(l) => Ok(Bound::new(py, l)?.into_any()),
            PyLTItem::TextBoxH(b) => Ok(Bound::new(py, b)?.into_any()),
            PyLTItem::TextBoxV(b) => Ok(Bound::new(py, b)?.into_any()),
            PyLTItem::Image(i) => Ok(Bound::new(py, i)?.into_any()),
            PyLTItem::Figure(f) => Ok(Bound::new(py, f)?.into_any()),
            PyLTItem::Page(p) => Ok(Bound::new(py, p)?.into_any()),
        }
    }
}

/// Layout character - a single character with position and font info.
#[pyclass(name = "LTChar")]
#[derive(Clone)]
pub struct PyLTChar {
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
    /// The character text
    text: String,
    /// Font name
    #[pyo3(get)]
    pub fontname: String,
    /// Font size
    #[pyo3(get)]
    pub size: f64,
    /// Whether the character is upright
    #[pyo3(get)]
    pub upright: bool,
    /// Character advance width
    #[pyo3(get)]
    pub adv: f64,
    /// Text rendering matrix (a, b, c, d, e, f)
    #[pyo3(get)]
    pub matrix: (f64, f64, f64, f64, f64, f64),
    /// Marked Content ID (for tagged PDF accessibility)
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag (e.g., "P", "Span", "H1")
    #[pyo3(get)]
    pub tag: Option<String>,
    /// Non-stroking colorspace name (e.g., "DeviceRGB")
    #[pyo3(get)]
    pub ncs: Option<String>,
    /// Non-stroking (fill) color as tuple of floats
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color as tuple of floats
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
}

impl PyLTChar {
    fn from_core(c: &bolivar_core::layout::LTChar) -> Self {
        Self {
            bbox: (c.x0(), c.y0(), c.x1(), c.y1()),
            text: c.get_text().to_string(),
            fontname: c.fontname().to_string(),
            size: c.size(),
            upright: c.upright(),
            adv: c.adv(),
            matrix: c.matrix(),
            mcid: c.mcid(),
            tag: c.tag(),
            ncs: c.ncs(),
            non_stroking_color: c.non_stroking_color().clone(),
            stroking_color: c.stroking_color().clone(),
        }
    }
}

#[pymethods]
impl PyLTChar {
    /// Get the character text
    fn get_text(&self) -> &str {
        &self.text
    }

    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTChar({:?}, fontname={:?})", self.text, self.fontname)
    }
}

/// Layout text line - horizontal.
#[pyclass(name = "LTTextLineHorizontal")]
#[derive(Clone)]
pub struct PyLTTextLineHorizontal {
    bbox: (f64, f64, f64, f64),
    items: Vec<PyLTItem>,
}

impl PyLTTextLineHorizontal {
    fn from_core(line: &bolivar_core::layout::LTTextLineHorizontal) -> Self {
        let mut items = Vec::new();
        for elem in line.iter() {
            match elem {
                bolivar_core::layout::TextLineElement::Char(c) => {
                    items.push(PyLTItem::Char(PyLTChar::from_core(c)));
                }
                bolivar_core::layout::TextLineElement::Anno(a) => {
                    items.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
                }
            }
        }
        Self {
            bbox: (line.x0(), line.y0(), line.x1(), line.y1()),
            items,
        }
    }
}

#[pymethods]
impl PyLTTextLineHorizontal {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout text line - vertical.
#[pyclass(name = "LTTextLineVertical")]
#[derive(Clone)]
pub struct PyLTTextLineVertical {
    bbox: (f64, f64, f64, f64),
    items: Vec<PyLTItem>,
}

impl PyLTTextLineVertical {
    fn from_core(line: &bolivar_core::layout::LTTextLineVertical) -> Self {
        let mut items = Vec::new();
        for elem in line.iter() {
            match elem {
                bolivar_core::layout::TextLineElement::Char(c) => {
                    items.push(PyLTItem::Char(PyLTChar::from_core(c)));
                }
                bolivar_core::layout::TextLineElement::Anno(a) => {
                    items.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
                }
            }
        }
        Self {
            bbox: (line.x0(), line.y0(), line.x1(), line.y1()),
            items,
        }
    }
}

#[pymethods]
impl PyLTTextLineVertical {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout text box - horizontal.
#[pyclass(name = "LTTextBoxHorizontal")]
#[derive(Clone)]
pub struct PyLTTextBoxHorizontal {
    bbox: (f64, f64, f64, f64),
    items: Vec<PyLTItem>,
}

impl PyLTTextBoxHorizontal {
    fn from_core(boxh: &bolivar_core::layout::LTTextBoxHorizontal) -> Self {
        let mut items = Vec::new();
        for line in boxh.iter() {
            items.push(PyLTItem::TextLineH(PyLTTextLineHorizontal::from_core(line)));
        }
        Self {
            bbox: (boxh.x0(), boxh.y0(), boxh.x1(), boxh.y1()),
            items,
        }
    }
}

#[pymethods]
impl PyLTTextBoxHorizontal {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout text box - vertical.
#[pyclass(name = "LTTextBoxVertical")]
#[derive(Clone)]
pub struct PyLTTextBoxVertical {
    bbox: (f64, f64, f64, f64),
    items: Vec<PyLTItem>,
}

impl PyLTTextBoxVertical {
    fn from_core(boxv: &bolivar_core::layout::LTTextBoxVertical) -> Self {
        let mut items = Vec::new();
        for line in boxv.iter() {
            items.push(PyLTItem::TextLineV(PyLTTextLineVertical::from_core(line)));
        }
        Self {
            bbox: (boxv.x0(), boxv.y0(), boxv.x1(), boxv.y1()),
            items,
        }
    }
}

#[pymethods]
impl PyLTTextBoxVertical {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout image.
#[pyclass(name = "LTImage")]
#[derive(Clone)]
pub struct PyLTImage {
    bbox: (f64, f64, f64, f64),
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub srcsize: (Option<i32>, Option<i32>),
    #[pyo3(get)]
    pub imagemask: bool,
    #[pyo3(get)]
    pub bits: i32,
    #[pyo3(get)]
    pub colorspace: Vec<String>,
}

impl PyLTImage {
    fn from_core(img: &bolivar_core::layout::LTImage) -> Self {
        Self {
            bbox: (img.x0(), img.y0(), img.x1(), img.y1()),
            name: img.name.clone(),
            srcsize: img.srcsize,
            imagemask: img.imagemask,
            bits: img.bits,
            colorspace: img.colorspace.clone(),
        }
    }
}

#[pymethods]
impl PyLTImage {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }
}

/// Layout figure (Form XObject).
#[pyclass(name = "LTFigure")]
#[derive(Clone)]
pub struct PyLTFigure {
    bbox: (f64, f64, f64, f64),
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub matrix: (f64, f64, f64, f64, f64, f64),
    items: Vec<PyLTItem>,
}

impl PyLTFigure {
    fn from_core(fig: &bolivar_core::layout::LTFigure) -> Self {
        let mut items = Vec::new();
        for child in fig.iter() {
            items.push(ltitem_to_py(child));
        }
        Self {
            bbox: (fig.x0(), fig.y0(), fig.x1(), fig.y1()),
            name: fig.name.clone(),
            matrix: fig.matrix,
            items,
        }
    }
}

#[pymethods]
impl PyLTFigure {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout rectangle - a rectangle in the PDF.
#[pyclass(name = "LTRect")]
#[derive(Clone)]
pub struct PyLTRect {
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
    /// Line width
    #[pyo3(get)]
    pub linewidth: f64,
    /// Whether the path is stroked
    #[pyo3(get)]
    pub stroke: bool,
    /// Whether the path is filled
    #[pyo3(get)]
    pub fill: bool,
    /// Non-stroking (fill) color
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
    /// Original path operations: list of (cmd, points) tuples
    #[pyo3(get)]
    pub original_path: Option<Vec<(char, Vec<(f64, f64)>)>>,
    /// Dashing style: (pattern, phase)
    #[pyo3(get)]
    pub dashing_style: Option<(Vec<f64>, f64)>,
    /// Marked Content ID
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag
    #[pyo3(get)]
    pub tag: Option<String>,
}

impl PyLTRect {
    fn from_core(r: &bolivar_core::layout::LTRect) -> Self {
        // Convert original_path from Option<Vec<(char, Vec<Point>)>> to Python-friendly format
        let original_path = r
            .original_path
            .as_ref()
            .map(|path| path.iter().map(|(cmd, pts)| (*cmd, pts.clone())).collect());
        Self {
            bbox: (r.x0(), r.y0(), r.x1(), r.y1()),
            linewidth: r.linewidth,
            stroke: r.stroke,
            fill: r.fill,
            non_stroking_color: r.non_stroking_color.clone(),
            stroking_color: r.stroking_color.clone(),
            original_path,
            dashing_style: r.dashing_style.clone(),
            mcid: r.mcid(),
            tag: r.tag(),
        }
    }
}

#[pymethods]
impl PyLTRect {
    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTRect(bbox={:?}, linewidth={})", self.bbox, self.linewidth)
    }
}

/// Layout line - a straight line in the PDF.
#[pyclass(name = "LTLine")]
#[derive(Clone)]
pub struct PyLTLine {
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
    /// Start point
    #[pyo3(get)]
    pub p0: (f64, f64),
    /// End point
    #[pyo3(get)]
    pub p1: (f64, f64),
    /// Line width
    #[pyo3(get)]
    pub linewidth: f64,
    /// Whether the path is stroked
    #[pyo3(get)]
    pub stroke: bool,
    /// Whether the path is filled
    #[pyo3(get)]
    pub fill: bool,
    /// Non-stroking (fill) color
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
    /// Original path operations
    #[pyo3(get)]
    pub original_path: Option<Vec<(char, Vec<(f64, f64)>)>>,
    /// Dashing style: (pattern, phase)
    #[pyo3(get)]
    pub dashing_style: Option<(Vec<f64>, f64)>,
    /// Marked Content ID
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag
    #[pyo3(get)]
    pub tag: Option<String>,
}

impl PyLTLine {
    fn from_core(l: &bolivar_core::layout::LTLine) -> Self {
        let original_path = l
            .original_path
            .as_ref()
            .map(|path| path.iter().map(|(cmd, pts)| (*cmd, pts.clone())).collect());
        Self {
            bbox: (l.x0(), l.y0(), l.x1(), l.y1()),
            p0: l.p0(),
            p1: l.p1(),
            linewidth: l.linewidth,
            stroke: l.stroke,
            fill: l.fill,
            non_stroking_color: l.non_stroking_color.clone(),
            stroking_color: l.stroking_color.clone(),
            original_path,
            dashing_style: l.dashing_style.clone(),
            mcid: l.mcid(),
            tag: l.tag(),
        }
    }
}

#[pymethods]
impl PyLTLine {
    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    /// Get points as list for pdfplumber compatibility
    #[getter]
    fn pts(&self) -> Vec<(f64, f64)> {
        vec![self.p0, self.p1]
    }

    fn __repr__(&self) -> String {
        format!("LTLine(p0={:?}, p1={:?})", self.p0, self.p1)
    }
}

/// Python wrapper for LTCurve
#[pyclass(name = "LTCurve")]
#[derive(Clone)]
pub struct PyLTCurve {
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
    /// Control points
    #[pyo3(get)]
    pub pts: Vec<(f64, f64)>,
    /// Line width
    #[pyo3(get)]
    pub linewidth: f64,
    /// Whether the path is stroked
    #[pyo3(get)]
    pub stroke: bool,
    /// Whether the path is filled
    #[pyo3(get)]
    pub fill: bool,
    /// Even-odd fill rule
    #[pyo3(get)]
    pub evenodd: bool,
    /// Non-stroking (fill) color
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
    /// Original path operations
    #[pyo3(get)]
    pub original_path: Option<Vec<(char, Vec<(f64, f64)>)>>,
    /// Dashing style: (pattern, phase)
    #[pyo3(get)]
    pub dashing_style: Option<(Vec<f64>, f64)>,
    /// Marked Content ID
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag
    #[pyo3(get)]
    pub tag: Option<String>,
}

impl PyLTCurve {
    fn from_core(c: &bolivar_core::layout::LTCurve) -> Self {
        let original_path = c
            .original_path
            .as_ref()
            .map(|path| path.iter().map(|(cmd, pts)| (*cmd, pts.clone())).collect());
        Self {
            bbox: (c.x0(), c.y0(), c.x1(), c.y1()),
            pts: c.pts.clone(),
            linewidth: c.linewidth,
            stroke: c.stroke,
            fill: c.fill,
            evenodd: c.evenodd,
            non_stroking_color: c.non_stroking_color.clone(),
            stroking_color: c.stroking_color.clone(),
            original_path,
            dashing_style: c.dashing_style.clone(),
            mcid: c.mcid(),
            tag: c.tag(),
        }
    }
}

#[pymethods]
impl PyLTCurve {
    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTCurve(pts={:?})", self.pts.len())
    }
}

/// Python wrapper for LTAnno (virtual annotation like spaces/newlines)
#[pyclass(name = "LTAnno")]
#[derive(Clone)]
pub struct PyLTAnno {
    /// The text content (space, newline, etc.)
    #[pyo3(get)]
    pub text: String,
}

impl PyLTAnno {
    fn from_core(a: &bolivar_core::layout::LTAnno) -> Self {
        Self {
            text: a.get_text().to_string(),
        }
    }
}

#[pymethods]
impl PyLTAnno {
    #[new]
    fn new(text: String) -> Self {
        Self { text }
    }

    fn get_text(&self) -> &str {
        &self.text
    }

    fn __repr__(&self) -> String {
        format!("LTAnno({:?})", self.text)
    }
}

fn resolve_threads(threads: Option<usize>) -> Option<usize> {
    threads.or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
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
fn process_page(
    doc: &PyPDFDocument,
    page: &PyPDFPage,
    laparams: Option<&PyLAParams>,
) -> PyResult<PyLTPage> {
    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());

    // Create resource manager
    let mut rsrcmgr = bolivar_core::pdfinterp::PDFResourceManager::with_caching(true);

    // Create aggregator for this page
    let mut aggregator = bolivar_core::converter::PDFPageAggregator::new(la, page.pageid as i32);

    // Recreate the core PDFPage with contents
    // This is a workaround since we can't store references across Python calls
    let core_page = bolivar_core::pdfpage::PDFPage {
        pageid: page.pageid,
        attrs: std::collections::HashMap::new(),
        label: page.label.clone(),
        mediabox: page.mediabox.map(|b| [b.0, b.1, b.2, b.3]),
        cropbox: page.cropbox.map(|b| [b.0, b.1, b.2, b.3]),
        bleedbox: page.bleedbox.map(|b| [b.0, b.1, b.2, b.3]),
        trimbox: page.trimbox.map(|b| [b.0, b.1, b.2, b.3]),
        artbox: page.artbox.map(|b| [b.0, b.1, b.2, b.3]),
        rotate: page.rotate,
        annots: None,
        resources: page.resources.clone(),
        contents: page.contents.clone(),
        user_unit: 1.0,
    };

    // Create interpreter and process page
    let mut interpreter =
        bolivar_core::pdfinterp::PDFPageInterpreter::new(&mut rsrcmgr, &mut aggregator);
    interpreter.process_page(&core_page, Some(&doc.inner));

    // Get the result
    let ltpage = aggregator.get_result();

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
///     threads: Optional thread count (defaults to no parallelism when None)
///
/// Returns:
///     List of LTPage objects
#[pyfunction]
#[pyo3(signature = (doc, laparams=None, threads=None))]
fn process_pages(
    py: Python<'_>,
    doc: &PyPDFDocument,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
) -> PyResult<Vec<PyLTPage>> {
    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: la,
        threads: resolve_threads(threads),
    };

    let pages = py
        .allow_threads(|| core_extract_pages_with_document(&doc.inner, options))
        .map_err(|e| PyValueError::new_err(format!("Failed to process pages: {}", e)))?;

    Ok(pages.iter().map(ltpage_to_py).collect())
}

/// Extract tables from a page using Rust table extraction.
#[pyfunction]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, table_settings = None, laparams = None, threads = None, force_crop = false))]
fn extract_tables_from_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    table_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
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
        threads: resolve_threads(threads),
    };
    let pages = py
        .allow_threads(|| core_extract_pages_with_document(&doc.inner, options))
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
    Ok(extract_tables_from_ltpage(ltpage, &geom, &settings))
}

/// Extract a single table from a page using Rust table extraction.
#[pyfunction]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, table_settings = None, laparams = None, threads = None, force_crop = false))]
fn extract_table_from_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    table_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
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
        threads: resolve_threads(threads),
    };
    let pages = py
        .allow_threads(|| core_extract_pages_with_document(&doc.inner, options))
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
    Ok(extract_table_from_ltpage(ltpage, &geom, &settings))
}

/// Extract words from a page using Rust text extraction.
#[pyfunction]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, text_settings = None, laparams = None, threads = None, force_crop = false))]
fn extract_words_from_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    text_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
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
        threads: resolve_threads(threads),
    };
    let pages = py
        .allow_threads(|| core_extract_pages_with_document(&doc.inner, options))
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
    let words = extract_words_from_ltpage(ltpage, &geom, settings);
    let mut out = Vec::with_capacity(words.len());
    for w in &words {
        out.push(word_obj_to_py(py, w)?);
    }
    Ok(out)
}

/// Extract text from a page using Rust text extraction.
#[pyfunction]
#[pyo3(signature = (doc, page_index, page_bbox, mediabox, initial_doctop = 0.0, text_settings = None, laparams = None, threads = None, force_crop = false))]
fn extract_text_from_page(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    page_bbox: (f64, f64, f64, f64),
    mediabox: (f64, f64, f64, f64),
    initial_doctop: f64,
    text_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
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
        threads: resolve_threads(threads),
    };
    let pages = py
        .allow_threads(|| core_extract_pages_with_document(&doc.inner, options))
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
    Ok(extract_text_from_ltpage(ltpage, &geom, settings))
}

fn ltpage_to_py(ltpage: &bolivar_core::layout::LTPage) -> PyLTPage {
    let items: Vec<PyLTItem> = ltpage.iter().map(ltitem_to_py).collect();
    PyLTPage {
        pageid: ltpage.pageid,
        rotate: ltpage.rotate,
        bbox: (ltpage.x0(), ltpage.y0(), ltpage.x1(), ltpage.y1()),
        items,
    }
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

/// Extract text from PDF bytes.
#[pyfunction]
#[pyo3(signature = (data, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, threads = None))]
fn extract_text(
    py: Python<'_>,
    data: &[u8],
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
) -> PyResult<String> {
    let options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
        threads: resolve_threads(threads),
    };
    let data = data.to_vec();
    let result = py.allow_threads(|| core_extract_text(&data, Some(options)));
    result.map_err(|e| PyValueError::new_err(format!("Failed to extract text: {}", e)))
}

/// Extract text from a PDF file path using memory-mapped I/O.
#[pyfunction]
#[pyo3(signature = (path, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, threads = None))]
fn extract_text_from_path(
    py: Python<'_>,
    path: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
) -> PyResult<String> {
    let file = File::open(path)
        .map_err(|e| PyValueError::new_err(format!("Failed to open PDF: {}", e)))?;
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
    let doc = PDFDocument::new_from_mmap(Arc::new(mmap), password)
        .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;

    let options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
        threads: resolve_threads(threads),
    };

    let result = py.allow_threads(|| core_extract_text_with_document(&doc, options));
    result.map_err(|e| PyValueError::new_err(format!("Failed to extract text: {}", e)))
}

/// Extract pages (layout) from PDF bytes.
#[pyfunction]
#[pyo3(signature = (data, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, threads = None))]
fn extract_pages(
    py: Python<'_>,
    data: &[u8],
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
) -> PyResult<Vec<PyLTPage>> {
    let options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
        threads: resolve_threads(threads),
    };
    let data = data.to_vec();
    let pages = py
        .allow_threads(|| {
            let iter = core_extract_pages(&data, Some(options))?;
            iter.collect::<bolivar_core::error::Result<Vec<_>>>()
        })
        .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {}", e)))?;
    Ok(pages.iter().map(ltpage_to_py).collect())
}

/// Extract pages (layout) from a PDF file path using memory-mapped I/O.
#[pyfunction]
#[pyo3(signature = (path, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None, threads = None))]
fn extract_pages_from_path(
    py: Python<'_>,
    path: &str,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
    threads: Option<usize>,
) -> PyResult<Vec<PyLTPage>> {
    let file = File::open(path)
        .map_err(|e| PyValueError::new_err(format!("Failed to open PDF: {}", e)))?;
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
    let doc = PDFDocument::new_from_mmap(Arc::new(mmap), password)
        .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;

    let mut options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
        threads: resolve_threads(threads),
    };
    // Match pdfminer.high_level.extract_pages default behavior.
    if options.laparams.is_none() {
        options.laparams = Some(bolivar_core::layout::LAParams::default());
    }

    let pages = py
        .allow_threads(|| core_extract_pages_with_document(&doc, options))
        .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {}", e)))?;
    Ok(pages.iter().map(ltpage_to_py).collect())
}

fn ltitem_to_py(item: &bolivar_core::layout::LTItem) -> PyLTItem {
    use bolivar_core::layout::{LTItem, TextBoxType, TextLineType};

    match item {
        LTItem::Char(c) => PyLTItem::Char(PyLTChar::from_core(c)),
        LTItem::Anno(a) => PyLTItem::Anno(PyLTAnno::from_core(a)),
        LTItem::Rect(r) => PyLTItem::Rect(PyLTRect::from_core(r)),
        LTItem::Line(l) => PyLTItem::Line(PyLTLine::from_core(l)),
        LTItem::Curve(c) => PyLTItem::Curve(PyLTCurve::from_core(c)),
        LTItem::Image(i) => PyLTItem::Image(PyLTImage::from_core(i)),
        LTItem::TextLine(line) => match line {
            TextLineType::Horizontal(l) => {
                PyLTItem::TextLineH(PyLTTextLineHorizontal::from_core(l))
            }
            TextLineType::Vertical(l) => PyLTItem::TextLineV(PyLTTextLineVertical::from_core(l)),
        },
        LTItem::TextBox(tbox) => match tbox {
            TextBoxType::Horizontal(b) => PyLTItem::TextBoxH(PyLTTextBoxHorizontal::from_core(b)),
            TextBoxType::Vertical(b) => PyLTItem::TextBoxV(PyLTTextBoxVertical::from_core(b)),
        },
        LTItem::Figure(fig) => PyLTItem::Figure(PyLTFigure::from_core(fig)),
        LTItem::Page(page) => PyLTItem::Page(ltpage_to_py(page)),
    }
}

/// Python module for bolivar PDF library.
#[pymodule]
fn _bolivar(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyLAParams>()?;
    m.add_class::<PyPDFDocument>()?;
    m.add_class::<PyPDFPage>()?;
    m.add_class::<PyLTPage>()?;
    m.add_class::<PyLTChar>()?;
    m.add_class::<PyLTTextLineHorizontal>()?;
    m.add_class::<PyLTTextLineVertical>()?;
    m.add_class::<PyLTTextBoxHorizontal>()?;
    m.add_class::<PyLTTextBoxVertical>()?;
    m.add_class::<PyLTImage>()?;
    m.add_class::<PyLTFigure>()?;
    m.add_class::<PyLTRect>()?;
    m.add_class::<PyLTLine>()?;
    m.add_class::<PyLTCurve>()?;
    m.add_class::<PyLTAnno>()?;
    m.add_function(wrap_pyfunction!(extract_text, m)?)?;
    m.add_function(wrap_pyfunction!(extract_text_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(process_page, m)?)?;
    m.add_function(wrap_pyfunction!(process_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_from_page, m)?)?;
    m.add_function(wrap_pyfunction!(extract_table_from_page, m)?)?;
    m.add_function(wrap_pyfunction!(extract_words_from_page, m)?)?;
    m.add_function(wrap_pyfunction!(extract_text_from_page, m)?)?;
    Ok(())
}

#[cfg(all(test, feature = "python-tests"))]
mod tests {
    use super::*;

    fn build_minimal_pdf_with_pages(page_count: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");

        let mut offsets: Vec<usize> = Vec::new();
        let push_obj = |buf: &mut Vec<u8>, obj: String, offsets: &mut Vec<usize>| {
            offsets.push(buf.len());
            buf.extend_from_slice(obj.as_bytes());
        };

        push_obj(
            &mut out,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
            &mut offsets,
        );

        let kids: String = (0..page_count)
            .map(|i| format!("{} 0 R", 3 + i))
            .collect::<Vec<_>>()
            .join(" ");
        push_obj(
            &mut out,
            format!(
                "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
                kids, page_count
            ),
            &mut offsets,
        );

        for i in 0..page_count {
            let page_id = 3 + i;
            let contents_id = 3 + page_count + i;
            push_obj(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents {} 0 R >>\nendobj\n",
                    page_id, contents_id
                ),
                &mut offsets,
            );
        }

        for i in 0..page_count {
            let contents_id = 3 + page_count + i;
            push_obj(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n",
                    contents_id
                ),
                &mut offsets,
            );
        }

        let xref_pos = out.len();
        let obj_count = offsets.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", obj_count + 1).as_bytes(),
        );
        for offset in offsets {
            out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }
        out.extend_from_slice(b"trailer\n<< /Size ");
        out.extend_from_slice((obj_count + 1).to_string().as_bytes());
        out.extend_from_slice(b" /Root 1 0 R >>\nstartxref\n");
        out.extend_from_slice(xref_pos.to_string().as_bytes());
        out.extend_from_slice(b"\n%%EOF");

        out
    }

    fn write_temp_pdf(data: &[u8]) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        path.push(format!("bolivar_py_test_{pid}_{stamp}.pdf"));
        std::fs::write(&path, data).expect("write temp pdf");
        path
    }

    #[test]
    fn test_extract_text_from_path_matches_bytes() {
        let pdf_data = build_minimal_pdf_with_pages(1);
        let path = write_temp_pdf(&pdf_data);

        Python::with_gil(|py| {
            let text_bytes = extract_text(py, &pdf_data, "", None, 0, true, None, None).unwrap();
            let text_path = extract_text_from_path(
                py,
                path.to_string_lossy().as_ref(),
                "",
                None,
                0,
                true,
                None,
                None,
            )
            .unwrap();
            assert_eq!(text_bytes, text_path);
        });

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_extract_pages_from_path_len() {
        let pdf_data = build_minimal_pdf_with_pages(2);
        let path = write_temp_pdf(&pdf_data);

        Python::with_gil(|py| {
            let pages = extract_pages_from_path(
                py,
                path.to_string_lossy().as_ref(),
                "",
                None,
                0,
                true,
                None,
                None,
            )
            .unwrap();
            assert_eq!(pages.len(), 2);
        });

        let _ = std::fs::remove_file(&path);
    }
}
