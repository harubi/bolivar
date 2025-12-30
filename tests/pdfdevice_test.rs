//! Tests for pdfstate and pdfdevice modules.
//!
//! Tests PDF graphics state, text state, and device trait implementations.

use bolivar::pdfcolor::PREDEFINED_COLORSPACE;
use bolivar::pdfdevice::{
    PDFDevice, PDFFontLike, PDFTextDevice, PDFTextSeq, PDFTextSeqItem, TagExtractor,
};
use bolivar::pdfstate::{Color, PDFGraphicState, PDFTextState};
use bolivar::psparser::PSLiteral;
use bolivar::utils::{MATRIX_IDENTITY, Matrix};
use std::io::Cursor;

/// Mock font for testing PDFTextDevice methods.
struct MockFont {
    vertical: bool,
    multibyte: bool,
}

impl MockFont {
    fn new() -> Self {
        Self {
            vertical: false,
            multibyte: false,
        }
    }
}

impl PDFFontLike for MockFont {
    fn is_vertical(&self) -> bool {
        self.vertical
    }

    fn is_multibyte(&self) -> bool {
        self.multibyte
    }

    fn decode(&self, data: &[u8]) -> Vec<u32> {
        // Simple 1:1 byte-to-CID mapping
        data.iter().map(|&b| b as u32).collect()
    }

    fn to_unichr(&self, cid: u32) -> Option<char> {
        char::from_u32(cid)
    }
}

// =============================================================================
// PDFTextState tests
// =============================================================================

#[test]
fn test_text_state_default() {
    let ts = PDFTextState::new();

    // Default values from Python: fontsize=0, charspace=0, wordspace=0, scaling=100
    assert!(ts.font.is_none());
    assert_eq!(ts.fontsize, 0.0);
    assert_eq!(ts.charspace, 0.0);
    assert_eq!(ts.wordspace, 0.0);
    assert_eq!(ts.scaling, 100.0);
    assert_eq!(ts.leading, 0.0);
    assert_eq!(ts.render, 0);
    assert_eq!(ts.rise, 0.0);

    // reset() sets matrix to identity and linematrix to origin
    assert_eq!(ts.matrix, MATRIX_IDENTITY);
    assert_eq!(ts.linematrix, (0.0, 0.0));
}

#[test]
fn test_text_state_copy() {
    let mut ts = PDFTextState::new();
    ts.fontsize = 12.0;
    ts.charspace = 1.5;
    ts.wordspace = 2.0;
    ts.scaling = 110.0;
    ts.leading = 14.0;
    ts.render = 1;
    ts.rise = 3.0;
    ts.matrix = (1.0, 0.0, 0.0, 1.0, 10.0, 20.0);
    ts.linematrix = (100.0, 200.0);

    let copy = ts.copy();

    assert_eq!(copy.fontsize, 12.0);
    assert_eq!(copy.charspace, 1.5);
    assert_eq!(copy.wordspace, 2.0);
    assert_eq!(copy.scaling, 110.0);
    assert_eq!(copy.leading, 14.0);
    assert_eq!(copy.render, 1);
    assert_eq!(copy.rise, 3.0);
    assert_eq!(copy.matrix, (1.0, 0.0, 0.0, 1.0, 10.0, 20.0));
    assert_eq!(copy.linematrix, (100.0, 200.0));
}

#[test]
fn test_text_state_reset() {
    let mut ts = PDFTextState::new();
    ts.matrix = (2.0, 0.0, 0.0, 2.0, 50.0, 50.0);
    ts.linematrix = (100.0, 200.0);

    ts.reset();

    assert_eq!(ts.matrix, MATRIX_IDENTITY);
    assert_eq!(ts.linematrix, (0.0, 0.0));

    // Other fields remain unchanged
    assert_eq!(ts.fontsize, 0.0);
}

// =============================================================================
// PDFGraphicState tests
// =============================================================================

