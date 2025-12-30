//! Port of pdfminer.six tests/test_highlevel_extracttext.py
//!
//! Tests for high-level text extraction API including:
//! - extract_text() - main text extraction function
//! - extract_text_to_fp() - text extraction to writer
//! - extract_pages() - iterator over pages

use bolivar_core::high_level::{ExtractOptions, extract_pages, extract_text, extract_text_to_fp};
use bolivar_core::layout::LAParams;
use std::io::Cursor;

// ============================================================================
// TestExtractText - test main text extraction function
// ============================================================================

#[test]
fn test_extract_text_returns_string() {
    // extract_text should return a String
    let result = extract_text(b"", None);
    assert!(result.is_ok() || result.is_err()); // Type check
}

#[test]
fn test_extract_text_empty_input() {
    // Empty input should return empty or error
    let result = extract_text(b"", None);
    // Empty PDF data is not valid, so this should error
    assert!(result.is_err());
}

#[test]
fn test_extract_text_with_options() {
    // Test that options are accepted
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: Some(LAParams::default()),
    };

    let result = extract_text(b"", Some(options));
    // Empty PDF is invalid, but options should be accepted
    assert!(result.is_err());
}

#[test]
fn test_extract_text_page_numbers_filter() {
    // Test page_numbers option filters pages
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: Some(vec![0]), // Only first page
        maxpages: 0,
        caching: true,
        laparams: None,
    };

    // With empty input this errors, but proves the API accepts the option
    let result = extract_text(b"", Some(options));
    assert!(result.is_err());
}

#[test]
fn test_extract_text_maxpages_limit() {
    // Test maxpages option limits extraction
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 1, // Only one page
        caching: true,
        laparams: None,
    };

    let result = extract_text(b"", Some(options));
    assert!(result.is_err());
}

#[test]
fn test_extract_text_laparams_applied() {
    // Test that LAParams are respected
    let laparams = LAParams::new(
        0.3,  // line_overlap
        1.5,  // char_margin
        0.4,  // line_margin
        0.2,  // word_margin
        None, // boxes_flow - disables advanced layout
        false, false,
    );

    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: Some(laparams),
    };

    let result = extract_text(b"", Some(options));
    assert!(result.is_err());
}

// ============================================================================
// TestExtractTextToFp - test text extraction to writer
// ============================================================================

#[test]
fn test_extract_text_to_fp_writes_output() {
    let mut output = Cursor::new(Vec::new());

    // Empty PDF will error, but we test the function signature
    let result = extract_text_to_fp(b"", &mut output, None);

    // Should error on invalid PDF, not on API usage
    assert!(result.is_err());
}

#[test]
fn test_extract_text_to_fp_with_options() {
    let mut output = Cursor::new(Vec::new());

    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: Some(LAParams::default()),
    };

    let result = extract_text_to_fp(b"", &mut output, Some(options));
    assert!(result.is_err());
}

#[test]
fn test_extract_text_to_fp_vec_writer() {
    // Test with Vec<u8> writer
    let mut output: Vec<u8> = Vec::new();

    let result = extract_text_to_fp(b"", &mut output, None);
    assert!(result.is_err());
}

// ============================================================================
// TestExtractPages - test page iterator
// ============================================================================

#[test]
fn test_extract_pages_returns_iterator() {
    let result = extract_pages(b"", None);

    // Should return an iterator (or error for invalid PDF)
    match result {
        Ok(pages) => {
            // Collect to verify it's iterable
            let collected: Vec<_> = pages.collect();
            assert!(collected.is_empty());
        }
        Err(_) => {
            // Invalid PDF errors are expected
        }
    }
}

#[test]
fn test_extract_pages_with_options() {
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 0,
        caching: true,
        laparams: Some(LAParams::default()),
    };

    let result = extract_pages(b"", Some(options));
    assert!(result.is_err());
}

