//! Port of pdfminer.six test_pdfpage.py
//!
//! Tests for PDFPage functionality.

use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdfpage::PDFPage;

// Embed test fixture at compile time (Miri-compatible)
const PAGELABELS_PDF: &[u8] = include_bytes!("fixtures/contrib/pagelabels.pdf");

/// Test that page labels are correctly assigned to each PDFPage.
/// Port of: test_page_labels
///
/// Original Python test:
/// ```python
/// def test_page_labels(self):
///     path = absolute_sample_path("contrib/pagelabels.pdf")
///     expected_labels = ["iii", "iv", "1", "2", "1"]
///
///     with open(path, "rb") as fp:
///         parser = PDFParser(fp)
///         doc = PDFDocument(parser)
///         for i, page in enumerate(PDFPage.create_pages(doc)):
///             assert page.label == expected_labels[i]
/// ```
#[test]
fn test_page_labels() {
    let doc = PDFDocument::new(PAGELABELS_PDF, "").expect("Failed to parse PDF");

    let expected_labels = ["iii", "iv", "1", "2", "1"];

    for (i, page_result) in PDFPage::create_pages(&doc).enumerate() {
        let page = page_result.expect("Failed to get page");
        let label = page.label.as_deref().unwrap_or("");
        assert_eq!(
            label, expected_labels[i],
            "Page {} label mismatch: expected '{}', got '{}'",
            i, expected_labels[i], label
        );
    }
}

#[test]
fn test_page_contents_lazy() {
    let doc = PDFDocument::new(PAGELABELS_PDF, "").expect("Failed to parse PDF");
    let page = PDFPage::create_pages(&doc)
        .next()
        .expect("Missing page")
        .expect("Failed to get page");
    assert!(page.contents.is_empty());
}
