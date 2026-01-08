//! Async streaming bindings for Python.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bolivar_core::api::stream::{DEFAULT_STREAM_BUFFER_CAPACITY, extract_pages_stream_from_doc as core_extract_pages_stream_from_doc};
use bolivar_core::error::Result as CoreResult;
use bolivar_core::high_level::{ExtractOptions, extract_pages_stream as core_extract_pages_stream};
use bolivar_core::layout::LTPage;
use pyo3::exceptions::{PyStopAsyncIteration, PyValueError};
use pyo3::prelude::*;
use tokio::sync::{Mutex, mpsc};

use crate::document::{PyPDFDocument, pdf_input_from_py};
use crate::layout::ltpage_to_py;
use crate::params::PyLAParams;

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
                    let py_page = Py::new(py, ltpage_to_py(&page))?;
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

/// Register stream-related functions with the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(async_runtime_poc, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_async, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pages_async_from_document, m)?)?;
    Ok(())
}
