//! Font, CMap, and encoding bindings for Python.

use std::collections::HashMap;
use std::sync::Mutex;

use bolivar_core::font::cmap::{CMap, CMapBase, CMapDB, IdentityCMap, IdentityCMapByte};
use bolivar_core::font::encoding::glyphname2unicode as core_glyphname2unicode;
use bolivar_core::font::encoding::{DiffEntry, name2unicode as core_name2unicode};
use bolivar_core::font::latin_enc::ENCODING as LATIN_ENCODING;
use bolivar_core::font::metrics::FONT_METRICS;
use bolivar_core::font::pdffont::{
    MockPdfFont, PDFCIDFont, PDFFont as CorePDFFont, get_widths as core_get_widths,
};
use bolivar_core::pdftypes::PDFObject;
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySequence, PySequenceMethods, PyTuple, PyType};

use crate::document::{PyPSKeyword, PyPSLiteral};

fn bytes_from_py(py: Python<'_>, data: &Bound<'_, PyAny>, label: &str) -> PyResult<Vec<u8>> {
    let buf = PyBuffer::<u8>::get(data)
        .map_err(|_| PyTypeError::new_err(format!("{label} expects a bytes-like object")))?;
    buf.to_vec(py)
}

fn psliteral_name(obj: &Bound<'_, PyAny>) -> Option<String> {
    if let Ok(lit) = obj.extract::<PyRef<'_, PyPSLiteral>>() {
        return Some(String::from_utf8_lossy(&lit.name).to_string());
    }
    if let Ok(kwd) = obj.extract::<PyRef<'_, PyPSKeyword>>() {
        return Some(String::from_utf8_lossy(&kwd.name).to_string());
    }
    if let Ok(name_attr) = obj.getattr("name") {
        if let Ok(bytes) = name_attr.downcast::<PyBytes>() {
            return Some(String::from_utf8_lossy(bytes.as_bytes()).to_string());
        }
        if let Ok(s) = name_attr.extract::<String>() {
            return Some(s);
        }
    }
    None
}

