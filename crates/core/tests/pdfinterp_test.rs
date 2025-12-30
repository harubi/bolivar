//! Tests for PDFContentParser - PDF content stream parser.
//!
//! PDFContentParser parses PDF content streams containing operators
//! like BT, ET, Tm, Tj, as well as inline images (BI/ID/EI).

use bolivar_core::cmapdb::CMapBase;
use bolivar_core::pdfinterp::{ContentToken, PDFContentParser};
use bolivar_core::psparser::PSToken;

// ============================================================================
// Basic content stream tokenization tests
// ============================================================================

#[test]
fn test_parse_simple_operators() {
    // Simple content stream with text operators
    let stream = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
    let parser = PDFContentParser::new(vec![stream.to_vec()]);

    let tokens: Vec<ContentToken> = parser.collect();

    // Should have: BT, /F1, 12, Tf, 100, 700, Td, (Hello), Tj, ET
    assert!(
        tokens.len() >= 10,
        "Expected at least 10 tokens, got {}",
        tokens.len()
    );

    // First should be BT keyword
    match &tokens[0] {
        ContentToken::Keyword(kw) => assert_eq!(kw, b"BT"),
        _ => panic!("Expected BT keyword, got {:?}", tokens[0]),
    }

    // Last should be ET keyword
    match tokens.last().unwrap() {
        ContentToken::Keyword(kw) => assert_eq!(kw, b"ET"),
        _ => panic!("Expected ET keyword"),
    }
}

#[test]
fn test_parse_graphics_operators() {
    // Graphics state operators
    let stream = b"q 1 0 0 1 0 0 cm 0.5 G 1 0 0 RG Q";
    let parser = PDFContentParser::new(vec![stream.to_vec()]);

    let tokens: Vec<ContentToken> = parser.collect();

    // First should be 'q' (save graphics state)
    match &tokens[0] {
        ContentToken::Keyword(kw) => assert_eq!(kw, b"q"),
        _ => panic!("Expected q keyword"),
    }

    // Last should be 'Q' (restore graphics state)
    match tokens.last().unwrap() {
        ContentToken::Keyword(kw) => assert_eq!(kw, b"Q"),
        _ => panic!("Expected Q keyword"),
    }
}

#[test]
fn test_parse_path_operators() {
    // Path construction operators
    let stream = b"100 200 m 300 400 l 500 600 700 800 900 1000 c h S";
    let parser = PDFContentParser::new(vec![stream.to_vec()]);

    let tokens: Vec<ContentToken> = parser.collect();

    // Should contain moveto, lineto, curveto, closepath, stroke
    let keywords: Vec<&[u8]> = tokens
        .iter()
        .filter_map(|t| match t {
            ContentToken::Keyword(kw) => Some(kw.as_slice()),
            _ => None,
        })
        .collect();

    assert!(
        keywords.contains(&b"m".as_slice()),
        "Missing moveto operator"
    );
    assert!(
        keywords.contains(&b"l".as_slice()),
        "Missing lineto operator"
    );
    assert!(
        keywords.contains(&b"c".as_slice()),
        "Missing curveto operator"
    );
    assert!(
        keywords.contains(&b"h".as_slice()),
        "Missing closepath operator"
    );
    assert!(
        keywords.contains(&b"S".as_slice()),
        "Missing stroke operator"
    );
}

#[test]
fn test_parse_text_showing_operators() {
    // Text showing operators: Tj, TJ, ', "
    let stream = b"BT (Hello) Tj [(H) 50 (ello)] TJ ET";
    let parser = PDFContentParser::new(vec![stream.to_vec()]);

    let tokens: Vec<ContentToken> = parser.collect();

    let keywords: Vec<&[u8]> = tokens
        .iter()
        .filter_map(|t| match t {
            ContentToken::Keyword(kw) => Some(kw.as_slice()),
            _ => None,
        })
        .collect();

    assert!(keywords.contains(&b"Tj".as_slice()), "Missing Tj operator");
    assert!(keywords.contains(&b"TJ".as_slice()), "Missing TJ operator");
}

// ============================================================================
// Inline image tests (BI/ID/EI)
// ============================================================================

#[test]
fn test_parse_inline_image_basic() {
    // Basic inline image: BI <dict> ID <data> EI
    let stream = b"BI /W 10 /H 10 /BPC 8 /CS /G ID 1234567890 EI";
    let parser = PDFContentParser::new(vec![stream.to_vec()]);

    let tokens: Vec<ContentToken> = parser.collect();

    // Should produce an InlineImage token
    let has_inline_image = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::InlineImage { .. }));
    assert!(
        has_inline_image,
        "Expected inline image token, got {:?}",
        tokens
    );
}

#[test]
fn test_parse_inline_image_with_real_data() {
    // Inline image with binary-like data
    let mut stream = Vec::new();
    stream.extend_from_slice(b"BI /W 2 /H 2 /BPC 8 /CS /RGB ID ");
    // 2x2 RGB image = 12 bytes of "image data"
    stream.extend_from_slice(&[
        0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
    ]);
    stream.extend_from_slice(b" EI");

    let parser = PDFContentParser::new(vec![stream]);
    let tokens: Vec<ContentToken> = parser.collect();

    let inline_image = tokens
        .iter()
        .find(|t| matches!(t, ContentToken::InlineImage { .. }));
    assert!(inline_image.is_some(), "Expected inline image");

    if let Some(ContentToken::InlineImage { dict, data }) = inline_image {
        // Check dictionary contains expected keys
        assert!(
            dict.contains_key("W") || dict.contains_key("Width"),
            "Missing width"
        );
        assert!(
            dict.contains_key("H") || dict.contains_key("Height"),
            "Missing height"
        );
        // Data should be present
        assert!(!data.is_empty(), "Image data should not be empty");
    }
}

#[test]
fn test_parse_inline_image_ascii85() {
    // Inline image with ASCII85 encoding ends with ~>
    let stream = b"BI /W 1 /H 1 /BPC 8 /CS /G /F /A85 ID z~> EI";
    let parser = PDFContentParser::new(vec![stream.to_vec()]);

    let tokens: Vec<ContentToken> = parser.collect();

    let inline_image = tokens
        .iter()
        .find(|t| matches!(t, ContentToken::InlineImage { .. }));
    assert!(
        inline_image.is_some(),
        "Expected inline image with ASCII85 filter"
    );
}

