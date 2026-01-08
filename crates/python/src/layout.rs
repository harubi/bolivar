//! Layout analysis types for Python.
//!
//! Provides PyLTPage, PyLTChar, PyLTTextLine*, PyLTTextBox*, PyLTItem and other
//! layout element types for exposing PDF layout analysis results to Python.

use bolivar_core::utils::HasBBox;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::io::Write;

use crate::convert::name_to_psliteral;
use crate::params::PyLAParams;

/// Layout page - result of processing a PDF page.
#[pyclass(name = "LTPage", dict)]
#[derive(Clone)]
pub struct PyLTPage {
    /// Page identifier (1-based page number)
    #[pyo3(get)]
    pub pageid: i32,
    /// Page rotation in degrees
    #[pyo3(get)]
    pub rotate: f64,
    /// Bounding box as (x0, y0, x1, y1)
    pub bbox: (f64, f64, f64, f64),
    /// Layout items on this page
    pub items: Vec<PyLTItem>,
}

#[pymethods]
impl PyLTPage {
    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTPage(pageid={}, bbox={:?})", self.pageid, self.bbox)
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Iterator over LTPage items
#[pyclass]
pub struct PyLTPageIter {
    pub items: Vec<PyLTItem>,
    pub index: usize,
}

#[pymethods]
impl PyLTPageIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<PyLTItem> {
        if slf.index < slf.items.len() {
            let item = slf.items[slf.index].clone();
            slf.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

/// Layout item - can be a character, line, box, etc.
#[derive(Clone)]
pub enum PyLTItem {
    Char(PyLTChar),
    Rect(PyLTRect),
    Line(PyLTLine),
    Curve(PyLTCurve),
    Anno(PyLTAnno),
    TextLineH(PyLTTextLineHorizontal),
    TextLineV(PyLTTextLineVertical),
    TextBoxH(PyLTTextBoxHorizontal),
    TextBoxV(PyLTTextBoxVertical),
    Image(PyLTImage),
    Figure(PyLTFigure),
    Page(PyLTPage),
}

impl<'py> IntoPyObject<'py> for PyLTItem {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            PyLTItem::Char(c) => {
                let bbox = c.bbox;
                let adv = c.adv;
                let size = c.size;
                let fontname = c.fontname.clone();
                let matrix = c.matrix;
                let upright = c.upright;
                let mcid = c.mcid;
                let tag = c.tag.clone();
                let text = c.text.clone();
                let non_stroking_color = c.non_stroking_color.clone();
                let stroking_color = c.stroking_color.clone();
                let bound = Bound::new(py, c)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("adv", adv)?;
                dict.set_item("size", size)?;
                dict.set_item("fontname", fontname)?;
                dict.set_item("matrix", matrix)?;
                dict.set_item("upright", upright)?;
                dict.set_item("mcid", mcid)?;
                dict.set_item("tag", tag)?;
                dict.set_item("text", text)?;
                dict.set_item("non_stroking_color", non_stroking_color)?;
                dict.set_item("stroking_color", stroking_color)?;
                Ok(bound.into_any())
            }
            PyLTItem::Rect(r) => {
                let bbox = r.bbox;
                let linewidth = r.linewidth;
                let stroke = r.stroke;
                let fill = r.fill;
                let non_stroking_color = r.non_stroking_color.clone();
                let stroking_color = r.stroking_color.clone();
                let bound = Bound::new(py, r)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("linewidth", linewidth)?;
                dict.set_item("stroke", stroke)?;
                dict.set_item("fill", fill)?;
                dict.set_item("non_stroking_color", non_stroking_color)?;
                dict.set_item("stroking_color", stroking_color)?;
                Ok(bound.into_any())
            }
            PyLTItem::Line(l) => {
                let bbox = l.bbox;
                let p0 = l.p0;
                let p1 = l.p1;
                let linewidth = l.linewidth;
                let stroke = l.stroke;
                let fill = l.fill;
                let non_stroking_color = l.non_stroking_color.clone();
                let stroking_color = l.stroking_color.clone();
                let bound = Bound::new(py, l)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("pts", vec![p0, p1])?;
                dict.set_item("linewidth", linewidth)?;
                dict.set_item("stroke", stroke)?;
                dict.set_item("fill", fill)?;
                dict.set_item("non_stroking_color", non_stroking_color)?;
                dict.set_item("stroking_color", stroking_color)?;
                Ok(bound.into_any())
            }
            PyLTItem::Curve(c) => {
                let bbox = c.bbox;
                let pts = c.pts.clone();
                let linewidth = c.linewidth;
                let stroke = c.stroke;
                let fill = c.fill;
                let evenodd = c.evenodd;
                let non_stroking_color = c.non_stroking_color.clone();
                let stroking_color = c.stroking_color.clone();
                let bound = Bound::new(py, c)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("pts", pts)?;
                dict.set_item("linewidth", linewidth)?;
                dict.set_item("stroke", stroke)?;
                dict.set_item("fill", fill)?;
                dict.set_item("evenodd", evenodd)?;
                dict.set_item("non_stroking_color", non_stroking_color)?;
                dict.set_item("stroking_color", stroking_color)?;
                Ok(bound.into_any())
            }
            PyLTItem::Anno(a) => {
                let text = a.text.clone();
                let bound = Bound::new(py, a)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("text", text)?;
                Ok(bound.into_any())
            }
            PyLTItem::TextLineH(l) => {
                let bbox = l.bbox;
                let bound = Bound::new(py, l)?;
                set_bbox_attrs(&bound, bbox)?;
                Ok(bound.into_any())
            }
            PyLTItem::TextLineV(l) => {
                let bbox = l.bbox;
                let bound = Bound::new(py, l)?;
                set_bbox_attrs(&bound, bbox)?;
                Ok(bound.into_any())
            }
            PyLTItem::TextBoxH(b) => {
                let bbox = b.bbox;
                let bound = Bound::new(py, b)?;
                set_bbox_attrs(&bound, bbox)?;
                Ok(bound.into_any())
            }
            PyLTItem::TextBoxV(b) => {
                let bbox = b.bbox;
                let bound = Bound::new(py, b)?;
                set_bbox_attrs(&bound, bbox)?;
                Ok(bound.into_any())
            }
            PyLTItem::Image(i) => {
                let bbox = i.bbox;
                let name = i.name.clone();
                let srcsize = i.srcsize;
                let imagemask = i.imagemask;
                let bits = i.bits;
                let colorspace = i.colorspace.clone();
                let bound = Bound::new(py, i)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("name", name)?;
                dict.set_item("srcsize", srcsize)?;
                dict.set_item("imagemask", imagemask)?;
                dict.set_item("bits", bits)?;
                dict.set_item("colorspace", colorspace)?;
                Ok(bound.into_any())
            }
            PyLTItem::Figure(f) => {
                let bbox = f.bbox;
                let name = f.name.clone();
                let matrix = f.matrix;
                let bound = Bound::new(py, f)?;
                set_bbox_attrs(&bound, bbox)?;
                let dict = instance_dict(&bound)?;
                dict.set_item("name", name)?;
                dict.set_item("matrix", matrix)?;
                Ok(bound.into_any())
            }
            PyLTItem::Page(p) => Ok(Bound::new(py, p)?.into_any()),
        }
    }
}

fn set_bbox_attrs(obj: &Bound<'_, PyAny>, bbox: (f64, f64, f64, f64)) -> PyResult<()> {
    let dict = instance_dict(obj)?;
    dict.set_item("x0", bbox.0)?;
    dict.set_item("y0", bbox.1)?;
    dict.set_item("x1", bbox.2)?;
    dict.set_item("y1", bbox.3)?;
    dict.set_item("width", bbox.2 - bbox.0)?;
    dict.set_item("height", bbox.3 - bbox.1)?;
    dict.set_item("bbox", bbox)?;
    Ok(())
}

fn instance_dict<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let dict_obj = obj.getattr("__dict__")?;
    Ok(dict_obj.downcast::<PyDict>()?.clone())
}