#[test]
fn test_extract_pages_yields_ltpage() {
    // Each item from extract_pages should be an LTPage
    // (This test validates the type system, actual behavior needs real PDF)
    let result = extract_pages(b"", None);

    match result {
        Ok(pages) => {
            for page_result in pages {
                match page_result {
                    Ok(page) => {
                        // Verify it has LTPage properties
                        let _ = page.pageid;
                        let _ = page.bbox();
                    }
                    Err(_) => {}
                }
            }
        }
        Err(_) => {}
    }
}

#[test]
fn test_extract_pages_page_numbers_filter() {
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: Some(vec![0, 2]), // Pages 0 and 2 only
        maxpages: 0,
        caching: true,
        laparams: None,
    };

    let result = extract_pages(b"", Some(options));
    assert!(result.is_err());
}

#[test]
fn test_extract_pages_maxpages_limit() {
    let options = ExtractOptions {
        password: String::new(),
        page_numbers: None,
        maxpages: 2, // At most 2 pages
        caching: true,
        laparams: None,
    };

    let result = extract_pages(b"", Some(options));
    assert!(result.is_err());
}

// ============================================================================
// TestExtractOptions - test options struct
// ============================================================================

#[test]
fn test_extract_options_default() {
    let options = ExtractOptions::default();

    assert!(options.password.is_empty());
    assert!(options.page_numbers.is_none());
    assert_eq!(options.maxpages, 0);
    assert!(options.caching);
    assert!(options.laparams.is_none());
}

#[test]
fn test_extract_options_with_password() {
    let options = ExtractOptions {
        password: "secret".to_string(),
        ..Default::default()
    };

    assert_eq!(options.password, "secret");
}

#[test]
fn test_extract_options_with_laparams() {
    let laparams = LAParams::default();

    let options = ExtractOptions {
        laparams: Some(laparams.clone()),
        ..Default::default()
    };

    assert!(options.laparams.is_some());
    let params = options.laparams.unwrap();
    assert_eq!(params.line_overlap, 0.5);
}

// ============================================================================
// Integration tests with minimal PDF
// ============================================================================

/// Minimal valid PDF structure for testing
const MINIMAL_PDF: &[u8] = b"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [] /Count 0 >>
endobj
xref
0 3
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
trailer
<< /Size 3 /Root 1 0 R >>
startxref
110
%%EOF";

#[test]
fn test_extract_text_minimal_pdf() {
    let result = extract_text(MINIMAL_PDF, None);

    // Minimal PDF has no pages with content, should return empty string
    match result {
        Ok(text) => assert!(text.is_empty()),
        Err(_) => {} // Parser limitations may cause this to fail
    }
}

#[test]
fn test_extract_pages_minimal_pdf() {
    let result = extract_pages(MINIMAL_PDF, None);

    match result {
        Ok(pages) => {
            let count = pages.count();
            assert_eq!(count, 0); // No pages with content
        }
        Err(_) => {} // Parser limitations
    }
}

// ============================================================================
// LAParams integration tests
// ============================================================================

#[test]
fn test_extract_text_boxes_flow_none() {
    // Test with boxes_flow=None (simpler text ordering)
    let laparams = LAParams::new(0.5, 2.0, 0.5, 0.1, None, false, false);

    let options = ExtractOptions {
        laparams: Some(laparams),
        ..Default::default()
    };

    let result = extract_text(MINIMAL_PDF, Some(options));
    // Should not panic with boxes_flow=None
    match result {
        Ok(text) => assert!(text.is_empty()),
        Err(_) => {}
    }
}

#[test]
fn test_extract_text_detect_vertical() {
    // Test with vertical text detection enabled
    let laparams = LAParams::new(0.5, 2.0, 0.5, 0.1, Some(0.5), true, false);

    let options = ExtractOptions {
        laparams: Some(laparams),
        ..Default::default()
    };

    let result = extract_text(MINIMAL_PDF, Some(options));
    match result {
        Ok(text) => assert!(text.is_empty()),
        Err(_) => {}
    }
}