#[test]
fn test_inline_image_followed_by_more_content() {
    // Content after inline image should still be parsed
    let stream = b"BI /W 1 /H 1 /BPC 8 /CS /G ID X EI q 1 0 0 1 0 0 cm Q";
    let parser = PDFContentParser::new(vec![stream.to_vec()]);

    let tokens: Vec<ContentToken> = parser.collect();

    // Should have inline image AND subsequent operators
    let has_inline_image = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::InlineImage { .. }));
    let has_q = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::Keyword(kw) if kw == b"q"));
    let has_cm = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::Keyword(kw) if kw == b"cm"));
    let has_restore = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::Keyword(kw) if kw == b"Q"));

    assert!(has_inline_image, "Missing inline image");
    assert!(has_q, "Missing q operator after inline image");
    assert!(has_cm, "Missing cm operator after inline image");
    assert!(has_restore, "Missing Q operator after inline image");
}

// ============================================================================
// Multiple streams tests
// ============================================================================

#[test]
fn test_parse_multiple_streams() {
    // Content split across multiple streams (common in PDF)
    let stream1 = b"BT /F1 12 Tf".to_vec();
    let stream2 = b"100 700 Td (Hello) Tj ET".to_vec();

    let parser = PDFContentParser::new(vec![stream1, stream2]);
    let tokens: Vec<ContentToken> = parser.collect();

    // Should seamlessly parse content from both streams
    let has_bt = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::Keyword(kw) if kw == b"BT"));
    let has_tf = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::Keyword(kw) if kw == b"Tf"));
    let has_td = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::Keyword(kw) if kw == b"Td"));
    let has_tj = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::Keyword(kw) if kw == b"Tj"));
    let has_et = tokens
        .iter()
        .any(|t| matches!(t, ContentToken::Keyword(kw) if kw == b"ET"));

    assert!(has_bt, "Missing BT");
    assert!(has_tf, "Missing Tf");
    assert!(has_td, "Missing Td");
    assert!(has_tj, "Missing Tj");
    assert!(has_et, "Missing ET");
}

#[test]
fn test_parse_empty_stream() {
    let parser = PDFContentParser::new(vec![]);
    let tokens: Vec<ContentToken> = parser.collect();
    assert!(tokens.is_empty(), "Empty streams should produce no tokens");
}

#[test]
fn test_parse_whitespace_only_stream() {
    let stream = b"   \n\t\r\n   ".to_vec();
    let parser = PDFContentParser::new(vec![stream]);
    let tokens: Vec<ContentToken> = parser.collect();
    assert!(
        tokens.is_empty(),
        "Whitespace-only stream should produce no tokens"
    );
}

// ============================================================================
// Operand/operator pattern tests
// ============================================================================

#[test]
fn test_operands_before_operator() {
    // In PDF content streams, operands come before operators
    let stream = b"1 2 3 4 re".to_vec();
    let parser = PDFContentParser::new(vec![stream]);

    let tokens: Vec<ContentToken> = parser.collect();

    // Should have 4 numbers followed by 're' keyword
    assert_eq!(tokens.len(), 5);

    // First 4 should be numbers
    for i in 0..4 {
        assert!(
            matches!(&tokens[i], ContentToken::Operand(PSToken::Int(_))),
            "Token {} should be integer, got {:?}",
            i,
            tokens[i]
        );
    }

    // Last should be 're' operator
    match &tokens[4] {
        ContentToken::Keyword(kw) => assert_eq!(kw, b"re"),
        _ => panic!("Expected 're' keyword"),
    }
}

#[test]
fn test_string_operand() {
    let stream = b"(Hello World) Tj".to_vec();
    let parser = PDFContentParser::new(vec![stream]);

    let tokens: Vec<ContentToken> = parser.collect();

    assert_eq!(tokens.len(), 2);

    // First should be string
    match &tokens[0] {
        ContentToken::Operand(PSToken::String(s)) => {
            assert_eq!(s, b"Hello World");
        }
        _ => panic!("Expected string operand, got {:?}", tokens[0]),
    }

    // Second should be Tj
    match &tokens[1] {
        ContentToken::Keyword(kw) => assert_eq!(kw, b"Tj"),
        _ => panic!("Expected Tj keyword"),
    }
}

#[test]
fn test_array_operand() {
    let stream = b"[(H) 50 (ello)] TJ".to_vec();
    let parser = PDFContentParser::new(vec![stream]);

    let tokens: Vec<ContentToken> = parser.collect();

    assert_eq!(tokens.len(), 2);

    // First should be array
    match &tokens[0] {
        ContentToken::Operand(PSToken::Array(arr)) => {
            assert_eq!(arr.len(), 3);
        }
        _ => panic!("Expected array operand, got {:?}", tokens[0]),
    }
}

