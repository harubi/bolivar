//! PDF document and page wrappers for Python.
//!
//! Provides PyPDFDocument, PyPDFPage, and parser classes with lazy page loading.

use bolivar_core::parser::{
    PDFParser as CorePDFParser, PSBaseParser as CorePSBaseParser,
    PSStackParser as CorePSStackParser,
};
use bolivar_core::pdfdocument::{DEFAULT_CACHE_CAPACITY, PDFDocument};
use bolivar_core::pdftypes::{PDFObject, PDFStream};
use bytes::Bytes;
use memmap2::Mmap;
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySequence, PyTuple, PyType};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::slice;
use std::sync::{Arc, Mutex};

use crate::convert::{
    intern_pskeyword, intern_psliteral, pdf_object_to_py, pdf_object_to_py_internal,
    pdf_object_to_py_simple, ps_exception, ps_name_to_bytes, pstoken_to_py, py_to_pdf_object,
};

/// Helper enum for PDF input data.
pub enum PdfInput {
    Shared(Bytes),
    Owned(Vec<u8>),
}

/// Wrapper for PyBuffer to allow AsRef<[u8]>.
pub struct PyBufferOwner {
    buffer: PyBuffer<u8>,
}

impl PyBufferOwner {
    pub fn new(buffer: PyBuffer<u8>) -> Self {
        Self { buffer }
    }
}

impl AsRef<[u8]> for PyBufferOwner {
    fn as_ref(&self) -> &[u8] {
        // Safety: PyBufferOwner owns the PyBuffer, so the backing memory is
        // valid for the lifetime of &self and len_bytes reflects the buffer size.
        unsafe {
            slice::from_raw_parts(self.buffer.buf_ptr().cast::<u8>(), self.buffer.len_bytes())
        }
    }
}

/// Convert Python object to PDF input data.
pub fn pdf_input_from_py(data: &Bound<'_, PyAny>) -> PyResult<PdfInput> {
    let buf = PyBuffer::<u8>::get(data)
        .map_err(|_| PyTypeError::new_err("data must be a bytes-like object"))?;
    if buf.readonly() && buf.is_c_contiguous() {
        let owner = PyBufferOwner::new(buf);
        Ok(PdfInput::Shared(Bytes::from_owner(owner)))
    } else {
        Ok(PdfInput::Owned(buf.to_vec(data.py())?))
    }
}

