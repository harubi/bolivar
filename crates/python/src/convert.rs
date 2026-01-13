//! PDF object conversion utilities for Python.
//!
//! Provides functions to convert between Rust PDFObject types and Python objects,
//! handling references, streams, and interned PS literals/keywords.

use bolivar_core::parser::PSToken;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdftypes::{PDFObject, PDFStream};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySequence, PySequenceMethods};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use crate::document::{PyPDFStream, PyPSKeyword, PyPSLiteral};

/// Global intern table for PSLiteral objects.
pub static PSLITERAL_TABLE: OnceLock<Mutex<HashMap<Vec<u8>, Py<PyAny>>>> = OnceLock::new();

/// Global intern table for PSKeyword objects.
pub static PSKEYWORD_TABLE: OnceLock<Mutex<HashMap<Vec<u8>, Py<PyAny>>>> = OnceLock::new();

/// Convert a name to a PSLiteral Python object.
pub fn name_to_psliteral(py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
    let psparser = py.import("pdfminer.psparser")?;
    let cls = psparser.getattr("PSLiteral")?;
    let obj = cls.call1((PyBytes::new(py, name.as_bytes()),))?;
    Ok(obj.into_any().unbind())
}

/// Convert a PDFObject to a Python object with full options.
pub fn pdf_object_to_py_internal(
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
pub fn pdf_object_to_py(
    py: Python<'_>,
    obj: &PDFObject,
    doc: &PDFDocument,
    visited: &mut HashSet<(u32, u32)>,
) -> PyResult<Py<PyAny>> {
    pdf_object_to_py_internal(py, obj, doc, visited, false, true, None)
}

/// Create a PS exception from pdfminer.psexceptions module.
pub fn ps_exception(py: Python<'_>, class_name: &str, msg: &str) -> PyErr {
    if let Ok(module) = py.import("pdfminer.psexceptions")
        && let Ok(cls) = module.getattr(class_name)
        && let Ok(err) = cls.call1((msg,))
    {
        return PyErr::from_value(err);
    }
    PyValueError::new_err(msg.to_string())
}

/// Convert PS name to bytes.
pub fn ps_name_to_bytes(name: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(bytes) = name.extract::<Vec<u8>>() {
        return Ok(bytes);
    }
    let s: String = name.extract()?;
    Ok(s.into_bytes())
}

pub(crate) fn psliteral_name(obj: &Bound<'_, PyAny>) -> Option<String> {
    if let Ok(lit) = obj.extract::<PyRef<'_, PyPSLiteral>>() {
        return Some(String::from_utf8_lossy(&lit.name).to_string());
    }
    if let Ok(kwd) = obj.extract::<PyRef<'_, PyPSKeyword>>() {
        return Some(String::from_utf8_lossy(&kwd.name).to_string());
    }
    if let Ok(name_attr) = obj.getattr("name") {
        if let Ok(bytes) = name_attr.cast::<PyBytes>() {
            return Some(String::from_utf8_lossy(bytes.as_bytes()).to_string());
        }
        if let Ok(s) = name_attr.extract::<String>() {
            return Some(s);
        }
    }
    None
}

/// Convert a Python object to a PDFObject.
pub fn py_to_pdf_object(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<PDFObject> {
    if obj.is_none() {
        return Ok(PDFObject::Null);
    }
    if let Ok(py_stream) = obj.extract::<PyRef<'_, PyPDFStream>>() {
        return Ok(PDFObject::Stream(Box::new(py_stream.stream.clone())));
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(PDFObject::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(PDFObject::Int(i));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(PDFObject::Real(f));
    }
    if let Ok(bytes) = obj.cast::<PyBytes>() {
        return Ok(PDFObject::String(bytes.as_bytes().to_vec()));
    }
    if let Some(name) = psliteral_name(obj) {
        return Ok(PDFObject::Name(name));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(PDFObject::Name(s));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = HashMap::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            let value = py_to_pdf_object(py, &v)?;
            map.insert(key, value);
        }
        return Ok(PDFObject::Dict(map));
    }
    if let Ok(seq) = obj.cast::<PySequence>() {
        if obj.cast::<PyBytes>().is_ok() {
            return Err(PyTypeError::new_err("bytes are not a sequence"));
        }
        let mut items = Vec::new();
        let len = seq.len()? as usize;
        for idx in 0..len {
            let item = seq.get_item(idx)?;
            let value = py_to_pdf_object(py, &item)?;
            items.push(value);
        }
        return Ok(PDFObject::Array(items));
    }

    Err(PyTypeError::new_err("unsupported PDF object type"))
}

/// Convert a Python object to a PDFObject, resolving references when possible.
pub fn py_to_pdf_object_resolving_refs(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
) -> PyResult<PDFObject> {
    if obj.hasattr("resolve")? {
        if let Ok(resolved) = obj.call_method0("resolve") {
            return py_to_pdf_object(py, &resolved);
        }
    }
    py_to_pdf_object(py, obj)
}

/// Intern a PSLiteral for efficient reuse.
pub fn intern_psliteral(py: Python<'_>, name: Vec<u8>) -> PyResult<Py<PyAny>> {
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

/// Intern a PSKeyword for efficient reuse.
pub fn intern_pskeyword(py: Python<'_>, name: Vec<u8>) -> PyResult<Py<PyAny>> {
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

/// Convert a PSToken to a Python object.
pub fn pstoken_to_py(py: Python<'_>, token: PSToken) -> PyResult<Py<PyAny>> {
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
                let key = k.into_pyobject(py)?.into_any();
                let val = pstoken_to_py(py, v)?;
                dict.set_item(key, val)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

/// Convert a PDFObject to a Python object (simple version without reference resolution).
pub fn pdf_object_to_py_simple(py: Python<'_>, obj: &PDFObject) -> PyResult<Py<PyAny>> {
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

/// Convert Python text to a String.
pub fn py_text_to_string(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = obj.extract::<String>() {
        return Ok(s);
    }
    if let Ok(bytes) = obj.extract::<Vec<u8>>() {
        return Ok(String::from_utf8_lossy(&bytes).to_string());
    }
    Ok(obj.str()?.to_string())
}

/// Convert Python Any to a HashMap<String, String>.
pub fn py_any_to_string_map(obj: &Bound<'_, PyAny>) -> PyResult<HashMap<String, String>> {
    obj.extract()
}

/// PyPDFStream implementation for from_core.
impl PyPDFStream {
    pub fn from_core(
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
