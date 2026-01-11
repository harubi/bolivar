//! Casting helpers for pdfminer.casting compatibility.

use bolivar_core::casting as core_casting;
use bolivar_core::pdftypes::PDFObject;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PySequence, PySequenceMethods};

fn obj_to_pdf(py: Python<'_>, obj: &Bound<'_, PyAny>) -> Option<PDFObject> {
    if obj.is_none() {
        return Some(PDFObject::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Some(PDFObject::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Some(PDFObject::Int(i));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Some(PDFObject::Real(f));
    }
    if let Ok(bytes) = obj.downcast::<PyBytes>() {
        return Some(PDFObject::String(bytes.as_bytes().to_vec()));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Some(PDFObject::String(s.into_bytes()));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = std::collections::HashMap::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract().ok()?;
            let value = obj_to_pdf(py, &v)?;
            map.insert(key, value);
        }
        return Some(PDFObject::Dict(map));
    }
    if let Ok(seq) = obj.downcast::<PySequence>() {
        if obj.downcast::<PyBytes>().is_ok() {
            return None;
        }
        let len = seq.len().ok()? as usize;
        let mut items = Vec::with_capacity(len);
        for idx in 0..len {
            let item = seq.get_item(idx).ok()?;
            let value = obj_to_pdf(py, &item)?;
            items.push(value);
        }
        return Some(PDFObject::Array(items));
    }
    None
}

#[pyfunction]
fn safe_int(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Option<i64>> {
    let obj = match obj_to_pdf(py, obj) {
        Some(obj) => obj,
        None => return Ok(None),
    };
    Ok(core_casting::safe_int(&obj))
}

#[pyfunction]
fn safe_float(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
    let obj = match obj_to_pdf(py, obj) {
        Some(obj) => obj,
        None => return Ok(None),
    };
    Ok(core_casting::safe_float(&obj))
}

#[pyfunction]
fn safe_matrix(
    py: Python<'_>,
    a: &Bound<'_, PyAny>,
    b: &Bound<'_, PyAny>,
    c: &Bound<'_, PyAny>,
    d: &Bound<'_, PyAny>,
    e: &Bound<'_, PyAny>,
    f: &Bound<'_, PyAny>,
) -> PyResult<Option<(f64, f64, f64, f64, f64, f64)>> {
    let a = match obj_to_pdf(py, a) {
        Some(v) => v,
        None => return Ok(None),
    };
    let b = match obj_to_pdf(py, b) {
        Some(v) => v,
        None => return Ok(None),
    };
    let c = match obj_to_pdf(py, c) {
        Some(v) => v,
        None => return Ok(None),
    };
    let d = match obj_to_pdf(py, d) {
        Some(v) => v,
        None => return Ok(None),
    };
    let e = match obj_to_pdf(py, e) {
        Some(v) => v,
        None => return Ok(None),
    };
    let f = match obj_to_pdf(py, f) {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(core_casting::safe_matrix(&a, &b, &c, &d, &e, &f))
}

#[pyfunction]
fn safe_rgb(
    py: Python<'_>,
    r: &Bound<'_, PyAny>,
    g: &Bound<'_, PyAny>,
    b: &Bound<'_, PyAny>,
) -> PyResult<Option<(f64, f64, f64)>> {
    let r = match obj_to_pdf(py, r) {
        Some(v) => v,
        None => return Ok(None),
    };
    let g = match obj_to_pdf(py, g) {
        Some(v) => v,
        None => return Ok(None),
    };
    let b = match obj_to_pdf(py, b) {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(core_casting::safe_rgb(&r, &g, &b))
}

#[pyfunction]
fn safe_cmyk(
    py: Python<'_>,
    c: &Bound<'_, PyAny>,
    m: &Bound<'_, PyAny>,
    y: &Bound<'_, PyAny>,
    k: &Bound<'_, PyAny>,
) -> PyResult<Option<(f64, f64, f64, f64)>> {
    let c = match obj_to_pdf(py, c) {
        Some(v) => v,
        None => return Ok(None),
    };
    let m = match obj_to_pdf(py, m) {
        Some(v) => v,
        None => return Ok(None),
    };
    let y = match obj_to_pdf(py, y) {
        Some(v) => v,
        None => return Ok(None),
    };
    let k = match obj_to_pdf(py, k) {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(core_casting::safe_cmyk(&c, &m, &y, &k))
}

#[pyfunction]
fn safe_rect(
    py: Python<'_>,
    a: &Bound<'_, PyAny>,
    b: &Bound<'_, PyAny>,
    c: &Bound<'_, PyAny>,
    d: &Bound<'_, PyAny>,
) -> PyResult<Option<(f64, f64, f64, f64)>> {
    let a = match obj_to_pdf(py, a) {
        Some(v) => v,
        None => return Ok(None),
    };
    let b = match obj_to_pdf(py, b) {
        Some(v) => v,
        None => return Ok(None),
    };
    let c = match obj_to_pdf(py, c) {
        Some(v) => v,
        None => return Ok(None),
    };
    let d = match obj_to_pdf(py, d) {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(core_casting::safe_rect(&a, &b, &c, &d))
}

#[pyfunction]
fn safe_rect_list(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
) -> PyResult<Option<(f64, f64, f64, f64)>> {
    let obj = match obj_to_pdf(py, obj) {
        Some(obj) => obj,
        None => return Ok(None),
    };
    Ok(core_casting::safe_rect_list(&obj))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(safe_int, m)?)?;
    m.add_function(wrap_pyfunction!(safe_float, m)?)?;
    m.add_function(wrap_pyfunction!(safe_matrix, m)?)?;
    m.add_function(wrap_pyfunction!(safe_rgb, m)?)?;
    m.add_function(wrap_pyfunction!(safe_cmyk, m)?)?;
    m.add_function(wrap_pyfunction!(safe_rect, m)?)?;
    m.add_function(wrap_pyfunction!(safe_rect_list, m)?)?;
    Ok(())
}