/// Layout character - a single character with position and font info.
#[pyclass(name = "LTChar", dict)]
#[derive(Clone)]
pub struct PyLTChar {
    /// Bounding box as (x0, y0, x1, y1)
    pub bbox: (f64, f64, f64, f64),
    /// The character text
    pub text: String,
    /// Font name
    #[pyo3(get)]
    pub fontname: String,
    /// Font size
    #[pyo3(get)]
    pub size: f64,
    /// Whether the character is upright
    #[pyo3(get)]
    pub upright: bool,
    /// Character advance width
    #[pyo3(get)]
    pub adv: f64,
    /// Text rendering matrix (a, b, c, d, e, f)
    #[pyo3(get)]
    pub matrix: (f64, f64, f64, f64, f64, f64),
    /// Marked Content ID (for tagged PDF accessibility)
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag (e.g., "P", "Span", "H1")
    #[pyo3(get)]
    pub tag: Option<String>,
    /// Non-stroking colorspace name (e.g., "DeviceRGB")
    pub ncs_name: Option<String>,
    /// Stroking colorspace name (e.g., "DeviceRGB")
    pub scs_name: Option<String>,
    /// Non-stroking (fill) color as tuple of floats
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color as tuple of floats
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
}

impl PyLTChar {
    pub fn from_core(c: &bolivar_core::layout::LTChar) -> Self {
        Self {
            bbox: (c.x0(), c.y0(), c.x1(), c.y1()),
            text: c.get_text().to_string(),
            fontname: c.fontname().to_string(),
            size: c.size(),
            upright: c.upright(),
            adv: c.adv(),
            matrix: c.matrix(),
            mcid: c.mcid(),
            tag: c.tag(),
            ncs_name: c.ncs(),
            scs_name: c.scs(),
            non_stroking_color: c.non_stroking_color().clone(),
            stroking_color: c.stroking_color().clone(),
        }
    }
}

#[pymethods]
impl PyLTChar {
    /// Get the character text
    fn get_text(&self) -> &str {
        &self.text
    }