#[test]
fn test_dict_operand() {
    // Dictionaries can appear as operands (e.g., in BDC operator)
    let stream = b"/Span << /MCID 0 >> BDC".to_vec();
    let parser = PDFContentParser::new(vec![stream]);

    let tokens: Vec<ContentToken> = parser.collect();

    // Should have: /Span, dict, BDC
    assert_eq!(tokens.len(), 3);

    // First is literal
    match &tokens[0] {
        ContentToken::Operand(PSToken::Literal(_)) => {}
        _ => panic!("Expected literal operand"),
    }

    // Second is dict
    match &tokens[1] {
        ContentToken::Operand(PSToken::Dict(_)) => {}
        _ => panic!("Expected dict operand, got {:?}", tokens[1]),
    }

    // Third is BDC
    match &tokens[2] {
        ContentToken::Keyword(kw) => assert_eq!(kw, b"BDC"),
        _ => panic!("Expected BDC keyword"),
    }
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_parse_with_comments() {
    // Comments should be skipped
    let stream = b"BT % this is a comment\n/F1 12 Tf ET".to_vec();
    let parser = PDFContentParser::new(vec![stream]);

    let tokens: Vec<ContentToken> = parser.collect();

    // Comment should be ignored
    let keywords: Vec<&[u8]> = tokens
        .iter()
        .filter_map(|t| match t {
            ContentToken::Keyword(kw) => Some(kw.as_slice()),
            _ => None,
        })
        .collect();

    assert!(keywords.contains(&b"BT".as_slice()));
    assert!(keywords.contains(&b"Tf".as_slice()));
    assert!(keywords.contains(&b"ET".as_slice()));
}

#[test]
fn test_parse_real_numbers() {
    let stream = b"0.5 0.25 0.75 1.0 rg".to_vec();
    let parser = PDFContentParser::new(vec![stream]);

    let tokens: Vec<ContentToken> = parser.collect();

    // First 4 should be real numbers
    for i in 0..4 {
        match &tokens[i] {
            ContentToken::Operand(PSToken::Real(_)) => {}
            ContentToken::Operand(PSToken::Int(_)) => {} // 1.0 might parse as 1
            _ => panic!("Token {} should be number, got {:?}", i, tokens[i]),
        }
    }
}

#[test]
fn test_parse_negative_numbers() {
    let stream = b"-100 -50 Td".to_vec();
    let parser = PDFContentParser::new(vec![stream]);

    let tokens: Vec<ContentToken> = parser.collect();

    assert_eq!(tokens.len(), 3);

    match &tokens[0] {
        ContentToken::Operand(PSToken::Int(n)) => assert_eq!(*n, -100),
        _ => panic!("Expected -100"),
    }

    match &tokens[1] {
        ContentToken::Operand(PSToken::Int(n)) => assert_eq!(*n, -50),
        _ => panic!("Expected -50"),
    }
}

#[test]
fn test_parse_hex_string() {
    let stream = b"<48656C6C6F> Tj".to_vec(); // "Hello" in hex
    let parser = PDFContentParser::new(vec![stream]);

    let tokens: Vec<ContentToken> = parser.collect();

    assert_eq!(tokens.len(), 2);

    match &tokens[0] {
        ContentToken::Operand(PSToken::String(s)) => {
            assert_eq!(s, b"Hello");
        }
        _ => panic!("Expected hex string"),
    }
}

// ============================================================================
// Position tracking tests
// ============================================================================

#[test]
fn test_token_positions() {
    let stream = b"BT 100 Td ET".to_vec();
    let mut parser = PDFContentParser::new(vec![stream]);

    // Use next_with_pos() to get individual tokens with positions
    let mut positions = Vec::new();
    while let Some((pos, _token)) = parser.next_with_pos() {
        positions.push(pos);
    }

    // Positions should be increasing
    for i in 1..positions.len() {
        assert!(
            positions[i] > positions[i - 1],
            "Positions should be increasing"
        );
    }
}

// ============================================================================
// PDFResourceManager tests
// ============================================================================

use bolivar_core::pdfinterp::PDFResourceManager;

/// Test: PDFResourceManager can be created with default caching enabled.
#[test]
fn test_resource_manager_creation_default() {
    let rsrcmgr = PDFResourceManager::new();
    assert!(
        rsrcmgr.caching_enabled(),
        "Default should have caching enabled"
    );
}

/// Test: PDFResourceManager can be created with caching disabled.
#[test]
fn test_resource_manager_creation_no_caching() {
    let rsrcmgr = PDFResourceManager::with_caching(false);
    assert!(!rsrcmgr.caching_enabled(), "Should have caching disabled");
}

/// Test: get_procset processes procs without error.
/// In Python, this is essentially a no-op that logs, so we just check it doesn't panic.
#[test]
fn test_resource_manager_get_procset() {
    let rsrcmgr = PDFResourceManager::new();
    // Should not panic - matches Python's no-op behavior
    rsrcmgr.get_procset(&["PDF", "Text", "ImageB", "ImageC", "ImageI"]);
}

/// Test: get_procset with empty slice.
#[test]
fn test_resource_manager_get_procset_empty() {
    let rsrcmgr = PDFResourceManager::new();
    rsrcmgr.get_procset(&[]);
}

/// Test: get_colorspace returns predefined color space.
#[test]
fn test_resource_manager_get_colorspace_predefined() {
    let rsrcmgr = PDFResourceManager::new();

    // DeviceGray
    let cs = rsrcmgr.get_colorspace("DeviceGray");
    assert!(cs.is_some());
    let cs = cs.unwrap();
    assert_eq!(cs.name, "DeviceGray");
    assert_eq!(cs.ncomponents, 1);

    // DeviceRGB
    let cs = rsrcmgr.get_colorspace("DeviceRGB");
    assert!(cs.is_some());
    let cs = cs.unwrap();
    assert_eq!(cs.name, "DeviceRGB");
    assert_eq!(cs.ncomponents, 3);

    // DeviceCMYK
    let cs = rsrcmgr.get_colorspace("DeviceCMYK");
    assert!(cs.is_some());
    let cs = cs.unwrap();
    assert_eq!(cs.name, "DeviceCMYK");
    assert_eq!(cs.ncomponents, 4);
}

/// Test: get_colorspace returns None for unknown color space.
#[test]
fn test_resource_manager_get_colorspace_unknown() {
    let rsrcmgr = PDFResourceManager::new();
    let cs = rsrcmgr.get_colorspace("UnknownColorSpace");
    assert!(cs.is_none());
}

/// Test: Font caching - same objid returns cached font.
#[test]
fn test_resource_manager_font_caching() {
    use bolivar_core::pdftypes::PDFObject;
    use std::collections::HashMap;

    let mut rsrcmgr = PDFResourceManager::new();

    // Create a mock font spec
    let mut spec = HashMap::new();
    spec.insert("Type".to_string(), PDFObject::Name("Font".to_string()));
    spec.insert("Subtype".to_string(), PDFObject::Name("Type1".to_string()));
    spec.insert(
        "BaseFont".to_string(),
        PDFObject::Name("Helvetica".to_string()),
    );

    // Get font with objid 1
    let objid = 1u64;
    let font_id_1 = rsrcmgr.get_font(Some(objid), &spec);

    // Get font again with same objid - should return same cached entry
    let font_id_2 = rsrcmgr.get_font(Some(objid), &spec);

    assert_eq!(font_id_1, font_id_2, "Same objid should return cached font");
}

/// Test: Font caching - None objid doesn't cache.
#[test]
fn test_resource_manager_font_no_cache_without_objid() {
    use bolivar_core::pdftypes::PDFObject;
    use std::collections::HashMap;

    let mut rsrcmgr = PDFResourceManager::new();

    let mut spec = HashMap::new();
    spec.insert("Type".to_string(), PDFObject::Name("Font".to_string()));
    spec.insert("Subtype".to_string(), PDFObject::Name("Type1".to_string()));
    spec.insert(
        "BaseFont".to_string(),
        PDFObject::Name("Helvetica".to_string()),
    );

    // Get font without objid
    let font_id_1 = rsrcmgr.get_font(None, &spec);
    let font_id_2 = rsrcmgr.get_font(None, &spec);

    // Both should succeed but may be different (no caching without objid)
    assert!(font_id_1 > 0);
    assert!(font_id_2 > 0);
}

/// Test: Font caching disabled - same objid creates new font each time.
#[test]
fn test_resource_manager_font_caching_disabled() {
    use bolivar_core::pdftypes::PDFObject;
    use std::collections::HashMap;

    let mut rsrcmgr = PDFResourceManager::with_caching(false);

    let mut spec = HashMap::new();
    spec.insert("Type".to_string(), PDFObject::Name("Font".to_string()));
    spec.insert("Subtype".to_string(), PDFObject::Name("Type1".to_string()));
    spec.insert(
        "BaseFont".to_string(),
        PDFObject::Name("Helvetica".to_string()),
    );

    let objid = 1u64;
    let font_id_1 = rsrcmgr.get_font(Some(objid), &spec);
    let font_id_2 = rsrcmgr.get_font(Some(objid), &spec);

    // With caching disabled, each call creates a new font
    assert_ne!(
        font_id_1, font_id_2,
        "Caching disabled should create new fonts"
    );
}

// ============================================================================
// get_cmap tests
// ============================================================================

/// Test: get_cmap returns CMap for Identity-H.
#[test]
fn test_resource_manager_get_cmap_identity_h() {
    let rsrcmgr = PDFResourceManager::new();
    let cmap = rsrcmgr.get_cmap("Identity-H", false).unwrap();
    assert!(!cmap.is_vertical());
    assert_eq!(cmap.attrs.get("CMapName"), Some(&"Identity-H".to_string()));
}

/// Test: get_cmap returns CMap for Identity-V with vertical mode.
#[test]
fn test_resource_manager_get_cmap_identity_v() {
    let rsrcmgr = PDFResourceManager::new();
    let cmap = rsrcmgr.get_cmap("Identity-V", false).unwrap();
    assert!(cmap.is_vertical());
    assert_eq!(cmap.attrs.get("CMapName"), Some(&"Identity-V".to_string()));
}

/// Test: get_cmap returns empty CMap for unknown name when strict=false.
#[test]
fn test_resource_manager_get_cmap_unknown_non_strict() {
    let rsrcmgr = PDFResourceManager::new();
    let cmap = rsrcmgr.get_cmap("UnknownCMap", false).unwrap();
    // Should return empty CMap (default)
    assert!(!cmap.is_vertical());
    assert!(cmap.attrs.is_empty());
}

/// Test: get_cmap returns error for unknown name when strict=true.
#[test]
fn test_resource_manager_get_cmap_unknown_strict() {
    use bolivar_core::PdfError;

    let rsrcmgr = PDFResourceManager::new();
    let result = rsrcmgr.get_cmap("UnknownCMap", true);
    assert!(result.is_err());
    match result.unwrap_err() {
        PdfError::CMapNotFound(name) => assert_eq!(name, "UnknownCMap"),
        e => panic!("Expected CMapNotFound error, got {:?}", e),
    }
}

/// Test: get_cmap handles DLIdent-H as identity CMap.
#[test]
fn test_resource_manager_get_cmap_dlident() {
    let rsrcmgr = PDFResourceManager::new();
    let cmap = rsrcmgr.get_cmap("DLIdent-H", false).unwrap();
    assert!(!cmap.is_vertical());
    assert_eq!(cmap.attrs.get("CMapName"), Some(&"DLIdent-H".to_string()));
}

/// Test: get_cmap handles OneByteIdentityH.
#[test]
fn test_resource_manager_get_cmap_one_byte_identity() {
    let rsrcmgr = PDFResourceManager::new();
    let cmap = rsrcmgr.get_cmap("OneByteIdentityH", false).unwrap();
    assert!(!cmap.is_vertical());
    assert_eq!(
        cmap.attrs.get("CMapName"),
        Some(&"OneByteIdentityH".to_string())
    );
}

// ============================================================================
// PDFPageInterpreter tests - Graphics operators
// ============================================================================

use bolivar_core::pdfdevice::{PDFDevice, PathSegment};
use bolivar_core::pdfinterp::PDFPageInterpreter;
use bolivar_core::pdfstate::PDFGraphicState;
use bolivar_core::utils::{MATRIX_IDENTITY, Matrix};

/// Mock device for testing - records operations called.
#[derive(Debug, Default)]
struct MockDevice {
    ctm: Option<Matrix>,
    paths_painted: Vec<(bool, bool, bool, Vec<PathSegment>)>,
    pages_begun: Vec<(u32, (f64, f64, f64, f64), Matrix)>,
    pages_ended: Vec<u32>,
}

impl PDFDevice for MockDevice {
    fn set_ctm(&mut self, ctm: Matrix) {
        self.ctm = Some(ctm);
    }

    fn ctm(&self) -> Option<Matrix> {
        self.ctm
    }

    fn begin_page(&mut self, pageid: u32, mediabox: (f64, f64, f64, f64), ctm: Matrix) {
        self.pages_begun.push((pageid, mediabox, ctm));
    }

    fn end_page(&mut self, pageid: u32) {
        self.pages_ended.push(pageid);
    }

    fn paint_path(
        &mut self,
        _graphicstate: &PDFGraphicState,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        path: &[PathSegment],
    ) {
        self.paths_painted
            .push((stroke, fill, evenodd, path.to_vec()));
    }
}

// ============================================================================
// Graphics State Stack (q/Q) tests
// ============================================================================

/// Test: gsave (q) pushes current state onto stack.
#[test]
fn test_interpreter_gsave_pushes_state() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    // Initialize with identity CTM
    interp.init_state(MATRIX_IDENTITY);

    // Modify line width
    interp.do_w(2.5);
    assert_eq!(interp.graphicstate().linewidth, 2.5);

    // Save state
    interp.do_q();

    // Modify again
    interp.do_w(5.0);
    assert_eq!(interp.graphicstate().linewidth, 5.0);

    // Restore - should get back to 2.5
    interp.do_Q();
    assert_eq!(interp.graphicstate().linewidth, 2.5);
}

