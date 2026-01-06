//! Layout analysis parameters and settings for Python.
//!
//! Provides PyLAParams for controlling text layout analysis and parsing functions
//! for table and text extraction settings.

use bolivar_core::table::{
    BBox, EdgeObj, ExplicitLine, Orientation, PageGeometry, TableSettings, TextDir, TextSettings,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySequence};

/// Layout analysis parameters.
///
/// Controls how characters are grouped into lines, words, and text boxes.
#[pyclass(name = "LAParams")]
#[derive(Debug, Clone)]
pub struct PyLAParams {
    #[pyo3(get, set)]
    pub line_overlap: f64,
    #[pyo3(get, set)]
    pub char_margin: f64,
    #[pyo3(get, set)]
    pub line_margin: f64,
    #[pyo3(get, set)]
    pub word_margin: f64,
    #[pyo3(get, set)]
    pub boxes_flow: Option<f64>,
    #[pyo3(get, set)]
    pub detect_vertical: bool,
    #[pyo3(get, set)]
    pub all_texts: bool,
}

#[pymethods]
impl PyLAParams {
    #[new]
    #[pyo3(signature = (
        line_overlap = 0.5,
        char_margin = 2.0,
        line_margin = 0.5,
        word_margin = 0.1,
        boxes_flow = Some(0.5),
        detect_vertical = false,
        all_texts = false
    ))]
    pub fn new(
        line_overlap: f64,
        char_margin: f64,
        line_margin: f64,
        word_margin: f64,
        boxes_flow: Option<f64>,
        detect_vertical: bool,
        all_texts: bool,
    ) -> Self {
        Self {
            line_overlap,
            char_margin,
            line_margin,
            word_margin,
            boxes_flow,
            detect_vertical,
            all_texts,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "LAParams(line_overlap={}, char_margin={}, line_margin={}, word_margin={}, boxes_flow={:?}, detect_vertical={}, all_texts={})",
            self.line_overlap,
            self.char_margin,
            self.line_margin,
            self.word_margin,
            self.boxes_flow,
            self.detect_vertical,
            self.all_texts
        )
    }
}

impl From<PyLAParams> for bolivar_core::layout::LAParams {
    fn from(py: PyLAParams) -> Self {
        bolivar_core::layout::LAParams::new(
            py.line_overlap,
            py.char_margin,
            py.line_margin,
            py.word_margin,
            py.boxes_flow,
            py.detect_vertical,
            py.all_texts,
        )
    }
}

impl From<bolivar_core::layout::LAParams> for PyLAParams {
    fn from(la: bolivar_core::layout::LAParams) -> Self {
        Self {
            line_overlap: la.line_overlap,
            char_margin: la.char_margin,
            line_margin: la.line_margin,
            word_margin: la.word_margin,
            boxes_flow: la.boxes_flow,
            detect_vertical: la.detect_vertical,
            all_texts: la.all_texts,
        }
    }
}

/// Parse a text direction string.
pub fn parse_text_dir(value: &str) -> Result<TextDir, PyErr> {
    match value {
        "ttb" => Ok(TextDir::Ttb),
        "btt" => Ok(TextDir::Btt),
        "ltr" => Ok(TextDir::Ltr),
        "rtl" => Ok(TextDir::Rtl),
        _ => Err(PyValueError::new_err(format!(
            "Invalid text direction: {}",
            value
        ))),
    }
}

/// Apply text settings from a Python dict.
pub fn apply_text_settings_from_dict(
    settings: &mut TextSettings,
    dict: &Bound<'_, PyDict>,
) -> PyResult<()> {
    let mut tolerance: Option<f64> = None;
    let mut x_set = false;
    let mut y_set = false;

    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        match key.as_str() {
            "x_tolerance" => {
                settings.x_tolerance = v.extract()?;
                x_set = true;
            }
            "y_tolerance" => {
                settings.y_tolerance = v.extract()?;
                y_set = true;
            }
            "tolerance" => {
                tolerance = Some(v.extract()?);
            }
            "x_tolerance_ratio" => settings.x_tolerance_ratio = Some(v.extract()?),
            "y_tolerance_ratio" => settings.y_tolerance_ratio = Some(v.extract()?),
            "keep_blank_chars" => settings.keep_blank_chars = v.extract()?,
            "use_text_flow" => settings.use_text_flow = v.extract()?,
            "vertical_ttb" => settings.vertical_ttb = v.extract()?,
            "horizontal_ltr" => settings.horizontal_ltr = v.extract()?,
            "line_dir" => settings.line_dir = parse_text_dir(&v.extract::<String>()?)?,
            "char_dir" => settings.char_dir = parse_text_dir(&v.extract::<String>()?)?,
            "line_dir_rotated" => {
                let val: String = v.extract()?;
                settings.line_dir_rotated = Some(parse_text_dir(&val)?);
            }
            "char_dir_rotated" => {
                let val: String = v.extract()?;
                settings.char_dir_rotated = Some(parse_text_dir(&val)?);
            }
            "split_at_punctuation" => settings.split_at_punctuation = v.extract()?,
            "expand_ligatures" => settings.expand_ligatures = v.extract()?,
            "layout" => settings.layout = v.extract()?,
            _ => {}
        }
    }

    if let Some(tol) = tolerance {
        if !x_set {
            settings.x_tolerance = tol;
        }
        if !y_set {
            settings.y_tolerance = tol;
        }
    }

    Ok(())
}