/// Read bytes and optional path from a Python file-like object.
pub fn read_bytes_and_path(
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
        && std::path::Path::new(&path_str).is_file()
    {
        let data = std::fs::read(&path_str)
            .map_err(|e| PyValueError::new_err(format!("failed to read {path_str}: {e}")))?;
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

/// Check if object is a PDF object reference.
fn is_pdf_obj_ref(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(obj.hasattr("objid")? && obj.hasattr("resolve")?)
}

/// Resolve a PDF object reference.
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

/// Parse a number tree from a PDF object.
fn parse_number_tree(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
    visited: &mut HashSet<i64>,
    out: &mut Vec<(i64, Py<PyAny>)>,
) -> PyResult<()> {
    if is_pdf_obj_ref(obj)?
        && let Ok(objid) = obj.getattr("objid")?.extract::<i64>()
    {
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

    let dict = match resolved.cast::<PyDict>() {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };

    if let Some(nums_obj) = dict.get_item("Nums")? {
        let nums_resolved = resolve_pdf_obj(py, &nums_obj)?;
        let nums = nums_resolved.bind(py);
        if let Ok(seq) = nums.cast::<PySequence>() {
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
        if let Ok(seq) = kids.cast::<PySequence>() {
            let len = seq.len().unwrap_or(0);
            for idx in 0..len {
                let kid = seq.get_item(idx)?;
                parse_number_tree(py, &kid, visited, out)?;
            }
        }
    }

    Ok(())
}

/// Number tree for PDF name/number trees.
#[pyclass(name = "NumberTree")]
pub struct PyNumberTree {
    obj: Py<PyAny>,
}

#[pymethods]
impl PyNumberTree {
    #[new]
    pub fn new(obj: Py<PyAny>) -> Self {
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

/// PDF resource manager - Rust-backed pdfminer compatibility.
#[pyclass(name = "PDFResourceManager")]
pub struct PyPDFResourceManager {
    inner: bolivar_core::pdfinterp::PDFResourceManager,
}

#[pymethods]
impl PyPDFResourceManager {
    #[new]
    #[pyo3(signature = (caching = true))]
    pub fn new(caching: bool) -> Self {
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
            && val > 0
        {
            objid_opt = Some(val);
        }
        let spec_dict: HashMap<String, PDFObject> = HashMap::new();
        Ok(self.inner.get_font(objid_opt, &spec_dict))
    }
}

/// PostScript literal name.
#[pyclass(name = "PSLiteral")]
pub struct PyPSLiteral {
    pub name: Vec<u8>,
}

#[pymethods]
impl PyPSLiteral {
    #[new]
    pub fn new(_py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<Self> {
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
    pub name: Vec<u8>,
}

#[pymethods]
impl PyPSKeyword {
    #[new]
    pub fn new(_py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<Self> {
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

/// LIT function - create an interned PSLiteral.
#[pyfunction(name = "LIT")]
pub fn lit(py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let bytes = ps_name_to_bytes(name)?;
    intern_psliteral(py, bytes)
}

/// KWD function - create an interned PSKeyword.
#[pyfunction(name = "KWD")]
pub fn kwd(py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let bytes = ps_name_to_bytes(name)?;
    intern_pskeyword(py, bytes)
}

/// PostScript base parser.
#[pyclass(name = "PSBaseParser", unsendable, subclass)]
pub struct PyPSBaseParser {
    parser: CorePSBaseParser<'static>,
}

#[pymethods]
impl PyPSBaseParser {
    #[new]
    pub fn new(py: Python<'_>, fp: &Bound<'_, PyAny>) -> PyResult<Self> {
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
#[pyclass(name = "PSStackParser", unsendable, subclass)]
pub struct PyPSStackParser {
    parser: CorePSStackParser<'static>,
}

#[pymethods]
impl PyPSStackParser {
    #[new]
    pub fn new(py: Python<'_>, fp: &Bound<'_, PyAny>) -> PyResult<Self> {
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
    pub fn new(py: Python<'_>, fp: &Bound<'_, PyAny>) -> PyResult<Self> {
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
    pub stream: PDFStream,
    pub attrs: Py<PyDict>,
    pub doc: Option<Py<PyAny>>,
}

#[pymethods]
impl PyPDFStream {
    #[new]
    #[pyo3(signature = (attrs, rawdata, doc = None))]
    fn new(
        py: Python<'_>,
        attrs: &Bound<'_, PyDict>,
        rawdata: &Bound<'_, PyAny>,
        doc: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let mut map = HashMap::new();
        for (k, v) in attrs.iter() {
            let key: String = k.extract()?;
            let value = py_to_pdf_object(py, &v)?;
            map.insert(key, value);
        }
        let data = if let Ok(bytes) = rawdata.cast::<PyBytes>() {
            bytes.as_bytes().to_vec()
        } else if let Ok(s) = rawdata.extract::<String>() {
            s.into_bytes()
        } else {
            return Err(PyTypeError::new_err("rawdata must be bytes or str"));
        };
        let stream = PDFStream::new(map, data);
        let attrs_obj: Py<PyDict> = attrs.clone().unbind();
        Ok(Self {
            stream,
            attrs: attrs_obj,
            doc,
        })
    }

    #[getter]
    fn attrs(&self, py: Python<'_>) -> Py<PyDict> {
        self.attrs.clone_ref(py)
    }

    #[getter(rawdata)]
    fn rawdata_bytes(&self, py: Python<'_>) -> Py<PyAny> {
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

    fn get_rawdata(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(PyBytes::new(py, self.stream.get_rawdata())
            .into_any()
            .unbind())
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
    pub inner: Arc<PDFDocument>,
    /// Cache resolved objects for faster PDFObjRef resolution
    pub resolved_cache: Mutex<HashMap<u32, Py<PyAny>>>,
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
    #[pyo3(signature = (data, password = "", caching = true))]
    pub fn new(data: &Bound<'_, PyAny>, password: &str, caching: bool) -> PyResult<Self> {
        let cache_capacity = if caching { DEFAULT_CACHE_CAPACITY } else { 0 };
        let doc = match pdf_input_from_py(data)? {
            PdfInput::Shared(bytes) => {
                PDFDocument::new_from_bytes_with_cache(bytes, password, cache_capacity)
            }
            PdfInput::Owned(bytes) => PDFDocument::new_with_cache(bytes, password, cache_capacity),
        }
        .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
        Ok(Self {
            inner: Arc::new(doc),
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
    #[pyo3(signature = (path, password = "", caching = true))]
    pub fn from_path(
        _cls: &Bound<'_, PyType>,
        path: &str,
        password: &str,
        caching: bool,
    ) -> PyResult<Self> {
        let file = File::open(path)
            .map_err(|e| PyValueError::new_err(format!("Failed to open PDF: {}", e)))?;
        // Safety: the file handle remains open for the duration of the map.
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| PyValueError::new_err(format!("Failed to mmap PDF: {}", e)))?;
        let cache_capacity = if caching { DEFAULT_CACHE_CAPACITY } else { 0 };
        let doc = PDFDocument::new_from_mmap_with_cache(mmap, password, cache_capacity)
            .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
        Ok(Self {
            inner: Arc::new(doc),
            resolved_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Get an iterator over pages in the document.
    ///
    /// Returns:
    ///     List of PDFPage objects
    fn get_pages(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Vec<PyPDFPage>> {
        let mut pages = Vec::new();
        // Safety: slf is a valid, live Python object for the duration of this call.
        let py_doc = unsafe { Py::<PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        for (idx, page_result) in
            bolivar_core::pdfpage::PDFPage::create_pages(&slf.inner).enumerate()
        {
            let page = page_result
                .map_err(|e| PyValueError::new_err(format!("Failed to get page {}: {}", idx, e)))?;
            pages.push(PyPDFPage::from_core(
                py,
                Arc::new(page),
                Arc::clone(&slf.inner),
                Some(&py_doc),
            )?);
        }
        Ok(pages)
    }

    /// Get the total number of pages in the document.
    fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    /// Get mediaboxes for all pages in the document.
    fn page_mediaboxes(&self) -> PyResult<Vec<(f64, f64, f64, f64)>> {
        let boxes = self
            .inner
            .page_mediaboxes()
            .map_err(|e| PyValueError::new_err(format!("Failed to get mediaboxes: {}", e)))?;
        Ok(boxes
            .into_iter()
            .map(|b| (b[0], b[1], b[2], b[3]))
            .collect())
    }

    /// Get page labels for the document (as a list).
    fn get_page_labels(&self) -> PyResult<Vec<String>> {
        let labels = self
            .inner
            .get_page_labels()
            .map_err(|e| PyValueError::new_err(format!("Failed to get page labels: {}", e)))?;
        Ok(labels.collect())
    }

    /// Get a single page by index.
    fn get_page(slf: PyRef<'_, Self>, py: Python<'_>, index: usize) -> PyResult<PyPDFPage> {
        // Safety: slf is a valid, live Python object for the duration of this call.
        let py_doc = unsafe { Py::<PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        let page = slf
            .inner
            .get_page_cached(index)
            .map_err(|e| PyValueError::new_err(format!("Failed to get page {}: {}", index, e)))?;
        PyPDFPage::from_core(py, page, Arc::clone(&slf.inner), Some(&py_doc))
    }

    /// Get document info dictionaries.
    ///
    /// Returns:
    ///     List of dictionaries containing document metadata (Producer, Creator, etc.)
    #[getter]
    fn info(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        // Safety: slf is a valid, live Python object for the duration of this call.
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
        // Safety: slf is a valid, live Python object for the duration of this call.
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

    /// Get object IDs for each xref table.
    #[getter]
    fn xref_objids(slf: PyRef<'_, Self>) -> PyResult<Vec<Vec<u32>>> {
        Ok(slf.inner.get_xref_objids())
    }

    /// Get fallback flags for each xref table.
    #[getter]
    fn xref_fallbacks(slf: PyRef<'_, Self>) -> PyResult<Vec<bool>> {
        Ok(slf
            .inner
            .get_trailers()
            .map(|(fallback, _)| fallback)
            .collect())
    }

    /// Get document catalog dictionary.
    #[getter]
    fn catalog(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let py_dict = PyDict::new(py);
        let catalog = slf.inner.catalog().clone();
        // Safety: slf is a valid, live Python object for the duration of this call.
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
            && let Some(obj) = cache.get(&objid)
        {
            return Ok(obj.clone_ref(py));
        }
        let obj = slf
            .inner
            .getobj(objid)
            .map_err(|e| PyValueError::new_err(format!("Failed to resolve object: {}", e)))?;
        let mut visited = HashSet::new();
        // Safety: slf is a valid, live Python object for the duration of this call.
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
    core: Arc<bolivar_core::pdfpage::PDFPage>,
    doc: Arc<PDFDocument>,
    /// Page annotations (resolved to Python objects)
    annots_list: Mutex<Option<Py<PyAny>>>,
    /// Full page attributes dict (resolved)
    attrs_dict: Mutex<Option<Py<PyAny>>>,
    /// Page resources dict (resolved)
    resources_dict: Mutex<Option<Py<PyAny>>>,
    py_doc: Option<Py<PyAny>>,
}

impl PyPDFPage {
    /// Create from core PDFPage, resolving annotations
    pub fn from_core(
        py: Python<'_>,
        page: Arc<bolivar_core::pdfpage::PDFPage>,
        doc: Arc<PDFDocument>,
        py_doc: Option<&Py<PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            pageid: page.pageid,
            mediabox: page.mediabox.map(|b| (b[0], b[1], b[2], b[3])),
            cropbox: page.cropbox.map(|b| (b[0], b[1], b[2], b[3])),
            bleedbox: page.bleedbox.map(|b| (b[0], b[1], b[2], b[3])),
            trimbox: page.trimbox.map(|b| (b[0], b[1], b[2], b[3])),
            artbox: page.artbox.map(|b| (b[0], b[1], b[2], b[3])),
            rotate: page.rotate,
            label: page.label.clone(),
            core: page,
            doc,
            annots_list: Mutex::new(None),
            attrs_dict: Mutex::new(None),
            resources_dict: Mutex::new(None),
            py_doc: py_doc.map(|p| p.clone_ref(py)),
        })
    }

    pub(crate) fn core_contents(&self) -> Vec<Vec<u8>> {
        self.core.get_contents(&self.doc)
    }

    pub(crate) fn core_resources(
        &self,
    ) -> std::collections::HashMap<String, bolivar_core::pdftypes::PDFObject> {
        self.core.resources.clone()
    }

    fn ensure_attrs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Ok(mut guard) = self.attrs_dict.lock() {
            if let Some(value) = guard.as_ref() {
                return Ok(value.clone_ref(py));
            }
            let attrs_dict = PyDict::new(py);
            for (k, v) in self.core.attrs.iter() {
                let mut visited = HashSet::new();
                let py_val = pdf_object_to_py_internal(
                    py,
                    v,
                    &self.doc,
                    &mut visited,
                    false,
                    true,
                    self.py_doc.as_ref(),
                )?;
                attrs_dict.set_item(k, py_val)?;
            }
            let obj = attrs_dict.into_any().unbind();
            *guard = Some(obj.clone_ref(py));
            return Ok(obj);
        }
        Ok(PyDict::new(py).into_any().unbind())
    }

    fn ensure_resources(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Ok(mut guard) = self.resources_dict.lock() {
            if let Some(value) = guard.as_ref() {
                return Ok(value.clone_ref(py));
            }
            let resources_dict = PyDict::new(py);
            for (k, v) in self.core.resources.iter() {
                let mut visited = HashSet::new();
                let py_val = pdf_object_to_py_internal(
                    py,
                    v,
                    &self.doc,
                    &mut visited,
                    false,
                    false,
                    self.py_doc.as_ref(),
                )?;
                resources_dict.set_item(k, py_val)?;
            }
            let obj = resources_dict.into_any().unbind();
            *guard = Some(obj.clone_ref(py));
            return Ok(obj);
        }
        Ok(PyDict::new(py).into_any().unbind())
    }

    fn ensure_annots(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Ok(mut guard) = self.annots_list.lock() {
            if let Some(value) = guard.as_ref() {
                return Ok(value.clone_ref(py));
            }
            let annots_obj = match self.core.annots.as_ref() {
                Some(obj) => obj,
                None => {
                    let empty = PyList::empty(py).into_any().unbind();
                    *guard = Some(empty.clone_ref(py));
                    return Ok(empty);
                }
            };
            let resolved = match annots_obj {
                PDFObject::Ref(objref) => self
                    .doc
                    .getobj(objref.objid)
                    .unwrap_or_else(|_| annots_obj.clone()),
                _ => annots_obj.clone(),
            };
            let list = PyList::empty(py);
            if let PDFObject::Array(arr) = resolved {
                for item in arr {
                    let mut visited = HashSet::new();
                    let py_item = pdf_object_to_py(py, &item, &self.doc, &mut visited)?;
                    if py_item.bind(py).is_none() {
                        continue;
                    }
                    list.append(py_item)?;
                }
            }
            let obj = list.into_any().unbind();
            *guard = Some(obj.clone_ref(py));
            return Ok(obj);
        }
        Ok(PyList::empty(py).into_any().unbind())
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
        self.ensure_annots(py)
            .unwrap_or_else(|_| PyList::empty(py).into_any().unbind())
    }

    /// Get page attributes dict (resolved)
    #[getter]
    fn attrs(&self, py: Python<'_>) -> Py<PyAny> {
        self.ensure_attrs(py)
            .unwrap_or_else(|_| PyDict::new(py).into_any().unbind())
    }

    /// Get page resources dict (resolved)
    #[getter]
    fn resources(&self, py: Python<'_>) -> Py<PyAny> {
        self.ensure_resources(py)
            .unwrap_or_else(|_| PyDict::new(py).into_any().unbind())
    }
}

/// Decode text using bolivar_core utilities.
#[pyfunction]
pub fn decode_text(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<String> {
    let buf = PyBuffer::<u8>::get(data)
        .map_err(|_| PyTypeError::new_err("decode_text expects bytes-like object"))?;
    let bytes = buf.to_vec(py)?;
    Ok(py.detach(|| bolivar_core::utils::decode_text(&bytes)))
}

/// Check if an object is a number.
#[pyfunction]
pub fn isnumber(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    if obj.extract::<i64>().is_ok() || obj.extract::<f64>().is_ok() {
        return Ok(true);
    }
    Ok(false)
}

/// Register the document module classes with the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPDFResourceManager>()?;
    m.add_class::<PyPSLiteral>()?;
    m.add_class::<PyPSKeyword>()?;
    m.add_class::<PyPSBaseParser>()?;
    m.add_class::<PyPSStackParser>()?;
    m.add_class::<PyPDFParser>()?;
    m.add_class::<PyPDFDocument>()?;
    m.add_class::<PyPDFStream>()?;
    m.add_class::<PyPDFPage>()?;
    m.add_class::<PyNumberTree>()?;
    m.add_function(wrap_pyfunction!(lit, m)?)?;
    m.add_function(wrap_pyfunction!(kwd, m)?)?;
    m.add_function(wrap_pyfunction!(decode_text, m)?)?;
    m.add_function(wrap_pyfunction!(isnumber, m)?)?;

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
    Ok(())
}