/// Test: grestore (Q) restores CTM.
#[test]
fn test_interpreter_grestore_restores_ctm() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    let initial_ctm = (2.0, 0.0, 0.0, 2.0, 100.0, 200.0);
    interp.init_state(initial_ctm);

    // Save state
    interp.do_q();

    // Modify CTM via cm
    interp.do_cm(1.0, 0.0, 0.0, 1.0, 50.0, 50.0);

    // CTM should be changed
    assert_ne!(interp.ctm(), initial_ctm);

    // Restore
    interp.do_Q();

    // CTM should be back to initial
    assert_eq!(interp.ctm(), initial_ctm);
}

/// Test: multiple gsave/grestore pairs work correctly (stack behavior).
#[test]
fn test_interpreter_nested_gsave_grestore() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);

    interp.do_w(1.0);
    interp.do_q(); // push state with linewidth=1.0

    interp.do_w(2.0);
    interp.do_q(); // push state with linewidth=2.0

    interp.do_w(3.0);
    assert_eq!(interp.graphicstate().linewidth, 3.0);

    interp.do_Q(); // restore to 2.0
    assert_eq!(interp.graphicstate().linewidth, 2.0);

    interp.do_Q(); // restore to 1.0
    assert_eq!(interp.graphicstate().linewidth, 1.0);
}