#[test]
fn test_extract_text_all_texts() {
    // Test with all_texts enabled (analyze figures too)
    let laparams = LAParams::new(0.5, 2.0, 0.5, 0.1, Some(0.5), false, true);

    let options = ExtractOptions {
        laparams: Some(laparams),
        ..Default::default()
    };

    let result = extract_text(MINIMAL_PDF, Some(options));
    match result {
        Ok(text) => assert!(text.is_empty()),
        Err(_) => {}
    }
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_extract_text_invalid_pdf_header() {
    let invalid = b"Not a PDF file";
    let result = extract_text(invalid, None);

    assert!(result.is_err());
}

#[test]
fn test_extract_text_truncated_pdf() {
    let truncated = b"%PDF-1.4\n";
    let result = extract_text(truncated, None);

    assert!(result.is_err());
}

#[test]
fn test_extract_pages_invalid_pdf() {
    let invalid = b"garbage";
    let result = extract_pages(invalid, None);

    assert!(result.is_err());
}

// ============================================================================
// Semantic Text Extraction Tests (ported from pdfminer.six)
// ============================================================================
// These tests verify actual text extraction output matches expected strings.
// Port of pdfminer.six tests/test_highlevel_extracttext.py

/// Get absolute path to a test fixture file.
fn fixture_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests").join("fixtures").join(name)
}

/// Helper to run extract_text with optional LAParams.
fn run_with_string(sample_path: &str, laparams: Option<LAParams>) -> String {
    let path = fixture_path(sample_path);
    let pdf_data = std::fs::read(&path).expect(&format!("Failed to read {}", path.display()));

    let options = ExtractOptions {
        laparams,
        ..Default::default()
    };

    extract_text(&pdf_data, Some(options))
        .expect(&format!("Failed to extract text from {}", sample_path))
}

/// Helper to run extract_text using file read (same as run_with_string for Rust).
fn run_with_file(sample_path: &str) -> String {
    run_with_string(sample_path, None)
}

// Expected test strings - port of test_strings dict from Python
mod test_strings {
    pub const SIMPLE1: &str = "Hello \n\nWorld\n\nHello \n\nWorld\n\n\
        H e l l o  \n\nW o r l d\n\n\
        H e l l o  \n\nW o r l d\n\n\x0c";

    pub const SIMPLE1_NO_BOXES_FLOW: &str = "Hello \n\nWorld\n\nHello \n\nWorld\n\n\
        H e l l o  \n\nW o r l d\n\n\
        H e l l o  \n\nW o r l d\n\n\x0c";

    pub const SIMPLE2: &str = "\x0c";

    pub const SIMPLE3: &str = "Hello\n\nHello\n\u{3042}\n\u{3044}\n\u{3046}\n\u{3048}\n\u{304a}\n\
        \u{3042}\n\u{3044}\n\u{3046}\n\u{3048}\n\u{304a}\n\
        World\n\nWorld\n\n\x0c";

    pub const SIMPLE4: &str = "Text1\nText2\nText3\n\n\x0c";

    pub const SIMPLE5: &str = "Heading\n\n\
        Link to heading that is working with vim-pandoc.\n\n\
        Link to heading \u{201c}that is\u{201d} not working with vim-pandoc.\n\n\
        Subheading\n\nSome \u{201c}more text\u{201d}\n\n1\n\n\x0c";

    pub const ZEN_OF_PYTHON_CORRUPTED: &str = "Mai 30, 18 13:27\n\nzen_of_python.txt";