    #[getter]
    fn ncs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.ncs_name.as_deref() {
            Some(name) => name_to_psliteral(py, name),
            None => Ok(py.None()),
        }
    }

    #[getter]
    fn scs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.scs_name.as_deref() {
            Some(name) => name_to_psliteral(py, name),
            None => Ok(py.None()),
        }
    }

    #[getter]
    fn graphicstate(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let types = py.import("types")?;
        let ns = types.getattr("SimpleNamespace")?.call0()?;

        let scolor = match &self.stroking_color {
            Some(values) => PyTuple::new(py, values)?.into_any(),
            None => 0i32.into_pyobject(py)?.into_any(),
        };
        let ncolor = match &self.non_stroking_color {
            Some(values) => PyTuple::new(py, values)?.into_any(),
            None => 0i32.into_pyobject(py)?.into_any(),
        };

        ns.setattr("scolor", scolor)?;
        ns.setattr("ncolor", ncolor)?;

        if let Some(name) = self.ncs_name.as_deref() {
            let ncs = name_to_psliteral(py, name)?;
            ns.setattr("ncs", ncs)?;
        }

        if let Some(name) = self.scs_name.as_deref() {
            let scs = name_to_psliteral(py, name)?;
            ns.setattr("scs", scs)?;
        }

        Ok(ns.into_any().unbind())
    }

    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTChar({:?}, fontname={:?})", self.text, self.fontname)
    }
}

/// Layout text line - horizontal.
#[pyclass(name = "LTTextLineHorizontal", dict)]
#[derive(Clone)]
pub struct PyLTTextLineHorizontal {
    pub bbox: (f64, f64, f64, f64),
    pub items: Vec<PyLTItem>,
}

impl PyLTTextLineHorizontal {
    pub fn from_core(line: &bolivar_core::layout::LTTextLineHorizontal) -> Self {
        let mut items = Vec::new();
        for elem in line.iter() {
            match elem {
                bolivar_core::layout::TextLineElement::Char(c) => {
                    items.push(PyLTItem::Char(PyLTChar::from_core(c)));
                }
                bolivar_core::layout::TextLineElement::Anno(a) => {
                    items.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
                }
            }
        }
        Self {
            bbox: (line.x0(), line.y0(), line.x1(), line.y1()),
            items,
        }
    }
}

