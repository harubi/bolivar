//! Async streaming bindings for Python.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use bolivar_core::api::stream::{
    PageStream, extract_pages_stream_from_doc as core_extract_pages_stream_from_doc,
    extract_words_pages_from_doc_with_geometries as core_extract_words_pages_from_doc_with_geometries,
};
use bolivar_core::error::Result as CoreResult;
use bolivar_core::layout::LTPage;
use bolivar_core::table::{TextDir, WordObj};
use pyo3::exceptions::{PyStopAsyncIteration, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::document::{PyPDFDocument, build_extract_options, open_document_from_input};
use crate::layout::ltpage_to_py;
use crate::params::{PyLAParams, parse_page_geometry, parse_text_settings};

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

enum AsyncStreamStep<T> {
    Item(T),
    End,
    Error(String),
}

struct AsyncStreamState<S> {
    stream: Option<S>,
    done: bool,
}

impl<S> AsyncStreamState<S> {
    const fn new(stream: S) -> Self {
        Self {
            stream: Some(stream),
            done: false,
        }
    }

    fn close(&mut self) {
        self.done = true;
        self.stream.take();
    }

    #[cfg(test)]
    const fn is_done(&self) -> bool {
        self.done
    }
}

impl<S> AsyncStreamState<S>
where
    S: Iterator<Item = CoreResult<LTPage>>,
{
    fn next_step(&mut self) -> AsyncStreamStep<LTPage> {
        if self.done {
            return AsyncStreamStep::End;
        }

        let Some(stream) = self.stream.as_mut() else {
            self.done = true;
            return AsyncStreamStep::End;
        };

        match stream.next() {
            Some(Ok(page)) => AsyncStreamStep::Item(page),
            Some(Err(err)) => {
                self.close();
                AsyncStreamStep::Error(format!("Failed to extract pages: {err}"))
            }
            None => {
                self.close();
                AsyncStreamStep::End
            }
        }
    }
}

#[pyclass]
pub struct AsyncPageStream {
    state: Arc<StdMutex<AsyncStreamState<PageStream>>>,
}

#[pymethods]
impl AsyncPageStream {
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__<'py>(&'py mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let locals = pyo3_async_runtimes::tokio::get_current_locals(py)?;
        let state = Arc::clone(&self.state);

        pyo3_async_runtimes::tokio::future_into_py_with_locals(py, locals, async move {
            let next = tokio::task::spawn_blocking(move || {
                let mut guard = state
                    .lock()
                    .map_err(|_| String::from("page stream lock poisoned"))?;
                Ok::<_, String>(guard.next_step())
            })
            .await
            .map_err(|err| PyValueError::new_err(format!("page stream task failed: {err}")))?;

            match next.map_err(PyValueError::new_err)? {
                AsyncStreamStep::Item(page) => Python::attach(|py| {
                    let py_page = Py::new(py, ltpage_to_py(page))?;
                    Ok(py_page.into_any())
                }),
                AsyncStreamStep::End => Err(PyStopAsyncIteration::new_err(())),
                AsyncStreamStep::Error(message) => Err(PyValueError::new_err(message)),
            }
        })
    }

    fn aclose<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let locals = pyo3_async_runtimes::tokio::get_current_locals(py)?;
        let state = Arc::clone(&self.state);

        pyo3_async_runtimes::tokio::future_into_py_with_locals(py, locals, async move {
            tokio::task::spawn_blocking(move || {
                let mut guard = state
                    .lock()
                    .map_err(|_| String::from("page stream lock poisoned"))?;
                guard.close();
                Ok::<_, String>(())
            })
            .await
            .map_err(|err| PyValueError::new_err(format!("page stream task failed: {err}")))?
            .map_err(PyValueError::new_err)?;
            Ok(())
        })
    }
}

impl Drop for AsyncPageStream {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.state.lock() {
            guard.close();
        }
    }
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
    let options = build_extract_options(password, page_numbers, maxpages, caching, laparams);
    let doc = open_document_from_input(data.py(), data, password, caching, true)?;
    let stream = core_extract_pages_stream_from_doc(doc, options)
        .map_err(|e| PyValueError::new_err(format!("Failed to extract pages: {e}")))?;

    Ok(AsyncPageStream {
        state: Arc::new(StdMutex::new(AsyncStreamState::new(stream))),
    })
}

/// Extract words for a single page using Rust layout+word extraction.
#[pyfunction(name = "_extract_words_for_page_indexed")]
#[pyo3(signature = (doc, page_index, geometry, text_settings = None, laparams = None, caching = true))]
pub fn extract_words_for_page_indexed(
    py: Python<'_>,
    doc: &PyPDFDocument,
    page_index: usize,
    geometry: &Bound<'_, PyAny>,
    text_settings: Option<Py<PyAny>>,
    laparams: Option<&PyLAParams>,
    caching: bool,
) -> PyResult<Option<Vec<Py<PyAny>>>> {
    let settings = parse_text_settings(py, text_settings)?;
    let geom = parse_page_geometry(geometry)?;
    let options = build_extract_options("", Some(vec![page_index]), 0, caching, laparams);

    let words: Vec<(usize, Vec<WordObj>)> = py.detach(|| {
        core_extract_words_pages_from_doc_with_geometries(
            Arc::clone(&doc.inner),
            options,
            settings,
            vec![geom],
        )
        .map_err(|e| PyValueError::new_err(format!("Failed to extract words: {e}")))
    })?;

    for (page_idx, page_words) in words {
        if page_idx != page_index {
            continue;
        }

        let mut row: Vec<Py<PyAny>> = Vec::with_capacity(page_words.len());
        for word in page_words {
            row.push(word_to_dict(py, word)?);
        }
        return Ok(Some(row));
    }

    Ok(None)
}
/// Register stream-related functions with the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract_pages_async, m)?)?;
    m.add_function(wrap_pyfunction!(extract_words_for_page_indexed, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AsyncStreamState, AsyncStreamStep};
    use bolivar_core::error::{PdfError, Result as CoreResult};
    use bolivar_core::layout::LTPage;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct ProbeStream {
        dropped: Arc<AtomicBool>,
    }

    impl Iterator for ProbeStream {
        type Item = CoreResult<LTPage>;

        fn next(&mut self) -> Option<Self::Item> {
            None
        }
    }

    impl Drop for ProbeStream {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn async_stream_state_close_drops_owned_stream() {
        let dropped = Arc::new(AtomicBool::new(false));
        let mut state = AsyncStreamState::new(ProbeStream {
            dropped: Arc::clone(&dropped),
        });

        state.close();

        assert!(state.is_done());
        assert!(dropped.load(Ordering::Relaxed));
    }

    struct ErrorStream;

    impl Iterator for ErrorStream {
        type Item = CoreResult<LTPage>;

        fn next(&mut self) -> Option<Self::Item> {
            Some(Err(PdfError::DecodeError(
                "stream closed before expected page 1 arrived".to_string(),
            )))
        }
    }

    #[test]
    fn async_stream_state_reports_underlying_stream_error() {
        let mut state = AsyncStreamState::new(ErrorStream);

        match state.next_step() {
            AsyncStreamStep::Error(message) => {
                assert!(message.contains("page 1"), "unexpected message: {message}");
            }
            AsyncStreamStep::Item(_) => panic!("expected stream error"),
            AsyncStreamStep::End => panic!("expected stream error"),
        }

        assert!(state.is_done());
        assert!(matches!(state.next_step(), AsyncStreamStep::End));
    }
}
