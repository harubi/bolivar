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
use bolivar_core::parser::{
    PDFParser as CorePDFParser, PSBaseParser as CorePSBaseParser,
    PSStackParser as CorePSStackParser, PSToken,
};
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdftypes::PDFObject;
use bolivar_core::pdftypes::PDFStream;
use bolivar_core::table::{
    BBox, CharObj, EdgeObj, ExplicitLine, Orientation, PageGeometry, TableSettings, TextDir,
    TextSettings, WordObj, extract_table_from_ltpage, extract_table_from_objects,
    extract_tables_from_ltpage, extract_tables_from_objects, extract_text_from_ltpage,
    extract_words_from_ltpage,
};
use bolivar_core::utils::HasBBox;
use bytes::Bytes;
use memmap2::Mmap;
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySequence, PyTuple, PyType};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::slice;
use std::sync::{Mutex, OnceLock};

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

enum PdfInput {
    Shared(Bytes),
    Owned(Vec<u8>),
}

struct PyBufferOwner {
    buffer: PyBuffer<u8>,
}

impl PyBufferOwner {
    fn new(buffer: PyBuffer<u8>) -> Self {
        Self { buffer }
    }
}

impl AsRef<[u8]> for PyBufferOwner {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(self.buffer.buf_ptr().cast::<u8>(), self.buffer.len_bytes())
        }
    }
}

fn pdf_input_from_py(data: &Bound<'_, PyAny>) -> PyResult<PdfInput> {
    let buf = PyBuffer::<u8>::get(data)
        .map_err(|_| PyTypeError::new_err("data must be a bytes-like object"))?;
    if buf.readonly() && buf.is_c_contiguous() {
        let owner = PyBufferOwner::new(buf);
        Ok(PdfInput::Shared(Bytes::from_owner(owner)))
    } else {
        Ok(PdfInput::Owned(buf.to_vec(data.py())?))
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
            let py_stream = PyPDFStream::from_core(
                py,
                stream,
                doc,
                visited,
                name_as_psliteral,
                resolve_refs,
                py_doc,
            )?;
            Ok(Py::new(py, py_stream)?.into_any())
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

fn ps_exception(py: Python<'_>, class_name: &str, msg: &str) -> PyErr {
    if let Ok(module) = py.import("pdfminer.psexceptions")
        && let Ok(cls) = module.getattr(class_name)
            && let Ok(err) = cls.call1((msg,)) {
                return PyErr::from_value(err);
            }
    PyValueError::new_err(msg.to_string())
}

static PSLITERAL_TABLE: OnceLock<Mutex<HashMap<Vec<u8>, Py<PyAny>>>> = OnceLock::new();
static PSKEYWORD_TABLE: OnceLock<Mutex<HashMap<Vec<u8>, Py<PyAny>>>> = OnceLock::new();

fn ps_name_to_bytes(name: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(bytes) = name.extract::<Vec<u8>>() {
        return Ok(bytes);
    }
    let s: String = name.extract()?;
    Ok(s.into_bytes())
}

fn intern_psliteral(py: Python<'_>, name: Vec<u8>) -> PyResult<Py<PyAny>> {
    let table = PSLITERAL_TABLE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = table.lock() {
        if let Some(existing) = guard.get(&name) {
            return Ok(existing.clone_ref(py));
        }
        let obj = Py::new(py, PyPSLiteral { name: name.clone() })?.into_any();
        guard.insert(name, obj.clone_ref(py));
        return Ok(obj);
    }
    // Fallback if mutex is poisoned.
    Ok(Py::new(py, PyPSLiteral { name })?.into_any())
}

fn intern_pskeyword(py: Python<'_>, name: Vec<u8>) -> PyResult<Py<PyAny>> {
    let table = PSKEYWORD_TABLE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = table.lock() {
        if let Some(existing) = guard.get(&name) {
            return Ok(existing.clone_ref(py));
        }
        let obj = Py::new(py, PyPSKeyword { name: name.clone() })?.into_any();
        guard.insert(name, obj.clone_ref(py));
        return Ok(obj);
    }
    Ok(Py::new(py, PyPSKeyword { name })?.into_any())
}

fn pstoken_to_py(py: Python<'_>, token: PSToken) -> PyResult<Py<PyAny>> {
    match token {
        PSToken::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any().unbind()),
        PSToken::Int(i) => Ok(i.into_pyobject(py)?.to_owned().into_any().unbind()),
        PSToken::Real(f) => Ok(f.into_pyobject(py)?.to_owned().into_any().unbind()),
        PSToken::Literal(name) => intern_psliteral(py, name.into_bytes()),
        PSToken::Keyword(kw) => intern_pskeyword(py, kw.as_bytes().to_vec()),
        PSToken::String(bytes) => Ok(PyBytes::new(py, &bytes).into_any().unbind()),
        PSToken::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                let py_item = pstoken_to_py(py, item)?;
                list.append(py_item)?;
            }
            Ok(list.into_any().unbind())
        }
        PSToken::Dict(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let key = intern_psliteral(py, k.into_bytes())?;
                let val = pstoken_to_py(py, v)?;
                dict.set_item(key, val)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn pdf_object_to_py_simple(py: Python<'_>, obj: &PDFObject) -> PyResult<Py<PyAny>> {
    match obj {
        PDFObject::Null => Ok(py.None()),
        PDFObject::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any().unbind()),
        PDFObject::Int(i) => Ok(i.into_pyobject(py)?.to_owned().into_any().unbind()),
        PDFObject::Real(f) => Ok(f.into_pyobject(py)?.to_owned().into_any().unbind()),
        PDFObject::Name(n) => intern_psliteral(py, n.as_bytes().to_vec()),
        PDFObject::String(s) => Ok(PyBytes::new(py, s).into_any().unbind()),
        PDFObject::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                let py_item = pdf_object_to_py_simple(py, item)?;
                list.append(py_item)?;
            }
            Ok(list.into_any().unbind())
        }
        PDFObject::Dict(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let key = intern_psliteral(py, k.as_bytes().to_vec())?;
                let val = pdf_object_to_py_simple(py, v)?;
                dict.set_item(key, val)?;
            }
            Ok(dict.into_any().unbind())
        }
        PDFObject::Ref(objref) => {
            let pdftypes = py.import("pdfminer.pdftypes")?;
            let cls = pdftypes.getattr("PDFObjRef")?;
            let obj = cls.call1((py.None(), objref.objid, objref.genno))?;
            Ok(obj.into_any().unbind())
        }
        PDFObject::Stream(_) => Ok(py.None()),
    }
}