#[pymethods]
impl PyLTTextLineHorizontal {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout text line - vertical.
#[pyclass(name = "LTTextLineVertical", dict)]
#[derive(Clone)]
pub struct PyLTTextLineVertical {
    pub bbox: (f64, f64, f64, f64),
    pub items: Vec<PyLTItem>,
}

impl PyLTTextLineVertical {
    pub fn from_core(line: &bolivar_core::layout::LTTextLineVertical) -> Self {
        let mut items = Vec::new();
        for elem in line.iter() {
            match elem {
                bolivar_core::layout::TextLineElement::Char(c) => {
                    items.push(PyLTItem::Char(PyLTChar::from_core(c)));
                }
                bolivar_core::layout::TextLineElement::Anno(a) => {
                    items.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
                }
            }
        }
        Self {
            bbox: (line.x0(), line.y0(), line.x1(), line.y1()),
            items,
        }
    }
}

#[pymethods]
impl PyLTTextLineVertical {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout text box - horizontal.
#[pyclass(name = "LTTextBoxHorizontal", dict)]
#[derive(Clone)]
pub struct PyLTTextBoxHorizontal {
    pub bbox: (f64, f64, f64, f64),
    pub items: Vec<PyLTItem>,
}

impl PyLTTextBoxHorizontal {
    pub fn from_core(boxh: &bolivar_core::layout::LTTextBoxHorizontal) -> Self {
        let mut items = Vec::new();
        for line in boxh.iter() {
            items.push(PyLTItem::TextLineH(PyLTTextLineHorizontal::from_core(line)));
        }
        Self {
            bbox: (boxh.x0(), boxh.y0(), boxh.x1(), boxh.y1()),
            items,
        }
    }
}

#[pymethods]
impl PyLTTextBoxHorizontal {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout text box - vertical.
#[pyclass(name = "LTTextBoxVertical", dict)]
#[derive(Clone)]
pub struct PyLTTextBoxVertical {
    pub bbox: (f64, f64, f64, f64),
    pub items: Vec<PyLTItem>,
}

impl PyLTTextBoxVertical {
    pub fn from_core(boxv: &bolivar_core::layout::LTTextBoxVertical) -> Self {
        let mut items = Vec::new();
        for line in boxv.iter() {
            items.push(PyLTItem::TextLineV(PyLTTextLineVertical::from_core(line)));
        }
        Self {
            bbox: (boxv.x0(), boxv.y0(), boxv.x1(), boxv.y1()),
            items,
        }
    }
}

#[pymethods]
impl PyLTTextBoxVertical {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout image.
#[pyclass(name = "LTImage", dict)]
#[derive(Clone)]
pub struct PyLTImage {
    pub bbox: (f64, f64, f64, f64),
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub srcsize: (Option<i32>, Option<i32>),
    #[pyo3(get)]
    pub imagemask: bool,
    #[pyo3(get)]
    pub bits: i32,
    #[pyo3(get)]
    pub colorspace: Vec<String>,
}

impl PyLTImage {
    pub fn from_core(img: &bolivar_core::layout::LTImage) -> Self {
        Self {
            bbox: (img.x0(), img.y0(), img.x1(), img.y1()),
            name: img.name.clone(),
            srcsize: img.srcsize,
            imagemask: img.imagemask,
            bits: img.bits,
            colorspace: img.colorspace.clone(),
        }
    }
}

#[pymethods]
impl PyLTImage {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }
}

/// Layout figure (Form XObject).
#[pyclass(name = "LTFigure", dict)]
#[derive(Clone)]
pub struct PyLTFigure {
    pub bbox: (f64, f64, f64, f64),
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub matrix: (f64, f64, f64, f64, f64, f64),
    pub items: Vec<PyLTItem>,
}

impl PyLTFigure {
    pub fn from_core(fig: &bolivar_core::layout::LTFigure) -> Self {
        let mut items = Vec::new();
        for child in fig.iter() {
            items.push(ltitem_to_py(child));
        }
        Self {
            bbox: (fig.x0(), fig.y0(), fig.x1(), fig.y1()),
            name: fig.name.clone(),
            matrix: fig.matrix,
            items,
        }
    }
}

#[pymethods]
impl PyLTFigure {
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyLTPageIter {
        PyLTPageIter {
            items: slf.items.clone(),
            index: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.items.len()
    }
}

/// Layout rectangle - a rectangle in the PDF.
#[pyclass(name = "LTRect", dict)]
#[derive(Clone)]
pub struct PyLTRect {
    /// Bounding box as (x0, y0, x1, y1)
    pub bbox: (f64, f64, f64, f64),
    /// Line width
    #[pyo3(get)]
    pub linewidth: f64,
    /// Whether the path is stroked
    #[pyo3(get)]
    pub stroke: bool,
    /// Whether the path is filled
    #[pyo3(get)]
    pub fill: bool,
    /// Non-stroking (fill) color
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
    /// Original path operations: list of (cmd, points) tuples
    #[pyo3(get)]
    pub original_path: Option<Vec<(char, Vec<(f64, f64)>)>>,
    /// Dashing style: (pattern, phase)
    #[pyo3(get)]
    pub dashing_style: Option<(Vec<f64>, f64)>,
    /// Marked Content ID
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag
    #[pyo3(get)]
    pub tag: Option<String>,
}

impl PyLTRect {
    pub fn from_core(r: &bolivar_core::layout::LTRect) -> Self {
        // Convert original_path from Option<Vec<(char, Vec<Point>)>> to Python-friendly format
        let original_path = r
            .original_path
            .as_ref()
            .map(|path| path.iter().map(|(cmd, pts)| (*cmd, pts.clone())).collect());
        Self {
            bbox: (r.x0(), r.y0(), r.x1(), r.y1()),
            linewidth: r.linewidth,
            stroke: r.stroke,
            fill: r.fill,
            non_stroking_color: r.non_stroking_color.clone(),
            stroking_color: r.stroking_color.clone(),
            original_path,
            dashing_style: r.dashing_style.clone(),
            mcid: r.mcid(),
            tag: r.tag(),
        }
    }
}

#[pymethods]
impl PyLTRect {
    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTRect(bbox={:?}, linewidth={})", self.bbox, self.linewidth)
    }
}

/// Layout line - a straight line in the PDF.
#[pyclass(name = "LTLine", dict)]
#[derive(Clone)]
pub struct PyLTLine {
    /// Bounding box as (x0, y0, x1, y1)
    pub bbox: (f64, f64, f64, f64),
    /// Start point
    #[pyo3(get)]
    pub p0: (f64, f64),
    /// End point
    #[pyo3(get)]
    pub p1: (f64, f64),
    /// Line width
    #[pyo3(get)]
    pub linewidth: f64,
    /// Whether the path is stroked
    #[pyo3(get)]
    pub stroke: bool,
    /// Whether the path is filled
    #[pyo3(get)]
    pub fill: bool,
    /// Non-stroking (fill) color
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
    /// Original path operations
    #[pyo3(get)]
    pub original_path: Option<Vec<(char, Vec<(f64, f64)>)>>,
    /// Dashing style: (pattern, phase)
    #[pyo3(get)]
    pub dashing_style: Option<(Vec<f64>, f64)>,
    /// Marked Content ID
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag
    #[pyo3(get)]
    pub tag: Option<String>,
}

impl PyLTLine {
    pub fn from_core(l: &bolivar_core::layout::LTLine) -> Self {
        let original_path = l
            .original_path
            .as_ref()
            .map(|path| path.iter().map(|(cmd, pts)| (*cmd, pts.clone())).collect());
        Self {
            bbox: (l.x0(), l.y0(), l.x1(), l.y1()),
            p0: l.p0(),
            p1: l.p1(),
            linewidth: l.linewidth,
            stroke: l.stroke,
            fill: l.fill,
            non_stroking_color: l.non_stroking_color.clone(),
            stroking_color: l.stroking_color.clone(),
            original_path,
            dashing_style: l.dashing_style.clone(),
            mcid: l.mcid(),
            tag: l.tag(),
        }
    }
}

#[pymethods]
impl PyLTLine {
    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    /// Get points as list for pdfplumber compatibility
    #[getter]
    fn pts(&self) -> Vec<(f64, f64)> {
        vec![self.p0, self.p1]
    }

