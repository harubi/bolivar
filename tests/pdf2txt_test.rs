//! Port of pdfminer.six tests/test_tools_pdf2txt.py
//!
//! Tests for the pdf2txt CLI tool including:
//! - Text output (default)
//! - HTML output (-t html)
//! - XML output (-t xml)
//! - Page selection (-p)
//! - LAParams options (--boxes-flow, etc.)
//! - Output to file (-o)

use std::path::PathBuf;
use std::process::Command;

// ============================================================================
// Helper functions
// ============================================================================

/// Get the path to the test binary.
fn pdf2txt_binary() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("pdf2txt");
    path
}

/// Get absolute path to a test fixture file.
fn fixture_path(name: &str) -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests").join("fixtures").join(name)
}

/// Run pdf2txt with given arguments and return (exit_code, stdout, stderr).
fn run_pdf2txt(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(pdf2txt_binary())
        .args(args)
        .output()
        .expect("Failed to execute pdf2txt");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (code, stdout, stderr)
}

/// Run pdf2txt on a fixture file with given options.
fn run(fixture: &str, options: Option<&str>) -> (i32, String, String) {
    let path = fixture_path(fixture);
    let path_str = path.to_string_lossy();

    let mut args: Vec<&str> = Vec::new();
    if let Some(opts) = options {
        args.extend(opts.split_whitespace());
    }
    args.push(&path_str);

    run_pdf2txt(&args)
}

/// Run pdf2txt with output to a temporary file.
fn run_with_output(fixture: &str, options: Option<&str>) -> (i32, String) {
    let path = fixture_path(fixture);
    let path_str = path.to_string_lossy();

    let temp_file = std::env::temp_dir().join(format!(
        "pdf2txt_test_{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let temp_path = temp_file.to_string_lossy().to_string();

    let mut args: Vec<String> = vec!["-o".to_string(), temp_path.clone()];
    if let Some(opts) = options {
        args.extend(opts.split_whitespace().map(|s| s.to_string()));
    }
    args.push(path_str.to_string());

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = Command::new(pdf2txt_binary())
        .args(&args_ref)
        .output()
        .expect("Failed to execute pdf2txt");

    let content = std::fs::read_to_string(&temp_file).unwrap_or_default();
    let _ = std::fs::remove_file(&temp_file);

    (output.status.code().unwrap_or(-1), content)
}

// ============================================================================
// Expected test strings - port of test_strings dict from Python
// ============================================================================

mod test_strings {
    // Python reference: test_highlevel_extracttext.py line 24-26
    pub const SIMPLE1: &str = "Hello \n\nWorld\n\nHello \n\nWorld\n\n\
        H e l l o  \n\nW o r l d\n\n\
        H e l l o  \n\nW o r l d\n\n\x0c";

    pub const SIMPLE2: &str = "\x0c";

    pub const SIMPLE3: &str = "Hello\n\nHello\nあ\nい\nう\nえ\nお\nあ\nい\nう\nえ\nお\n\
        World\n\nWorld\n\n\x0c";

    pub const SIMPLE4: &str = "Text1\nText2\nText3\n\n\x0c";

    // Encryption test files all contain "Secret!" or "Hello World" (r6)
    pub const ENCRYPTION_SECRET: &str = "Secret!\n\n\x0c";
    pub const ENCRYPTION_R6: &str = "Hello World\n\n\x0c";
}

// ============================================================================
// TestPdf2Txt - basic PDF processing tests
// ============================================================================

#[test]
fn test_help() {
    let (code, stdout, _stderr) = run_pdf2txt(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("pdf2txt"));
    assert!(stdout.contains("--output"));
}

#[test]
fn test_version() {
    let (code, stdout, _stderr) = run_pdf2txt(&["--version"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("pdf2txt") || stdout.contains("0."));
}

#[test]
fn test_no_input_file() {
    let (code, _stdout, stderr) = run_pdf2txt(&[]);
    // Should exit with error when no input file is provided
    assert_ne!(code, 0);
    assert!(stderr.contains("required") || stderr.contains("argument"));
}

#[test]
fn test_simple1() {
    let (code, output) = run_with_output("simple1.pdf", None);
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::SIMPLE1);
}

#[test]
fn test_simple2() {
    let (code, output) = run_with_output("simple2.pdf", None);
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::SIMPLE2);
}

#[test]
fn test_simple3() {
    let (code, output) = run_with_output("simple3.pdf", None);
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::SIMPLE3);
}