fn py_to_pdf_object(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<PDFObject> {
    if obj.is_none() {
        return Ok(PDFObject::Null);
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
    if let Ok(bytes) = obj.downcast::<PyBytes>() {
        return Ok(PDFObject::String(bytes.as_bytes().to_vec()));
    }
    if let Some(name) = psliteral_name(obj) {
        return Ok(PDFObject::Name(name));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(PDFObject::Name(s));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = HashMap::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            let value = py_to_pdf_object(py, &v)?;
            map.insert(key, value);
        }
        return Ok(PDFObject::Dict(map));
    }
    if let Ok(seq) = obj.downcast::<PySequence>() {
        if obj.downcast::<PyBytes>().is_ok() {
            return Err(PyTypeError::new_err("bytes are not a sequence"));
        }
        let mut items = Vec::new();
        let len = seq.len()?;
        for idx in 0..len {
            let item = seq.get_item(idx)?;
            let value = py_to_pdf_object(py, &item)?;
            items.push(value);
        }
        return Ok(PDFObject::Array(items));
    }

    Err(PyTypeError::new_err("unsupported PDF object type"))
}

fn py_to_pdf_object_resolving_refs(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<PDFObject> {
    if obj.hasattr("resolve")? {
        if let Ok(resolved) = obj.call_method0("resolve") {
            return py_to_pdf_object(py, &resolved);
        }
    }
    py_to_pdf_object(py, obj)
}

fn insert_code2cid(
    py: Python<'_>,
    root: &Bound<'_, PyDict>,
    code: &[u8],
    cid: u32,
) -> PyResult<()> {
    let mut current: Bound<'_, PyDict> = root.clone();
    for (idx, &byte) in code.iter().enumerate() {
        if idx + 1 == code.len() {
            current.set_item(byte, cid)?;
        } else {
            let next = match current.get_item(byte)? {
                Some(obj) => obj.downcast::<PyDict>()?.clone(),
                None => {
                    let dict = PyDict::new(py);
                    current.set_item(byte, &dict)?;
                    dict
                }
            };
            current = next;
        }
    }
    Ok(())
}

#[pyclass(name = "CMap")]
pub struct PyCMap {
    inner: CMap,
    attrs_cache: Mutex<Option<Py<PyAny>>>,
    code2cid_cache: Mutex<Option<Py<PyAny>>>,
}

impl PyCMap {
    pub fn from_core(cmap: CMap) -> Self {
        Self {
            inner: cmap,
            attrs_cache: Mutex::new(None),
            code2cid_cache: Mutex::new(None),
        }
    }
}

#[pymethods]
impl PyCMap {
    #[new]
    pub fn new() -> Self {
        Self::from_core(CMap::new())
    }

    fn __repr__(&self) -> String {
        match self.inner.name() {
            Some(name) => format!("<CMap: {name}>"),
            None => "<CMap: ?>".to_string(),
        }
    }

    #[getter]
    fn attrs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Ok(mut guard) = self.attrs_cache.lock() {
            if let Some(value) = guard.as_ref() {
                return Ok(value.clone_ref(py));
            }
            let dict = PyDict::new(py);
            for (k, v) in self.inner.attrs.iter() {
                dict.set_item(k, v)?;
            }
            let obj = dict.into_any().unbind();
            *guard = Some(obj.clone_ref(py));
            return Ok(obj);
        }
        Ok(PyDict::new(py).into_any().unbind())
    }

    #[getter]
    fn code2cid(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Ok(mut guard) = self.code2cid_cache.lock() {
            if let Some(value) = guard.as_ref() {
                return Ok(value.clone_ref(py));
            }

            let dict = PyDict::new(py);
            for (code, cid) in self.inner.explicit_mappings() {
                insert_code2cid(py, &dict, &code, cid)?;
            }

            for (start, end, cid_start) in self.inner.range_entries() {
                match (start.len(), end.len()) {
                    (1, 1) => {
                        let s = start[0];
                        let e = end[0];
                        if s <= e {
                            for (offset, byte) in (s..=e).enumerate() {
                                insert_code2cid(py, &dict, &[byte], cid_start + offset as u32)?;
                            }
                        }
                    }
                    (2, 2) => {
                        let s = u16::from_be_bytes([start[0], start[1]]);
                        let e = u16::from_be_bytes([end[0], end[1]]);
                        if s <= e {
                            for offset in 0..=(e - s) {
                                let code = (s + offset).to_be_bytes();
                                insert_code2cid(py, &dict, &code, cid_start + offset as u32)?;
                            }
                        }
                    }
                    _ => {
                        // Avoid massive expansions for larger ranges; include a representative mapping.
                        insert_code2cid(py, &dict, &start, cid_start)?;
                    }
                }
            }

            let obj = dict.into_any().unbind();
            *guard = Some(obj.clone_ref(py));
            return Ok(obj);
        }
        Ok(PyDict::new(py).into_any().unbind())
    }

    fn decode(&self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let bytes = bytes_from_py(py, data, "CMap.decode")?;
        let items: Vec<u32> = self.inner.decode(&bytes).collect();
        let tuple = PyTuple::new(py, items)?;
        Ok(tuple.into_any().unbind())
    }
}

#[pyclass(name = "IdentityCMap")]
pub struct PyIdentityCMap {
    inner: IdentityCMap,
}

#[pymethods]
impl PyIdentityCMap {
    #[new]
    #[pyo3(signature = (vertical = false))]
    pub fn new(vertical: bool) -> Self {
        Self {
            inner: IdentityCMap::new(vertical),
        }
    }

    fn decode(&self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let bytes = bytes_from_py(py, data, "IdentityCMap.decode")?;
        let items: Vec<u32> = self.inner.decode(&bytes).collect();
        let tuple = PyTuple::new(py, items)?;
        Ok(tuple.into_any().unbind())
    }
}

#[pyclass(name = "IdentityCMapByte")]
pub struct PyIdentityCMapByte {
    inner: IdentityCMapByte,
}

#[pymethods]
impl PyIdentityCMapByte {
    #[new]
    #[pyo3(signature = (vertical = false))]
    pub fn new(vertical: bool) -> Self {
        Self {
            inner: IdentityCMapByte::new(vertical),
        }
    }

    fn decode(&self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let bytes = bytes_from_py(py, data, "IdentityCMapByte.decode")?;
        let items: Vec<u32> = self.inner.decode(&bytes).collect();
        let tuple = PyTuple::new(py, items)?;
        Ok(tuple.into_any().unbind())
    }
}

#[pyclass(name = "CMapDB")]
pub struct PyCMapDB;

#[pymethods]
impl PyCMapDB {
    #[classmethod]
    fn is_identity_cmap(_cls: &Bound<'_, PyType>, name: &str) -> bool {
        CMapDB::is_identity_cmap(name)
    }

    #[classmethod]
    fn is_identity_cmap_byte(_cls: &Bound<'_, PyType>, name: &str) -> bool {
        CMapDB::is_identity_cmap_byte(name)
    }

    #[classmethod]
    fn is_cjk_2byte_cmap(_cls: &Bound<'_, PyType>, name: &str) -> bool {
        CMapDB::is_cjk_2byte_cmap(name)
    }

    #[classmethod]
    fn is_vertical(_cls: &Bound<'_, PyType>, name: &str) -> bool {
        CMapDB::is_vertical(name)
    }
}

#[pyfunction]
pub fn name2unicode(name: &str) -> PyResult<String> {
    core_name2unicode(name).map_err(|_| {
        PyKeyError::new_err(format!(
            "Could not convert unicode name \"{name}\" to character"
        ))
    })
}

#[pyclass(name = "EncodingDB")]
pub struct PyEncodingDB;

#[pymethods]
impl PyEncodingDB {
    #[classmethod]
    #[pyo3(signature = (name, diff = None))]
    fn get_encoding(
        _cls: &Bound<'_, PyType>,
        name: &str,
        diff: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let mut entries = Vec::new();
        if let Some(diff) = diff {
            if let Ok(seq) = diff.downcast::<PySequence>() {
                let len = seq.len()?;
                for idx in 0..len {
                    let item = seq.get_item(idx)?;
                    if let Ok(code) = item.extract::<u8>() {
                        entries.push(DiffEntry::Code(code));
                    } else if let Some(name) = psliteral_name(&item) {
                        entries.push(DiffEntry::Name(name));
                    }
                }
            }
        }
        let encoding = if entries.is_empty() {
            bolivar_core::font::EncodingDB::get_encoding(name, None)
        } else {
            bolivar_core::font::EncodingDB::get_encoding(name, Some(&entries))
        };

        Python::attach(|py| {
            let dict = PyDict::new(py);
            for (k, v) in encoding {
                dict.set_item(k, v)?;
            }
            Ok(dict.into_any().unbind())
        })
    }
}

#[pyfunction]
pub fn glyphname2unicode(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    for (name, ch) in core_glyphname2unicode().iter() {
        dict.set_item(*name, ch.to_string())?;
    }
    Ok(dict.into_any().unbind())
}

#[pyfunction]
pub fn latin_encoding(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let list = PyList::empty(py);
    for &(name, std, mac, win, pdf) in LATIN_ENCODING {
        let tuple = PyTuple::new(
            py,
            [
                name.into_pyobject(py)?.into_any(),
                std.into_pyobject(py)?.into_any(),
                mac.into_pyobject(py)?.into_any(),
                win.into_pyobject(py)?.into_any(),
                pdf.into_pyobject(py)?.into_any(),
            ],
        )?;
        list.append(tuple)?;
    }
    Ok(list.into_any().unbind())
}

#[pyfunction]
pub fn font_metrics(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    for (name, metrics) in FONT_METRICS.iter() {
        let props = PyDict::new(py);
        props.set_item("FontName", *name)?;
        props.set_item("Ascent", metrics.ascent)?;
        props.set_item("Descent", metrics.descent)?;

        let widths = PyDict::new(py);
        for (ch, width) in metrics.widths.iter() {
            widths.set_item(ch.to_string(), *width)?;
        }

        dict.set_item(*name, (props, widths))?;
    }
    Ok(dict.into_any().unbind())
}

#[pyclass(name = "PDFFont", subclass)]
pub struct PyPDFFont {
    inner: MockPdfFont,
}

#[pymethods]
impl PyPDFFont {
    #[new]
    #[pyo3(signature = (descriptor, widths, default_width = 0.0))]
    pub fn new(
        _py: Python<'_>,
        descriptor: &Bound<'_, PyAny>,
        widths: &Bound<'_, PyAny>,
        default_width: f64,
    ) -> PyResult<Self> {
        let _ = descriptor; // descriptor is currently unused by the mock font
        let widths_dict = widths
            .downcast::<PyDict>()
            .map_err(|_| PyTypeError::new_err("widths must be a dict"))?;
        let mut width_map: HashMap<u32, Option<f64>> = HashMap::new();
        for (k, v) in widths_dict.iter() {
            let key = if let Ok(i) = k.extract::<u32>() {
                Some(i)
            } else if let Ok(s) = k.extract::<String>() {
                s.parse::<u32>().ok()
            } else {
                None
            };
            if let Some(cid) = key {
                if v.is_none() {
                    width_map.insert(cid, None);
                } else if let Ok(width) = v.extract::<f64>() {
                    width_map.insert(cid, Some(width));
                }
            }
        }
        let descriptor_map: HashMap<String, PDFObject> = HashMap::new();
        Ok(Self {
            inner: MockPdfFont::new(descriptor_map, width_map, default_width),
        })
    }

    pub fn to_unichr(&self, cid: u32) -> Option<String> {
        self.inner.to_unichr(cid)
    }

    pub fn char_width(&self, cid: u32) -> f64 {
        self.inner.char_width(cid)
    }
}

#[pyclass(name = "PDFCIDFont")]
pub struct PyPDFCIDFont {
    inner: PDFCIDFont,
}

#[pymethods]
impl PyPDFCIDFont {
    #[new]
    #[pyo3(signature = (rsrcmgr, spec, strict = false))]
    pub fn new(
        py: Python<'_>,
        rsrcmgr: &Bound<'_, PyAny>,
        spec: &Bound<'_, PyAny>,
        strict: bool,
    ) -> PyResult<Self> {
        let _ = rsrcmgr; // unused for now
        let _ = strict;
        let spec_dict = spec
            .downcast::<PyDict>()
            .map_err(|_| PyTypeError::new_err("spec must be a dict"))?;
        let mut map = HashMap::new();
        for (k, v) in spec_dict.iter() {
            let key: String = k.extract()?;
            let value = py_to_pdf_object(py, &v)?;
            map.insert(key, value);
        }
        Ok(Self {
            inner: PDFCIDFont::new(&map, None),
        })
    }

    #[pyo3(signature = (spec, strict = false))]
    pub fn get_cmap_from_spec(
        &self,
        py: Python<'_>,
        spec: &Bound<'_, PyAny>,
        strict: bool,
    ) -> PyResult<Py<PyAny>> {
        let _ = spec;
        let _ = strict;
        dyn_cmap_to_py(py, &self.inner.cmap)
    }

    pub fn char_width(&self, cid: u32) -> f64 {
        self.inner.char_width(cid)
    }

    pub fn to_unichr(&self, cid: u32) -> Option<String> {
        self.inner.to_unichr(cid)
    }
}

fn dyn_cmap_to_py(
    py: Python<'_>,
    cmap: &bolivar_core::font::pdffont::DynCMap,
) -> PyResult<Py<PyAny>> {
    use bolivar_core::font::pdffont::DynCMap;
    match cmap {
        DynCMap::CMap(cmap) => Ok(Py::new(py, PyCMap::from_core((**cmap).clone()))?.into_any()),
        DynCMap::IdentityCMap(cmap) => Ok(Py::new(
            py,
            PyIdentityCMap {
                inner: cmap.clone(),
            },
        )?
        .into_any()),
        DynCMap::IdentityCMapByte(cmap) => Ok(Py::new(
            py,
            PyIdentityCMapByte {
                inner: cmap.clone(),
            },
        )?
        .into_any()),
    }
}

#[pyfunction]
pub fn get_widths(py: Python<'_>, seq: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let seq = seq
        .downcast::<PySequence>()
        .map_err(|_| PyTypeError::new_err("get_widths expects a sequence"))?;
    let mut objects = Vec::new();
    let len = seq.len()?;
    for idx in 0..len {
        let item = seq.get_item(idx)?;
        let obj = py_to_pdf_object_resolving_refs(py, &item)?;
        objects.push(obj);
    }
    let widths = core_get_widths(
        &objects,
        None::<&fn(&bolivar_core::pdftypes::PDFObjRef) -> Option<PDFObject>>,
    );
    let dict = PyDict::new(py);
    for (k, v) in widths {
        dict.set_item(k, v)?;
    }
    Ok(dict.into_any().unbind())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCMap>()?;
    m.add_class::<PyIdentityCMap>()?;
    m.add_class::<PyIdentityCMapByte>()?;
    m.add_class::<PyCMapDB>()?;
    m.add_class::<PyEncodingDB>()?;
    m.add_class::<PyPDFFont>()?;
    m.add_class::<PyPDFCIDFont>()?;
    m.add_function(wrap_pyfunction!(name2unicode, m)?)?;
    m.add_function(wrap_pyfunction!(glyphname2unicode, m)?)?;
    m.add_function(wrap_pyfunction!(latin_encoding, m)?)?;
    m.add_function(wrap_pyfunction!(font_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(get_widths, m)?)?;
    Ok(())
}
