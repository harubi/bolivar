//! Image export bindings for Python.

use std::sync::Mutex;

use bolivar_core::image::ImageWriter;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::document::PyPDFStream;

#[pyclass(name = "ImageWriter")]
pub struct PyImageWriter {
    inner: Mutex<ImageWriter>,
}

#[pymethods]
impl PyImageWriter {
    #[new]
    pub fn new(output_dir: &str) -> PyResult<Self> {
        let writer = ImageWriter::new(output_dir)
            .map_err(|e| PyValueError::new_err(format!("Failed to create ImageWriter: {e}")))?;
        Ok(Self {
            inner: Mutex::new(writer),
        })
    }

    #[pyo3(signature = (name, stream, srcsize=None, bits=8, colorspace=None))]
    pub fn export_image(
        &self,
        name: &str,
        stream: &PyPDFStream,
        srcsize: Option<(i32, i32)>,
        bits: i32,
        colorspace: Option<Vec<String>>,
    ) -> PyResult<String> {
        let srcsize = srcsize
            .map(|(w, h)| (Some(w), Some(h)))
            .unwrap_or((None, None));
        let colorspace = colorspace.unwrap_or_default();
        self.inner
            .lock()
            .map_err(|_| PyRuntimeError::new_err("ImageWriter mutex poisoned"))?
            .export_image(name, &stream.stream, srcsize, bits, &colorspace)
            .map_err(|e| PyValueError::new_err(format!("Failed to export image: {e}")))
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyImageWriter>()?;
    Ok(())
}
