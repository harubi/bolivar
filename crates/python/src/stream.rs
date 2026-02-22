//! Async streaming bindings for Python.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};

use bolivar_core::api::stream::{
    DEFAULT_STREAM_BUFFER_CAPACITY, TableStream,
    extract_pages_stream_from_doc as core_extract_pages_stream_from_doc,
    extract_tables_stream_from_doc_with_geometries as core_extract_tables_stream_from_doc_with_geometries,
    extract_text_pages_from_doc_with_geometries as core_extract_text_pages_from_doc_with_geometries,
    extract_words_pages_from_doc_with_geometries as core_extract_words_pages_from_doc_with_geometries,
};
use bolivar_core::error::Result as CoreResult;
use bolivar_core::high_level::{ExtractOptions, extract_pages_stream as core_extract_pages_stream};
use bolivar_core::layout::LTPage;
use bolivar_core::table::{TextDir, WordObj};
use pyo3::exceptions::{PyStopAsyncIteration, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tokio::sync::{Mutex, mpsc};

use crate::document::{PyPDFDocument, pdf_input_from_py};
use crate::layout::ltpage_to_py;
use crate::params::{PyLAParams, parse_page_geometries, parse_table_settings, parse_text_settings};
fn text_dir_to_str(direction: TextDir) -> &'static str {
    match direction {
        TextDir::Ttb => "ttb",
        TextDir::Btt => "btt",
        TextDir::Ltr => "ltr",
        TextDir::Rtl => "rtl",
    }
}

fn word_to_dict(py: Python<'_>, word: WordObj) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("text", word.text)?;
    out.set_item("x0", word.x0)?;
    out.set_item("x1", word.x1)?;
    out.set_item("top", word.top)?;
    out.set_item("doctop", word.doctop)?;
    out.set_item("bottom", word.bottom)?;
    out.set_item("upright", word.upright)?;
    out.set_item("height", word.height)?;
    out.set_item("width", word.width)?;
    out.set_item("direction", text_dir_to_str(word.direction))?;
    Ok(out.into_any().unbind())
}

#[pyclass]
pub struct AsyncPageStream {
    rx: Arc<Mutex<mpsc::Receiver<CoreResult<LTPage>>>>,
    done: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

#[pymethods]
impl AsyncPageStream {
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__<'py>(&'py mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rx = Arc::clone(&self.rx);
        let done = Arc::clone(&self.done);
        let cancel = Arc::clone(&self.cancel);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if done.load(Ordering::Relaxed) {
                return Err(PyStopAsyncIteration::new_err(()));
            }

            let mut guard = rx.lock().await;
            match guard.recv().await {
                Some(Ok(page)) => Python::attach(|py| {
                    let py_page = Py::new(py, ltpage_to_py(page))?;
                    Ok(py_page.into_any())
                }),
                Some(Err(err)) => {
                    cancel.store(true, Ordering::Relaxed);
                    done.store(true, Ordering::Relaxed);
                    Err(PyValueError::new_err(format!(
                        "Failed to extract pages: {err}"
                    )))
                }
                None => {
                    done.store(true, Ordering::Relaxed);
                    Err(PyStopAsyncIteration::new_err(()))
                }
            }
        })
    }

    fn aclose<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let cancel = Arc::clone(&self.cancel);
        let done = Arc::clone(&self.done);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            cancel.store(true, Ordering::Relaxed);
            done.store(true, Ordering::Relaxed);
            Ok(())
        })
    }
}

impl Drop for AsyncPageStream {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[pyclass]
pub struct PyTableStream {
    stream: StdMutex<Option<TableStream>>,
}

#[pymethods]
impl PyTableStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(
        &mut self,
        py: Python<'_>,
    ) -> PyResult<Option<(usize, Vec<Vec<Vec<Option<String>>>>)>> {
        let mut stream = {
            let mut guard = self
                .stream
                .lock()
                .map_err(|_| PyValueError::new_err("table stream lock poisoned"))?;
            guard
                .take()
                .ok_or_else(|| PyValueError::new_err("table stream closed"))?
        };
        let (next, stream) = py.detach(|| {
            let next = stream.next();
            (next, stream)
        });
        let mut guard = self
            .stream
            .lock()
            .map_err(|_| PyValueError::new_err("table stream lock poisoned"))?;
        *guard = Some(stream);
        match next {
            None => Ok(None),
            Some(Ok((page_idx, tables))) => Ok(Some((page_idx, tables))),
            Some(Err(err)) => Err(PyValueError::new_err(format!(
                "Failed to extract tables: {err}"
            ))),
        }
    }
}

/// Async runtime sanity check for pyo3-async-runtimes.
#[pyfunction]
pub fn async_runtime_poc(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(42_u32) })
}

/// Extract pages asynchronously from PDF bytes.
#[pyfunction]
#[pyo3(signature = (data, password = "", page_numbers = None, maxpages = 0, caching = true, laparams = None))]
pub fn extract_pages_async(
    data: &Bound<'_, PyAny>,
    password: &str,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
) -> PyResult<AsyncPageStream> {
    let options = ExtractOptions {
        password: password.to_string(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };

    let stream = match pdf_input_from_py(data)? {
        crate::document::PdfInput::Shared(bytes) => {
            core_extract_pages_stream(bytes.as_ref(), Some(options))
                .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {e}")))?
        }
        crate::document::PdfInput::Owned(bytes) => core_extract_pages_stream(&bytes, Some(options))
            .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {e}")))?,
    };

    let (tx, rx) = mpsc::channel(DEFAULT_STREAM_BUFFER_CAPACITY);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);

    std::thread::spawn(move || {
        for item in stream {
            if cancel_worker.load(Ordering::Relaxed) {
                return;
            }
            let is_err = item.is_err();
            if tx.blocking_send(item).is_err() {
                return;
            }
            if is_err {
                return;
            }
        }
    });

    Ok(AsyncPageStream {
        rx: Arc::new(Mutex::new(rx)),
        done: Arc::new(AtomicBool::new(false)),
        cancel,
    })
}