#[test]
fn test_graphic_state_default() {
    let gs = PDFGraphicState::new();

    assert_eq!(gs.linewidth, 0.0);
    assert!(gs.linecap.is_none());
    assert!(gs.linejoin.is_none());
    assert!(gs.miterlimit.is_none());
    assert!(gs.dash.is_none());
    assert!(gs.intent.is_none());
    assert!(gs.flatness.is_none());

    // Default color is DeviceGray with value 0
    assert_eq!(gs.scolor, Color::Gray(0.0));
    assert_eq!(gs.ncolor, Color::Gray(0.0));

    // Default colorspace is DeviceGray
    assert_eq!(gs.scs.name, "DeviceGray");
    assert_eq!(gs.ncs.name, "DeviceGray");
}

#[test]
fn test_graphic_state_copy() {
    let mut gs = PDFGraphicState::new();
    gs.linewidth = 2.5;
    gs.linecap = Some(1);
    gs.linejoin = Some(2);
    gs.miterlimit = Some(10.0);
    gs.dash = Some((vec![3.0, 5.0], 0.0));
    gs.scolor = Color::Rgb(1.0, 0.0, 0.0);
    gs.ncolor = Color::Cmyk(0.0, 1.0, 1.0, 0.0);

    let copy = gs.copy();

    assert_eq!(copy.linewidth, 2.5);
    assert_eq!(copy.linecap, Some(1));
    assert_eq!(copy.linejoin, Some(2));
    assert_eq!(copy.miterlimit, Some(10.0));
    assert_eq!(copy.dash, Some((vec![3.0, 5.0], 0.0)));
    assert_eq!(copy.scolor, Color::Rgb(1.0, 0.0, 0.0));
    assert_eq!(copy.ncolor, Color::Cmyk(0.0, 1.0, 1.0, 0.0));
}

#[test]
fn test_color_variants() {
    let gray = Color::Gray(0.5);
    let rgb = Color::Rgb(1.0, 0.5, 0.0);
    let cmyk = Color::Cmyk(0.0, 1.0, 1.0, 0.0);

    // Test pattern matching works
    match gray {
        Color::Gray(v) => assert_eq!(v, 0.5),
        _ => panic!("Expected Gray"),
    }

    match rgb {
        Color::Rgb(r, g, b) => {
            assert_eq!(r, 1.0);
            assert_eq!(g, 0.5);
            assert_eq!(b, 0.0);
        }
        _ => panic!("Expected RGB"),
    }

    match cmyk {
        Color::Cmyk(c, m, y, k) => {
            assert_eq!(c, 0.0);
            assert_eq!(m, 1.0);
            assert_eq!(y, 1.0);
            assert_eq!(k, 0.0);
        }
        _ => panic!("Expected CMYK"),
    }
}

// =============================================================================
// PDFDevice trait tests
// =============================================================================

#[test]
fn test_pdf_device_set_ctm() {
    struct TestDevice {
        ctm: Option<Matrix>,
    }

    impl PDFDevice for TestDevice {
        fn set_ctm(&mut self, ctm: Matrix) {
            self.ctm = Some(ctm);
        }

        fn ctm(&self) -> Option<Matrix> {
            self.ctm
        }
    }

    let mut device = TestDevice { ctm: None };
    assert!(device.ctm().is_none());

    let ctm = (2.0, 0.0, 0.0, 2.0, 100.0, 100.0);
    device.set_ctm(ctm);
    assert_eq!(device.ctm(), Some(ctm));
}

#[test]
fn test_pdf_device_default_methods() {
    // Default implementations should do nothing but not panic
    struct MinimalDevice {
        ctm: Option<Matrix>,
    }

    impl PDFDevice for MinimalDevice {
        fn set_ctm(&mut self, ctm: Matrix) {
            self.ctm = Some(ctm);
        }

        fn ctm(&self) -> Option<Matrix> {
            self.ctm
        }
    }

    let mut device = MinimalDevice { ctm: None };

    // These should all be no-ops
    device.begin_tag(&PSLiteral::new("P"), None);
    device.end_tag();
    device.do_tag(&PSLiteral::new("Span"), None);
    device.begin_figure("Fig1", (0.0, 0.0, 100.0, 100.0), MATRIX_IDENTITY);
    device.end_figure("Fig1");
}

