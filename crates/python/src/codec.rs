//! Codec bindings for Python.
//!
//! Exposes stream decode utilities (ascii85, lzw, runlength), RC4 (Arcfour),
//! and CCITT fax helpers backed by bolivar-core.

use bolivar_core::arcfour::Arcfour;
use bolivar_core::ascii85::{
    ascii85decode as core_ascii85decode, asciihexdecode as core_asciihexdecode,
};
use bolivar_core::ccitt::{
    CCITTFaxDecoder as CoreCCITTFaxDecoder, CCITTG4Parser as CoreCCITTG4Parser,
};
use bolivar_core::lzw::{
    lzwdecode as core_lzwdecode, lzwdecode_with_earlychange as core_lzwdecode_with_earlychange,
};
use bolivar_core::runlength::rldecode as core_rldecode;
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

fn bytes_from_py(py: Python<'_>, data: &Bound<'_, PyAny>, label: &str) -> PyResult<Vec<u8>> {
    let buf = PyBuffer::<u8>::get(data)
        .map_err(|_| PyTypeError::new_err(format!("{label} expects a bytes-like object")))?;
    buf.to_vec(py)
}

fn bits_from_py(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Vec<i8>> {
    if let Ok(buf) = PyBuffer::<u8>::get(data) {
        let bytes = buf.to_vec(py)?;
        return Ok(bytes
            .into_iter()
            .map(|b| if b == 0 { 0 } else { 1 })
            .collect());
    }

    let items: Vec<i64> = data
        .extract()
        .map_err(|_| PyTypeError::new_err("bits must be bytes-like or a sequence of ints"))?;
    Ok(items
        .into_iter()
        .map(|b| if b == 0 { 0 } else { 1 })
        .collect())
}

#[pyfunction]
pub fn ascii85decode(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let bytes = bytes_from_py(py, data, "ascii85decode")?;
    let decoded = py
        .detach(|| core_ascii85decode(&bytes))
        .map_err(|e| PyValueError::new_err(format!("ascii85decode failed: {e}")))?;
    Ok(PyBytes::new(py, &decoded).into_any().unbind())
}

#[pyfunction]
pub fn asciihexdecode(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let bytes = bytes_from_py(py, data, "asciihexdecode")?;
    let decoded = py
        .detach(|| core_asciihexdecode(&bytes))
        .map_err(|e| PyValueError::new_err(format!("asciihexdecode failed: {e}")))?;
    Ok(PyBytes::new(py, &decoded).into_any().unbind())
}

#[pyfunction]
pub fn lzwdecode(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let bytes = bytes_from_py(py, data, "lzwdecode")?;
    let decoded = py
        .detach(|| core_lzwdecode(&bytes))
        .map_err(|e| PyValueError::new_err(format!("lzwdecode failed: {e}")))?;
    Ok(PyBytes::new(py, &decoded).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (data, early_change = 1))]
pub fn lzwdecode_with_earlychange(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    early_change: i32,
) -> PyResult<Py<PyAny>> {
    let bytes = bytes_from_py(py, data, "lzwdecode_with_earlychange")?;
    let decoded = py
        .detach(|| core_lzwdecode_with_earlychange(&bytes, early_change))
        .map_err(|e| PyValueError::new_err(format!("lzwdecode failed: {e}")))?;
    Ok(PyBytes::new(py, &decoded).into_any().unbind())
}

#[pyfunction]
pub fn rldecode(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let bytes = bytes_from_py(py, data, "rldecode")?;
    let decoded = py
        .detach(|| core_rldecode(&bytes))
        .map_err(|e| PyValueError::new_err(format!("rldecode failed: {e}")))?;
    Ok(PyBytes::new(py, &decoded).into_any().unbind())
}

#[pyclass(name = "Arcfour")]
pub struct PyArcfour {
    inner: Arcfour,
}

#[pymethods]
impl PyArcfour {
    #[new]
    pub fn new(py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Self> {
        let key = bytes_from_py(py, key, "Arcfour")?;
        if key.is_empty() || key.len() > 256 {
            return Err(PyValueError::new_err("RC4 key must be 1-256 bytes"));
        }
        Ok(Self {
            inner: Arcfour::new(&key),
        })
    }

    pub fn process(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let bytes = bytes_from_py(py, data, "Arcfour.process")?;
        let out = py.detach(|| self.inner.process(&bytes));
        Ok(PyBytes::new(py, &out).into_any().unbind())
    }
}

#[pyclass(name = "CCITTG4Parser")]
pub struct PyCCITTG4Parser {
    parser: CoreCCITTG4Parser,
}

#[pymethods]
impl PyCCITTG4Parser {
    #[new]
    #[pyo3(signature = (width, bytealign = false))]
    pub fn new(width: usize, bytealign: bool) -> Self {
        Self {
            parser: CoreCCITTG4Parser::new(width, bytealign),
        }
    }

    #[getter]
    fn _curline(&self) -> Vec<i8> {
        self.parser.curline().to_vec()
    }

    #[setter]
    #[allow(non_snake_case)]
    fn set__curline(&mut self, value: Vec<i8>) {
        self.parser.set_curline(value);
    }

    #[getter]
    fn _curpos(&self) -> isize {
        self.parser.curpos()
    }

    #[setter]
    #[allow(non_snake_case)]
    fn set__curpos(&mut self, value: isize) {
        self.parser.set_curpos(value);
    }

    #[getter]
    fn _color(&self) -> i8 {
        self.parser.color()
    }

    #[setter]
    #[allow(non_snake_case)]
    fn set__color(&mut self, value: i8) {
        self.parser.set_color(value);
    }

    fn _reset_line(&mut self) {
        self.parser.reset_line();
    }

    fn _do_vertical(&mut self, dx: i32) {
        self.parser.do_vertical(dx);
    }

    fn _do_pass(&mut self) {
        self.parser.do_pass();
    }

    fn _do_horizontal(&mut self, n1: usize, n2: usize) {
        self.parser.do_horizontal(n1, n2);
    }

    fn _get_bits(&self) -> String {
        self.parser.get_bits()
    }

    fn feedbytes(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let bytes = bytes_from_py(py, data, "CCITTG4Parser.feedbytes")?;
        self.parser.feedbytes(&bytes);
        Ok(())
    }
}

#[pyclass(name = "CCITTFaxDecoder")]
pub struct PyCCITTFaxDecoder {
    decoder: CoreCCITTFaxDecoder,
}

#[pymethods]
impl PyCCITTFaxDecoder {
    #[new]
    #[pyo3(signature = (width, bytealign = false, reversed = false))]
    pub fn new(width: usize, bytealign: bool, reversed: bool) -> Self {
        Self {
            decoder: CoreCCITTFaxDecoder::new(width, bytealign, reversed),
        }
    }

    pub fn feedbytes(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let bytes = bytes_from_py(py, data, "CCITTFaxDecoder.feedbytes")?;
        self.decoder.feedbytes(&bytes);
        Ok(())
    }

    pub fn output_line(
        &mut self,
        py: Python<'_>,
        y: usize,
        bits: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let bits = bits_from_py(py, bits)?;
        self.decoder.output_line(y, &bits);
        Ok(())
    }

    pub fn close(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(PyBytes::new(py, &self.decoder.close()).into_any().unbind())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(ascii85decode, m)?)?;
    m.add_function(wrap_pyfunction!(asciihexdecode, m)?)?;
    m.add_function(wrap_pyfunction!(lzwdecode, m)?)?;
    m.add_function(wrap_pyfunction!(lzwdecode_with_earlychange, m)?)?;
    m.add_function(wrap_pyfunction!(rldecode, m)?)?;
    m.add_class::<PyArcfour>()?;
    m.add_class::<PyCCITTG4Parser>()?;
    m.add_class::<PyCCITTFaxDecoder>()?;
    Ok(())
}
