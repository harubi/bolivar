//! Compatibility-only helpers for cropped/filtered pdfplumber table extraction.

use bolivar_core::arena::PageArena;
use bolivar_core::table::{CharObj, EdgeObj, Orientation, PageGeometry};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PySequence};

fn dict_f64(dict: &Bound<'_, PyDict>, key: &str) -> Option<f64> {
    dict.get_item(key)
        .ok()
        .flatten()
        .and_then(|v| v.extract::<f64>().ok())
}

fn dict_bool(dict: &Bound<'_, PyDict>, key: &str) -> Option<bool> {
    dict.get_item(key)
        .ok()
        .flatten()
        .and_then(|v| v.extract::<bool>().ok())
}

fn dict_text(dict: &Bound<'_, PyDict>, key: &str) -> Option<String> {
    let value = dict.get_item(key).ok().flatten()?;
    if let Ok(s) = value.extract::<String>() {
        return Some(s);
    }
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Some(String::from_utf8_lossy(bytes.as_bytes()).to_string());
    }
    None
}

fn char_from_dict(
    arena: &mut PageArena,
    dict: &Bound<'_, PyDict>,
    default_doctop: f64,
) -> Option<CharObj> {
    let text = dict_text(dict, "text")?;
    let x0 = dict_f64(dict, "x0")?;
    let x1 = dict_f64(dict, "x1")?;
    let top = dict_f64(dict, "top")?;
    let bottom = dict_f64(dict, "bottom")?;
    let doctop = dict_f64(dict, "doctop").unwrap_or(default_doctop + top);
    let width = dict_f64(dict, "width").unwrap_or((x1 - x0).abs());
    let height = dict_f64(dict, "height").unwrap_or((bottom - top).abs());
    let size = dict_f64(dict, "size").unwrap_or(0.0);
    let upright = dict_bool(dict, "upright").unwrap_or(true);
    Some(CharObj {
        text: arena.intern(&text),
        x0,
        x1,
        top,
        bottom,
        doctop,
        width,
        height,
        size,
        upright,
    })
}

fn line_edge_from_dict(dict: &Bound<'_, PyDict>) -> Option<EdgeObj> {
    let x0 = dict_f64(dict, "x0")?;
    let x1 = dict_f64(dict, "x1")?;
    let top = dict_f64(dict, "top")?;
    let bottom = dict_f64(dict, "bottom")?;
    let width = dict_f64(dict, "width").unwrap_or((x1 - x0).abs());
    let height = dict_f64(dict, "height").unwrap_or((bottom - top).abs());
    let orientation = if (top - bottom).abs() < f64::EPSILON {
        Some(Orientation::Horizontal)
    } else {
        Some(Orientation::Vertical)
    };
    Some(EdgeObj {
        x0,
        x1,
        top,
        bottom,
        width,
        height,
        orientation,
        object_type: "line",
    })
}

fn rect_edges_from_dict(dict: &Bound<'_, PyDict>) -> Option<Vec<EdgeObj>> {
    let x0 = dict_f64(dict, "x0")?;
    let x1 = dict_f64(dict, "x1")?;
    let top = dict_f64(dict, "top")?;
    let bottom = dict_f64(dict, "bottom")?;
    let width = (x1 - x0).abs();
    let height = (bottom - top).abs();
    Some(vec![
        EdgeObj {
            x0,
            x1,
            top,
            bottom: top,
            width,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "rect_edge",
        },
        EdgeObj {
            x0,
            x1,
            top: bottom,
            bottom,
            width,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "rect_edge",
        },
        EdgeObj {
            x0,
            x1: x0,
            top,
            bottom,
            width: 0.0,
            height,
            orientation: Some(Orientation::Vertical),
            object_type: "rect_edge",
        },
        EdgeObj {
            x0: x1,
            x1,
            top,
            bottom,
            width: 0.0,
            height,
            orientation: Some(Orientation::Vertical),
            object_type: "rect_edge",
        },
    ])
}

fn curve_points_to_edges(points: &[(f64, f64)], object_type: &'static str) -> Vec<EdgeObj> {
    let mut edges = Vec::new();
    for pair in points.windows(2) {
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
            object_type,
        });
    }
    edges
}