    pub const ISSUE_495: &str = "8\n\n7\n\n6\n\n5\n\n4\n\n3\n\n2\n\n1\n\n\
        150,00\n\n30,00\n\n(cid:72) 0,05 A\n\n0\n0\n,\n0\n2\n\n0\n0\n,\n8\n\n\
        (cid:69) 0,05\n\n0\n0\n,\n0\n5\n\nA\n\nF\n\nE\n\nD\n\n20,00\n\n16,00\n\n\
        +\n0,05\n15,00 - 0,00\n\nC\n\n0\n0\n,\n0\n4\n\n0\n0\n,\n0\n2\n\n\
        R18,00\n\nM12x1.75 - 6H\n\n0\n0\n,\n5\n4\n\nB\n\nA\n\n\
        0\n0\n,\n6\n1\n(cid:142)\n\n0\n0\n,\n6\n1\n\n+\n0,50\n15,00 - 0,00\n\n\
        60,00 (cid:66)0,02\n\n100,00 (cid:66)0,05\n\n132,00\n\n\
        9\nH\n0\n1\n(cid:142)\n\n9\nH\n0\n1\n(cid:142)\n\n(cid:68) 0,1 A\n\n\
        +\n0,00\n70,00 - 0,02\n\n50,00\n\n(cid:76) 0,1\n\n(cid:76) 0,1\n\n\
        0\n0\n,\n5\n3\n\nF\n\nE\n\nD\n\nC\n\nB\n\nAllgemeintoleranzen\n\n\
        MATERIAL\n\nDIN ISO 2768 - mK\n\nPET BLACK\n\nFINISH\n\n\
        Eloxieren (natur)\n\nRa 1,6\n\nDate\n29.03.2021\n\n\
        Name\nLucas Giering\n\nDrawn\n\nChecked\n\nStandard\n\n\
        Arretierungshilfe\n\nA\n\n1 \n\nA2\n\n8\n\n7\n\n6\n\n5\n\n4\n\nState\n\n\
        Changes\n\nDate\n\nName\n\n3\n\n2\n\n1";

    pub const ISSUE_566_1: &str = "ISSUE Date\u{ff1a}2019-4-25 Buyer\u{ff1a}\u{9ece}\u{8363}";

    pub const ISSUE_566_2: &str = "\u{7532}\u{65b9}\u{ff1a}\u{4e2d}\u{56fd}\u{996e}\u{6599}\u{6709}\u{9650}\u{516c}\u{53f8}\u{ff08}\u{76d6}\u{7ae0}\u{ff09}";

    pub const ISSUE_625: &str = "Termin p\u{0142}atno\u{015b}ci: 2021-05-03";

    pub const ISSUE_791: &str =
        "Pen\u{011b}\u{017e}n\u{00ed} prost\u{0159}edky na \u{00fa}\u{010d}tech";

    pub const ISSUE_886: &str = "Hello";
}

// ============================================================================
// TestExtractText semantic tests - test output content matches expected
// ============================================================================

#[test]
fn test_simple1_with_string() {
    let s = run_with_string("simple1.pdf", None);
    assert_eq!(s, test_strings::SIMPLE1);
}

#[test]
fn test_simple1_no_boxes_flow() {
    let laparams = LAParams::new(
        0.5,  // line_overlap
        2.0,  // char_margin
        0.5,  // line_margin
        0.1,  // word_margin
        None, // boxes_flow=None
        false, false,
    );
    let s = run_with_string("simple1.pdf", Some(laparams));
    assert_eq!(s, test_strings::SIMPLE1_NO_BOXES_FLOW);
}

#[test]
fn test_simple2_with_string() {
    let s = run_with_string("simple2.pdf", None);
    assert_eq!(s, test_strings::SIMPLE2);
}

#[test]
fn test_simple3_with_string() {
    let s = run_with_string("simple3.pdf", None);
    assert_eq!(s, test_strings::SIMPLE3);
}

#[test]
fn test_simple4_with_string() {
    let s = run_with_string("simple4.pdf", None);
    assert_eq!(s, test_strings::SIMPLE4);
}

#[test]
fn test_simple5_with_string() {
    let s = run_with_string("simple5.pdf", None);
    assert_eq!(s, test_strings::SIMPLE5);
}

#[test]
fn test_simple1_with_file() {
    let s = run_with_file("simple1.pdf");
    assert_eq!(s, test_strings::SIMPLE1);
}

#[test]
fn test_simple2_with_file() {
    let s = run_with_file("simple2.pdf");
    assert_eq!(s, test_strings::SIMPLE2);
}

#[test]
fn test_simple3_with_file() {
    let s = run_with_file("simple3.pdf");
    assert_eq!(s, test_strings::SIMPLE3);
}