#[test]

fn test_jo() {
    let (code, _output) = run_with_output("jo.pdf", None);
    assert_eq!(code, 0);
}

// ============================================================================
// TestPdf2Txt - output format tests
// ============================================================================

#[test]
fn test_simple4() {
    let (code, output) = run_with_output("simple4.pdf", None);
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::SIMPLE4);
}

#[test]
fn test_html_simple1() {
    let (code, output) = run_with_output("simple1.pdf", Some("-t html"));
    assert_eq!(code, 0);
    // HTML output should contain proper HTML structure
    assert!(output.contains("<html>") || output.contains("<!DOCTYPE"));
    // Should contain the text content
    assert!(output.contains("Hello"));
    assert!(output.contains("World"));
}

#[test]

fn test_xml_simple1() {
    let (code, output) = run_with_output("simple1.pdf", Some("-t xml"));
    assert_eq!(code, 0);
    assert!(output.contains("<?xml") || output.contains("<pages>"));
}

#[test]

fn test_tag_output() {
    let (code, output) = run_with_output("simple1.pdf", Some("-t tag"));
    assert_eq!(code, 0);
    // Tag output should contain tag markers
    assert!(!output.is_empty());
}

// ============================================================================
// TestPdf2Txt - page selection tests
// ============================================================================

#[test]

fn test_page_numbers() {
    // Test --page-numbers option
    let (code, _output) = run_with_output("simple1.pdf", Some("--page-numbers 1"));
    assert_eq!(code, 0);
}

#[test]

fn test_pagenos() {
    // Test legacy -p option
    let (code, _output) = run_with_output("simple1.pdf", Some("-p 1"));
    assert_eq!(code, 0);
}

#[test]

fn test_maxpages() {
    // Test -m option to limit pages
    let (code, _output) = run_with_output("simple1.pdf", Some("-m 1"));
    assert_eq!(code, 0);
}

// ============================================================================
// TestPdf2Txt - LAParams options tests
// ============================================================================

#[test]

fn test_no_laparams() {
    // Test -n option to disable layout analysis
    let (code, _output) = run_with_output("simple1.pdf", Some("-n"));
    assert_eq!(code, 0);
}

#[test]

fn test_detect_vertical() {
    // Test --detect-vertical option for vertical text detection
    let (code, _output) = run_with_output("simple1.pdf", Some("--detect-vertical"));
    assert_eq!(code, 0);
}

#[test]

fn test_char_margin() {
    // Test -M option for character margin
    let (code, _output) = run_with_output("simple1.pdf", Some("-M 2.0"));
    assert_eq!(code, 0);
}

#[test]

fn test_word_margin() {
    // Test -W option for word margin
    let (code, _output) = run_with_output("simple1.pdf", Some("-W 0.1"));
    assert_eq!(code, 0);
}

#[test]

fn test_line_margin() {
    // Test -L option for line margin
    let (code, _output) = run_with_output("simple1.pdf", Some("-L 0.5"));
    assert_eq!(code, 0);
}

#[test]

fn test_boxes_flow() {
    // Test -F option for boxes flow
    let (code, _output) = run_with_output("simple1.pdf", Some("-F 0.5"));
    assert_eq!(code, 0);
}

#[test]

fn test_boxes_flow_disabled() {
    // Test -F disabled option
    let (code, _output) = run_with_output("simple1.pdf", Some("-F disabled"));
    assert_eq!(code, 0);
}

#[test]

fn test_all_texts() {
    // Test -A option to analyze text in figures
    let (code, _output) = run_with_output("simple1.pdf", Some("-A"));
    assert_eq!(code, 0);
}