    fn __repr__(&self) -> String {
        format!("LTLine(p0={:?}, p1={:?})", self.p0, self.p1)
    }
}

/// Python wrapper for LTCurve
#[pyclass(name = "LTCurve", dict)]
#[derive(Clone)]
pub struct PyLTCurve {
    /// Bounding box as (x0, y0, x1, y1)
    pub bbox: (f64, f64, f64, f64),
    /// Control points
    #[pyo3(get)]
    pub pts: Vec<(f64, f64)>,
    /// Line width
    #[pyo3(get)]
    pub linewidth: f64,
    /// Whether the path is stroked
    #[pyo3(get)]
    pub stroke: bool,
    /// Whether the path is filled
    #[pyo3(get)]
    pub fill: bool,
    /// Even-odd fill rule
    #[pyo3(get)]
    pub evenodd: bool,
    /// Non-stroking (fill) color
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
    /// Original path operations
    #[pyo3(get)]
    pub original_path: Option<Vec<(char, Vec<(f64, f64)>)>>,
    /// Dashing style: (pattern, phase)
    #[pyo3(get)]
    pub dashing_style: Option<(Vec<f64>, f64)>,
    /// Marked Content ID
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag
    #[pyo3(get)]
    pub tag: Option<String>,
}

impl PyLTCurve {
    pub fn from_core(c: &bolivar_core::layout::LTCurve) -> Self {
        let original_path = c
            .original_path
            .as_ref()
            .map(|path| path.iter().map(|(cmd, pts)| (*cmd, pts.clone())).collect());
        Self {
            bbox: (c.x0(), c.y0(), c.x1(), c.y1()),
            pts: c.pts.clone(),
            linewidth: c.linewidth,
            stroke: c.stroke,
            fill: c.fill,
            evenodd: c.evenodd,
            non_stroking_color: c.non_stroking_color.clone(),
            stroking_color: c.stroking_color.clone(),
            original_path,
            dashing_style: c.dashing_style.clone(),
            mcid: c.mcid(),
            tag: c.tag(),
        }
    }
}

#[pymethods]
impl PyLTCurve {
    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTCurve(pts={:?})", self.pts.len())
    }
}

/// Python wrapper for LTAnno (virtual annotation like spaces/newlines)
#[pyclass(name = "LTAnno", dict)]
#[derive(Clone)]
pub struct PyLTAnno {
    /// The text content (space, newline, etc.)
    #[pyo3(get)]
    pub text: String,
}

impl PyLTAnno {
    pub fn from_core(a: &bolivar_core::layout::LTAnno) -> Self {
        Self {
            text: a.get_text().to_string(),
        }
    }
}

#[pymethods]
impl PyLTAnno {
    #[new]
    pub fn new(text: String) -> Self {
        Self { text }
    }

    fn get_text(&self) -> &str {
        &self.text
    }

    fn __repr__(&self) -> String {
        format!("LTAnno({:?})", self.text)
    }
}

/// PyWriter for writing to Python file objects.
pub struct PyWriter {
    outfp: Py<PyAny>,
}

impl PyWriter {
    pub fn new(outfp: Py<PyAny>) -> Self {
        Self { outfp }
    }
}

impl Write for PyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use pyo3::types::PyBytes;
        Python::attach(|py| {
            let out = self.outfp.bind(py);
            let bytes = PyBytes::new(py, buf);
            out.call_method1("write", (bytes,))
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            Ok(buf.len())
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Python::attach(|py| {
            let out = self.outfp.bind(py);
            if let Ok(has_flush) = out.hasattr("flush")
                && has_flush
            {
                out.call_method0("flush")
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
            }
            Ok(())
        })
    }
}

/// TextConverter for converting layout to text.
#[pyclass(name = "TextConverter")]
pub struct PyTextConverter {
    converter: bolivar_core::converter::TextConverter<PyWriter>,
}

#[pymethods]
impl PyTextConverter {
    #[new]
    #[pyo3(signature = (rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None, showpageno=false, imagewriter=None))]
    pub fn new(
        rsrcmgr: &Bound<'_, PyAny>,
        outfp: Py<PyAny>,
        codec: &str,
        pageno: i32,
        laparams: Option<&PyLAParams>,
        showpageno: bool,
        imagewriter: Option<&Bound<'_, PyAny>>,
    ) -> Self {
        let _ = rsrcmgr;
        let _ = imagewriter;
        let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
        let mut converter = bolivar_core::converter::TextConverter::new(
            PyWriter::new(outfp),
            codec,
            pageno,
            la,
            showpageno,
        );
        converter.set_showpageno(showpageno);
        Self { converter }
    }

