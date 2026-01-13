//! Utility bindings for pdfminer-compatible helpers.
//!
//! Exposes matrix math, Plane, formatting helpers, and unpad_aes.

use bolivar_core::codec::unpad_aes as core_unpad_aes;
use bolivar_core::utils::{
    HasBBox, Matrix, Plane as CorePlane, Point, Rect, apply_matrix_pt as core_apply_matrix_pt,
    apply_matrix_rect as core_apply_matrix_rect, format_int_alpha as core_format_int_alpha,
    format_int_roman as core_format_int_roman, mult_matrix as core_mult_matrix,
    shorten_str as core_shorten_str, translate_matrix as core_translate_matrix,
};
use pyo3::buffer::PyBuffer;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PySequence, PySequenceMethods};

#[pyfunction]
fn mult_matrix(m1: Matrix, m0: Matrix) -> Matrix {
    core_mult_matrix(m1, m0)
}

#[pyfunction]
fn translate_matrix(m: Matrix, v: Point) -> Matrix {
    core_translate_matrix(m, v)
}

#[pyfunction]
fn apply_matrix_pt(m: Matrix, v: Point) -> Point {
    core_apply_matrix_pt(m, v)
}

#[pyfunction]
fn apply_matrix_rect(m: Matrix, rect: Rect) -> Rect {
    core_apply_matrix_rect(m, rect)
}

#[pyfunction]
fn format_int_alpha(value: u32) -> String {
    core_format_int_alpha(value)
}

#[pyfunction]
fn format_int_roman(value: u32) -> String {
    core_format_int_roman(value)
}

#[pyfunction]
fn shorten_str(s: &str, size: usize) -> String {
    core_shorten_str(s, size)
}

#[pyfunction]
fn unpad_aes(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let buf = PyBuffer::<u8>::get(data)
        .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected bytes-like object"))?;
    let bytes = buf.to_vec(py)?;
    let unpadded = core_unpad_aes(&bytes);
    Ok(PyBytes::new(py, unpadded).unbind())
}

struct PyPlaneItem {
    bbox: Rect,
    obj: Py<PyAny>,
    ptr: usize,
}

impl HasBBox for PyPlaneItem {
    fn x0(&self) -> f64 {
        self.bbox.0
    }

    fn y0(&self) -> f64 {
        self.bbox.1
    }

    fn x1(&self) -> f64 {
        self.bbox.2
    }

    fn y1(&self) -> f64 {
        self.bbox.3
    }
}

impl PartialEq for PyPlaneItem {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

#[pyclass(name = "Plane")]
pub struct PyPlane {
    inner: CorePlane<PyPlaneItem>,
    bbox: Rect,
}

#[pymethods]
impl PyPlane {
    #[new]
    #[pyo3(signature = (bbox, gridsize = 50))]
    fn new(bbox: Rect, gridsize: i32) -> Self {
        Self {
            inner: CorePlane::new(bbox, gridsize),
            bbox,
        }
    }

    fn add(&mut self, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        let item = plane_item_from_py(obj)?;
        self.inner.add(item);
        Ok(())
    }

    fn extend(&mut self, objs: &Bound<'_, PyAny>) -> PyResult<()> {
        let seq = objs.cast::<PySequence>()?;
        let len = seq.len()? as usize;
        let mut items = Vec::with_capacity(len);
        for idx in 0..len {
            let obj = seq.get_item(idx)?;
            items.push(plane_item_from_py(&obj)?);
        }
        self.inner.extend(items);
        Ok(())
    }

    fn remove(&mut self, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        let ptr = obj.as_ptr() as usize;
        let temp = PyPlaneItem {
            bbox: (0.0, 0.0, 0.0, 0.0),
            obj: obj.clone().unbind(),
            ptr,
        };
        Ok(self.inner.remove(&temp))
    }

    fn find(&self, py: Python<'_>, bbox: Rect) -> PyResult<Vec<Py<PyAny>>> {
        Ok(self
            .inner
            .find(bbox)
            .into_iter()
            .map(|item| item.obj.clone_ref(py))
            .collect())
    }

    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyPlaneIter {
        let items = slf
            .inner
            .find(slf.bbox)
            .into_iter()
            .map(|item| item.obj.clone_ref(py))
            .collect();
        PyPlaneIter { items, index: 0 }
    }

    fn __len__(&self) -> usize {
        self.inner.find(self.bbox).len()
    }
}

#[pyclass]
struct PyPlaneIter {
    items: Vec<Py<PyAny>>,
    index: usize,
}

#[pymethods]
impl PyPlaneIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> Option<Py<PyAny>> {
        if slf.index < slf.items.len() {
            let item = slf.items[slf.index].clone_ref(py);
            slf.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

fn plane_item_from_py(obj: &Bound<'_, PyAny>) -> PyResult<PyPlaneItem> {
    let bbox = extract_bbox(obj)?;
    let ptr = obj.as_ptr() as usize;
    Ok(PyPlaneItem {
        bbox,
        obj: obj.clone().unbind(),
        ptr,
    })
}

fn extract_bbox(obj: &Bound<'_, PyAny>) -> PyResult<Rect> {
    if let Ok(bbox_obj) = obj.getattr("bbox") {
        if let Ok(bbox) = bbox_obj.extract::<Rect>() {
            return Ok(bbox);
        }
    }
    let x0: f64 = obj.getattr("x0")?.extract()?;
    let y0: f64 = obj.getattr("y0")?.extract()?;
    let x1: f64 = obj.getattr("x1")?.extract()?;
    let y1: f64 = obj.getattr("y1")?.extract()?;
    Ok((x0, y0, x1, y1))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(mult_matrix, m)?)?;
    m.add_function(wrap_pyfunction!(translate_matrix, m)?)?;
    m.add_function(wrap_pyfunction!(apply_matrix_pt, m)?)?;
    m.add_function(wrap_pyfunction!(apply_matrix_rect, m)?)?;
    m.add_function(wrap_pyfunction!(format_int_alpha, m)?)?;
    m.add_function(wrap_pyfunction!(format_int_roman, m)?)?;
    m.add_function(wrap_pyfunction!(shorten_str, m)?)?;
    m.add_function(wrap_pyfunction!(unpad_aes, m)?)?;
    m.add_class::<PyPlane>()?;
    Ok(())
}