fn read_bytes_and_path(
    py: Python<'_>,
    fp: &Bound<'_, PyAny>,
) -> PyResult<(Vec<u8>, Option<String>)> {
    if let Ok(buf) = PyBuffer::<u8>::get(fp) {
        return Ok((buf.to_vec(py)?, None));
    }

    if let Ok(os) = py.import("os")
        && let Ok(fspath) = os.getattr("fspath")
            && let Ok(path_obj) = fspath.call1((fp,))
                && let Ok(path_str) = path_obj.extract::<String>()
                    && std::path::Path::new(&path_str).is_file() {
                        let data = std::fs::read(&path_str).map_err(|e| {
                            PyValueError::new_err(format!("failed to read {path_str}: {e}"))
                        })?;
                        return Ok((data, Some(path_str)));
                    }

    if fp.hasattr("read")? {
        let start_pos = fp
            .call_method0("tell")
            .ok()
            .and_then(|v| v.extract::<u64>().ok());
        let data: Vec<u8> = fp.call_method0("read")?.extract()?;
        if let Some(pos) = start_pos {
            let _ = fp.call_method1("seek", (pos,));
        }
        let path = fp
            .getattr("name")
            .ok()
            .and_then(|v| v.extract::<String>().ok())
            .filter(|p| std::path::Path::new(p).is_file());
        return Ok((data, path));
    }

    Err(PyTypeError::new_err(
        "expected bytes-like object, file-like object, or path-like",
    ))
}

fn py_text_to_string(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = obj.extract::<String>() {
        return Ok(s);
    }
    if let Ok(bytes) = obj.extract::<Vec<u8>>() {
        return Ok(String::from_utf8_lossy(&bytes).to_string());
    }
    Ok(obj.str()?.to_string())
}

