//! Python bindings for bolivar PDF library
//!
//! This crate provides PyO3 bindings to expose bolivar's PDF parsing
//! functionality to Python, with a pdfminer.six-compatible API.

use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::utils::HasBBox;

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
        for (idx, page_result) in bolivar_core::pdfpage::PDFPage::create_pages(&self.inner).enumerate() {
            let page = page_result
                .map_err(|e| PyValueError::new_err(format!("Failed to get page {}: {}", idx, e)))?;
            pages.push(PyPDFPage::from_core(page));
        }
        Ok(pages)
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
    // Future: Line, TextBox, Figure, etc.
}

impl<'py> IntoPyObject<'py> for PyLTItem {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            PyLTItem::Char(c) => Ok(Bound::new(py, c)?.into_any()),
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
    let la: bolivar_core::layout::LAParams = laparams
        .map(|p| p.clone().into())
        .unwrap_or_default();

    // Create resource manager
    let mut rsrcmgr = bolivar_core::pdfinterp::PDFResourceManager::with_caching(true);

    // Create aggregator for this page
    let mut aggregator = bolivar_core::converter::PDFPageAggregator::new(
        Some(la.clone()),
        page.pageid as i32,
    );

    // Recreate the core PDFPage with contents
    // This is a workaround since we can't store references across Python calls
    let core_page = bolivar_core::pdfpage::PDFPage {
        pageid: page.pageid,
        attrs: std::collections::HashMap::new(),
        label: page.label.clone(),
        mediabox: page.mediabox.map(|b| [b.0, b.1, b.2, b.3]),
        cropbox: page.cropbox.map(|b| [b.0, b.1, b.2, b.3]),
        rotate: page.rotate,
        annots: None,
        resources: page.resources.clone(),
        contents: page.contents.clone(),
        user_unit: 1.0,
    };

    // Create interpreter and process page
    let mut interpreter = bolivar_core::pdfinterp::PDFPageInterpreter::new(
        &mut rsrcmgr,
        &mut aggregator,
    );
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
    use bolivar_core::layout::{LTItem, TextLineType, TextBoxType, TextLineElement};

    match item {
        LTItem::Char(c) => {
            chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
        }
        LTItem::TextLine(line) => {
            // Match on the TextLineType variant to get the inner line with iter()
            match line {
                TextLineType::Horizontal(l) => {
                    for elem in l.iter() {
                        if let TextLineElement::Char(c) = elem {
                            chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
                        }
                    }
                }
                TextLineType::Vertical(l) => {
                    for elem in l.iter() {
                        if let TextLineElement::Char(c) = elem {
                            chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
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
                            if let TextLineElement::Char(c) = elem {
                                chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
                            }
                        }
                    }
                }
                TextBoxType::Vertical(b) => {
                    for line in b.iter() {
                        for elem in line.iter() {
                            if let TextLineElement::Char(c) = elem {
                                chars.push(PyLTItem::Char(PyLTChar::from_core(c)));
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
        // Skip non-text items (Anno, Curve, Line, Rect, Image)
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
    m.add_function(wrap_pyfunction!(process_page, m)?)?;
    Ok(())
}