    pub fn _receive_layout(&mut self, ltpage: &PyLTPage) {
        let core_page = py_ltpage_to_core(ltpage);
        self.converter.receive_layout(core_page);
    }

    fn close(&mut self) {
        self.converter.flush();
    }
}

/// HTMLConverter for converting layout to HTML.
#[pyclass(name = "HTMLConverter")]
pub struct PyHTMLConverter {
    converter: bolivar_core::converter::HTMLConverter<PyWriter>,
}

#[pymethods]
impl PyHTMLConverter {
    #[new]
    #[pyo3(signature = (rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None, scale=1.0, fontscale=1.0, layoutmode="normal", showpageno=true, pagemargin=50, imagewriter=None, debug=0, rect_colors=None, text_colors=None))]
    pub fn new(
        rsrcmgr: &Bound<'_, PyAny>,
        outfp: Py<PyAny>,
        codec: &str,
        pageno: i32,
        laparams: Option<&PyLAParams>,
        scale: f64,
        fontscale: f64,
        layoutmode: &str,
        showpageno: bool,
        pagemargin: i32,
        imagewriter: Option<&Bound<'_, PyAny>>,
        debug: i32,
        rect_colors: Option<&Bound<'_, PyAny>>,
        text_colors: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        use crate::convert::py_any_to_string_map;

        let _ = rsrcmgr;
        let _ = imagewriter;
        let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
        let writer = PyWriter::new(outfp);
        let mut converter =
            if (scale - 1.0).abs() > f64::EPSILON || (fontscale - 1.0).abs() > f64::EPSILON {
                bolivar_core::converter::HTMLConverter::with_options(
                    writer, codec, pageno, la, scale, fontscale,
                )
            } else if debug > 0 {
                bolivar_core::converter::HTMLConverter::with_debug(writer, codec, pageno, la, debug)
            } else {
                bolivar_core::converter::HTMLConverter::new(writer, codec, pageno, la)
            };

        converter.set_layoutmode(layoutmode);
        converter.set_showpageno(showpageno);
        converter.set_pagemargin(pagemargin);
        converter.set_scale(scale);
        converter.set_fontscale(fontscale);

        if let Some(obj) = rect_colors {
            converter.set_rect_colors(py_any_to_string_map(obj)?);
        } else if debug > 0 {
            converter.set_rect_colors(
                bolivar_core::converter::HTMLConverter::<PyWriter>::debug_rect_colors(),
            );
        }

        if let Some(obj) = text_colors {
            converter.set_text_colors(py_any_to_string_map(obj)?);
        } else if debug > 0 {
            converter.set_text_colors(
                bolivar_core::converter::HTMLConverter::<PyWriter>::debug_text_colors(),
            );
        }

        Ok(Self { converter })
    }

    pub fn _receive_layout(&mut self, ltpage: &PyLTPage) {
        let core_page = py_ltpage_to_core(ltpage);
        self.converter.receive_layout(core_page);
    }

    fn close(&mut self) {
        self.converter.close();
        self.converter.flush();
    }
}

/// XMLConverter for converting layout to XML.
#[pyclass(name = "XMLConverter")]
pub struct PyXMLConverter {
    converter: bolivar_core::converter::XMLConverter<PyWriter>,
}

#[pymethods]
impl PyXMLConverter {
    #[new]
    #[pyo3(signature = (rsrcmgr, outfp, codec="utf-8", pageno=1, laparams=None, stripcontrol=false, imagewriter=None))]
    pub fn new(
        rsrcmgr: &Bound<'_, PyAny>,
        outfp: Py<PyAny>,
        codec: &str,
        pageno: i32,
        laparams: Option<&PyLAParams>,
        stripcontrol: bool,
        imagewriter: Option<&Bound<'_, PyAny>>,
    ) -> Self {
        let _ = rsrcmgr;
        let _ = imagewriter;
        let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());
        let mut converter = bolivar_core::converter::XMLConverter::with_options(
            PyWriter::new(outfp),
            codec,
            pageno,
            la,
            stripcontrol,
        );
        converter.set_stripcontrol(stripcontrol);
        Self { converter }
    }

    pub fn _receive_layout(&mut self, ltpage: &PyLTPage) {
        let core_page = py_ltpage_to_core(ltpage);
        self.converter.receive_layout(core_page);
    }