// =============================================================================
// PDFTextDevice tests
// =============================================================================

#[test]
fn test_text_device_render_char_default() {
    // Default render_char returns 0
    struct TestTextDevice {
        ctm: Option<Matrix>,
    }

    impl PDFDevice for TestTextDevice {
        fn set_ctm(&mut self, ctm: Matrix) {
            self.ctm = Some(ctm);
        }

        fn ctm(&self) -> Option<Matrix> {
            self.ctm
        }
    }

    impl PDFTextDevice for TestTextDevice {}

    let mut device = TestTextDevice { ctm: None };
    let font = MockFont::new();

    let gs = PDFGraphicState::new();
    let result = device.render_char(
        MATRIX_IDENTITY,
        &font,
        12.0, // fontsize
        1.0,  // scaling
        0.0,  // rise
        65,   // cid for 'A'
        &PREDEFINED_COLORSPACE.get("DeviceGray").unwrap(),
        &gs,
    );

    assert_eq!(result, 0.0);
}

#[test]
fn test_text_device_render_string_horizontal() {
    struct TestTextDevice {
        ctm: Option<Matrix>,
        rendered_cids: Vec<u32>,
    }

    impl PDFDevice for TestTextDevice {
        fn set_ctm(&mut self, ctm: Matrix) {
            self.ctm = Some(ctm);
        }

        fn ctm(&self) -> Option<Matrix> {
            self.ctm
        }
    }

    impl PDFTextDevice for TestTextDevice {
        fn render_char<F: PDFFontLike>(
            &mut self,
            _matrix: Matrix,
            _font: &F,
            _fontsize: f64,
            _scaling: f64,
            _rise: f64,
            cid: u32,
            _ncs: &bolivar::pdfcolor::PDFColorSpace,
            _graphicstate: &PDFGraphicState,
        ) -> f64 {
            self.rendered_cids.push(cid);
            10.0 // Return fixed width
        }
    }

    let mut device = TestTextDevice {
        ctm: Some(MATRIX_IDENTITY),
        rendered_cids: Vec::new(),
    };
    let font = MockFont::new();
    let gs = PDFGraphicState::new();
    let ncs = PREDEFINED_COLORSPACE.get("DeviceGray").unwrap();

    // Create text sequence: "AB" (bytes 65, 66)
    let seq: PDFTextSeq = vec![PDFTextSeqItem::Bytes(vec![65, 66])];

    let pos = device.render_string_horizontal(
        &seq,
        MATRIX_IDENTITY,
        (0.0, 0.0),
        &font,
        12.0,  // fontsize
        1.0,   // scaling
        0.0,   // charspace
        0.0,   // wordspace
        0.0,   // rise
        0.012, // dxscale (0.001 * 12 * 1.0)
        ncs,
        &gs,
    );

    // Check that both characters were rendered
    assert_eq!(device.rendered_cids, vec![65, 66]);
    // Final x position should be 20.0 (2 chars * 10.0 width each)
    assert_eq!(pos, (20.0, 0.0));
}