/// Parse text settings from a Python object.
pub fn parse_text_settings(
    py: Python<'_>,
    text_settings: Option<Py<PyAny>>,
) -> PyResult<TextSettings> {
    let mut settings = TextSettings::default();
    let Some(obj) = text_settings else {
        return Ok(settings);
    };
    let obj = obj.bind(py);
    if obj.is_none() {
        return Ok(settings);
    }
    let dict = obj
        .downcast::<PyDict>()
        .map_err(|_| PyValueError::new_err("text_settings must be a dict when provided"))?;
    apply_text_settings_from_dict(&mut settings, dict)?;
    Ok(settings)
}

/// Parse explicit lines from a Python object.
fn parse_explicit_lines(_py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Vec<ExplicitLine>> {
    if obj.is_none() {
        return Ok(Vec::new());
    }
    let seq = obj
        .downcast::<PySequence>()
        .map_err(|_| PyValueError::new_err("explicit lines must be a list/tuple"))?;
    let mut out = Vec::new();
    let len = seq.len().unwrap_or(0);
    for i in 0..len {
        let item = seq.get_item(i)?;
        if let Ok(val) = item.extract::<f64>() {
            out.push(ExplicitLine::Coord(val));
            continue;
        }
        if let Ok(dict) = item.downcast::<PyDict>() {
            if let Some(pts_obj) = dict.get_item("pts")? {
                let pts: Vec<(f64, f64)> = pts_obj.extract()?;
                out.push(ExplicitLine::Curve(pts));
                continue;
            }
            let obj_type: Option<String> =
                dict.get_item("object_type")?.and_then(|v| v.extract().ok());
            let x0: Option<f64> = dict.get_item("x0")?.and_then(|v| v.extract().ok());
            let x1: Option<f64> = dict.get_item("x1")?.and_then(|v| v.extract().ok());
            let top: Option<f64> = dict.get_item("top")?.and_then(|v| v.extract().ok());
            let bottom: Option<f64> = dict.get_item("bottom")?.and_then(|v| v.extract().ok());
            if let (Some(x0), Some(x1), Some(top), Some(bottom)) = (x0, x1, top, bottom) {
                if obj_type.as_deref() == Some("rect") {
                    out.push(ExplicitLine::Rect(BBox {
                        x0,
                        x1,
                        top,
                        bottom,
                    }));
                    continue;
                }
                let width = dict
                    .get_item("width")?
                    .and_then(|v| v.extract().ok())
                    .unwrap_or(x1 - x0);
                let height = dict
                    .get_item("height")?
                    .and_then(|v| v.extract().ok())
                    .unwrap_or(bottom - top);
                let orientation = dict
                    .get_item("orientation")?
                    .and_then(|v| v.extract::<String>().ok())
                    .and_then(|o| match o.as_str() {
                        "v" => Some(Orientation::Vertical),
                        "h" => Some(Orientation::Horizontal),
                        _ => None,
                    })
                    .or_else(|| {
                        if (x0 - x1).abs() < 1e-9 {
                            Some(Orientation::Vertical)
                        } else if (top - bottom).abs() < 1e-9 {
                            Some(Orientation::Horizontal)
                        } else {
                            None
                        }
                    });
                out.push(ExplicitLine::Edge(EdgeObj {
                    x0,
                    x1,
                    top,
                    bottom,
                    width,
                    height,
                    orientation,
                    object_type: "explicit_edge",
                }));
                continue;
            }
        }
    }
    Ok(out)
}

/// Parse table settings from a Python object.
pub fn parse_table_settings(
    py: Python<'_>,
    table_settings: Option<Py<PyAny>>,
) -> PyResult<TableSettings> {
    let mut settings = TableSettings::default();
    let Some(obj) = table_settings else {
        return Ok(settings);
    };
    let obj = obj.bind(py);
    if obj.is_none() {
        return Ok(settings);
    }
    let dict = obj
        .downcast::<PyDict>()
        .map_err(|_| PyValueError::new_err("table_settings must be a dict when provided"))?;

    let mut text_settings = settings.text_settings.clone();

    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        if let Some(stripped) = key.strip_prefix("text_") {
            let tmp = PyDict::new(py);
            tmp.set_item(stripped, v)?;
            apply_text_settings_from_dict(&mut text_settings, &tmp)?;
            continue;
        }

        match key.as_str() {
            "vertical_strategy" => settings.vertical_strategy = v.extract()?,
            "horizontal_strategy" => settings.horizontal_strategy = v.extract()?,
            "snap_tolerance" => {
                let val: f64 = v.extract()?;
                settings.snap_x_tolerance = val;
                settings.snap_y_tolerance = val;
            }
            "snap_x_tolerance" => settings.snap_x_tolerance = v.extract()?,
            "snap_y_tolerance" => settings.snap_y_tolerance = v.extract()?,
            "join_tolerance" => {
                let val: f64 = v.extract()?;
                settings.join_x_tolerance = val;
                settings.join_y_tolerance = val;
            }
            "join_x_tolerance" => settings.join_x_tolerance = v.extract()?,
            "join_y_tolerance" => settings.join_y_tolerance = v.extract()?,
            "edge_min_length" => settings.edge_min_length = v.extract()?,
            "edge_min_length_prefilter" => settings.edge_min_length_prefilter = v.extract()?,
            "min_words_vertical" => settings.min_words_vertical = v.extract()?,
            "min_words_horizontal" => settings.min_words_horizontal = v.extract()?,
            "intersection_tolerance" => {
                let val: f64 = v.extract()?;
                settings.intersection_x_tolerance = val;
                settings.intersection_y_tolerance = val;
            }
            "intersection_x_tolerance" => settings.intersection_x_tolerance = v.extract()?,
            "intersection_y_tolerance" => settings.intersection_y_tolerance = v.extract()?,
            "text_settings" => {
                if !v.is_none() {
                    let ts_dict = v.downcast::<PyDict>().map_err(|_| {
                        PyValueError::new_err("text_settings must be a dict when provided")
                    })?;
                    apply_text_settings_from_dict(&mut text_settings, ts_dict)?;
                }
            }
            "text_layout" => {
                if !v.is_none() {
                    text_settings.layout = v.extract()?;
                }
            }
            "explicit_vertical_lines" => {
                settings.explicit_vertical_lines = parse_explicit_lines(py, &v)?;
            }
            "explicit_horizontal_lines" => {
                settings.explicit_horizontal_lines = parse_explicit_lines(py, &v)?;
            }
            _ => {}
        }
    }

    settings.text_settings = text_settings;
    Ok(settings)
}