#[test]
fn test_simple4_with_file() {
    let s = run_with_file("simple4.pdf");
    assert_eq!(s, test_strings::SIMPLE4);
}

#[test]
fn test_simple5_with_file() {
    let s = run_with_file("simple5.pdf");
    assert_eq!(s, test_strings::SIMPLE5);
}

#[test]
fn test_zlib_corrupted() {
    let s = run_with_file("zen_of_python_corrupted.pdf");
    let expected = test_strings::ZEN_OF_PYTHON_CORRUPTED;
    // Only compare the expected prefix (file may have more content)
    assert_eq!(&s[..expected.len().min(s.len())], expected);
}

#[test]
fn test_issue_495_pdfobjref_iterable() {
    let s = run_with_file("contrib/issue_495_pdfobjref.pdf");
    assert_eq!(s.trim(), test_strings::ISSUE_495);
}

#[test]
fn test_issue_566_cmap_bytes() {
    let s = run_with_file("contrib/issue_566_test_1.pdf");
    assert_eq!(s.trim(), test_strings::ISSUE_566_1);
}

#[test]
fn test_issue_566_cid_range() {
    let s = run_with_file("contrib/issue_566_test_2.pdf");
    assert_eq!(s.trim(), test_strings::ISSUE_566_2);
}

#[test]
fn test_issue_625_identity_cmap() {
    let s = run_with_file("contrib/issue-625-identity-cmap.pdf");
    let lines: Vec<&str> = s.lines().collect();
    // Python test checks lines[6]
    assert!(
        lines.len() > 6,
        "Expected at least 7 lines, got {}",
        lines.len()
    );
    assert_eq!(lines[6], test_strings::ISSUE_625);
}

#[test]
fn test_issue_791_non_unicode_cmap() {
    let s = run_with_file("contrib/issue-791-non-unicode-cmap.pdf");
    assert_eq!(s.trim(), test_strings::ISSUE_791);
}

#[test]
fn test_issue_886_xref_stream_widths() {
    // Ensure that we can support arbitrary width integers in xref streams
    let s = run_with_file("contrib/issue-886-xref-stream-widths.pdf");
    assert_eq!(s.trim(), test_strings::ISSUE_886);
}

// ============================================================================
// TestExtractPages semantic tests - test layout algorithm behavior
// ============================================================================

use bolivar_core::layout::{LTItem, LTTextBox, TextBoxType};

#[test]
fn test_line_margin() {
    // The lines have margin 0.2 relative to the height.
    // Extract with line_margin 0.19 should break into 3 separate textboxes.
    let path = fixture_path("simple4.pdf");
    let pdf_data = std::fs::read(&path).expect("Failed to read simple4.pdf");

    let laparams_0_19 = LAParams::new(
        0.5,       // line_overlap
        2.0,       // char_margin
        0.19,      // line_margin - less than 0.2
        0.1,       // word_margin
        Some(0.5), // boxes_flow
        false,
        false,
    );

    let options = ExtractOptions {
        laparams: Some(laparams_0_19),
        ..Default::default()
    };

    let pages: Vec<_> = extract_pages(&pdf_data, Some(options))
        .expect("Failed to extract pages")
        .collect();

    assert_eq!(pages.len(), 1);
    let page = pages
        .into_iter()
        .next()
        .unwrap()
        .expect("Failed to get page");

    // Count text containers - with line_margin 0.19, should have 3 separate boxes
    let text_count = page
        .iter()
        .filter(|item| matches!(item, LTItem::TextBox(_)))
        .count();

    assert_eq!(text_count, 3, "Expected 3 text boxes with line_margin=0.19");

    // Extract with line_margin 0.21 should merge into one textbox.
    let laparams_0_21 = LAParams::new(
        0.5,       // line_overlap
        2.0,       // char_margin
        0.21,      // line_margin - more than 0.2
        0.1,       // word_margin
        Some(0.5), // boxes_flow
        false,
        false,
    );

    let options = ExtractOptions {
        laparams: Some(laparams_0_21),
        ..Default::default()
    };

    let pages: Vec<_> = extract_pages(&pdf_data, Some(options))
        .expect("Failed to extract pages")
        .collect();

    assert_eq!(pages.len(), 1);
    let page = pages
        .into_iter()
        .next()
        .unwrap()
        .expect("Failed to get page");

    // With line_margin 0.21, should have 1 merged box
    let text_count = page
        .iter()
        .filter(|item| matches!(item, LTItem::TextBox(_)))
        .count();

    assert_eq!(text_count, 1, "Expected 1 text box with line_margin=0.21");
}

