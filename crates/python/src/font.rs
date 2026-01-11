//! Font, CMap, and encoding bindings for Python.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bolivar_core::font::cmap::{
    CMap, CMapBase, CMapDB, IdentityCMap, IdentityCMapByte, UnicodeMap,
};
use bolivar_core::font::encoding::glyphname2unicode as core_glyphname2unicode;
use bolivar_core::font::encoding::{DiffEntry, name2unicode as core_name2unicode};
use bolivar_core::font::latin_enc::ENCODING as LATIN_ENCODING;
use bolivar_core::font::metrics::FONT_METRICS;
use bolivar_core::font::pdffont::{
    MockPdfFont, PDFCIDFont, PDFFont as CorePDFFont, get_widths as core_get_widths,
};
use bolivar_core::pdftypes::PDFObject;
use flate2::read::GzDecoder;
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySequence, PySequenceMethods, PyTuple, PyType};
use serde_json::Value;

use crate::convert::{psliteral_name, py_to_pdf_object, py_to_pdf_object_resolving_refs};

fn bytes_from_py(py: Python<'_>, data: &Bound<'_, PyAny>, label: &str) -> PyResult<Vec<u8>> {
    let buf = PyBuffer::<u8>::get(data)
        .map_err(|_| PyTypeError::new_err(format!("{label} expects a bytes-like object")))?;
    buf.to_vec(py)
}

// py_to_pdf_object helpers now live in convert.rs for reuse.

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

fn cmap_directories(py: Python<'_>) -> PyResult<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let env_dir = std::env::var("CMAP_PATH").unwrap_or_else(|_| "/usr/share/pdfminer/".to_string());
    dirs.push(PathBuf::from(env_dir));

    let pdfminer = py.import("pdfminer")?;
    let file: String = pdfminer.getattr("__file__")?.extract()?;
    let base_dir = Path::new(&file)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    dirs.push(base_dir.join("cmap"));
    Ok(dirs)
}

fn find_cmap_json_path(py: Python<'_>, name: &str) -> PyResult<PathBuf> {
    let filename = format!("{name}.json.gz");
    for dir in cmap_directories(py)? {
        let candidate = dir.join(&filename);
        if !candidate.exists() {
            continue;
        }
        let resolved_dir = match dir.canonicalize() {
            Ok(path) => path,
            Err(_) => continue,
        };
        let resolved_path = match candidate.canonicalize() {
            Ok(path) => path,
            Err(_) => continue,
        };
        if resolved_path.starts_with(&resolved_dir) {
            return Ok(resolved_path);
        }
    }
    Err(PyKeyError::new_err(format!("CMap not found: {name}")))
}

fn load_cmap_json(py: Python<'_>, name: &str) -> PyResult<Value> {
    let path = find_cmap_json_path(py, name)?;
    let file = File::open(&path)
        .map_err(|e| PyValueError::new_err(format!("failed to open {}: {e}", path.display())))?;
    let decoder = GzDecoder::new(file);
    serde_json::from_reader(decoder)
        .map_err(|e| PyValueError::new_err(format!("failed to parse {}: {e}", path.display())))
}

fn parse_code2cid_json(value: &Value, prefix: &mut Vec<u8>, cmap: &mut CMap) -> PyResult<()> {
    let obj = value
        .as_object()
        .ok_or_else(|| PyValueError::new_err("CODE2CID must be an object"))?;
    for (key, entry) in obj.iter() {
        let byte: u16 = key
            .parse()
            .map_err(|_| PyValueError::new_err(format!("CODE2CID key is not int: {key}")))?;
        if byte > u8::MAX as u16 {
            return Err(PyValueError::new_err(format!(
                "CODE2CID key out of range: {key}"
            )));
        }
        prefix.push(byte as u8);
        if entry.is_object() {
            parse_code2cid_json(entry, prefix, cmap)?;
        } else if let Some(cid) = entry.as_u64() {
            let cid = u32::try_from(cid)
                .map_err(|_| PyValueError::new_err("CODE2CID cid out of range"))?;
            cmap.add_code2cid(prefix.as_slice(), cid);
        } else {
            return Err(PyValueError::new_err(
                "CODE2CID values must be objects or integers",
            ));
        }
        prefix.pop();
    }
    Ok(())
}