/// Test: grestore on empty stack is a no-op (doesn't panic).
#[test]
fn test_interpreter_grestore_empty_stack() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_w(5.0);

    // Q on empty stack should not change state
    interp.do_Q();
    assert_eq!(interp.graphicstate().linewidth, 5.0);
}

// ============================================================================
// CTM Modification (cm) tests
// ============================================================================

/// Test: cm operator concatenates matrix to CTM.
#[test]
fn test_interpreter_cm_concatenates_matrix() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);

    // Apply translation
    interp.do_cm(1.0, 0.0, 0.0, 1.0, 100.0, 200.0);

    let ctm = interp.ctm();
    // Translation should be applied
    assert_eq!(ctm.4, 100.0);
    assert_eq!(ctm.5, 200.0);
}

/// Test: cm operator with scaling.
#[test]
fn test_interpreter_cm_scaling() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);

    // Apply 2x scaling
    interp.do_cm(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);

    let ctm = interp.ctm();
    assert_eq!(ctm.0, 2.0); // a
    assert_eq!(ctm.3, 2.0); // d
}

/// Test: cm operator notifies device of CTM change.
#[test]
fn test_interpreter_cm_updates_device() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_cm(1.0, 0.0, 0.0, 1.0, 50.0, 75.0);

    // Device should have the updated CTM
    assert!(device.ctm.is_some());
    let dev_ctm = device.ctm.unwrap();
    assert_eq!(dev_ctm.4, 50.0);
    assert_eq!(dev_ctm.5, 75.0);
}

// ============================================================================
// Graphics State Parameters tests
// ============================================================================

/// Test: w operator sets line width.
#[test]
fn test_interpreter_do_w_sets_linewidth() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_w(3.5);

    assert_eq!(interp.graphicstate().linewidth, 3.5);
}

/// Test: J operator sets line cap.
#[test]
fn test_interpreter_do_J_sets_linecap() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_J(2);

    assert_eq!(interp.graphicstate().linecap, Some(2));
}

/// Test: j operator sets line join.
#[test]
fn test_interpreter_do_j_sets_linejoin() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_j(1);

    assert_eq!(interp.graphicstate().linejoin, Some(1));
}

/// Test: M operator sets miter limit.
#[test]
fn test_interpreter_do_M_sets_miterlimit() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_M(10.0);

    assert_eq!(interp.graphicstate().miterlimit, Some(10.0));
}

/// Test: d operator sets dash pattern.
#[test]
fn test_interpreter_do_d_sets_dash() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_d(vec![3.0, 2.0], 0.0);

    let dash = interp.graphicstate().dash.as_ref().unwrap();
    assert_eq!(dash.0, vec![3.0, 2.0]);
    assert_eq!(dash.1, 0.0);
}

/// Test: ri operator sets rendering intent.
#[test]
fn test_interpreter_do_ri_sets_intent() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_ri("RelativeColorimetric");

    assert_eq!(
        interp.graphicstate().intent,
        Some("RelativeColorimetric".to_string())
    );
}

/// Test: i operator sets flatness.
#[test]
fn test_interpreter_do_i_sets_flatness() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_i(0.5);

    assert_eq!(interp.graphicstate().flatness, Some(0.5));
}

// ============================================================================
// Path Construction tests
// ============================================================================

/// Test: m operator begins new subpath.
#[test]
fn test_interpreter_do_m_moveto() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(100.0, 200.0);

    let path = interp.current_path();
    assert_eq!(path.len(), 1);
    assert!(matches!(path[0], PathSegment::MoveTo(100.0, 200.0)));
}

/// Test: l operator appends line segment.
#[test]
fn test_interpreter_do_l_lineto() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(0.0, 0.0);
    interp.do_l(100.0, 100.0);

    let path = interp.current_path();
    assert_eq!(path.len(), 2);
    assert!(matches!(path[1], PathSegment::LineTo(100.0, 100.0)));
}

/// Test: c operator appends cubic bezier curve.
#[test]
fn test_interpreter_do_c_curveto() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(0.0, 0.0);
    interp.do_c(10.0, 20.0, 30.0, 40.0, 50.0, 60.0);

    let path = interp.current_path();
    assert_eq!(path.len(), 2);
    assert!(matches!(
        path[1],
        PathSegment::CurveTo(10.0, 20.0, 30.0, 40.0, 50.0, 60.0)
    ));
}

/// Test: v operator appends curve with replicated initial point.
#[test]
fn test_interpreter_do_v_curveto_initial() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(0.0, 0.0);
    interp.do_v(30.0, 40.0, 50.0, 60.0);

    // v uses current point as first control point
    let path = interp.current_path();
    assert_eq!(path.len(), 2);
    // First control point should be the current point (0, 0)
    if let PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) = path[1] {
        assert_eq!((x1, y1), (0.0, 0.0));
        assert_eq!((x2, y2), (30.0, 40.0));
        assert_eq!((x3, y3), (50.0, 60.0));
    } else {
        panic!("Expected CurveTo segment");
    }
}

/// Test: y operator appends curve with replicated final point.
#[test]
fn test_interpreter_do_y_curveto_final() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(0.0, 0.0);
    interp.do_y(10.0, 20.0, 50.0, 60.0);

    // y uses endpoint as second control point
    let path = interp.current_path();
    assert_eq!(path.len(), 2);
    if let PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) = path[1] {
        assert_eq!((x1, y1), (10.0, 20.0));
        assert_eq!((x2, y2), (50.0, 60.0)); // replicated
        assert_eq!((x3, y3), (50.0, 60.0));
    } else {
        panic!("Expected CurveTo segment");
    }
}

/// Test: h operator closes subpath.
#[test]
fn test_interpreter_do_h_closepath() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(0.0, 0.0);
    interp.do_l(100.0, 0.0);
    interp.do_l(100.0, 100.0);
    interp.do_h();

    let path = interp.current_path();
    assert_eq!(path.len(), 4);
    assert!(matches!(path[3], PathSegment::ClosePath));
}

/// Test: re operator appends rectangle.
#[test]
fn test_interpreter_do_re_rectangle() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_re(10.0, 20.0, 100.0, 50.0);

    // Rectangle should produce: m l l l h
    let path = interp.current_path();
    assert_eq!(path.len(), 5);
    assert!(matches!(path[0], PathSegment::MoveTo(10.0, 20.0)));
    assert!(matches!(path[1], PathSegment::LineTo(110.0, 20.0))); // x+w, y
    assert!(matches!(path[2], PathSegment::LineTo(110.0, 70.0))); // x+w, y+h
    assert!(matches!(path[3], PathSegment::LineTo(10.0, 70.0))); // x, y+h
    assert!(matches!(path[4], PathSegment::ClosePath));
}