    fn close(&mut self) {
        self.converter.close();
        self.converter.flush();
    }
}

// Conversion functions between Python and core types

fn py_textline_element_from_item(item: &PyLTItem) -> Option<bolivar_core::layout::TextLineElement> {
    match item {
        PyLTItem::Char(c) => Some(bolivar_core::layout::TextLineElement::Char(
            py_ltchar_to_core(c),
        )),
        PyLTItem::Anno(a) => Some(bolivar_core::layout::TextLineElement::Anno(
            bolivar_core::layout::LTAnno::new(&a.text),
        )),
        _ => None,
    }
}

fn py_ltchar_to_core(c: &PyLTChar) -> bolivar_core::layout::LTChar {
    let mut ch = bolivar_core::layout::LTChar::with_colors_matrix(
        c.bbox,
        &c.text,
        &c.fontname,
        c.size,
        c.upright,
        c.adv,
        c.matrix,
        c.mcid,
        c.tag.clone(),
        c.non_stroking_color.clone(),
        c.stroking_color.clone(),
    );
    ch.set_ncs(c.ncs_name.clone());
    ch.set_scs(c.scs_name.clone());
    ch
}

fn py_textline_h_to_core(
    line: &PyLTTextLineHorizontal,
) -> bolivar_core::layout::LTTextLineHorizontal {
    let mut core_line = bolivar_core::layout::LTTextLineHorizontal::new(0.1);
    for item in &line.items {
        if let Some(elem) = py_textline_element_from_item(item) {
            core_line.add_element(elem);
        }
    }
    core_line.set_bbox(line.bbox);
    core_line
}

fn py_textline_v_to_core(line: &PyLTTextLineVertical) -> bolivar_core::layout::LTTextLineVertical {
    let mut core_line = bolivar_core::layout::LTTextLineVertical::new(0.1);
    for item in &line.items {
        if let Some(elem) = py_textline_element_from_item(item) {
            core_line.add_element(elem);
        }
    }
    core_line.set_bbox(line.bbox);
    core_line
}

pub fn py_ltitem_to_core(item: &PyLTItem) -> bolivar_core::layout::LTItem {
    match item {
        PyLTItem::Char(c) => bolivar_core::layout::LTItem::Char(py_ltchar_to_core(c)),
        PyLTItem::Anno(a) => {
            bolivar_core::layout::LTItem::Anno(bolivar_core::layout::LTAnno::new(&a.text))
        }
        PyLTItem::Rect(r) => {
            let original_path = r.original_path.clone();
            let dashing_style = r.dashing_style.clone();
            let mut rect = if original_path.is_some() || dashing_style.is_some() {
                bolivar_core::layout::LTRect::new_with_dashing(
                    r.linewidth,
                    r.bbox,
                    r.stroke,
                    r.fill,
                    false,
                    r.stroking_color.clone(),
                    r.non_stroking_color.clone(),
                    original_path,
                    dashing_style,
                )
            } else {
                bolivar_core::layout::LTRect::new(
                    r.linewidth,
                    r.bbox,
                    r.stroke,
                    r.fill,
                    false,
                    r.stroking_color.clone(),
                    r.non_stroking_color.clone(),
                )
            };
            rect.set_marked_content(r.mcid, r.tag.clone());
            bolivar_core::layout::LTItem::Rect(rect)
        }
        PyLTItem::Line(l) => {
            let original_path = l.original_path.clone();
            let dashing_style = l.dashing_style.clone();
            let mut line = if original_path.is_some() || dashing_style.is_some() {
                bolivar_core::layout::LTLine::new_with_dashing(
                    l.linewidth,
                    l.p0,
                    l.p1,
                    l.stroke,
                    l.fill,
                    false,
                    l.stroking_color.clone(),
                    l.non_stroking_color.clone(),
                    original_path,
                    dashing_style,
                )
            } else {
                bolivar_core::layout::LTLine::new(
                    l.linewidth,
                    l.p0,
                    l.p1,
                    l.stroke,
                    l.fill,
                    false,
                    l.stroking_color.clone(),
                    l.non_stroking_color.clone(),
                )
            };
            line.set_marked_content(l.mcid, l.tag.clone());
            bolivar_core::layout::LTItem::Line(line)
        }
        PyLTItem::Curve(c) => {
            let original_path = c.original_path.clone();
            let dashing_style = c.dashing_style.clone();
            let mut curve = if original_path.is_some() || dashing_style.is_some() {
                bolivar_core::layout::LTCurve::new_with_dashing(
                    c.linewidth,
                    c.pts.clone(),
                    c.stroke,
                    c.fill,
                    c.evenodd,
                    c.stroking_color.clone(),
                    c.non_stroking_color.clone(),
                    original_path,
                    dashing_style,
                )
            } else {
                bolivar_core::layout::LTCurve::new(
                    c.linewidth,
                    c.pts.clone(),
                    c.stroke,
                    c.fill,
                    c.evenodd,
                    c.stroking_color.clone(),
                    c.non_stroking_color.clone(),
                )
            };
            curve.set_marked_content(c.mcid, c.tag.clone());
            bolivar_core::layout::LTItem::Curve(curve)
        }
        PyLTItem::TextLineH(l) => bolivar_core::layout::LTItem::TextLine(
            bolivar_core::layout::TextLineType::Horizontal(py_textline_h_to_core(l)),
        ),
        PyLTItem::TextLineV(l) => bolivar_core::layout::LTItem::TextLine(
            bolivar_core::layout::TextLineType::Vertical(py_textline_v_to_core(l)),
        ),
        PyLTItem::TextBoxH(b) => {
            let mut boxh = bolivar_core::layout::LTTextBoxHorizontal::new();
            for item in &b.items {
                if let PyLTItem::TextLineH(line) = item {
                    boxh.add(py_textline_h_to_core(line));
                }
            }
            bolivar_core::layout::LTItem::TextBox(bolivar_core::layout::TextBoxType::Horizontal(
                boxh,
            ))
        }
        PyLTItem::TextBoxV(b) => {
            let mut boxv = bolivar_core::layout::LTTextBoxVertical::new();
            for item in &b.items {
                if let PyLTItem::TextLineV(line) = item {
                    boxv.add(py_textline_v_to_core(line));
                }
            }
            bolivar_core::layout::LTItem::TextBox(bolivar_core::layout::TextBoxType::Vertical(boxv))
        }
        PyLTItem::Image(i) => {
            bolivar_core::layout::LTItem::Image(bolivar_core::layout::LTImage::new(
                &i.name,
                i.bbox,
                i.srcsize,
                i.imagemask,
                i.bits,
                i.colorspace.clone(),
            ))
        }
        PyLTItem::Figure(f) => {
            let bbox = f.bbox;
            let width = bbox.2 - bbox.0;
            let height = bbox.3 - bbox.1;
            let mut fig = bolivar_core::layout::LTFigure::new(
                &f.name,
                (bbox.0, bbox.1, width, height),
                f.matrix,
            );
            for child in &f.items {
                fig.add(py_ltitem_to_core(child));
            }
            bolivar_core::layout::LTItem::Figure(Box::new(fig))
        }
        PyLTItem::Page(p) => {
            let core = py_ltpage_to_core(p);
            bolivar_core::layout::LTItem::Page(Box::new(core))
        }
    }
}

pub fn py_ltpage_to_core(page: &PyLTPage) -> bolivar_core::layout::LTPage {
    let mut core_page = bolivar_core::layout::LTPage::new(page.pageid, page.bbox, page.rotate);
    for item in &page.items {
        core_page.add(py_ltitem_to_core(item));
    }
    core_page
}

pub fn ltpage_to_py(ltpage: &bolivar_core::layout::LTPage) -> PyLTPage {
    let items: Vec<PyLTItem> = ltpage.iter().map(ltitem_to_py).collect();
    PyLTPage {
        pageid: ltpage.pageid,
        rotate: ltpage.rotate,
        bbox: (ltpage.x0(), ltpage.y0(), ltpage.x1(), ltpage.y1()),
        items,
    }
}

pub fn ltitem_to_py(item: &bolivar_core::layout::LTItem) -> PyLTItem {
    use bolivar_core::layout::{LTItem, TextBoxType, TextLineType};

    match item {
        LTItem::Char(c) => PyLTItem::Char(PyLTChar::from_core(c)),
        LTItem::Anno(a) => PyLTItem::Anno(PyLTAnno::from_core(a)),
        LTItem::Rect(r) => PyLTItem::Rect(PyLTRect::from_core(r)),
        LTItem::Line(l) => PyLTItem::Line(PyLTLine::from_core(l)),
        LTItem::Curve(c) => PyLTItem::Curve(PyLTCurve::from_core(c)),
        LTItem::Image(i) => PyLTItem::Image(PyLTImage::from_core(i)),
        LTItem::TextLine(line) => match line {
            TextLineType::Horizontal(l) => {
                PyLTItem::TextLineH(PyLTTextLineHorizontal::from_core(l))
            }
            TextLineType::Vertical(l) => PyLTItem::TextLineV(PyLTTextLineVertical::from_core(l)),
        },
        LTItem::TextBox(tbox) => match tbox {
            TextBoxType::Horizontal(b) => PyLTItem::TextBoxH(PyLTTextBoxHorizontal::from_core(b)),
            TextBoxType::Vertical(b) => PyLTItem::TextBoxV(PyLTTextBoxVertical::from_core(b)),
        },
        LTItem::Figure(fig) => PyLTItem::Figure(PyLTFigure::from_core(fig)),
        LTItem::Page(page) => PyLTItem::Page(ltpage_to_py(page)),
    }
}

/// Register the layout module classes with the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyLTPage>()?;
    m.add_class::<PyLTChar>()?;
    m.add_class::<PyLTTextLineHorizontal>()?;
    m.add_class::<PyLTTextLineVertical>()?;
    m.add_class::<PyLTTextBoxHorizontal>()?;
    m.add_class::<PyLTTextBoxVertical>()?;
    m.add_class::<PyLTImage>()?;
    m.add_class::<PyLTFigure>()?;
    m.add_class::<PyLTRect>()?;
    m.add_class::<PyLTLine>()?;
    m.add_class::<PyLTCurve>()?;
    m.add_class::<PyLTAnno>()?;
    m.add_class::<PyTextConverter>()?;
    m.add_class::<PyHTMLConverter>()?;
    m.add_class::<PyXMLConverter>()?;
    Ok(())
}
