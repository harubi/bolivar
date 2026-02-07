//! Python bindings for bolivar PDF library
//!
//! This crate provides PyO3 bindings to expose bolivar's PDF parsing
//! functionality to Python, with a pdfminer.six-compatible API.

mod casting;
mod codec;
mod convert;
mod document;
mod font;
mod image;
mod layout;
mod params;
mod stream;
mod table;
mod utils;

use pyo3::prelude::*;

/// Python module for bolivar PDF library.
#[pymodule]
fn _bolivar(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Register classes and functions from each module
    params::register(m)?;
    casting::register(m)?;
    codec::register(m)?;
    document::register(m)?;
    font::register(m)?;
    image::register(m)?;
    layout::register(m)?;
    stream::register(m)?;
    table::register(m)?;
    utils::register(m)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{extract_pages_from_path, extract_text, extract_text_from_path};
    use pyo3::types::PyBytes;
    use std::sync::Once;

    static PY_INIT: Once = Once::new();

    fn ensure_python_initialized() {
        PY_INIT.call_once(Python::initialize);
    }

    fn build_minimal_pdf_with_pages(page_count: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");

        let mut offsets: Vec<usize> = Vec::new();
        let push_obj = |buf: &mut Vec<u8>, obj: String, offsets: &mut Vec<usize>| {
            offsets.push(buf.len());
            buf.extend_from_slice(obj.as_bytes());
        };

        push_obj(
            &mut out,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
            &mut offsets,
        );

        let kids: String = (0..page_count)
            .map(|i| format!("{} 0 R", 3 + i))
            .collect::<Vec<_>>()
            .join(" ");
        push_obj(
            &mut out,
            format!(
                "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
                kids, page_count
            ),
            &mut offsets,
        );

        for i in 0..page_count {
            let page_id = 3 + i;
            let contents_id = 3 + page_count + i;
            push_obj(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents {} 0 R >>\nendobj\n",
                    page_id, contents_id
                ),
                &mut offsets,
            );
        }

        for i in 0..page_count {
            let contents_id = 3 + page_count + i;
            push_obj(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n",
                    contents_id
                ),
                &mut offsets,
            );
        }

        let xref_pos = out.len();
        let obj_count = offsets.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", obj_count + 1).as_bytes(),
        );
        for offset in offsets {
            out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }
        out.extend_from_slice(b"trailer\n<< /Size ");
        out.extend_from_slice((obj_count + 1).to_string().as_bytes());
        out.extend_from_slice(b" /Root 1 0 R >>\nstartxref\n");
        out.extend_from_slice(xref_pos.to_string().as_bytes());
        out.extend_from_slice(b"\n%%EOF");

        out
    }

    fn write_temp_pdf(data: &[u8]) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        path.push(format!("bolivar_py_test_{pid}_{stamp}.pdf"));
        std::fs::write(&path, data).expect("write temp pdf");
        path
    }

    #[test]
    fn test_extract_text_from_path_matches_bytes() {
        ensure_python_initialized();
        let pdf_data = build_minimal_pdf_with_pages(1);
        let path = write_temp_pdf(&pdf_data);

        Python::attach(|py| {
            let py_bytes = PyBytes::new(py, &pdf_data);
            let text_bytes = extract_text(py, py_bytes.as_any(), "", None, 0, true, None).unwrap();
            let text_path = extract_text_from_path(
                py,
                path.to_string_lossy().as_ref(),
                "",
                None,
                0,
                true,
                None,
            )
            .unwrap();
            assert_eq!(text_bytes, text_path);
        });

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_extract_pages_from_path_len() {
        ensure_python_initialized();
        let pdf_data = build_minimal_pdf_with_pages(2);
        let path = write_temp_pdf(&pdf_data);

        Python::attach(|py| {
            let pages = extract_pages_from_path(
                py,
                path.to_string_lossy().as_ref(),
                "",
                None,
                0,
                true,
                None,
            )
            .unwrap();
            assert_eq!(pages.len(), 2);
        });

        let _ = std::fs::remove_file(&path);
    }
}