#[test]
fn test_text_device_render_string_with_positioning() {
    struct TestTextDevice {
        ctm: Option<Matrix>,
    }

    impl PDFDevice for TestTextDevice {
        fn set_ctm(&mut self, ctm: Matrix) {
            self.ctm = Some(ctm);
        }

        fn ctm(&self) -> Option<Matrix> {
            self.ctm
        }
    }

    impl PDFTextDevice for TestTextDevice {
        fn render_char<F: PDFFontLike>(
            &mut self,
            _matrix: Matrix,
            _font: &F,
            _fontsize: f64,
            _scaling: f64,
            _rise: f64,
            _cid: u32,
            _ncs: &bolivar::pdfcolor::PDFColorSpace,
            _graphicstate: &PDFGraphicState,
        ) -> f64 {
            10.0
        }
    }

    let mut device = TestTextDevice {
        ctm: Some(MATRIX_IDENTITY),
    };
    let font = MockFont::new();
    let gs = PDFGraphicState::new();
    let ncs = PREDEFINED_COLORSPACE.get("DeviceGray").unwrap();

    // Create text sequence with positioning: "A", -100, "B"
    // -100 moves x backward by 100 * dxscale
    let seq: PDFTextSeq = vec![
        PDFTextSeqItem::Bytes(vec![65]), // 'A'
        PDFTextSeqItem::Number(-100.0),  // move forward
        PDFTextSeqItem::Bytes(vec![66]), // 'B'
    ];

    let dxscale = 0.012;
    let pos = device.render_string_horizontal(
        &seq,
        MATRIX_IDENTITY,
        (0.0, 0.0),
        &font,
        12.0,
        1.0,
        0.0,
        0.0,
        0.0,
        dxscale,
        ncs,
        &gs,
    );

    // x = 10 (A width) + 100 * 0.012 (positioning, -(-100) = +100) + 10 (B width) = 21.2
    let expected_x = 10.0 + 100.0 * dxscale + 10.0;
    assert!((pos.0 - expected_x).abs() < 0.001);
}

#[test]
fn test_text_device_render_string_vertical() {
    struct TestTextDevice {
        ctm: Option<Matrix>,
    }

    impl PDFDevice for TestTextDevice {
        fn set_ctm(&mut self, ctm: Matrix) {
            self.ctm = Some(ctm);
        }

        fn ctm(&self) -> Option<Matrix> {
            self.ctm
        }
    }

    impl PDFTextDevice for TestTextDevice {
        fn render_char<F: PDFFontLike>(
            &mut self,
            _matrix: Matrix,
            _font: &F,
            _fontsize: f64,
            _scaling: f64,
            _rise: f64,
            _cid: u32,
            _ncs: &bolivar::pdfcolor::PDFColorSpace,
            _graphicstate: &PDFGraphicState,
        ) -> f64 {
            12.0 // Return fixed height for vertical text
        }
    }

    let mut device = TestTextDevice {
        ctm: Some(MATRIX_IDENTITY),
    };
    let font = MockFont::new();
    let gs = PDFGraphicState::new();
    let ncs = PREDEFINED_COLORSPACE.get("DeviceGray").unwrap();

    let seq: PDFTextSeq = vec![PDFTextSeqItem::Bytes(vec![65, 66])];

    let pos = device.render_string_vertical(
        &seq,
        MATRIX_IDENTITY,
        (0.0, 0.0),
        &font,
        12.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.012,
        ncs,
        &gs,
    );

    // x stays at 0, y advances by 2 * 12.0 = 24.0
    assert_eq!(pos.0, 0.0);
    assert_eq!(pos.1, 24.0);
}

// =============================================================================
// TagExtractor tests
// =============================================================================

#[test]
fn test_tag_extractor_write() {
    let mut buffer = Cursor::new(Vec::new());

    {
        let mut extractor = TagExtractor::new(&mut buffer, "utf-8");

        // Simulate tag operations
        extractor.begin_tag(&PSLiteral::new("P"), None);
        extractor.write("Hello, World!");
        extractor.end_tag();
    }

    let output = String::from_utf8(buffer.into_inner()).unwrap();
    assert!(output.contains("<P>"));
    assert!(output.contains("Hello, World!"));
    assert!(output.contains("</P>"));
}

#[test]
fn test_tag_extractor_nested_tags() {
    let mut buffer = Cursor::new(Vec::new());

    {
        let mut extractor = TagExtractor::new(&mut buffer, "utf-8");

        extractor.begin_tag(&PSLiteral::new("Document"), None);
        extractor.begin_tag(&PSLiteral::new("P"), None);
        extractor.write("Nested content");
        extractor.end_tag(); // </P>
        extractor.end_tag(); // </Document>
    }

    let output = String::from_utf8(buffer.into_inner()).unwrap();
    assert!(output.contains("<Document>"));
    assert!(output.contains("<P>"));
    assert!(output.contains("Nested content"));
    assert!(output.contains("</P>"));
    assert!(output.contains("</Document>"));
}