// ============================================================================
// Path Painting tests
// ============================================================================

/// Test: S operator strokes path.
#[test]
fn test_interpreter_do_S_stroke() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();

    {
        let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);
        interp.init_state(MATRIX_IDENTITY);
        interp.do_m(0.0, 0.0);
        interp.do_l(100.0, 100.0);
        interp.do_S();
        // Path should be cleared after painting
        assert!(interp.current_path().is_empty());
    }

    // Device should receive paint_path with stroke=true, fill=false
    assert_eq!(device.paths_painted.len(), 1);
    let (stroke, fill, evenodd, _) = &device.paths_painted[0];
    assert!(*stroke);
    assert!(!*fill);
    assert!(!*evenodd);
}

/// Test: s operator closes and strokes path.
#[test]
fn test_interpreter_do_s_close_stroke() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(0.0, 0.0);
    interp.do_l(100.0, 0.0);
    interp.do_l(100.0, 100.0);
    interp.do_s();

    // Path should have close before stroke
    assert_eq!(device.paths_painted.len(), 1);
    let (stroke, fill, _, path) = &device.paths_painted[0];
    assert!(*stroke);
    assert!(!*fill);
    // Last segment should be ClosePath
    assert!(matches!(path.last(), Some(PathSegment::ClosePath)));
}

/// Test: f operator fills path with nonzero winding rule.
#[test]
fn test_interpreter_do_f_fill() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_re(0.0, 0.0, 100.0, 100.0);
    interp.do_f();

    assert_eq!(device.paths_painted.len(), 1);
    let (stroke, fill, evenodd, _) = &device.paths_painted[0];
    assert!(!*stroke);
    assert!(*fill);
    assert!(!*evenodd); // nonzero winding
}

/// Test: f* operator fills path with even-odd rule.
#[test]
fn test_interpreter_do_f_star_fill_evenodd() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_re(0.0, 0.0, 100.0, 100.0);
    interp.do_f_star();

    assert_eq!(device.paths_painted.len(), 1);
    let (stroke, fill, evenodd, _) = &device.paths_painted[0];
    assert!(!*stroke);
    assert!(*fill);
    assert!(*evenodd); // even-odd
}

/// Test: B operator fills and strokes path.
#[test]
fn test_interpreter_do_B_fill_stroke() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_re(0.0, 0.0, 100.0, 100.0);
    interp.do_B();

    assert_eq!(device.paths_painted.len(), 1);
    let (stroke, fill, evenodd, _) = &device.paths_painted[0];
    assert!(*stroke);
    assert!(*fill);
    assert!(!*evenodd);
}

/// Test: B* operator fills and strokes with even-odd rule.
#[test]
fn test_interpreter_do_B_star_fill_stroke_evenodd() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_re(0.0, 0.0, 100.0, 100.0);
    interp.do_B_star();

    assert_eq!(device.paths_painted.len(), 1);
    let (stroke, fill, evenodd, _) = &device.paths_painted[0];
    assert!(*stroke);
    assert!(*fill);
    assert!(*evenodd);
}

/// Test: b operator closes, fills and strokes.
#[test]
fn test_interpreter_do_b_close_fill_stroke() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(0.0, 0.0);
    interp.do_l(100.0, 0.0);
    interp.do_l(100.0, 100.0);
    interp.do_b();

    assert_eq!(device.paths_painted.len(), 1);
    let (stroke, fill, evenodd, path) = &device.paths_painted[0];
    assert!(*stroke);
    assert!(*fill);
    assert!(!*evenodd);
    assert!(matches!(path.last(), Some(PathSegment::ClosePath)));
}

/// Test: b* operator closes, fills and strokes with even-odd.
#[test]
fn test_interpreter_do_b_star_close_fill_stroke_evenodd() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_m(0.0, 0.0);
    interp.do_l(100.0, 0.0);
    interp.do_l(100.0, 100.0);
    interp.do_b_star();

    assert_eq!(device.paths_painted.len(), 1);
    let (stroke, fill, evenodd, path) = &device.paths_painted[0];
    assert!(*stroke);
    assert!(*fill);
    assert!(*evenodd);
    assert!(matches!(path.last(), Some(PathSegment::ClosePath)));
}

/// Test: n operator ends path without painting.
#[test]
fn test_interpreter_do_n_endpath() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();

    {
        let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);
        interp.init_state(MATRIX_IDENTITY);
        interp.do_m(0.0, 0.0);
        interp.do_l(100.0, 100.0);
        interp.do_n();
        // Path should be cleared
        assert!(interp.current_path().is_empty());
    }

    // No paint_path should be called
    assert!(device.paths_painted.is_empty());
}

// ============================================================================
// Clipping tests
// ============================================================================

/// Test: W operator sets clipping path (nonzero winding).
/// Note: Clipping doesn't immediately paint; it sets up for subsequent operations.
#[test]
fn test_interpreter_do_W_clipping() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_re(0.0, 0.0, 100.0, 100.0);
    interp.do_W();

    // W doesn't clear path or paint - it just marks for clipping
    // Path should still be present
    assert!(!interp.current_path().is_empty());
}

/// Test: W* operator sets clipping path (even-odd rule).
#[test]
fn test_interpreter_do_W_star_clipping_evenodd() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_re(0.0, 0.0, 100.0, 100.0);
    interp.do_W_star();

    // W* doesn't clear path or paint
    assert!(!interp.current_path().is_empty());
}

// ============================================================================
// Integration tests - combining operators
// ============================================================================

/// Test: Complete path drawing sequence.
#[test]
fn test_interpreter_complete_path_sequence() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();

    {
        let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);
        interp.init_state(MATRIX_IDENTITY);

        // Save state
        interp.do_q();

        // Set up graphics state
        interp.do_w(2.0);
        interp.do_J(1);
        interp.do_j(1);

        // Transform
        interp.do_cm(1.0, 0.0, 0.0, 1.0, 50.0, 50.0);

        // Build path
        interp.do_m(0.0, 0.0);
        interp.do_l(100.0, 0.0);
        interp.do_l(100.0, 100.0);
        interp.do_l(0.0, 100.0);
        interp.do_h();

        // Fill and stroke
        interp.do_B();

        // Restore state
        interp.do_Q();

        // Verify state was restored
        assert_eq!(interp.graphicstate().linewidth, 0.0); // default
    }

    // Verify device received the painted path
    assert_eq!(device.paths_painted.len(), 1);
}

// ============================================================================
// Text Operators tests - BT/ET (Text Object)
// ============================================================================

