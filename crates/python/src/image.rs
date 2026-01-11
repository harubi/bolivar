//! Image export bindings for Python.

use std::cell::RefCell;

use bolivar_core::image::ImageWriter;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(name = "ImageWriter", unsendable)]
pub struct PyImageWriter {
    inner: RefCell<ImageWriter>,
}

#[pymethods]
impl PyImageWriter {
    #[new]
    pub fn new(output_dir: &str) -> PyResult<Self> {
        let writer = ImageWriter::new(output_dir)
            .map_err(|e| PyValueError::new_err(format!("Failed to create ImageWriter: {e}")))?;
        Ok(Self {
            inner: RefCell::new(writer),
        })
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyImageWriter>()?;
    Ok(())
}