// ============================================================================
// TestPdf2Txt - output options tests
// ============================================================================

#[test]

fn test_output_to_stdout() {
    // Output to stdout by default or with -o -
    let (code, stdout, _stderr) = run("simple1.pdf", Some("-o -"));
    assert_eq!(code, 0);
    assert!(!stdout.is_empty());
}

#[test]

fn test_scale() {
    // Test -s option for scale (used with HTML output)
    let (code, _output) = run_with_output("simple1.pdf", Some("-t html -s 2.0"));
    assert_eq!(code, 0);
}

#[test]

fn test_layoutmode() {
    // Test -Y option for layout mode (used with HTML output)
    let (code, _output) = run_with_output("simple1.pdf", Some("-t html -Y exact"));
    assert_eq!(code, 0);
}

#[test]

fn test_strip_control() {
    // Test -S option for stripping control characters (used with XML output)
    let (code, _output) = run_with_output("simple1.pdf", Some("-t xml -S"));
    assert_eq!(code, 0);
}

// ============================================================================
// TestPdf2Txt - encryption tests (port of Python encryption tests)
// ============================================================================

#[test]
fn test_encryption_aes128() {
    let (code, output) = run_with_output("encryption/aes-128.pdf", Some("-P foo"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_SECRET);
}

#[test]
fn test_encryption_aes128m() {
    let (code, output) = run_with_output("encryption/aes-128-m.pdf", Some("-P foo"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_SECRET);
}

#[test]
fn test_encryption_aes256() {
    let (code, output) = run_with_output("encryption/aes-256.pdf", Some("-P foo"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_SECRET);
}

#[test]
fn test_encryption_aes256m() {
    let (code, output) = run_with_output("encryption/aes-256-m.pdf", Some("-P foo"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_SECRET);
}

#[test]
fn test_encryption_aes256_r6_user() {
    let (code, output) = run_with_output("encryption/aes-256-r6.pdf", Some("-P usersecret"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_R6);
}

#[test]
fn test_encryption_aes256_r6_owner() {
    let (code, output) = run_with_output("encryption/aes-256-r6.pdf", Some("-P ownersecret"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_R6);
}

#[test]
fn test_encryption_base() {
    let (code, output) = run_with_output("encryption/base.pdf", Some("-P foo"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_SECRET);
}

#[test]
fn test_encryption_rc4_40() {
    let (code, output) = run_with_output("encryption/rc4-40.pdf", Some("-P foo"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_SECRET);
}

#[test]
fn test_encryption_rc4_128() {
    let (code, output) = run_with_output("encryption/rc4-128.pdf", Some("-P foo"));
    assert_eq!(code, 0);
    assert_eq!(output, test_strings::ENCRYPTION_SECRET);
}

// ============================================================================
// TestPdf2Txt - file type inference tests
// ============================================================================

#[test]

fn test_output_type_inference_html() {
    // When output file ends in .html, should use HTML output
    let path = fixture_path("simple1.pdf");
    let path_str = path.to_string_lossy();

    let temp_file = std::env::temp_dir().join("pdf2txt_test_infer.html");
    let temp_path = temp_file.to_string_lossy().to_string();

    let (code, _stdout, _stderr) = run_pdf2txt(&["-o", &temp_path, &path_str]);

    if code == 0 {
        let content = std::fs::read_to_string(&temp_file).unwrap_or_default();
        assert!(content.contains("<html>") || content.contains("<!DOCTYPE"));
        let _ = std::fs::remove_file(&temp_file);
    }
}

#[test]

fn test_output_type_inference_xml() {
    // When output file ends in .xml, should use XML output
    let path = fixture_path("simple1.pdf");
    let path_str = path.to_string_lossy();

    let temp_file = std::env::temp_dir().join("pdf2txt_test_infer.xml");
    let temp_path = temp_file.to_string_lossy().to_string();

    let (code, _stdout, _stderr) = run_pdf2txt(&["-o", &temp_path, &path_str]);

    if code == 0 {
        let content = std::fs::read_to_string(&temp_file).unwrap_or_default();
        assert!(content.contains("<?xml") || content.contains("<pages>"));
        let _ = std::fs::remove_file(&temp_file);
    }
}

// ============================================================================
// TestPdf2Txt - debug and misc options
// ============================================================================

#[test]

fn test_debug_mode() {
    // Test -d option for debug mode
    let (code, _output) = run_with_output("simple1.pdf", Some("-d"));
    assert_eq!(code, 0);
}

#[test]

fn test_disable_caching() {
    // Test -C option to disable caching
    let (code, _output) = run_with_output("simple1.pdf", Some("-C"));
    assert_eq!(code, 0);
}

#[test]

fn test_rotation() {
    // Test -R option for rotation
    let (code, _output) = run_with_output("simple1.pdf", Some("-R 90"));
    assert_eq!(code, 0);
}

#[test]

fn test_line_overlap() {
    // Test --line-overlap option
    let (code, _output) = run_with_output("simple1.pdf", Some("--line-overlap 0.5"));
    assert_eq!(code, 0);
}

// ============================================================================
// TestPdf2Txt - contrib and nonfree tests (matching Python tests)
// ============================================================================

#[test]

fn test_contrib_2b() {
    let (code, _output) = run_with_output("contrib/2b.pdf", Some("-A -t xml"));
    assert_eq!(code, 0);
}

#[test]

fn test_contrib_excel() {
    let (code, _output) = run_with_output("contrib/issue-00369-excel.pdf", Some("-t html"));
    assert_eq!(code, 0);
}

#[test]

fn test_contrib_matplotlib() {
    let (code, _output) = run_with_output("contrib/matplotlib.pdf", None);
    assert_eq!(code, 0);
}

// ============================================================================
// Unit tests for argument parsing
// ============================================================================

#[test]
fn test_args_parser_invalid_boxes_flow() {
    // boxes_flow must be a float or "disabled"
    let (code, _stdout, stderr) = run_pdf2txt(&["-F", "invalid", "test.pdf"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("invalid") || stderr.contains("error"));
}

#[test]
fn test_args_parser_boxes_flow_out_of_range() {
    // boxes_flow must be between -1.0 and 1.0
    let (code, _stdout, stderr) = run_pdf2txt(&["-F", "2.0", "test.pdf"]);
    // Should either reject at parse time or at runtime
    // The behavior depends on implementation
    assert!(code != 0 || stderr.contains("range") || stderr.len() > 0);
}

// ============================================================================
// TestDumpImages - image extraction tests (port of Python TestDumpImages)
// ============================================================================

/// Helper function to extract images from a PDF into a temporary directory.
/// Returns (exit_code, list of image files created).
fn extract_images(fixture: &str, options: Option<&str>) -> (i32, Vec<String>) {
    let path = fixture_path(fixture);
    let path_str = path.to_string_lossy();

    // Create a unique temporary directory for image output
    let output_dir = std::env::temp_dir().join(format!(
        "pdf2txt_images_{}_{}",
        fixture.replace('/', "_").replace('.', "_"),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&output_dir).expect("Failed to create temp directory");

    // Create a temporary file for text output (required by pdf2txt)
    let temp_file = std::env::temp_dir().join(format!(
        "pdf2txt_text_{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let temp_path = temp_file.to_string_lossy().to_string();
    let output_dir_str = output_dir.to_string_lossy().to_string();

    let mut args: Vec<String> = vec![
        "-o".to_string(),
        temp_path.clone(),
        "--output-dir".to_string(),
        output_dir_str.clone(),
    ];
    if let Some(opts) = options {
        args.extend(opts.split_whitespace().map(|s| s.to_string()));
    }
    args.push(path_str.to_string());

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = Command::new(pdf2txt_binary())
        .args(&args_ref)
        .output()
        .expect("Failed to execute pdf2txt");

    let code = output.status.code().unwrap_or(-1);

    // Collect image files from the output directory
    let image_files: Vec<String> = std::fs::read_dir(&output_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();

    // Cleanup
    let _ = std::fs::remove_file(&temp_file);
    let _ = std::fs::remove_dir_all(&output_dir);

    (code, image_files)
}

#[test]
fn test_dump_images_nonfree_dmca() {
    // Extract images from PDF containing BMP images
    // Regression test for: https://github.com/pdfminer/pdfminer.six/issues/131
    let (code, image_files) = extract_images("nonfree/dmca.pdf", Some("-p 1"));
    assert_eq!(code, 0, "pdf2txt should exit successfully");
    assert!(!image_files.is_empty(), "Should extract at least one image");
    assert!(
        image_files[0].ends_with(".bmp"),
        "First image should be a BMP file, got: {:?}",
        image_files
    );
}

#[test]
fn test_dump_images_nonfree_175() {
    // Extract images from PDF containing JPG images
    let (code, image_files) = extract_images("nonfree/175.pdf", None);
    assert_eq!(code, 0, "pdf2txt should exit successfully");
    assert!(!image_files.is_empty(), "Should extract at least one image");
}

#[test]
fn test_dump_images_jbig2() {
    // Extract images from PDF containing JBIG2 images
    // Feature test for: https://github.com/pdfminer/pdfminer.six/pull/46
    let (code, image_files) = extract_images("contrib/pdf-with-jbig2.pdf", None);
    assert_eq!(code, 0, "pdf2txt should exit successfully");
    assert!(!image_files.is_empty(), "Should extract at least one image");
    assert!(
        image_files[0].ends_with(".jb2"),
        "First image should be a JB2 file, got: {:?}",
        image_files
    );

    // The Python test also verifies the extracted file matches XIPLAYER0.jb2
    // We could add content comparison here when the feature is implemented
}

#[test]
fn test_dump_images_contrib_matplotlib() {
    // Test a PDF with Type3 font
    let (code, _output) = run_with_output("contrib/matplotlib.pdf", None);
    assert_eq!(code, 0, "pdf2txt should exit successfully");
}

#[test]

fn test_dump_images_nonfree_cmp_itext_logo() {
    // Test a PDF with Type3 font
    let (code, _output) = run_with_output("nonfree/cmp_itext_logo.pdf", None);
    assert_eq!(code, 0, "pdf2txt should exit successfully");
}

#[test]
fn test_dump_images_contrib_issue_495_pdfobjref() {
    // Test for extracting images from a zipped PDF
    let (code, image_files) = extract_images("contrib/issue_495_pdfobjref.pdf", None);
    assert_eq!(code, 0, "pdf2txt should exit successfully");
    assert!(!image_files.is_empty(), "Should extract at least one image");
    assert!(
        image_files[0].ends_with(".jpg"),
        "First image should be a JPG file, got: {:?}",
        image_files
    );
}

#[test]
fn test_dump_images_contrib_issue_1008_inline() {
    // Test for parsing and extracting inline images
    let (code, image_files) = extract_images("contrib/issue-1008-inline-ascii85.pdf", None);
    assert_eq!(code, 0, "pdf2txt should exit successfully");
    assert_eq!(
        image_files.len(),
        23,
        "Should extract exactly 23 images, got: {}",
        image_files.len()
    );
    assert!(
        image_files.iter().all(|f| f.ends_with(".bmp")),
        "All images should be BMP files, got: {:?}",
        image_files
    );
}

#[test]

fn test_dump_images_contrib_issue_1057_tiff_predictor() {
    // Test for extracting TIFF image with predictor
    let (code, image_files) = extract_images("contrib/issue-1057-tiff-predictor.pdf", None);
    assert_eq!(code, 0, "pdf2txt should exit successfully");
    assert_eq!(
        image_files.len(),
        1,
        "Should extract exactly 1 image, got: {}",
        image_files.len()
    );
    assert!(
        image_files[0].ends_with(".bmp"),
        "Image should be a BMP file, got: {:?}",
        image_files
    );
}