/// Test: BT operator resets text matrix and line matrix.
#[test]
fn test_interpreter_do_BT_resets_text_state() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);

    // Modify text state matrix (simulate prior text operations)
    interp.textstate_mut().matrix = (2.0, 0.0, 0.0, 2.0, 100.0, 200.0);
    interp.textstate_mut().linematrix = (50.0, 75.0);

    // BT should reset matrices to identity
    interp.do_BT();

    let ts = interp.textstate();
    assert_eq!(
        ts.matrix, MATRIX_IDENTITY,
        "Text matrix should be identity after BT"
    );
    assert_eq!(
        ts.linematrix,
        (0.0, 0.0),
        "Line matrix should be (0,0) after BT"
    );
}

/// Test: ET operator ends text object (no-op but should not panic).
#[test]
fn test_interpreter_do_ET_ends_text_object() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    // ET should complete without error
    interp.do_ET();
}

/// Test: BT/ET pair preserves text state parameters (only matrices reset).
#[test]
fn test_interpreter_BT_preserves_text_parameters() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);

    // Set text state parameters
    interp.do_Tc(0.5); // character spacing
    interp.do_Tw(1.0); // word spacing
    interp.do_Tz(150.0); // horizontal scaling
    interp.do_TL(12.0); // text leading
    interp.do_Tr(1); // render mode
    interp.do_Ts(2.0); // text rise

    // BT should only reset matrices, not these parameters
    interp.do_BT();

    let ts = interp.textstate();
    assert_eq!(ts.charspace, 0.5, "Character spacing should be preserved");
    assert_eq!(ts.wordspace, 1.0, "Word spacing should be preserved");
    assert_eq!(ts.scaling, 150.0, "Scaling should be preserved");
    assert_eq!(ts.leading, -12.0, "Leading should be preserved (negated)");
    assert_eq!(ts.render, 1, "Render mode should be preserved");
    assert_eq!(ts.rise, 2.0, "Text rise should be preserved");
}

// ============================================================================
// Text State Operators tests - Tc, Tw, Tz, TL, Tf, Tr, Ts
// ============================================================================

/// Test: Tc operator sets character spacing.
#[test]
fn test_interpreter_do_Tc_sets_charspace() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_Tc(2.5);

    assert_eq!(interp.textstate().charspace, 2.5);
}

/// Test: Tw operator sets word spacing.
#[test]
fn test_interpreter_do_Tw_sets_wordspace() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_Tw(1.5);

    assert_eq!(interp.textstate().wordspace, 1.5);
}

/// Test: Tz operator sets horizontal scaling.
#[test]
fn test_interpreter_do_Tz_sets_scaling() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_Tz(200.0); // 200% scaling

    assert_eq!(interp.textstate().scaling, 200.0);
}

/// Test: TL operator sets text leading (negated per Python implementation).
#[test]
fn test_interpreter_do_TL_sets_leading() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_TL(14.0);

    // Python: self.textstate.leading = -leading
    assert_eq!(interp.textstate().leading, -14.0);
}

/// Test: Tf operator sets font and fontsize.
#[test]
fn test_interpreter_do_Tf_sets_font_and_size() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_Tf("F1", 12.0);

    assert_eq!(interp.textstate().fontsize, 12.0);
    // Font would be looked up from fontmap - for now just verify size
}

/// Test: Tr operator sets text rendering mode.
#[test]
fn test_interpreter_do_Tr_sets_render_mode() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_Tr(2); // Render mode 2 = fill then stroke

    assert_eq!(interp.textstate().render, 2);
}

/// Test: Ts operator sets text rise.
#[test]
fn test_interpreter_do_Ts_sets_rise() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_Ts(5.0); // 5 point superscript

    assert_eq!(interp.textstate().rise, 5.0);
}

/// Test: Ts with negative value for subscript.
#[test]
fn test_interpreter_do_Ts_negative_for_subscript() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_Ts(-3.0);

    assert_eq!(interp.textstate().rise, -3.0);
}

// ============================================================================
// Text Positioning Operators tests - Td, TD, Tm, T*
// ============================================================================

/// Test: Td operator moves text position.
#[test]
fn test_interpreter_do_Td_moves_text_position() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    interp.do_Td(100.0, 200.0);

    let ts = interp.textstate();
    // With identity matrix: e_new = tx*a + ty*c + e = 100*1 + 200*0 + 0 = 100
    //                       f_new = tx*b + ty*d + f = 100*0 + 200*1 + 0 = 200
    assert_eq!(ts.matrix.4, 100.0, "e component should be 100");
    assert_eq!(ts.matrix.5, 200.0, "f component should be 200");
    assert_eq!(ts.linematrix, (0.0, 0.0), "Line matrix reset after Td");
}

/// Test: Td with existing text matrix.
#[test]
fn test_interpreter_do_Td_with_existing_matrix() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();

    // Set initial text matrix with translation
    interp.do_Tm(1.0, 0.0, 0.0, 1.0, 50.0, 50.0);

    // Now apply Td
    interp.do_Td(10.0, 20.0);

    let ts = interp.textstate();
    // e_new = 10*1 + 20*0 + 50 = 60
    // f_new = 10*0 + 20*1 + 50 = 70
    assert_eq!(ts.matrix.4, 60.0);
    assert_eq!(ts.matrix.5, 70.0);
}

/// Test: TD operator moves text and sets leading.
#[test]
fn test_interpreter_do_TD_moves_and_sets_leading() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    interp.do_TD(0.0, -14.0);

    let ts = interp.textstate();
    // Position update
    assert_eq!(ts.matrix.5, -14.0, "f component should be -14");
    // Leading should be set to ty (not negated like TL)
    assert_eq!(ts.leading, -14.0, "Leading should be set to ty value");
}

/// Test: Tm operator sets text matrix directly.
#[test]
fn test_interpreter_do_Tm_sets_text_matrix() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    interp.do_Tm(2.0, 0.0, 0.0, 2.0, 100.0, 200.0);

    let ts = interp.textstate();
    assert_eq!(ts.matrix, (2.0, 0.0, 0.0, 2.0, 100.0, 200.0));
    assert_eq!(ts.linematrix, (0.0, 0.0), "Line matrix reset after Tm");
}

/// Test: T* operator moves to next line using leading.
#[test]
fn test_interpreter_do_T_star_moves_to_next_line() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    interp.do_TL(14.0); // Sets leading to -14
    interp.do_Tm(1.0, 0.0, 0.0, 1.0, 0.0, 700.0); // Start at y=700

    interp.do_T_star();

    let ts = interp.textstate();
    // T* does: e_new = leading*c + e, f_new = leading*d + f
    // With identity-like matrix (a=1, b=0, c=0, d=1):
    // e_new = -14*0 + 0 = 0
    // f_new = -14*1 + 700 = 686
    assert_eq!(ts.matrix.5, 686.0, "Y position should move by leading");
}