fn parse_cid2unichr_json(value: &Value) -> PyResult<HashMap<u32, String>> {
    let obj = value
        .as_object()
        .ok_or_else(|| PyValueError::new_err("CID2UNICHR must be an object"))?;
    let mut out = HashMap::new();
    for (key, entry) in obj.iter() {
        let cid: u32 = key
            .parse()
            .map_err(|_| PyValueError::new_err(format!("CID2UNICHR key is not int: {key}")))?;
        let ch = entry
            .as_str()
            .ok_or_else(|| PyValueError::new_err("CID2UNICHR values must be strings"))?;
        out.insert(cid, ch.to_string());
    }
    Ok(out)
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

#[pyclass(name = "UnicodeMap")]
pub struct PyUnicodeMap {
    inner: UnicodeMap,
    cid2unichr_cache: Mutex<Option<Py<PyAny>>>,
    cid2unichr_data: HashMap<u32, String>,
}

impl PyUnicodeMap {
    fn from_parts(inner: UnicodeMap, cid2unichr_data: HashMap<u32, String>) -> Self {
        Self {
            inner,
            cid2unichr_cache: Mutex::new(None),
            cid2unichr_data,
        }
    }
}

#[pymethods]
impl PyUnicodeMap {
    #[new]
    pub fn new() -> Self {
        Self::from_parts(UnicodeMap::new(), HashMap::new())
    }

    fn __repr__(&self) -> String {
        match self.inner.attrs.get("CMapName") {
            Some(name) => format!("<UnicodeMap: {name}>"),
            None => "<UnicodeMap: ?>".to_string(),
        }
    }

    #[getter]
    fn cid2unichr(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Ok(mut guard) = self.cid2unichr_cache.lock() {
            if let Some(value) = guard.as_ref() {
                return Ok(value.clone_ref(py));
            }
            let dict = PyDict::new(py);
            for (cid, ch) in self.cid2unichr_data.iter() {
                dict.set_item(*cid, ch)?;
            }
            let obj = dict.into_any().unbind();
            *guard = Some(obj.clone_ref(py));
            return Ok(obj);
        }
        Ok(PyDict::new(py).into_any().unbind())
    }

    pub fn get_unichr(&self, cid: u32) -> Option<String> {
        self.inner.get_unichr(cid)
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

    #[classmethod]
    fn get_cmap(_cls: &Bound<'_, PyType>, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        let name = name.replace('\0', "");
        if CMapDB::is_identity_cmap_byte(&name) {
            let cmap = PyIdentityCMapByte::new(CMapDB::is_vertical(&name));
            return Ok(Py::new(py, cmap)?.into_any());
        }
        if CMapDB::is_identity_cmap(&name) {
            let cmap = PyIdentityCMap::new(CMapDB::is_vertical(&name));
            return Ok(Py::new(py, cmap)?.into_any());
        }

        let data = load_cmap_json(py, &name)?;
        let code2cid = data
            .get("CODE2CID")
            .ok_or_else(|| PyValueError::new_err("CODE2CID missing from CMap data"))?;
        let is_vertical = data
            .get("IS_VERTICAL")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| CMapDB::is_vertical(&name));

        let mut cmap = CMap::new();
        cmap.attrs.insert("CMapName".to_string(), name.clone());
        if is_vertical {
            cmap.set_vertical(true);
            cmap.attrs.insert("WMode".to_string(), "1".to_string());
        }

        let mut prefix = Vec::new();
        parse_code2cid_json(code2cid, &mut prefix, &mut cmap)?;
        Ok(Py::new(py, PyCMap::from_core(cmap))?.into_any())
    }

    #[classmethod]
    #[pyo3(signature = (name, vertical = false))]
    fn get_unicode_map(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: &str,
        vertical: bool,
    ) -> PyResult<Py<PyAny>> {
        let name = name.replace('\0', "");
        let data = load_cmap_json(py, &format!("to-unicode-{name}"))?;
        let key = if vertical {
            "CID2UNICHR_V"
        } else {
            "CID2UNICHR_H"
        };
        let cid2unichr = data
            .get(key)
            .ok_or_else(|| PyValueError::new_err(format!("{key} missing from Unicode map")))?;
        let parsed = parse_cid2unichr_json(cid2unichr)?;

        let mut umap = UnicodeMap::new();
        umap.attrs.insert("CMapName".to_string(), name.clone());
        if vertical {
            umap.set_vertical(true);
            umap.attrs.insert("WMode".to_string(), "1".to_string());
        }

        let mut cid2unichr_data: HashMap<u32, String> = HashMap::new();
        for (cid, ch) in parsed {
            if ch == "\u{00a0}" && cid2unichr_data.get(&cid).map(|s| s.as_str()) == Some(" ") {
                continue;
            }
            umap.add_cid2unichr(cid, ch.clone());
            cid2unichr_data.insert(cid, ch);
        }

        Ok(Py::new(py, PyUnicodeMap::from_parts(umap, cid2unichr_data))?.into_any())
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

    #[getter]
    pub fn cmap(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
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
    m.add_class::<PyUnicodeMap>()?;
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
