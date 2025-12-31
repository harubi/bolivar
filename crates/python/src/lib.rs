//! Python bindings for bolivar PDF library
//!
//! This crate provides PyO3 bindings to expose bolivar's PDF parsing
//! functionality to Python, with a pdfminer.six-compatible API.

use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdftypes::PDFObject;
use bolivar_core::utils::HasBBox;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Convert a PDFObject to a string representation for Python.
fn pdf_object_to_string(obj: &PDFObject) -> String {
    match obj {
        PDFObject::Null => "null".to_string(),
        PDFObject::Bool(b) => b.to_string(),
        PDFObject::Int(i) => i.to_string(),
        PDFObject::Real(r) => r.to_string(),
        PDFObject::Name(n) => n.clone(),
        PDFObject::String(s) => {
            // Try to decode as UTF-8, fall back to lossy
            String::from_utf8(s.clone()).unwrap_or_else(|_| String::from_utf8_lossy(s).to_string())
        }
        PDFObject::Array(arr) => {
            let items: Vec<String> = arr.iter().map(pdf_object_to_string).collect();
            format!("[{}]", items.join(", "))
        }
        PDFObject::Dict(dict) => {
            let items: Vec<String> = dict
                .iter()
                .map(|(k, v)| format!("{}: {}", k, pdf_object_to_string(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        PDFObject::Stream(_) => "<stream>".to_string(),
        PDFObject::Ref(objref) => format!("{} {} R", objref.objid, objref.genno),
    }
}

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
    fn new(
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

/// PDF Document - main entry point for PDF parsing.
///
/// Creates a document from PDF bytes and provides access to pages.
#[pyclass(name = "PDFDocument")]
pub struct PyPDFDocument {
    /// The underlying Rust PDFDocument (owns the data via Arc)
    inner: PDFDocument,
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
    #[pyo3(signature = (data, password = ""))]
    fn new(data: &[u8], password: &str) -> PyResult<Self> {
        let doc = PDFDocument::new(data, password)
            .map_err(|e| PyValueError::new_err(format!("Failed to parse PDF: {}", e)))?;
        Ok(Self { inner: doc })
    }

    /// Get an iterator over pages in the document.
    ///
    /// Returns:
    ///     List of PDFPage objects
    fn get_pages(&self) -> PyResult<Vec<PyPDFPage>> {
        let mut pages = Vec::new();
        for (idx, page_result) in
            bolivar_core::pdfpage::PDFPage::create_pages(&self.inner).enumerate()
        {
            let page = page_result
                .map_err(|e| PyValueError::new_err(format!("Failed to get page {}: {}", idx, e)))?;
            pages.push(PyPDFPage::from_core(page));
        }
        Ok(pages)
    }

    /// Get document info dictionaries.
    ///
    /// Returns:
    ///     List of dictionaries containing document metadata (Producer, Creator, etc.)
    #[getter]
    fn info(&self) -> Vec<std::collections::HashMap<String, String>> {
        self.inner
            .info()
            .iter()
            .map(|dict| {
                dict.iter()
                    .map(|(k, v)| (k.clone(), pdf_object_to_string(v)))
                    .collect()
            })
            .collect()
    }
}

/// PDF Page - represents a single page in a PDF document.
#[pyclass(name = "PDFPage")]
#[derive(Clone)]
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
    /// Internal: decoded page contents (for processing)
    contents: Vec<Vec<u8>>,
    /// Internal: page resources (serialized for later use)
    resources: std::collections::HashMap<String, bolivar_core::pdftypes::PDFObject>,
}

impl PyPDFPage {
    /// Create from core PDFPage
    fn from_core(page: bolivar_core::pdfpage::PDFPage) -> Self {
        Self {
            pageid: page.pageid,
            mediabox: page.mediabox.map(|b| (b[0], b[1], b[2], b[3])),
            cropbox: page.cropbox.map(|b| (b[0], b[1], b[2], b[3])),
            bleedbox: page.bleedbox.map(|b| (b[0], b[1], b[2], b[3])),
            trimbox: page.trimbox.map(|b| (b[0], b[1], b[2], b[3])),
            artbox: page.artbox.map(|b| (b[0], b[1], b[2], b[3])),
            rotate: page.rotate,
            label: page.label.clone(),
            contents: page.contents.clone(),
            resources: page.resources.clone(),
        }
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
}

/// Layout page - result of processing a PDF page.
#[pyclass(name = "LTPage")]
#[derive(Clone)]
pub struct PyLTPage {
    /// Page identifier (1-based page number)
    #[pyo3(get)]
    pub pageid: i32,
    /// Page rotation in degrees
    #[pyo3(get)]
    pub rotate: f64,
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
    /// Layout items on this page
    items: Vec<PyLTItem>,
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
    items: Vec<PyLTItem>,
    index: usize,
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
}

impl<'py> IntoPyObject<'py> for PyLTItem {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            PyLTItem::Char(c) => Ok(Bound::new(py, c)?.into_any()),
            PyLTItem::Rect(r) => Ok(Bound::new(py, r)?.into_any()),
            PyLTItem::Line(l) => Ok(Bound::new(py, l)?.into_any()),
            PyLTItem::Curve(c) => Ok(Bound::new(py, c)?.into_any()),
            PyLTItem::Anno(a) => Ok(Bound::new(py, a)?.into_any()),
        }
    }
}

/// Layout character - a single character with position and font info.
#[pyclass(name = "LTChar")]
#[derive(Clone)]
pub struct PyLTChar {
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
    /// The character text
    text: String,
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
    /// Marked Content ID (for tagged PDF accessibility)
    #[pyo3(get)]
    pub mcid: Option<i32>,
    /// Marked Content tag (e.g., "P", "Span", "H1")
    #[pyo3(get)]
    pub tag: Option<String>,
    /// Non-stroking (fill) color as tuple of floats
    #[pyo3(get)]
    pub non_stroking_color: Option<Vec<f64>>,
    /// Stroking color as tuple of floats
    #[pyo3(get)]
    pub stroking_color: Option<Vec<f64>>,
}

impl PyLTChar {
    fn from_core(c: &bolivar_core::layout::LTChar) -> Self {
        Self {
            bbox: (c.x0(), c.y0(), c.x1(), c.y1()),
            text: c.get_text().to_string(),
            fontname: c.fontname().to_string(),
            size: c.size(),
            upright: c.upright(),
            adv: c.adv(),
            mcid: c.mcid(),
            tag: c.tag(),
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

    /// Get bounding box as (x0, y0, x1, y1)
    #[getter]
    fn bbox(&self) -> (f64, f64, f64, f64) {
        self.bbox
    }

    fn __repr__(&self) -> String {
        format!("LTChar({:?}, fontname={:?})", self.text, self.fontname)
    }
}

/// Layout rectangle - a rectangle in the PDF.
#[pyclass(name = "LTRect")]
#[derive(Clone)]
pub struct PyLTRect {
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
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
}

impl PyLTRect {
    fn from_core(r: &bolivar_core::layout::LTRect) -> Self {
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
#[pyclass(name = "LTLine")]
#[derive(Clone)]
pub struct PyLTLine {
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
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
}

impl PyLTLine {
    fn from_core(l: &bolivar_core::layout::LTLine) -> Self {
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
#[pyclass(name = "LTCurve")]
#[derive(Clone)]
pub struct PyLTCurve {
    /// Bounding box as (x0, y0, x1, y1)
    bbox: (f64, f64, f64, f64),
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
}

impl PyLTCurve {
    fn from_core(c: &bolivar_core::layout::LTCurve) -> Self {
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
#[pyclass(name = "LTAnno")]
#[derive(Clone)]
pub struct PyLTAnno {
    /// The text content (space, newline, etc.)
    #[pyo3(get)]
    pub text: String,
}

impl PyLTAnno {
    fn from_core(a: &bolivar_core::layout::LTAnno) -> Self {
        Self {
            text: a.get_text().to_string(),
        }
    }
}

#[pymethods]
impl PyLTAnno {
    #[new]
    fn new(text: String) -> Self {
        Self { text }
    }

    fn get_text(&self) -> &str {
        &self.text
    }

    fn __repr__(&self) -> String {
        format!("LTAnno({:?})", self.text)
    }
}

/// Process a PDF page and return its layout.
///
/// Args:
///     doc: PDFDocument instance
///     page: PDFPage to process
///     laparams: Layout analysis parameters
///
/// Returns:
///     LTPage with layout analysis results
#[pyfunction]
#[pyo3(signature = (doc, page, laparams=None))]
fn process_page(
    doc: &PyPDFDocument,
    page: &PyPDFPage,
    laparams: Option<&PyLAParams>,
) -> PyResult<PyLTPage> {
    let la: Option<bolivar_core::layout::LAParams> = laparams.map(|p| p.clone().into());

    // Create resource manager
    let mut rsrcmgr = bolivar_core::pdfinterp::PDFResourceManager::with_caching(true);

    // Create aggregator for this page
    let mut aggregator = bolivar_core::converter::PDFPageAggregator::new(la, page.pageid as i32);

    // Recreate the core PDFPage with contents
    // This is a workaround since we can't store references across Python calls
    let core_page = bolivar_core::pdfpage::PDFPage {
        pageid: page.pageid,
        attrs: std::collections::HashMap::new(),
        label: page.label.clone(),
        mediabox: page.mediabox.map(|b| [b.0, b.1, b.2, b.3]),
        cropbox: page.cropbox.map(|b| [b.0, b.1, b.2, b.3]),
        bleedbox: page.bleedbox.map(|b| [b.0, b.1, b.2, b.3]),
        trimbox: page.trimbox.map(|b| [b.0, b.1, b.2, b.3]),
        artbox: page.artbox.map(|b| [b.0, b.1, b.2, b.3]),
        rotate: page.rotate,
        annots: None,
        resources: page.resources.clone(),
        contents: page.contents.clone(),
        user_unit: 1.0,
    };

    // Create interpreter and process page
    let mut interpreter =
        bolivar_core::pdfinterp::PDFPageInterpreter::new(&mut rsrcmgr, &mut aggregator);
    interpreter.process_page(&core_page, Some(&doc.inner));

    // Get the result
    let ltpage = aggregator.get_result();

    // Convert to Python types
    let mut items = Vec::new();
    for item in ltpage.iter() {
        collect_chars(item, &mut items);
    }

    Ok(PyLTPage {
        pageid: ltpage.pageid,
        rotate: ltpage.rotate,
        bbox: (ltpage.x0(), ltpage.y0(), ltpage.x1(), ltpage.y1()),
        items,
    })
}

/// Recursively collect LTChar items from layout tree
fn collect_chars(item: &bolivar_core::layout::LTItem, chars: &mut Vec<PyLTItem>) {
    use bolivar_core::layout::{LTItem, TextBoxType, TextLineElement, TextLineType};

    match item {
        LTItem::Char(c) => {
            chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
        }
        LTItem::TextLine(line) => {
            // Match on the TextLineType variant to get the inner line with iter()
            match line {
                TextLineType::Horizontal(l) => {
                    for elem in l.iter() {
                        match elem {
                            TextLineElement::Char(c) => {
                                chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
                            }
                            TextLineElement::Anno(a) => {
                                chars.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
                            }
                        }
                    }
                }
                TextLineType::Vertical(l) => {
                    for elem in l.iter() {
                        match elem {
                            TextLineElement::Char(c) => {
                                chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
                            }
                            TextLineElement::Anno(a) => {
                                chars.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
                            }
                        }
                    }
                }
            }
        }
        LTItem::TextBox(tbox) => {
            // Match on the TextBoxType variant to get lines
            match tbox {
                TextBoxType::Horizontal(b) => {
                    for line in b.iter() {
                        for elem in line.iter() {
                            match elem {
                                TextLineElement::Char(c) => {
                                    chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
                                }
                                TextLineElement::Anno(a) => {
                                    chars.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
                                }
                            }
                        }
                    }
                }
                TextBoxType::Vertical(b) => {
                    for line in b.iter() {
                        for elem in line.iter() {
                            match elem {
                                TextLineElement::Char(c) => {
                                    chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
                                }
                                TextLineElement::Anno(a) => {
                                    chars.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
                                }
                            }
                        }
                    }
                }
            }
        }
        LTItem::Figure(fig) => {
            for child in fig.iter() {
                collect_chars(child, chars);
            }
        }
        LTItem::Page(page) => {
            for child in page.iter() {
                collect_chars(child, chars);
            }
        }
        LTItem::Rect(r) => {
            chars.push(PyLTItem::Rect(PyLTRect::from_core(r)));
        }
        LTItem::Line(l) => {
            chars.push(PyLTItem::Line(PyLTLine::from_core(l)));
        }
        LTItem::Curve(c) => {
            chars.push(PyLTItem::Curve(PyLTCurve::from_core(c)));
        }
        LTItem::Anno(a) => {
            chars.push(PyLTItem::Anno(PyLTAnno::from_core(a)));
        }
        // Skip other non-text items (Image)
        _ => {}
    }
}

/// Python module for bolivar PDF library.
#[pymodule]
fn _bolivar(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyLAParams>()?;
    m.add_class::<PyPDFDocument>()?;
    m.add_class::<PyPDFPage>()?;
    m.add_class::<PyLTPage>()?;
    m.add_class::<PyLTChar>()?;
    m.add_class::<PyLTRect>()?;
    m.add_class::<PyLTLine>()?;
    m.add_class::<PyLTCurve>()?;
    m.add_class::<PyLTAnno>()?;
    m.add_function(wrap_pyfunction!(process_page, m)?)?;
    Ok(())
}