/// Test: Multiple T* calls move down correctly.
#[test]
fn test_interpreter_do_T_star_multiple_lines() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    interp.do_TL(12.0); // Leading of -12
    interp.do_Tm(1.0, 0.0, 0.0, 1.0, 0.0, 600.0);

    interp.do_T_star(); // Move to 588
    interp.do_T_star(); // Move to 576
    interp.do_T_star(); // Move to 564

    let ts = interp.textstate();
    assert_eq!(ts.matrix.5, 564.0);
}

// ============================================================================
// Text Showing Operators tests - Tj, TJ, ', "
// ============================================================================

use bolivar_core::pdfdevice::PDFTextSeq;

/// Extended mock device that records text rendering calls.
#[derive(Debug, Default)]
struct TextMockDevice {
    ctm: Option<Matrix>,
    paths_painted: Vec<(bool, bool, bool, Vec<PathSegment>)>,
    text_rendered: Vec<PDFTextSeq>,
}

impl PDFDevice for TextMockDevice {
    fn set_ctm(&mut self, ctm: Matrix) {
        self.ctm = Some(ctm);
    }

    fn ctm(&self) -> Option<Matrix> {
        self.ctm
    }

    fn begin_page(&mut self, _pageid: u32, _mediabox: (f64, f64, f64, f64), _ctm: Matrix) {}
    fn end_page(&mut self, _pageid: u32) {}

    fn paint_path(
        &mut self,
        _graphicstate: &PDFGraphicState,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        path: &[PathSegment],
    ) {
        self.paths_painted
            .push((stroke, fill, evenodd, path.to_vec()));
    }

    fn render_string(
        &mut self,
        _textstate: &mut bolivar_core::pdfstate::PDFTextState,
        seq: &PDFTextSeq,
        _ncs: &bolivar_core::pdfcolor::PDFColorSpace,
        _graphicstate: &PDFGraphicState,
    ) {
        self.text_rendered.push(seq.clone());
    }
}

/// Test: Tj operator shows text string.
#[test]
fn test_interpreter_do_Tj_shows_text() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = TextMockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    interp.do_Tf("F1", 12.0);
    interp.do_Tj(b"Hello".to_vec());
    interp.do_ET();

    // Device should have received the text
    assert_eq!(device.text_rendered.len(), 1);
}

/// Test: TJ operator shows text with positioning.
#[test]
fn test_interpreter_do_TJ_shows_positioned_text() {
    use bolivar_core::pdfdevice::PDFTextSeqItem;

    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = TextMockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    interp.do_Tf("F1", 12.0);

    // TJ array: [(H) 50 (ello)]
    let seq = vec![
        PDFTextSeqItem::Bytes(b"H".to_vec()),
        PDFTextSeqItem::Number(50.0),
        PDFTextSeqItem::Bytes(b"ello".to_vec()),
    ];
    interp.do_TJ(seq);
    interp.do_ET();

    assert_eq!(device.text_rendered.len(), 1);
    assert_eq!(device.text_rendered[0].len(), 3);
}

/// Test: ' (quote) operator moves to next line and shows text.
#[test]
fn test_interpreter_do_quote_moves_and_shows() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = TextMockDevice::default();

    let final_y;
    {
        let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

        interp.init_state(MATRIX_IDENTITY);
        interp.do_BT();
        interp.do_TL(14.0);
        interp.do_Tf("F1", 12.0);
        interp.do_Tm(1.0, 0.0, 0.0, 1.0, 0.0, 700.0);

        interp.do_quote(b"Next line".to_vec());
        interp.do_ET();

        // Position should have moved (T* was called internally)
        final_y = interp.textstate().matrix.5;
    }

    // Text should be rendered
    assert_eq!(device.text_rendered.len(), 1);
    assert_eq!(final_y, 686.0);
}

/// Test: " (doublequote) operator sets spacing, moves, and shows text.
#[test]
fn test_interpreter_do_doublequote_sets_spacing_and_shows() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = TextMockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);
    interp.do_BT();
    interp.do_TL(14.0);
    interp.do_Tf("F1", 12.0);

    // " operator: aw ac string => Tw(aw), Tc(ac), '(string)
    interp.do_doublequote(1.0, 0.5, b"Spaced text".to_vec());
    interp.do_ET();

    // Word spacing and char spacing should be set
    assert_eq!(interp.textstate().wordspace, 1.0);
    assert_eq!(interp.textstate().charspace, 0.5);
    // Text should be rendered
    assert_eq!(device.text_rendered.len(), 1);
}

// ============================================================================
// Text state persistence through q/Q
// ============================================================================

/// Test: Text state is saved and restored with q/Q.
#[test]
fn test_interpreter_text_state_saved_with_gsave() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = MockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);

    // Set text state
    interp.do_Tc(1.0);
    interp.do_Tw(2.0);
    interp.do_Tz(150.0);
    interp.do_Ts(3.0);

    // Save
    interp.do_q();

    // Modify
    interp.do_Tc(5.0);
    interp.do_Tw(6.0);
    interp.do_Tz(200.0);
    interp.do_Ts(7.0);

    assert_eq!(interp.textstate().charspace, 5.0);

    // Restore
    interp.do_Q();

    // Should be back to original values
    assert_eq!(interp.textstate().charspace, 1.0);
    assert_eq!(interp.textstate().wordspace, 2.0);
    assert_eq!(interp.textstate().scaling, 150.0);
    assert_eq!(interp.textstate().rise, 3.0);
}

// ============================================================================
// Integration test - complete text rendering sequence
// ============================================================================

/// Test: Complete text rendering sequence simulating real PDF content.
#[test]
fn test_interpreter_complete_text_sequence() {
    let mut rsrcmgr = PDFResourceManager::new();
    let mut device = TextMockDevice::default();
    let mut interp = PDFPageInterpreter::new(&mut rsrcmgr, &mut device);

    interp.init_state(MATRIX_IDENTITY);

    // Typical PDF text sequence
    interp.do_q();
    interp.do_BT();
    interp.do_Tf("F1", 12.0);
    interp.do_TL(14.0);
    interp.do_Tm(1.0, 0.0, 0.0, 1.0, 72.0, 720.0);

    interp.do_Tj(b"First line".to_vec());
    interp.do_T_star();
    interp.do_Tj(b"Second line".to_vec());

    interp.do_ET();
    interp.do_Q();

    // Two text rendering calls
    assert_eq!(device.text_rendered.len(), 2);
}
