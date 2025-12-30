//! Port of pdfminer.six tests/test_font_size.py
//!
//! Tests for font size extraction from PDF text.
//! The test PDF contains text where the displayed number equals the font size
//! used to render that number (e.g., "12" rendered in 12pt font).

use bolivar_core::high_level::{ExtractOptions, extract_pages};
use bolivar_core::layout::{LAParams, LTItem, LTTextBox, LTTextLine, TextBoxType, TextLineElement};

// ============================================================================
// Helper functions
// ============================================================================

/// Get absolute path to a test sample file.
fn sample_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests").join("samples").join(name)
}

// ============================================================================
// Font size extraction tests
// ============================================================================

/// Test that font sizes are correctly extracted from characters.
///
/// Port of Python test_font_size():
/// ```python
/// def test_font_size():
///     path = absolute_sample_path("font-size-test.pdf")
///     for page in extract_pages(path):
///         for text_box in page:
///             if isinstance(text_box, LTTextBox):
///                 for line in text_box:
///                     possible_number = line.get_text().strip()
///                     if possible_number.isdigit():
///                         expected_size = int(possible_number)
///                         for char in line:
///                             if isinstance(char, LTChar):
///                                 actual_size = int(round(char.size))
///                                 assert expected_size == actual_size
/// ```
///
/// NOTE: This test requires full PDF content stream interpretation to be implemented.
/// Currently the extract_pages function creates page structures but doesn't populate
/// them with text content from the PDF. Once PDFPageInterpreter is fully integrated,
/// this test will verify that font sizes are correctly extracted from real PDF files.
#[test]

fn test_font_size() {
    let path = sample_path("font-size-test.pdf");
    let pdf_data = std::fs::read(&path).expect("Failed to read font-size-test.pdf");

    let options = ExtractOptions {
        laparams: Some(LAParams::default()),
        ..Default::default()
    };

    let pages = extract_pages(&pdf_data, Some(options)).expect("Failed to extract pages");

    let mut found_any = false;

    for page_result in pages {
        let page = page_result.expect("Failed to get page");

        for item in page.iter() {
            if let LTItem::TextBox(text_box) = item {
                // Get text from the text box
                let _text = match text_box {
                    TextBoxType::Horizontal(tb) => tb.get_text(),
                    TextBoxType::Vertical(tb) => tb.get_text(),
                };

                // Iterate through lines in the text box
                match text_box {
                    TextBoxType::Horizontal(tb) => {
                        for line in tb.iter() {
                            let line_text = line.get_text().trim().to_string();

                            // Check if the line text is a digit (font size indicator)
                            if line_text.chars().all(|c: char| c.is_ascii_digit())
                                && !line_text.is_empty()
                            {
                                let expected_size: i32 = line_text.parse().unwrap();

                                // Check each character's font size
                                for element in line.iter() {
                                    if let TextLineElement::Char(ch) = element {
                                        let actual_size = ch.size().round() as i32;
                                        assert_eq!(
                                            expected_size,
                                            actual_size,
                                            "Font size mismatch: expected {} but got {} for char '{}'",
                                            expected_size,
                                            actual_size,
                                            ch.get_text()
                                        );
                                        found_any = true;
                                    }
                                }
                            }
                        }
                    }
                    TextBoxType::Vertical(tb) => {
                        for line in tb.iter() {
                            let line_text = line.get_text().trim().to_string();

                            if line_text.chars().all(|c: char| c.is_ascii_digit())
                                && !line_text.is_empty()
                            {
                                let expected_size: i32 = line_text.parse().unwrap();

                                for element in line.iter() {
                                    if let TextLineElement::Char(ch) = element {
                                        let actual_size = ch.size().round() as i32;
                                        assert_eq!(
                                            expected_size,
                                            actual_size,
                                            "Font size mismatch: expected {} but got {} for char '{}'",
                                            expected_size,
                                            actual_size,
                                            ch.get_text()
                                        );
                                        found_any = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Ensure we actually tested something
    assert!(
        found_any,
        "No font size comparisons were made - test PDF may not have been parsed correctly"
    );
}

/// Test that LTChar correctly stores and returns font size.
#[test]
fn test_ltchar_size_accessor() {
    use bolivar_core::layout::LTChar;

    let char1 = LTChar::new((0.0, 0.0, 10.0, 12.0), "A", "Helvetica", 12.0, true, 10.0);
    assert_eq!(char1.size(), 12.0);

    let char2 = LTChar::new((0.0, 0.0, 20.0, 24.0), "B", "Times-Roman", 24.0, true, 20.0);
    assert_eq!(char2.size(), 24.0);

    let char3 = LTChar::new((0.0, 0.0, 8.0, 8.5), "x", "Courier", 8.5, true, 8.0);
    assert_eq!(char3.size(), 8.5);
}

/// Test font size with various sizes to verify rounding behavior.
#[test]
fn test_ltchar_size_rounding() {
    use bolivar_core::layout::LTChar;

    // Test that size is stored exactly as provided
    let sizes = [
        6.0, 8.0, 10.0, 11.5, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 36.0, 48.0, 72.0,
    ];

    for &size in &sizes {
        let ch = LTChar::new((0.0, 0.0, size, size), "X", "Test", size, true, size);
        assert_eq!(
            ch.size(),
            size,
            "LTChar should preserve exact font size {}",
            size
        );
    }
}

/// Test that fontname is correctly stored and retrieved.
#[test]
fn test_ltchar_fontname() {
    use bolivar_core::layout::LTChar;

    let fontnames = [
        "Helvetica",
        "Times-Roman",
        "Courier",
        "Arial-Bold",
        "ABCDEF+CustomFont",
    ];

    for fontname in &fontnames {
        let ch = LTChar::new((0.0, 0.0, 10.0, 10.0), "A", fontname, 10.0, true, 10.0);
        assert_eq!(ch.fontname(), *fontname);
    }
}

/// Test character text extraction.
#[test]
fn test_ltchar_text() {
    use bolivar_core::layout::LTChar;

    let test_cases = [("A", "A"), ("1", "1"), ("!", "!"), (" ", " ")];

    for (input, expected) in &test_cases {
        let ch = LTChar::new((0.0, 0.0, 10.0, 10.0), input, "Test", 10.0, true, 10.0);
        assert_eq!(ch.get_text(), *expected);
    }
}

/// Test that upright flag is correctly stored.
#[test]
fn test_ltchar_upright() {
    use bolivar_core::layout::LTChar;

    let upright_char = LTChar::new((0.0, 0.0, 10.0, 10.0), "A", "Test", 10.0, true, 10.0);
    assert!(upright_char.upright());

    let rotated_char = LTChar::new((0.0, 0.0, 10.0, 10.0), "A", "Test", 10.0, false, 10.0);
    assert!(!rotated_char.upright());
}

/// Test character advance width.
#[test]
fn test_ltchar_advance() {
    use bolivar_core::layout::LTChar;

    let ch = LTChar::new((0.0, 0.0, 10.0, 12.0), "W", "Test", 12.0, true, 15.0);
    assert_eq!(ch.adv(), 15.0);
}