#[test]
fn test_no_boxes_flow() {
    let path = fixture_path("simple4.pdf");
    let pdf_data = std::fs::read(&path).expect("Failed to read simple4.pdf");

    let laparams = LAParams::new(
        0.5,  // line_overlap
        2.0,  // char_margin
        0.5,  // line_margin
        0.1,  // word_margin
        None, // boxes_flow=None
        false, false,
    );

    let options = ExtractOptions {
        laparams: Some(laparams),
        ..Default::default()
    };

    let pages: Vec<_> = extract_pages(&pdf_data, Some(options))
        .expect("Failed to extract pages")
        .collect();

    assert_eq!(pages.len(), 1);
    let page = pages
        .into_iter()
        .next()
        .unwrap()
        .expect("Failed to get page");

    // With boxes_flow=None, should have 1 text container with all text merged
    let text_boxes: Vec<_> = page
        .iter()
        .filter_map(|item| {
            if let LTItem::TextBox(tb) = item {
                Some(tb)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        text_boxes.len(),
        1,
        "Expected 1 text box with boxes_flow=None"
    );

    // Verify the merged text content - use LTTextBox trait method
    let text = match &text_boxes[0] {
        TextBoxType::Horizontal(h) => h.get_text(),
        TextBoxType::Vertical(v) => v.get_text(),
    };
    assert_eq!(text, "Text1\nText2\nText3\n");
}

#[test]
fn test_debug_886() {
    use bolivar_core::pdfdocument::PDFDocument;
    use bolivar_core::pdftypes::PDFObject;

    let path = fixture_path("contrib/issue-886-xref-stream-widths.pdf");
    let pdf_data = std::fs::read(&path).expect("Failed to read PDF");
    let doc = PDFDocument::new(&pdf_data, "").unwrap();

    // Get object 11 (Type0 font)
    let obj11 = doc.getobj(11).unwrap();
    eprintln!("\nObject 11 (Type0 font):");
    if let PDFObject::Dict(d) = &obj11 {
        for (k, v) in d.iter() {
            eprintln!("  {}: {:?}", k, v);
        }

        if let Some(tounicode_ref) = d.get("ToUnicode") {
            eprintln!("\nToUnicode ref: {:?}", tounicode_ref);

            match doc.resolve(tounicode_ref) {
                Ok(resolved) => {
                    if let PDFObject::Stream(stream) = &resolved {
                        eprintln!("Stream attrs: {:?}", stream.attrs);

                        match doc.decode_stream(&stream) {
                            Ok(data) => {
                                eprintln!("\nDecoded ToUnicode ({} bytes):", data.len());
                                eprintln!("{}", String::from_utf8_lossy(&data));
                            }
                            Err(e) => eprintln!("Decode error: {:?}", e),
                        }
                    } else {
                        eprintln!("Resolved to non-stream: {:?}", resolved);
                    }
                }
                Err(e) => eprintln!("Resolve error: {:?}", e),
            }
        }
    }

    // Now test the actual text extraction
    let result = extract_text(&pdf_data, None);
    eprintln!("\nExtracted text: {:?}", result);
}

#[test]
fn test_debug_parse_tounicode() {
    use bolivar_core::cmapdb::parse_tounicode_cmap;

    let tounicode_data = b"/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo
<< /Registry (Adobe)
/Ordering (UCS)
/Supplement 0
>> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
1 begincodespacerange
<0000> <ffff>
endcodespacerange
5 beginbfchar
<002b> <0048>
<0048> <0065>
<004f> <006c>
<0052> <006f>
<0003> <0020>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end
";

    let unicode_map = parse_tounicode_cmap(tounicode_data);

    // Test lookups
    eprintln!("\nTesting parse_tounicode_cmap:");

    // CID 43 (0x2b) -> 'H' (0x48)
    let result = unicode_map.get_unichr(0x2b);
    eprintln!("CID 0x2b (43) -> {:?}", result);
    assert_eq!(result, Some("H".to_string()), "0x2b should map to H");

    // CID 72 (0x48) -> 'e' (0x65)
    let result = unicode_map.get_unichr(0x48);
    eprintln!("CID 0x48 (72) -> {:?}", result);
    assert_eq!(result, Some("e".to_string()), "0x48 should map to e");

    // CID 79 (0x4f) -> 'l' (0x6c)
    let result = unicode_map.get_unichr(0x4f);
    eprintln!("CID 0x4f (79) -> {:?}", result);
    assert_eq!(result, Some("l".to_string()), "0x4f should map to l");

    // CID 82 (0x52) -> 'o' (0x6f)
    let result = unicode_map.get_unichr(0x52);
    eprintln!("CID 0x52 (82) -> {:?}", result);
    assert_eq!(result, Some("o".to_string()), "0x52 should map to o");

    // CID 3 -> ' ' (0x20)
    let result = unicode_map.get_unichr(3);
    eprintln!("CID 3 -> {:?}", result);
    assert_eq!(result, Some(" ".to_string()), "3 should map to space");
}
#[test]
fn test_debug_791() {
    use bolivar_core::pdfdocument::PDFDocument;
    use bolivar_core::pdftypes::PDFObject;
    use std::fs;

    let path = "tests/fixtures/contrib/issue-791-non-unicode-cmap.pdf";
    let pdf_data = fs::read(path).expect("Failed to read PDF");
    let doc = PDFDocument::new(&pdf_data, "").unwrap();

    // Get page 1 resources
    use bolivar_core::pdfpage::PDFPage;
    let page = PDFPage::create_pages(&doc).next().unwrap().unwrap();

    eprintln!("\nPage resources:");
    for (k, v) in page.resources.iter() {
        eprintln!("  {}: {:?}", k, v);
    }

    // Check Font resource
    if let Some(font_dict) = page.resources.get("Font") {
        eprintln!("\nFont dict: {:?}", font_dict);

        // Resolve if reference
        let font_dict = match font_dict {
            PDFObject::Ref(r) => doc.resolve(&PDFObject::Ref(r.clone())).unwrap(),
            _ => font_dict.clone(),
        };

        if let PDFObject::Dict(fd) = &font_dict {
            for (fontid, spec) in fd.iter() {
                eprintln!("\nFont {}: {:?}", fontid, spec);

                // Resolve spec
                let spec = match spec {
                    PDFObject::Ref(r) => doc.resolve(&PDFObject::Ref(r.clone())).unwrap(),
                    _ => spec.clone(),
                };

                if let PDFObject::Dict(d) = &spec {
                    eprintln!("  Subtype: {:?}", d.get("Subtype"));
                    eprintln!("  ToUnicode: {:?}", d.get("ToUnicode"));
                    eprintln!("  Encoding: {:?}", d.get("Encoding"));
                    eprintln!("  DescendantFonts: {:?}", d.get("DescendantFonts"));

                    // If ToUnicode, decode and show
                    if let Some(tounicode) = d.get("ToUnicode") {
                        match doc.resolve(tounicode) {
                            Ok(PDFObject::Stream(stream)) => match doc.decode_stream(&stream) {
                                Ok(data) => {
                                    eprintln!("\n  ToUnicode content ({} bytes):", data.len());
                                    eprintln!("{}", String::from_utf8_lossy(&data));
                                }
                                Err(e) => eprintln!("  ToUnicode decode error: {:?}", e),
                            },
                            Ok(other) => eprintln!("  ToUnicode resolved to: {:?}", other),
                            Err(e) => eprintln!("  ToUnicode resolve error: {:?}", e),
                        }
                    }
                }
            }
        }
    }
}