#[test]
fn test_tag_extractor_do_tag() {
    let mut buffer = Cursor::new(Vec::new());

    {
        let mut extractor = TagExtractor::new(&mut buffer, "utf-8");
        // do_tag writes opening tag but doesn't add to stack
        extractor.do_tag(&PSLiteral::new("BR"), None);
    }

    let output = String::from_utf8(buffer.into_inner()).unwrap();
    assert!(output.contains("<BR>"));
}

#[test]
fn test_tag_extractor_pageno() {
    let mut buffer = Cursor::new(Vec::new());
    let mut extractor = TagExtractor::new(&mut buffer, "utf-8");

    assert_eq!(extractor.pageno(), 0);
    extractor.increment_pageno();
    assert_eq!(extractor.pageno(), 1);
}

#[test]
fn test_tag_extractor_render_string() {
    let mut buffer = Cursor::new(Vec::new());

    {
        let mut extractor = TagExtractor::new(&mut buffer, "utf-8");
        let mut textstate = PDFTextState::new();
        let gs = PDFGraphicState::new();
        let ncs = PREDEFINED_COLORSPACE.get("DeviceGray").unwrap();

        // Create text sequence with ASCII text "Hello"
        let seq: PDFTextSeq = vec![PDFTextSeqItem::Bytes(b"Hello".to_vec())];

        PDFTextDevice::render_string(&mut extractor, &mut textstate, &seq, ncs, &gs);
    }

    let output = String::from_utf8(buffer.into_inner()).unwrap();
    // TagExtractor extracts ASCII text directly (stub implementation)
    assert!(output.contains("Hello"));
}

#[test]
fn test_tag_extractor_render_string_escapes_special_chars() {
    let mut buffer = Cursor::new(Vec::new());

    {
        let mut extractor = TagExtractor::new(&mut buffer, "utf-8");
        let mut textstate = PDFTextState::new();
        let gs = PDFGraphicState::new();
        let ncs = PREDEFINED_COLORSPACE.get("DeviceGray").unwrap();

        // Create text sequence with special XML characters
        let seq: PDFTextSeq = vec![PDFTextSeqItem::Bytes(b"<test>".to_vec())];

        PDFTextDevice::render_string(&mut extractor, &mut textstate, &seq, ncs, &gs);
    }

    let output = String::from_utf8(buffer.into_inner()).unwrap();
    // Should escape < and > for XML safety
    assert!(output.contains("&lt;"));
    assert!(output.contains("&gt;"));
}

#[test]
fn test_pdf_device_render_string_with_textstate() {
    // Test that PDFDevice::render_string accepts textstate parameter
    struct TestDevice {
        ctm: Option<Matrix>,
        render_string_called: bool,
    }

    impl PDFDevice for TestDevice {
        fn set_ctm(&mut self, ctm: Matrix) {
            self.ctm = Some(ctm);
        }

        fn ctm(&self) -> Option<Matrix> {
            self.ctm
        }

        fn render_string(
            &mut self,
            textstate: &mut PDFTextState,
            _seq: &PDFTextSeq,
            _ncs: &bolivar::pdfcolor::PDFColorSpace,
            _graphicstate: &PDFGraphicState,
        ) {
            // Verify we can access textstate fields
            assert_eq!(textstate.fontsize, 12.0);
            self.render_string_called = true;
        }
    }

    let mut device = TestDevice {
        ctm: Some(MATRIX_IDENTITY),
        render_string_called: false,
    };
    let mut textstate = PDFTextState::new();
    textstate.fontsize = 12.0;
    let gs = PDFGraphicState::new();
    let ncs = PREDEFINED_COLORSPACE.get("DeviceGray").unwrap();
    let seq: PDFTextSeq = vec![];

    device.render_string(&mut textstate, &seq, ncs, &gs);
    assert!(device.render_string_called);
}