#[pyfunction]
fn decode_text(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<String> {
    let buf = PyBuffer::<u8>::get(data)
        .map_err(|_| PyTypeError::new_err("decode_text expects bytes-like object"))?;
    let bytes = buf.to_vec(py)?;
    Ok(bolivar_core::utils::decode_text(&bytes))
}

#[pyfunction]
fn isnumber(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    if obj.extract::<i64>().is_ok() || obj.extract::<f64>().is_ok() {
        return Ok(true);
    }
    Ok(false)
}

fn is_pdf_obj_ref(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(obj.hasattr("objid")? && obj.hasattr("resolve")?)
}

fn resolve_pdf_obj(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let mut current = obj.clone().into_any().unbind();
    loop {
        let bound = current.bind(py);
        if !is_pdf_obj_ref(bound)? {
            return Ok(current);
        }
        let resolved = match bound.call_method0("resolve") {
            Ok(v) => v.into_any().unbind(),
            Err(_) => return Ok(py.None()),
        };
        current = resolved;
    }
}

fn parse_number_tree(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
    visited: &mut HashSet<i64>,
    out: &mut Vec<(i64, Py<PyAny>)>,
) -> PyResult<()> {
    if is_pdf_obj_ref(obj)?
        && let Ok(objid) = obj.getattr("objid")?.extract::<i64>() {
            if visited.contains(&objid) {
                return Ok(());
            }
            visited.insert(objid);
        }

    let resolved = resolve_pdf_obj(py, obj)?;
    let resolved = resolved.bind(py);
    if resolved.is_none() {
        return Ok(());
    }

    let dict = match resolved.downcast::<PyDict>() {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };

    if let Some(nums_obj) = dict.get_item("Nums")? {
        let nums_resolved = resolve_pdf_obj(py, &nums_obj)?;
        let nums = nums_resolved.bind(py);
        if let Ok(seq) = nums.downcast::<PySequence>() {
            let len = seq.len().unwrap_or(0);
            let mut idx = 0;
            while idx + 1 < len {
                let key_obj = seq.get_item(idx)?;
                let key_resolved = resolve_pdf_obj(py, &key_obj)?;
                let key_bound = key_resolved.bind(py);
                if let Ok(key_val) = key_bound.extract::<i64>() {
                    let val_obj = seq.get_item(idx + 1)?;
                    out.push((key_val, val_obj.into_any().unbind()));
                }
                idx += 2;
            }
        }
    }

    if let Some(kids_obj) = dict.get_item("Kids")? {
        let kids_resolved = resolve_pdf_obj(py, &kids_obj)?;
        let kids = kids_resolved.bind(py);
        if let Ok(seq) = kids.downcast::<PySequence>() {
            let len = seq.len().unwrap_or(0);
            for idx in 0..len {
                let kid = seq.get_item(idx)?;
                parse_number_tree(py, &kid, visited, out)?;
            }
        }
    }

    Ok(())
}

#[pyclass(name = "NumberTree")]
struct PyNumberTree {
    obj: Py<PyAny>,
}

#[pymethods]
impl PyNumberTree {
    #[new]
    fn new(obj: Py<PyAny>) -> Self {
        Self { obj }
    }

    #[getter]
    fn values(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let vals = self.compute_values(py)?;
        let list = PyList::empty(py);
        for (k, v) in vals {
            let k_obj = k.into_pyobject(py)?.into_any().unbind();
            let tup = PyTuple::new(py, [k_obj, v.clone_ref(py)])?;
            list.append(tup)?;
        }
        Ok(list.into_any().unbind())
    }

    fn lookup(&self, py: Python<'_>, key: i64) -> PyResult<Py<PyAny>> {
        let vals = self.compute_values(py)?;
        for (k, v) in vals {
            if k == key {
                return Ok(v);
            }
            if k > key {
                break;
            }
        }
        Ok(py.None())
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let values = self.values(py)?;
        Ok(values
            .bind(py)
            .call_method0("__iter__")?
            .into_any()
            .unbind())
    }
}

impl PyNumberTree {
    fn compute_values(&self, py: Python<'_>) -> PyResult<Vec<(i64, Py<PyAny>)>> {
        let mut visited = HashSet::new();
        let mut out = Vec::new();
        let obj = self.obj.bind(py);
        parse_number_tree(py, obj, &mut visited, &mut out)?;
        Ok(out)
    }
}

fn page_objects_to_chars_edges(
    _py: Python<'_>,
    page: &Bound<'_, PyAny>,
) -> PyResult<(Vec<CharObj>, Vec<EdgeObj>, PageGeometry)> {
    let bbox: (f64, f64, f64, f64) = page.getattr("bbox")?.extract()?;
    let mediabox: (f64, f64, f64, f64) = page
        .getattr("mediabox")
        .ok()
        .and_then(|v| v.extract().ok())
        .unwrap_or(bbox);
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
            && let Ok(pts) = pts_obj.extract::<Vec<(f64, f64)>>() {
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

    fn parse_explicit_lines(_py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Vec<ExplicitLine>> {
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
                let obj_type: Option<String> =
                    dict.get_item("object_type")?.and_then(|v| v.extract().ok());
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
            "text_layout" => {
                if !v.is_none() {
                    text_settings.layout = v.extract()?;
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

/// PDF resource manager - Rust-backed pdfminer compatibility.
#[pyclass(name = "PDFResourceManager")]
pub struct PyPDFResourceManager {
    inner: bolivar_core::pdfinterp::PDFResourceManager,
}

#[pymethods]
impl PyPDFResourceManager {
    #[new]
    #[pyo3(signature = (caching = true))]
    fn new(caching: bool) -> Self {
        Self {
            inner: bolivar_core::pdfinterp::PDFResourceManager::with_caching(caching),
        }
    }

    #[getter]
    fn caching(&self) -> bool {
        self.inner.caching_enabled()
    }

    fn get_font(&mut self, objid: Option<u64>, spec: &Bound<'_, PyAny>) -> PyResult<u64> {
        let mut objid_opt = objid.filter(|id| *id > 0);
        if objid_opt.is_none()
            && let Ok(attr) = spec.getattr("objid")
                && let Ok(val) = attr.extract::<u64>()
                    && val > 0 {
                        objid_opt = Some(val);
                    }
        let spec_dict: HashMap<String, PDFObject> = HashMap::new();
        Ok(self.inner.get_font(objid_opt, &spec_dict))
    }
}

/// PostScript literal name.
#[pyclass(name = "PSLiteral")]
pub struct PyPSLiteral {
    name: Vec<u8>,
}

#[pymethods]
impl PyPSLiteral {
    #[new]
    fn new(_py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            name: ps_name_to_bytes(name)?,
        })
    }

    #[getter]
    fn name(&self, py: Python<'_>) -> Py<PyAny> {
        PyBytes::new(py, &self.name).into_any().unbind()
    }

    fn __repr__(&self) -> String {
        format!("/{}", String::from_utf8_lossy(&self.name))
    }
}

/// PostScript keyword.
#[pyclass(name = "PSKeyword")]
pub struct PyPSKeyword {
    name: Vec<u8>,
}

#[pymethods]
impl PyPSKeyword {
    #[new]
    fn new(_py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            name: ps_name_to_bytes(name)?,
        })
    }

    #[getter]
    fn name(&self, py: Python<'_>) -> Py<PyAny> {
        PyBytes::new(py, &self.name).into_any().unbind()
    }

    fn __repr__(&self) -> String {
        format!("/{}", String::from_utf8_lossy(&self.name))
    }
}

#[pyfunction]
fn LIT(py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let bytes = ps_name_to_bytes(name)?;
    intern_psliteral(py, bytes)
}

#[pyfunction]
fn KWD(py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let bytes = ps_name_to_bytes(name)?;
    intern_pskeyword(py, bytes)
}

/// PostScript base parser.
#[pyclass(name = "PSBaseParser", unsendable)]
pub struct PyPSBaseParser {
    parser: CorePSBaseParser<'static>,
}

#[pymethods]
impl PyPSBaseParser {
    #[new]
    fn new(py: Python<'_>, fp: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (data, _) = read_bytes_and_path(py, fp)?;
        Ok(Self {
            parser: CorePSBaseParser::from_bytes(&data),
        })
    }

    fn nexttoken(&mut self, py: Python<'_>) -> PyResult<(usize, Py<PyAny>)> {
        match self.parser.next_token() {
            Some(Ok((pos, tok))) => Ok((pos, pstoken_to_py(py, tok)?)),
            Some(Err(e)) => Err(ps_exception(py, "PSSyntaxError", &format!("{e}"))),
            None => Err(ps_exception(py, "PSEOF", "Unexpected EOF")),
        }
    }

    fn tell(&self) -> usize {
        self.parser.tell()
    }

    fn seek(&mut self, pos: usize) {
        self.parser.set_pos(pos);
    }
}

/// PostScript stack parser.
#[pyclass(name = "PSStackParser", unsendable)]
pub struct PyPSStackParser {
    parser: CorePSStackParser<'static>,
}

#[pymethods]
impl PyPSStackParser {
    #[new]
    fn new(py: Python<'_>, fp: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (data, _) = read_bytes_and_path(py, fp)?;
        Ok(Self {
            parser: CorePSStackParser::from_bytes(&data),
        })
    }

    fn nextobject(&mut self, py: Python<'_>) -> PyResult<(usize, Py<PyAny>)> {
        match self.parser.next_object() {
            Some(Ok((pos, tok))) => Ok((pos, pstoken_to_py(py, tok)?)),
            Some(Err(e)) => Err(ps_exception(py, "PSSyntaxError", &format!("{e}"))),
            None => Err(ps_exception(py, "PSEOF", "Unexpected EOF")),
        }
    }
}

/// PDF object parser.
#[pyclass(name = "PDFParser", unsendable)]
pub struct PyPDFParser {
    data: Vec<u8>,
    path: Option<String>,
    parser: CorePDFParser<'static>,
}

#[pymethods]
impl PyPDFParser {
    #[new]
    fn new(py: Python<'_>, fp: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (data, path) = read_bytes_and_path(py, fp)?;
        Ok(Self {
            data: data.clone(),
            path,
            parser: CorePDFParser::from_bytes(&data),
        })
    }

    fn get_data(&self) -> Vec<u8> {
        self.data.clone()
    }

    fn get_path(&self) -> Option<String> {
        self.path.clone()
    }

    fn nextobject(&mut self, py: Python<'_>) -> PyResult<(usize, Py<PyAny>)> {
        let pos = self.parser.tell();
        let obj = self
            .parser
            .parse_object()
            .map_err(|e| ps_exception(py, "PSSyntaxError", &format!("{e}")))?;
        let py_obj = pdf_object_to_py_simple(py, &obj)?;
        Ok((pos, py_obj))
    }
}

/// PDF Stream - dictionary attributes + binary data.
#[pyclass(name = "PDFStream")]
pub struct PyPDFStream {
    stream: PDFStream,
    attrs: Py<PyDict>,
    doc: Option<Py<PyAny>>,
}

impl PyPDFStream {
    fn from_core(
        py: Python<'_>,
        stream: &PDFStream,
        doc: &PDFDocument,
        visited: &mut HashSet<(u32, u32)>,
        name_as_psliteral: bool,
        resolve_refs: bool,
        py_doc: Option<&Py<PyAny>>,
    ) -> PyResult<Self> {
        let py_dict = PyDict::new(py);
        for (k, v) in stream.attrs.iter() {
            let py_val = pdf_object_to_py_internal(
                py,
                v,
                doc,
                visited,
                name_as_psliteral,
                resolve_refs,
                py_doc,
            )?;
            py_dict.set_item(k, py_val)?;
        }
        Ok(Self {
            stream: stream.clone(),
            attrs: py_dict.unbind(),
            doc: py_doc.map(|doc_obj| doc_obj.clone_ref(py)),
        })
    }
}

#[pymethods]
impl PyPDFStream {
    #[getter]
    fn attrs(&self, py: Python<'_>) -> Py<PyDict> {
        self.attrs.clone_ref(py)
    }

    #[getter]
    fn rawdata(&self, py: Python<'_>) -> Py<PyAny> {
        PyBytes::new(py, self.stream.get_rawdata())
            .into_any()
            .unbind()
    }

    #[getter]
    fn decipher(&self, py: Python<'_>) -> Py<PyAny> {
        py.None()
    }

    fn get_data(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let decoded = if let Some(doc_obj) = &self.doc {
            let doc: PyRef<'_, PyPDFDocument> = doc_obj.extract(py)?;
            doc.inner
                .decode_stream(&self.stream)
                .map_err(|e| PyValueError::new_err(format!("decode failed: {e}")))?
        } else {
            self.stream.get_rawdata().to_vec()
        };
        Ok(PyBytes::new(py, &decoded).into_any().unbind())
    }

    fn __repr__(&self) -> String {
        format!("PDFStream({} bytes)", self.stream.get_rawdata().len())
    }
}

/// PDF Document - main entry point for PDF parsing.
///
/// Creates a document from PDF bytes and provides access to pages.
#[pyclass(name = "PDFDocument")]
pub struct PyPDFDocument {
    /// The underlying Rust PDFDocument (owns the data via Bytes)
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
    fn new(data: &Bound<'_, PyAny>, password: &str) -> PyResult<Self> {
        let doc = match pdf_input_from_py(data)? {
            PdfInput::Shared(bytes) => PDFDocument::new_from_bytes(bytes, password),
            PdfInput::Owned(bytes) => PDFDocument::new(bytes, password),
        }
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
        let doc = PDFDocument::new_from_mmap(mmap, password)
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
        if let Ok(cache) = slf.resolved_cache.lock()
            && let Some(obj) = cache.get(&objid) {
                return Ok(obj.clone_ref(py));
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
            let py_val = pdf_object_to_py_internal(py, v, doc, &mut visited, true, false, py_doc)?;
            attrs_dict.set_item(k, py_val)?;
        }

        let resources_dict = PyDict::new(py);
        for (k, v) in page.resources.iter() {
            let mut visited = HashSet::new();
            let py_val = pdf_object_to_py_internal(py, v, doc, &mut visited, true, false, py_doc)?;
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
#[pyclass(name = "LTPage", dict)]
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
            PyLTItem::Char(c) => {
                let bbox = c.bbox;
                let adv = c.adv;
                let size = c.size;
                let fontname = c.fontname.clone();
                let matrix = c.matrix;
                let upright = c.upright;
                let mcid = c.mcid;
                let tag = c.tag.clone();
                let text = c.text.clone();
                let non_stroking_color = c.non_stroking_color.clone();
                let stroking_color = c.stroking_color.clone();
                let bound = Bound::new(py, c)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("adv", adv)?;
                dict.set_item("size", size)?;
                dict.set_item("fontname", fontname)?;
                dict.set_item("matrix", matrix)?;
                dict.set_item("upright", upright)?;
                dict.set_item("mcid", mcid)?;
                dict.set_item("tag", tag)?;
                dict.set_item("text", text)?;
                dict.set_item("non_stroking_color", non_stroking_color)?;
                dict.set_item("stroking_color", stroking_color)?;
                Ok(bound.into_any())
            }
            PyLTItem::Rect(r) => {
                let bbox = r.bbox;
                let linewidth = r.linewidth;
                let stroke = r.stroke;
                let fill = r.fill;
                let non_stroking_color = r.non_stroking_color.clone();
                let stroking_color = r.stroking_color.clone();
                let bound = Bound::new(py, r)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("linewidth", linewidth)?;
                dict.set_item("stroke", stroke)?;
                dict.set_item("fill", fill)?;
                dict.set_item("non_stroking_color", non_stroking_color)?;
                dict.set_item("stroking_color", stroking_color)?;
                Ok(bound.into_any())
            }
            PyLTItem::Line(l) => {
                let bbox = l.bbox;
                let p0 = l.p0;
                let p1 = l.p1;
                let linewidth = l.linewidth;
                let stroke = l.stroke;
                let fill = l.fill;
                let non_stroking_color = l.non_stroking_color.clone();
                let stroking_color = l.stroking_color.clone();
                let bound = Bound::new(py, l)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("pts", vec![p0, p1])?;
                dict.set_item("linewidth", linewidth)?;
                dict.set_item("stroke", stroke)?;
                dict.set_item("fill", fill)?;
                dict.set_item("non_stroking_color", non_stroking_color)?;
                dict.set_item("stroking_color", stroking_color)?;
                Ok(bound.into_any())
            }
            PyLTItem::Curve(c) => {
                let bbox = c.bbox;
                let pts = c.pts.clone();
                let linewidth = c.linewidth;
                let stroke = c.stroke;
                let fill = c.fill;
                let evenodd = c.evenodd;
                let non_stroking_color = c.non_stroking_color.clone();
                let stroking_color = c.stroking_color.clone();
                let bound = Bound::new(py, c)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("pts", pts)?;
                dict.set_item("linewidth", linewidth)?;
                dict.set_item("stroke", stroke)?;
                dict.set_item("fill", fill)?;
                dict.set_item("evenodd", evenodd)?;
                dict.set_item("non_stroking_color", non_stroking_color)?;
                dict.set_item("stroking_color", stroking_color)?;
                Ok(bound.into_any())
            }
            PyLTItem::Anno(a) => {
                let text = a.text.clone();
                let bound = Bound::new(py, a)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("text", text)?;
                Ok(bound.into_any())
            }
            PyLTItem::TextLineH(l) => {
                let bbox = l.bbox;
                let bound = Bound::new(py, l)?;
                set_bbox_attrs(&bound, bbox)?;
                Ok(bound.into_any())
            }
            PyLTItem::TextLineV(l) => {
                let bbox = l.bbox;
                let bound = Bound::new(py, l)?;
                set_bbox_attrs(&bound, bbox)?;
                Ok(bound.into_any())
            }
            PyLTItem::TextBoxH(b) => {
                let bbox = b.bbox;
                let bound = Bound::new(py, b)?;
                set_bbox_attrs(&bound, bbox)?;
                Ok(bound.into_any())
            }
            PyLTItem::TextBoxV(b) => {
                let bbox = b.bbox;
                let bound = Bound::new(py, b)?;
                set_bbox_attrs(&bound, bbox)?;
                Ok(bound.into_any())
            }
            PyLTItem::Image(i) => {
                let bbox = i.bbox;
                let name = i.name.clone();
                let srcsize = i.srcsize;
                let imagemask = i.imagemask;
                let bits = i.bits;
                let colorspace = i.colorspace.clone();
                let bound = Bound::new(py, i)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("name", name)?;
                dict.set_item("srcsize", srcsize)?;
                dict.set_item("imagemask", imagemask)?;
                dict.set_item("bits", bits)?;
                dict.set_item("colorspace", colorspace)?;
                Ok(bound.into_any())
            }
            PyLTItem::Figure(f) => {
                let bbox = f.bbox;
                let name = f.name.clone();
                let matrix = f.matrix;
                let bound = Bound::new(py, f)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("name", name)?;
                dict.set_item("matrix", matrix)?;
                Ok(bound.into_any())
            }
            PyLTItem::Page(p) => Ok(Bound::new(py, p)?.into_any()),
        }
    }
}

fn set_bbox_attrs(obj: &Bound<'_, PyAny>, bbox: (f64, f64, f64, f64)) -> PyResult<()> {
    let dict = instance_dict(obj)?;
    dict.set_item("x0", bbox.0)?;
    dict.set_item("y0", bbox.1)?;
    dict.set_item("x1", bbox.2)?;
    dict.set_item("y1", bbox.3)?;
    dict.set_item("width", bbox.2 - bbox.0)?;
    dict.set_item("height", bbox.3 - bbox.1)?;
    dict.set_item("bbox", bbox)?;
    Ok(())
}

fn instance_dict<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let dict_obj = obj.getattr("__dict__")?;
    Ok(dict_obj.downcast::<PyDict>()?.clone())
}

/// Layout character - a single character with position and font info.
#[pyclass(name = "LTChar", dict)]
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
    ncs_name: Option<String>,
    /// Stroking colorspace name (e.g., "DeviceRGB")
    scs_name: Option<String>,
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
            ncs_name: c.ncs(),
            scs_name: c.scs(),
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

    #[getter]
    fn ncs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.ncs_name.as_deref() {
            Some(name) => name_to_psliteral(py, name),
            None => Err(pyo3::exceptions::PyAttributeError::new_err("ncs")),
        }
    }

    #[getter]
    fn scs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.scs_name.as_deref() {
            Some(name) => name_to_psliteral(py, name),
            None => Err(pyo3::exceptions::PyAttributeError::new_err("scs")),
        }
    }

    #[getter]
    fn graphicstate(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let types = py.import("types")?;
        let ns = types.getattr("SimpleNamespace")?.call0()?;

        let scolor = match &self.stroking_color {
            Some(values) => PyTuple::new(py, values)?.into_any(),
            None => 0i32.into_pyobject(py)?.into_any(),
        };
        let ncolor = match &self.non_stroking_color {
            Some(values) => PyTuple::new(py, values)?.into_any(),
            None => 0i32.into_pyobject(py)?.into_any(),
        };

        ns.setattr("scolor", scolor)?;
        ns.setattr("ncolor", ncolor)?;

        if let Some(name) = self.ncs_name.as_deref() {
            let ncs = name_to_psliteral(py, name)?;
            ns.setattr("ncs", ncs)?;
        }

        if let Some(name) = self.scs_name.as_deref() {
            let scs = name_to_psliteral(py, name)?;
            ns.setattr("scs", scs)?;
        }

        Ok(ns.into_any().unbind())
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
#[pyclass(name = "LTTextLineHorizontal", dict)]
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
#[pyclass(name = "LTTextLineVertical", dict)]
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
#[pyclass(name = "LTTextBoxHorizontal", dict)]
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
#[pyclass(name = "LTTextBoxVertical", dict)]
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
#[pyclass(name = "LTImage", dict)]
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
#[pyclass(name = "LTFigure", dict)]
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
#[pyclass(name = "LTRect", dict)]
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
#[pyclass(name = "LTLine", dict)]
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
#[pyclass(name = "LTCurve", dict)]
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
#[pyclass(name = "LTAnno", dict)]
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

struct PyWriter {
    outfp: Py<PyAny>,
}

impl PyWriter {
    fn new(outfp: Py<PyAny>) -> Self {
        Self { outfp }
    }
}

impl Write for PyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Python::with_gil(|py| {
            let out = self.outfp.bind(py);
            let bytes = PyBytes::new(py, buf);
            out.call_method1("write", (bytes,))
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            Ok(buf.len())
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Python::with_gil(|py| {
            let out = self.outfp.bind(py);
            if let Ok(has_flush) = out.hasattr("flush")
                && has_flush {
                    out.call_method0("flush").map_err(|e| {
                        std::io::Error::other(e.to_string())
                    })?;
                }
            Ok(())
        })
    }
}

fn py_textline_element_from_item(item: &PyLTItem) -> Option<bolivar_core::layout::TextLineElement> {
    match item {
        PyLTItem::Char(c) => Some(bolivar_core::layout::TextLineElement::Char(
            py_ltchar_to_core(c),
        )),
        PyLTItem::Anno(a) => Some(bolivar_core::layout::TextLineElement::Anno(
            bolivar_core::layout::LTAnno::new(&a.text),
        )),
        _ => None,
    }
}

fn py_ltchar_to_core(c: &PyLTChar) -> bolivar_core::layout::LTChar {
    let mut ch = bolivar_core::layout::LTChar::with_colors_matrix(
        c.bbox,
        &c.text,
        &c.fontname,
        c.size,
        c.upright,
        c.adv,
        c.matrix,
        c.mcid,
        c.tag.clone(),
        c.non_stroking_color.clone(),
        c.stroking_color.clone(),
    );
    ch.set_ncs(c.ncs_name.clone());
    ch.set_scs(c.scs_name.clone());
    ch
}

fn py_textline_h_to_core(
    line: &PyLTTextLineHorizontal,
) -> bolivar_core::layout::LTTextLineHorizontal {
    let mut core_line = bolivar_core::layout::LTTextLineHorizontal::new(0.1);
    for item in &line.items {
        if let Some(elem) = py_textline_element_from_item(item) {
            core_line.add_element(elem);
        }
    }
    core_line.set_bbox(line.bbox);
    core_line
}

fn py_textline_v_to_core(line: &PyLTTextLineVertical) -> bolivar_core::layout::LTTextLineVertical {
    let mut core_line = bolivar_core::layout::LTTextLineVertical::new(0.1);
    for item in &line.items {
        if let Some(elem) = py_textline_element_from_item(item) {
            core_line.add_element(elem);
        }
    }
    core_line.set_bbox(line.bbox);
    core_line
}

fn py_ltitem_to_core(item: &PyLTItem) -> bolivar_core::layout::LTItem {
    match item {
        PyLTItem::Char(c) => bolivar_core::layout::LTItem::Char(py_ltchar_to_core(c)),
        PyLTItem::Anno(a) => {
            bolivar_core::layout::LTItem::Anno(bolivar_core::layout::LTAnno::new(&a.text))
        }
        PyLTItem::Rect(r) => {
            let original_path = r.original_path.clone();
            let dashing_style = r.dashing_style.clone();
            let mut rect = if original_path.is_some() || dashing_style.is_some() {
                bolivar_core::layout::LTRect::new_with_dashing(
                    r.linewidth,
                    r.bbox,
                    r.stroke,
                    r.fill,
                    false,
                    r.stroking_color.clone(),
                    r.non_stroking_color.clone(),
                    original_path,
                    dashing_style,
                )
            } else {
                bolivar_core::layout::LTRect::new(
                    r.linewidth,
                    r.bbox,
                    r.stroke,
                    r.fill,
                    false,
                    r.stroking_color.clone(),
                    r.non_stroking_color.clone(),
                )
            };
            rect.set_marked_content(r.mcid, r.tag.clone());
            bolivar_core::layout::LTItem::Rect(rect)
        }
        PyLTItem::Line(l) => {
            let original_path = l.original_path.clone();
            let dashing_style = l.dashing_style.clone();
            let mut line = if original_path.is_some() || dashing_style.is_some() {
                bolivar_core::layout::LTLine::new_with_dashing(
                    l.linewidth,
                    l.p0,
                    l.p1,
                    l.stroke,
                    l.fill,
                    false,
                    l.stroking_color.clone(),
                    l.non_stroking_color.clone(),
                    original_path,
                    dashing_style,
                )
            } else {
                bolivar_core::layout::LTLine::new(
                    l.linewidth,
                    l.p0,
                    l.p1,
                    l.stroke,
                    l.fill,
                    false,
                    l.stroking_color.clone(),
                    l.non_stroking_color.clone(),
                )
            };
            line.set_marked_content(l.mcid, l.tag.clone());
            bolivar_core::layout::LTItem::Line(line)
        }
        PyLTItem::Curve(c) => {
            let original_path = c.original_path.clone();
            let dashing_style = c.dashing_style.clone();
            let mut curve = if original_path.is_some() || dashing_style.is_some() {
                bolivar_core::layout::LTCurve::new_with_dashing(
                    c.linewidth,
                    c.pts.clone(),
                    c.stroke,
                    c.fill,
                    c.evenodd,
                    c.stroking_color.clone(),
                    c.non_stroking_color.clone(),
                    original_path,
                    dashing_style,
                )
            } else {
                bolivar_core::layout::LTCurve::new(
                    c.linewidth,
                    c.pts.clone(),
                    c.stroke,
                    c.fill,
                    c.evenodd,
                    c.stroking_color.clone(),
                    c.non_stroking_color.clone(),
                )
            };
            curve.set_marked_content(c.mcid, c.tag.clone());
            bolivar_core::layout::LTItem::Curve(curve)
        }
        PyLTItem::TextLineH(l) => bolivar_core::layout::LTItem::TextLine(
            bolivar_core::layout::TextLineType::Horizontal(py_textline_h_to_core(l)),
        ),
        PyLTItem::TextLineV(l) => bolivar_core::layout::LTItem::TextLine(
            bolivar_core::layout::TextLineType::Vertical(py_textline_v_to_core(l)),
        ),
        PyLTItem::TextBoxH(b) => {
            let mut boxh = bolivar_core::layout::LTTextBoxHorizontal::new();
            for item in &b.items {
                if let PyLTItem::TextLineH(line) = item {
                    boxh.add(py_textline_h_to_core(line));
                }
            }
            bolivar_core::layout::LTItem::TextBox(bolivar_core::layout::TextBoxType::Horizontal(
                boxh,
            ))
        }
        PyLTItem::TextBoxV(b) => {
            let mut boxv = bolivar_core::layout::LTTextBoxVertical::new();
            for item in &b.items {
                if let PyLTItem::TextLineV(line) = item {
                    boxv.add(py_textline_v_to_core(line));
                }
            }
            bolivar_core::layout::LTItem::TextBox(bolivar_core::layout::TextBoxType::Vertical(boxv))
        }
        PyLTItem::Image(i) => {
            bolivar_core::layout::LTItem::Image(bolivar_core::layout::LTImage::new(
                &i.name,
                i.bbox,
                i.srcsize,
                i.imagemask,
                i.bits,
                i.colorspace.clone(),
            ))
        }
        PyLTItem::Figure(f) => {
            let bbox = f.bbox;
            let width = bbox.2 - bbox.0;
            let height = bbox.3 - bbox.1;
            let mut fig = bolivar_core::layout::LTFigure::new(
                &f.name,
                (bbox.0, bbox.1, width, height),
                f.matrix,
            );
            for child in &f.items {
                fig.add(py_ltitem_to_core(child));
            }
            bolivar_core::layout::LTItem::Figure(Box::new(fig))
        }
        PyLTItem::Page(p) => {
            let core = py_ltpage_to_core(p);
            bolivar_core::layout::LTItem::Page(Box::new(core))
        }
    }
}

fn py_ltpage_to_core(page: &PyLTPage) -> bolivar_core::layout::LTPage {
    let mut core_page = bolivar_core::layout::LTPage::new(page.pageid, page.bbox, page.rotate);
    for item in &page.items {
        core_page.add(py_ltitem_to_core(item));
    }
    core_page
}

#[pyclass(name = "TextConverter")]
pub struct PyTextConverter {
    converter: bolivar_core::converter::TextConverter<PyWriter>,
}

#[pymethods]
impl PyTextConverter {
    #[new]
    #[pyo3(signature = (rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None, showpageno=false, imagewriter=None))]
    fn new(
        rsrcmgr: &Bound<'_, PyAny>,
        outfp: Py<PyAny>,
        codec: &str,
        pageno: i32,
        laparams: Option<&PyLAParams>,
        showpageno: bool,
        imagewriter: Option<&Bound<'_, PyAny>>,
    ) -> Self {
        let _ = rsrcmgr;
        let _ = imagewriter;
        let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
        let mut converter = bolivar_core::converter::TextConverter::new(
            PyWriter::new(outfp),
            codec,
            pageno,
            la,
            showpageno,
        );
        converter.set_showpageno(showpageno);
        Self { converter }
    }

    fn _receive_layout(&mut self, ltpage: &PyLTPage) {
        let core_page = py_ltpage_to_core(ltpage);
        self.converter.receive_layout(core_page);
    }

    fn close(&mut self) {
        self.converter.flush();
    }
}

#[pyclass(name = "HTMLConverter")]
pub struct PyHTMLConverter {
    converter: bolivar_core::converter::HTMLConverter<PyWriter>,
}

#[pymethods]
impl PyHTMLConverter {
    #[new]
    #[pyo3(signature = (rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None, scale=1.0, fontscale=1.0, layoutmode="normal", showpageno=true, pagemargin=50, imagewriter=None, debug=0, rect_colors=None, text_colors=None))]
    fn new(
        rsrcmgr: &Bound<'_, PyAny>,
        outfp: Py<PyAny>,
        codec: &str,
        pageno: i32,
        laparams: Option<&PyLAParams>,
        scale: f64,
        fontscale: f64,
        layoutmode: &str,
        showpageno: bool,
        pagemargin: i32,
        imagewriter: Option<&Bound<'_, PyAny>>,
        debug: i32,
        rect_colors: Option<&Bound<'_, PyAny>>,
        text_colors: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let _ = rsrcmgr;
        let _ = imagewriter;
        let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
        let writer = PyWriter::new(outfp);
        let mut converter =
            if (scale - 1.0).abs() > f64::EPSILON || (fontscale - 1.0).abs() > f64::EPSILON {
                bolivar_core::converter::HTMLConverter::with_options(
                    writer, codec, pageno, la, scale, fontscale,
                )
            } else if debug > 0 {
                bolivar_core::converter::HTMLConverter::with_debug(writer, codec, pageno, la, debug)
            } else {
                bolivar_core::converter::HTMLConverter::new(writer, codec, pageno, la)
            };

        converter.set_layoutmode(layoutmode);
        converter.set_showpageno(showpageno);
        converter.set_pagemargin(pagemargin);
        converter.set_scale(scale);
        converter.set_fontscale(fontscale);

        if let Some(obj) = rect_colors {
            converter.set_rect_colors(py_any_to_string_map(obj)?);
        } else if debug > 0 {
            converter.set_rect_colors(
                bolivar_core::converter::HTMLConverter::<PyWriter>::debug_rect_colors(),
            );
        }

        if let Some(obj) = text_colors {
            converter.set_text_colors(py_any_to_string_map(obj)?);
        } else if debug > 0 {
            converter.set_text_colors(
                bolivar_core::converter::HTMLConverter::<PyWriter>::debug_text_colors(),
            );
        }

        Ok(Self { converter })
    }

    fn _receive_layout(&mut self, ltpage: &PyLTPage) {
        let core_page = py_ltpage_to_core(ltpage);
        self.converter.receive_layout(core_page);
    }

    fn close(&mut self) {
        self.converter.close();
        self.converter.flush();
    }
}

#[pyclass(name = "XMLConverter")]
pub struct PyXMLConverter {
    converter: bolivar_core::converter::XMLConverter<PyWriter>,
}

#[pymethods]
impl PyXMLConverter {
    #[new]
    #[pyo3(signature = (rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None, stripcontrol=false, imagewriter=None))]
    fn new(
        rsrcmgr: &Bound<'_, PyAny>,
        outfp: Py<PyAny>,
        codec: &str,
        pageno: i32,
        laparams: Option<&PyLAParams>,
        stripcontrol: bool,
        imagewriter: Option<&Bound<'_, PyAny>>,
    ) -> Self {
        let _ = rsrcmgr;
        let _ = imagewriter;
        let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
        let mut converter = bolivar_core::converter::XMLConverter::with_options(
            PyWriter::new(outfp),
            codec,
            pageno,
            la,
            stripcontrol,
        );
        converter.set_stripcontrol(stripcontrol);
        Self { converter }
    }

    fn _receive_layout(&mut self, ltpage: &PyLTPage) {
        let core_page = py_ltpage_to_core(ltpage);
        self.converter.receive_layout(core_page);
    }

    fn close(&mut self) {
        self.converter.close();
        self.converter.flush();
    }
}

fn py_any_to_string_map(obj: &Bound<'_, PyAny>) -> PyResult<HashMap<String, String>> {
    obj.extract()
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

/// Extract tables from a filtered/cropped pdfplumber Page using Rust table extraction.
#[pyfunction]
#[pyo3(signature = (page, table_settings = None))]
fn extract_tables_from_page_filtered(
    py: Python<'_>,
    page: &Bound<'_, PyAny>,
    table_settings: Option<Py<PyAny>>,
) -> PyResult<Vec<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let (chars, edges, geom) = page_objects_to_chars_edges(py, page)?;
    Ok(extract_tables_from_objects(chars, edges, &geom, &settings))
}

/// Extract a single table from a filtered/cropped pdfplumber Page using Rust table extraction.
#[pyfunction]
#[pyo3(signature = (page, table_settings = None))]
fn extract_table_from_page_filtered(
    py: Python<'_>,
    page: &Bound<'_, PyAny>,
    table_settings: Option<Py<PyAny>>,
) -> PyResult<Option<Vec<Vec<Option<String>>>>> {
    let settings = parse_table_settings(py, table_settings)?;
    let (chars, edges, geom) = page_objects_to_chars_edges(py, page)?;
    Ok(extract_table_from_objects(chars, edges, &geom, &settings))
}

/// Repair a PDF and return the repaired bytes.
#[pyfunction]
fn repair_pdf(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let (bytes, _path) = read_bytes_and_path(py, data)?;
    let repaired = bolivar_core::document::repair::repair_bytes(&bytes)
        .map_err(|e| PyValueError::new_err(format!("repair failed: {e}")))?;
    Ok(PyBytes::new(py, &repaired).into_any().unbind())
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
    data: &Bound<'_, PyAny>,
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
    let result = match pdf_input_from_py(data)? {
        PdfInput::Shared(bytes) => {
            let doc = PDFDocument::new_from_bytes(bytes, password)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.allow_threads(|| core_extract_text_with_document(&doc, options))
        }
        PdfInput::Owned(bytes) => py.allow_threads(|| core_extract_text(&bytes, Some(options))),
    };
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
    let doc = PDFDocument::new_from_mmap(mmap, password)
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
    data: &Bound<'_, PyAny>,
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
    let pages = match pdf_input_from_py(data)? {
        PdfInput::Shared(bytes) => {
            let doc = PDFDocument::new_from_bytes(bytes, password)
                .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
            py.allow_threads(|| core_extract_pages_with_document(&doc, options))
        }
        PdfInput::Owned(bytes) => py.allow_threads(|| {
            let iter = core_extract_pages(&bytes, Some(options))?;
            iter.collect::<bolivar_core::error::Result<Vec<_>>>()
        }),
    }
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
    let doc = PDFDocument::new_from_mmap(mmap, password)
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
    m.add_class::<PyPDFResourceManager>()?;
    m.add_class::<PyPSLiteral>()?;
    m.add_class::<PyPSKeyword>()?;
    m.add_class::<PyPSBaseParser>()?;
    m.add_class::<PyPSStackParser>()?;
    m.add_class::<PyPDFParser>()?;
    m.add_class::<PyPDFDocument>()?;
    m.add_class::<PyPDFStream>()?;
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
    m.add_class::<PyTextConverter>()?;
    m.add_class::<PyHTMLConverter>()?;
    m.add_class::<PyXMLConverter>()?;
    m.add_class::<PyNumberTree>()?;
    m.add_function(wrap_pyfunction!(LIT, m)?)?;
    m.add_function(wrap_pyfunction!(KWD, m)?)?;
    let py = m.py();
    let encoding_bytes: Vec<u8> = (0u8..=255).collect();
    let pdf_doc_encoding = bolivar_core::utils::decode_text(&encoding_bytes);
    m.add("PDFDocEncoding", pdf_doc_encoding)?;
    m.add("INF", (1i64 << 31) - 1)?;
    m.add(
        "MATRIX_IDENTITY",
        (1.0_f64, 0.0_f64, 0.0_f64, 1.0_f64, 0.0_f64, 0.0_f64),
    )?;
    m.add("KEYWORD_PROC_BEGIN", intern_pskeyword(py, b"{".to_vec())?)?;
    m.add("KEYWORD_PROC_END", intern_pskeyword(py, b"}".to_vec())?)?;
    m.add("KEYWORD_ARRAY_BEGIN", intern_pskeyword(py, b"[".to_vec())?)?;
    m.add("KEYWORD_ARRAY_END", intern_pskeyword(py, b"]".to_vec())?)?;
    m.add("KEYWORD_DICT_BEGIN", intern_pskeyword(py, b"<<".to_vec())?)?;
    m.add("KEYWORD_DICT_END", intern_pskeyword(py, b">>".to_vec())?)?;
    m.add_function(wrap_pyfunction!(decode_text, m)?)?;
    m.add_function(wrap_pyfunction!(isnumber, m)?)?;
    m.add_function(wrap_pyfunction!(extract_text, m)?)?;
    m.add_function(wrap_pyfunction!(extract_text_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(process_page, m)?)?;
    m.add_function(wrap_pyfunction!(process_pages, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_from_page, m)?)?;
    m.add_function(wrap_pyfunction!(extract_table_from_page, m)?)?;
    m.add_function(wrap_pyfunction!(extract_tables_from_page_filtered, m)?)?;
    m.add_function(wrap_pyfunction!(extract_table_from_page_filtered, m)?)?;
    m.add_function(wrap_pyfunction!(repair_pdf, m)?)?;
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
            let py_bytes = PyBytes::new(py, &pdf_data);
            let text_bytes =
                extract_text(py, py_bytes.as_any(), "", None, 0, true, None, None).unwrap();
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