fn points_from_obj(obj: &Bound<'_, PyAny>) -> Vec<(f64, f64)> {
    if let Ok(points) = obj.extract::<Vec<(f64, f64)>>() {
        return points;
    }
    let Ok(seq) = obj.cast::<PySequence>() else {
        return Vec::new();
    };
    let len = seq.len().unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..len {
        let item = match seq.get_item(i) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if let Ok(pt) = item.extract::<(f64, f64)>() {
            out.push(pt);
            continue;
        }
        let Ok(seg) = item.cast::<PySequence>() else {
            continue;
        };
        let seg_len = seg.len().unwrap_or(0);
        for j in 0..seg_len {
            let seg_item = match seg.get_item(j) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if j == 0 {
                if seg_item.extract::<String>().is_ok() || seg_item.cast::<PyBytes>().is_ok() {
                    continue;
                }
            }
            if let Ok(pt) = seg_item.extract::<(f64, f64)>() {
                out.push(pt);
            }
        }
    }
    out
}

fn curve_edges_from_dict(dict: &Bound<'_, PyDict>) -> Option<Vec<EdgeObj>> {
    if let Some(pts_obj) = dict.get_item("pts").ok().flatten() {
        let points = points_from_obj(&pts_obj);
        if points.len() >= 2 {
            return Some(curve_points_to_edges(&points, "curve_edge"));
        }
    }
    if let Some(path_obj) = dict.get_item("path").ok().flatten() {
        let points = points_from_obj(&path_obj);
        if points.len() >= 2 {
            return Some(curve_points_to_edges(&points, "curve_edge"));
        }
    }
    None
}

fn append_chars_from_list(
    list: &Bound<'_, PyAny>,
    initial_doctop: f64,
    arena: &mut PageArena,
    out: &mut Vec<CharObj>,
) -> PyResult<()> {
    let seq = list
        .cast::<PySequence>()
        .map_err(|_| PyValueError::new_err("char objects must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(dict) = item.cast::<PyDict>() {
            if let Some(obj) = char_from_dict(arena, &dict, initial_doctop) {
                out.push(obj);
            }
        }
    }
    Ok(())
}

fn append_line_edges(list: &Bound<'_, PyAny>, out: &mut Vec<EdgeObj>) -> PyResult<()> {
    let seq = list
        .cast::<PySequence>()
        .map_err(|_| PyValueError::new_err("line objects must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(dict) = item.cast::<PyDict>() {
            if let Some(edge) = line_edge_from_dict(&dict) {
                out.push(edge);
            }
        }
    }
    Ok(())
}

fn append_rect_edges(list: &Bound<'_, PyAny>, out: &mut Vec<EdgeObj>) -> PyResult<()> {
    let seq = list
        .cast::<PySequence>()
        .map_err(|_| PyValueError::new_err("rect objects must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(dict) = item.cast::<PyDict>() {
            if let Some(edges) = rect_edges_from_dict(&dict) {
                out.extend(edges);
            }
        }
    }
    Ok(())
}

fn append_curve_edges(list: &Bound<'_, PyAny>, out: &mut Vec<EdgeObj>) -> PyResult<()> {
    let seq = list
        .cast::<PySequence>()
        .map_err(|_| PyValueError::new_err("curve objects must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(dict) = item.cast::<PyDict>() {
            if let Some(edges) = curve_edges_from_dict(&dict) {
                out.extend(edges);
            }
        }
    }
    Ok(())
}

pub(crate) fn compat_lists_to_chars_edges(
    chars: &Bound<'_, PyAny>,
    lines: &Bound<'_, PyAny>,
    rects: &Bound<'_, PyAny>,
    curves: &Bound<'_, PyAny>,
    geom: &PageGeometry,
    arena: &mut PageArena,
) -> PyResult<(Vec<CharObj>, Vec<EdgeObj>)> {
    let mut char_objs = Vec::new();
    let mut edges = Vec::new();
    append_chars_from_list(chars, geom.initial_doctop, arena, &mut char_objs)?;
    append_line_edges(lines, &mut edges)?;
    append_rect_edges(rects, &mut edges)?;
    append_curve_edges(curves, &mut edges)?;
    Ok((char_objs, edges))
}