/// Extract pages asynchronously from an existing PDFDocument.
#[pyfunction]
#[pyo3(signature = (doc, page_numbers = None, maxpages = 0, caching = true, laparams = None))]
pub fn extract_pages_async_from_document(
    doc: &PyPDFDocument,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&PyLAParams>,
) -> PyResult<AsyncPageStream> {
    let options = ExtractOptions {
        password: String::new(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };

    let stream = core_extract_pages_stream_from_doc(Arc::clone(&doc.inner), options)
        .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {e}")))?;

    let (tx, rx) = mpsc::channel(DEFAULT_STREAM_BUFFER_CAPACITY);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);

    std::thread::spawn(move || {
        for item in stream {
            if cancel_worker.load(Ordering::Relaxed) {
                return;
            }
            let is_err = item.is_err();
            if tx.blocking_send(item).is_err() {
                return;
            }
            if is_err {
                return;
            }
        }
    });

    Ok(AsyncPageStream {
        rx: Arc::new(Mutex::new(rx)),
        done: Arc::new(AtomicBool::new(false)),
        cancel,
    })
}

/// Extract tables as a blocking stream from an existing PDFDocument.
#[pyfunction(name = "_extract_tables_stream")]
#[pyo3(signature = (doc, geometries, table_settings = None, laparams = None, page_numbers = None, maxpages = 0, caching = true))]
pub fn extract_tables_stream(
    py: Python<'_>,
    doc: &PyPDFDocument,
    geometries: &Bound<'_, PyAny>,
    table_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
) -> PyResult<PyTableStream> {
    let settings = parse_table_settings(py, table_settings)?;
    let geoms = parse_page_geometries(geometries)?;
    let options = ExtractOptions {
        password: String::new(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };

    let stream = core_extract_tables_stream_from_doc_with_geometries(
        Arc::clone(&doc.inner),
        options,
        settings,
        geoms,
    )
    .map_err(|e| PyValueError::new_err(format!("Failed to extract tables: {e}")))?;

    Ok(PyTableStream {
        stream: StdMutex::new(Some(stream)),
    })
}

/// Extract per-page text in page-index order using Rust layout+text extraction.
#[pyfunction(name = "_extract_text_stream")]
#[pyo3(signature = (doc, geometries, text_settings = None, laparams = None, page_numbers = None, maxpages = 0, caching = true))]
pub fn extract_text_stream(
    py: Python<'_>,
    doc: &PyPDFDocument,
    geometries: &Bound<'_, PyAny>,
    text_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
) -> PyResult<Vec<(usize, String)>> {
    let settings = parse_text_settings(py, text_settings)?;
    let geoms = parse_page_geometries(geometries)?;
    let options = ExtractOptions {
        password: String::new(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };

    py.detach(|| {
        core_extract_text_pages_from_doc_with_geometries(
            Arc::clone(&doc.inner),
            options,
            settings,
            geoms,
        )
        .map_err(|e| PyValueError::new_err(format!("Failed to extract text: {e}")))
    })
}

/// Extract per-page words in page-index order using Rust layout+word extraction.
#[pyfunction(name = "_extract_words_stream")]
#[pyo3(signature = (doc, geometries, text_settings = None, laparams = None, page_numbers = None, maxpages = 0, caching = true))]
pub fn extract_words_stream(
    py: Python<'_>,
    doc: &PyPDFDocument,
    geometries: &Bound<'_, PyAny>,
    text_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
) -> PyResult<Vec<(usize, Vec<Py<PyAny>>)>> {
    let settings = parse_text_settings(py, text_settings)?;
    let geoms = parse_page_geometries(geometries)?;
    let options = ExtractOptions {
        password: String::new(),
        page_numbers,
        maxpages,
        caching,
        laparams: laparams.map(|p| p.clone().into()),
    };

    let words: Vec<(usize, Vec<WordObj>)> = py.detach(|| {
        core_extract_words_pages_from_doc_with_geometries(
            Arc::clone(&doc.inner),
            options,
            settings,
            geoms,
        )
        .map_err(|e| PyValueError::new_err(format!("Failed to extract words: {e}")))
    })?;

    let mut out: Vec<(usize, Vec<Py<PyAny>>)> = Vec::with_capacity(words.len());
    for (page_idx, page_words) in words {
        let mut row: Vec<Py<PyAny>> = Vec::with_capacity(page_words.len());
        for word in page_words {
            row.push(word_to_dict(py, word)?);
        }
        out.push((page_idx, row));
    }
    Ok(out)
}
/// Register stream-related functions with the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(async_runtime_poc, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_async, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_async_from_document, m)?)?;
    m.add_class::<PyTableStream>()?;
    m.add_function(wrap_pyfunction!(extract_tables_stream, m)?)?;
    m.add_function(wrap_pyfunction!(extract_text_stream, m)?)?;
    m.add_function(wrap_pyfunction!(extract_words_stream, m)?)?;
    Ok(())
}