/// Parse a bbox from a Python object.
pub fn parse_bbox(obj: &Bound<'_, PyAny>, label: &str) -> PyResult<(f64, f64, f64, f64)> {
    if let Ok(val) = obj.extract::<(f64, f64, f64, f64)>() {
        return Ok(val);
    }
    if let Ok(vals) = obj.extract::<Vec<f64>>() {
        if vals.len() == 4 {
            return Ok((vals[0], vals[1], vals[2], vals[3]));
        }
    }
    Err(PyValueError::new_err(format!(
        "{label} must be a 4-tuple/list of floats"
    )))
}

/// Parse a page geometry from a Python object.
pub fn parse_page_geometry(obj: &Bound<'_, PyAny>) -> PyResult<PageGeometry> {
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let page_bbox_obj = dict
            .get_item("page_bbox")?
            .ok_or_else(|| PyValueError::new_err("geometry missing page_bbox"))?;
        let mediabox_obj = dict
            .get_item("mediabox")?
            .ok_or_else(|| PyValueError::new_err("geometry missing mediabox"))?;
        let initial_doctop = match dict.get_item("initial_doctop")? {
            Some(val) => val.extract::<f64>()?,
            None => 0.0,
        };
        let force_crop = match dict.get_item("force_crop")? {
            Some(val) => val.extract::<bool>()?,
            None => false,
        };

        return Ok(PageGeometry {
            page_bbox: parse_bbox(&page_bbox_obj, "page_bbox")?,
            mediabox: parse_bbox(&mediabox_obj, "mediabox")?,
            initial_doctop,
            force_crop,
        });
    }

    let seq = obj
        .downcast::<PySequence>()
        .map_err(|_| PyValueError::new_err("geometry must be a dict or 4-item sequence"))?;
    let len = seq.len().unwrap_or(0);
    if len != 4 {
        return Err(PyValueError::new_err("geometry must be a 4-item sequence"));
    }
    let page_bbox_obj = seq.get_item(0)?;
    let mediabox_obj = seq.get_item(1)?;
    let initial_doctop = seq.get_item(2)?.extract::<f64>()?;
    let force_crop = seq.get_item(3)?.extract::<bool>()?;

    Ok(PageGeometry {
        page_bbox: parse_bbox(&page_bbox_obj, "page_bbox")?,
        mediabox: parse_bbox(&mediabox_obj, "mediabox")?,
        initial_doctop,
        force_crop,
    })
}

/// Parse page geometries from a Python sequence.
pub fn parse_page_geometries(geometries: &Bound<'_, PyAny>) -> PyResult<Vec<PageGeometry>> {
    let seq = geometries
        .downcast::<PySequence>()
        .map_err(|_| PyValueError::new_err("geometries must be a list/tuple"))?;
    let len = seq.len().unwrap_or(0);
    let mut out = Vec::with_capacity(len as usize);
    for idx in 0..len {
        let item = seq.get_item(idx)?;
        out.push(parse_page_geometry(&item)?);
    }
    Ok(out)
}

/// Register the params module classes with the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyLAParams>()?;
    Ok(())
}
