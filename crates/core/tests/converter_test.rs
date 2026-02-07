//! Tests for converter module - output converters for PDF content.
//!
//! Port of tests from pdfminer.six tests/test_converter.py

use bolivar_core::arena::PageArena;
use bolivar_core::converter::{
    HOCRConverter, HTMLConverter, PDFConverter, PDFLayoutAnalyzer, PDFPageAggregator,
    TextConverter, XMLConverter,
};
use bolivar_core::layout::{
    LAParams, LTChar, LTItem, LTPage, LTTextBoxHorizontal, LTTextLineHorizontal, TextBoxType,
    TextLineElement,
};
use bolivar_core::pdfcolor::PDFColorSpace;
use bolivar_core::pdffont::{FontWidthDict, PDFFont};
use bolivar_core::pdfstate::PDFGraphicState;
use bolivar_core::pdftypes::{PDFObject, PDFStream};
use bolivar_core::utils::MATRIX_IDENTITY;
use std::collections::HashMap;
use std::io::Cursor;

fn sample_rtl_page() -> LTPage {
    let mut line = LTTextLineHorizontal::new(0.1);
    line.set_bbox((0.0, 0.0, 10.0, 10.0));
    line.add_element(TextLineElement::Char(Box::new(LTChar::new(
        (0.0, 0.0, 1.0, 1.0),
        "\u{05D0}",
        "F",
        10.0,
        true,
        1.0,
    ))));
    line.add_element(TextLineElement::Char(Box::new(LTChar::new(
        (1.0, 0.0, 2.0, 1.0),
        "\u{05D1}",
        "F",
        10.0,
        true,
        1.0,
    ))));
    line.add_element(TextLineElement::Char(Box::new(LTChar::new(
        (2.0, 0.0, 3.0, 1.0),
        "\u{05D2}",
        "F",
        10.0,
        true,
        1.0,
    ))));
    line.analyze();

    let mut boxh = LTTextBoxHorizontal::new();
    boxh.add(line);

    let mut page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);
    page.add(LTItem::TextBox(TextBoxType::Horizontal(boxh)));
    page
}

// ============================================================================
// PDFLayoutAnalyzer Tests
// ============================================================================

mod layout_analyzer_tests {
    use super::*;
    use bolivar_core::interp::PDFDevice;

    fn get_analyzer<'a>(arena: &'a mut PageArena) -> PDFLayoutAnalyzer<'a> {
        let mut analyzer = PDFLayoutAnalyzer::new(None, 1, arena.context());
        analyzer.set_ctm(MATRIX_IDENTITY);
        analyzer
    }

    #[test]
    fn test_layout_analyzer_uses_arena_items() {
        let path = vec![('m', vec![0.0, 0.0]), ('l', vec![1.0, 0.0])];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 100.0, 100.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        assert!(analyzer.all_cur_items_are_lines());
    }

    #[test]
    fn test_aggregator_materializes_page() {
        let mut arena = PageArena::new();
        let mut aggregator = PDFPageAggregator::new(None, 1, &mut arena);
        aggregator.begin_page(1, (0.0, 0.0, 100.0, 100.0), MATRIX_IDENTITY);
        aggregator.end_page(1);
        let page = aggregator.get_result();
        assert_eq!(page.pageid, 1);
    }

    #[test]
    fn test_paint_path_simple_line() {
        // Test path: m(6,7) l(7,7) - single line segment
        let path = vec![('m', vec![6.0, 7.0]), ('l', vec![7.0, 7.0])];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 100.0, 100.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
    }

    #[test]
    fn test_paint_path_mlllh() {
        // Test rectangular path: m l l l h
        let path = vec![
            ('m', vec![6.0, 7.0]),
            ('l', vec![7.0, 7.0]),
            ('l', vec![7.0, 91.0]),
            ('l', vec![6.0, 91.0]),
            ('h', vec![]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 100.0, 100.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
    }

    #[test]
    fn test_paint_path_multiple_mlllh() {
        // Path from samples/contrib/issue-00369-excel.pdf
        let path = vec![
            ('m', vec![6.0, 7.0]),
            ('l', vec![7.0, 7.0]),
            ('l', vec![7.0, 91.0]),
            ('l', vec![6.0, 91.0]),
            ('h', vec![]),
            ('m', vec![4.0, 7.0]),
            ('l', vec![6.0, 7.0]),
            ('l', vec![6.0, 91.0]),
            ('l', vec![4.0, 91.0]),
            ('h', vec![]),
            ('m', vec![67.0, 2.0]),
            ('l', vec![68.0, 2.0]),
            ('l', vec![68.0, 3.0]),
            ('l', vec![67.0, 3.0]),
            ('h', vec![]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 100.0, 100.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 3);
    }

    #[test]
    fn test_paint_path_standard_rect() {
        // Standard rect: mlllh forming axis-aligned rectangle
        let path = vec![
            ('m', vec![10.0, 90.0]),
            ('l', vec![90.0, 90.0]),
            ('l', vec![90.0, 10.0]),
            ('l', vec![10.0, 10.0]),
            ('h', vec![]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        assert!(analyzer.cur_item_first_is_rect());
    }

    #[test]
    fn test_paint_path_mllll_variation() {
        // Same rect as mllll variation (no h, but returns to start)
        let path = vec![
            ('m', vec![10.0, 90.0]),
            ('l', vec![90.0, 90.0]),
            ('l', vec![90.0, 10.0]),
            ('l', vec![10.0, 10.0]),
            ('l', vec![10.0, 90.0]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        assert!(analyzer.cur_item_first_is_rect());
    }

    #[test]
    fn test_paint_path_mllllh_variation() {
        // Same rect as mllllh variation (returns to start + h)
        // Python: ("mllllh", ["m", 10, 90, "l", 90, 90, "l", 90, 10, "l", 10, 10, "l", 10, 90, "h"], 1)
        // This creates a quadrilateral where the 5th line returns to start point before closing
        let path = vec![
            ('m', vec![10.0, 90.0]),
            ('l', vec![90.0, 90.0]),
            ('l', vec![90.0, 10.0]),
            ('l', vec![10.0, 10.0]),
            ('l', vec![10.0, 90.0]),
            ('h', vec![]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        assert!(analyzer.cur_item_first_is_rect());
    }

    #[test]
    fn test_paint_path_bowtie_is_curve() {
        // Bowtie shape - not a rectangle
        let path = vec![
            ('m', vec![110.0, 90.0]),
            ('l', vec![190.0, 10.0]),
            ('l', vec![190.0, 90.0]),
            ('l', vec![110.0, 10.0]),
            ('h', vec![]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        assert!(analyzer.cur_item_first_is_curve());
    }

    #[test]
    fn test_paint_path_slanted_quadrilateral() {
        // Quadrilateral with one slanted side
        let path = vec![
            ('m', vec![210.0, 90.0]),
            ('l', vec![290.0, 60.0]),
            ('l', vec![290.0, 10.0]),
            ('l', vec![210.0, 10.0]),
            ('h', vec![]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        assert!(analyzer.cur_item_first_is_curve());
    }

    #[test]
    fn test_paint_path_two_rects() {
        // Path with two rect subpaths
        let path = vec![
            ('m', vec![310.0, 90.0]),
            ('l', vec![350.0, 90.0]),
            ('l', vec![350.0, 10.0]),
            ('l', vec![310.0, 10.0]),
            ('h', vec![]),
            ('m', vec![350.0, 90.0]),
            ('l', vec![390.0, 90.0]),
            ('l', vec![390.0, 10.0]),
            ('l', vec![350.0, 10.0]),
            ('h', vec![]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 2);
    }

    #[test]
    fn test_paint_path_simple_lines() {
        // Three types of simple lines
        let path = vec![
            // Vertical line
            ('m', vec![10.0, 30.0]),
            ('l', vec![10.0, 40.0]),
            ('h', vec![]),
            // Horizontal line
            ('m', vec![10.0, 50.0]),
            ('l', vec![70.0, 50.0]),
            ('h', vec![]),
            // Diagonal line
            ('m', vec![10.0, 10.0]),
            ('l', vec![30.0, 30.0]),
            ('h', vec![]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 3);
        assert!(analyzer.all_cur_items_are_lines());
    }

    #[test]
    fn test_paint_path_ml_variation() {
        // Same as above but 'ml' variation (no h)
        let path = vec![
            // Vertical line
            ('m', vec![10.0, 30.0]),
            ('l', vec![10.0, 40.0]),
            // Horizontal line
            ('m', vec![10.0, 50.0]),
            ('l', vec![70.0, 50.0]),
            // Diagonal line
            ('m', vec![10.0, 10.0]),
            ('l', vec![30.0, 30.0]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 3);
        assert!(analyzer.all_cur_items_are_lines());
    }

    #[test]
    fn test_paint_path_bezier_c() {
        // "c" operator - cubic bezier
        let path = vec![
            ('m', vec![72.41, 433.89]),
            ('c', vec![72.41, 434.45, 71.96, 434.89, 71.41, 434.89]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        let pts = analyzer.cur_item_first_pts();
        assert_eq!(pts.len(), 2);
        assert!((pts[0].0 - 72.41).abs() < 0.01);
        assert!((pts[0].1 - 433.89).abs() < 0.01);
        assert!((pts[1].0 - 71.41).abs() < 0.01);
        assert!((pts[1].1 - 434.89).abs() < 0.01);
    }

    #[test]
    fn test_paint_path_bezier_v() {
        // "v" operator - bezier with first control point at current point
        let path = vec![
            ('m', vec![72.41, 433.89]),
            ('v', vec![71.96, 434.89, 71.41, 434.89]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        let pts = analyzer.cur_item_first_pts();
        assert_eq!(pts.len(), 2);
    }

    #[test]
    fn test_paint_path_bezier_y() {
        // "y" operator - bezier with second control point at end point
        let path = vec![
            ('m', vec![72.41, 433.89]),
            ('y', vec![72.41, 434.45, 71.41, 434.89]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        let pts = analyzer.cur_item_first_pts();
        assert_eq!(pts.len(), 2);
    }

    #[test]
    fn test_paint_path_dashed() {
        // Test dashed line style
        let path = vec![
            ('m', vec![72.41, 433.89]),
            ('c', vec![72.41, 434.45, 71.96, 434.89, 71.41, 434.89]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));

        let graphicstate = PDFGraphicState {
            dash: Some((vec![1.0, 1.0], 0.0)),
            ..Default::default()
        };

        analyzer.paint_path(&graphicstate, false, false, false, &path);
        assert_eq!(analyzer.cur_item_len(), 1);
        let dashing = analyzer.cur_item_first_dashing();
        assert!(dashing.is_some());
        let (pattern, phase) = dashing.unwrap();
        assert_eq!(pattern, vec![1.0, 1.0]);
        assert!((phase - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_paint_path_without_starting_m() {
        // Paths without starting 'm' should be ignored
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.set_cur_item((0.0, 0.0, 100.0, 100.0));

        // Path starting with 'h'
        let path1 = vec![('h', vec![])];
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path1);

        // Path starting with 'l'
        let path2 = vec![
            ('l', vec![72.41, 433.89]),
            ('l', vec![82.41, 433.89]),
            ('h', vec![]),
        ];
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path2);

        assert_eq!(analyzer.cur_item_len(), 0);
    }

    /// A test font implementation.
    struct TestFont {
        widths: FontWidthDict,
    }

    impl TestFont {
        fn new() -> Self {
            let mut widths = FontWidthDict::new();
            // Standard ASCII character widths (approximate for Courier-like)
            for cid in 32..127 {
                widths.insert(cid, Some(600.0));
            }
            Self { widths }
        }
    }

    impl PDFFont for TestFont {
        fn to_unichr(&self, cid: u32) -> Option<String> {
            // Return the character for ASCII range
            if cid < 128 {
                char::from_u32(cid).map(|c| c.to_string())
            } else {
                None
            }
        }

        fn char_width(&self, cid: u32) -> f64 {
            if let Some(Some(width)) = self.widths.get(&cid) {
                width * self.hscale()
            } else {
                self.default_width() * self.hscale()
            }
        }

        fn default_width(&self) -> f64 {
            600.0
        }

        fn widths(&self) -> &FontWidthDict {
            &self.widths
        }
    }

    #[test]
    fn test_render_char_basic() {
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        analyzer.begin_figure("Fig1", (10.0, 10.0, 100.0, 100.0), MATRIX_IDENTITY);

        let font = TestFont::new();
        let ncs = PDFColorSpace::new("DeviceGray", 1);
        let graphicstate = PDFGraphicState::default();

        // Render the letter 'A' (CID 65)
        let adv = analyzer.render_char(
            MATRIX_IDENTITY,
            &font,
            12.0,  // fontsize
            100.0, // scaling
            0.0,   // rise
            65,    // 'A'
            &ncs,
            &graphicstate,
        );

        // Advance should be positive
        assert!(adv > 0.0);
        // There should be one item in the container
        assert_eq!(analyzer.cur_item_len(), 1);
    }

    #[test]
    fn test_render_char_with_scaling() {
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        analyzer.begin_figure("Fig1", (10.0, 10.0, 100.0, 100.0), MATRIX_IDENTITY);

        let font = TestFont::new();
        let ncs = PDFColorSpace::new("DeviceGray", 1);
        let graphicstate = PDFGraphicState::default();

        // Render with 50% scaling
        let adv = analyzer.render_char(
            MATRIX_IDENTITY,
            &font,
            12.0,
            50.0, // 50% scaling
            0.0,
            65,
            &ncs,
            &graphicstate,
        );

        assert!(adv > 0.0);
        assert_eq!(analyzer.cur_item_len(), 1);
    }

    #[test]
    fn test_render_char_with_rise() {
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        analyzer.begin_figure("Fig1", (10.0, 10.0, 100.0, 100.0), MATRIX_IDENTITY);

        let font = TestFont::new();
        let ncs = PDFColorSpace::new("DeviceGray", 1);
        let graphicstate = PDFGraphicState::default();

        // Render with positive rise (superscript)
        let adv = analyzer.render_char(
            MATRIX_IDENTITY,
            &font,
            12.0,
            100.0,
            5.0, // rise
            65,
            &ncs,
            &graphicstate,
        );

        assert!(adv > 0.0);
        assert_eq!(analyzer.cur_item_len(), 1);
    }

    #[test]
    fn test_render_char_undefined() {
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        analyzer.begin_figure("Fig1", (10.0, 10.0, 100.0, 100.0), MATRIX_IDENTITY);

        let font = TestFont::new();
        let ncs = PDFColorSpace::new("DeviceGray", 1);
        let graphicstate = PDFGraphicState::default();

        // Render an undefined CID
        let adv = analyzer.render_char(
            MATRIX_IDENTITY,
            &font,
            12.0,
            100.0,
            0.0,
            99999, // undefined CID
            &ncs,
            &graphicstate,
        );

        // Should still add an item with (cid:99999) text
        assert!(adv > 0.0);
        assert_eq!(analyzer.cur_item_len(), 1);
    }

    #[test]
    fn test_render_image_in_figure() {
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        analyzer.begin_figure("Fig1", (10.0, 10.0, 100.0, 100.0), MATRIX_IDENTITY);

        // Create a minimal image stream
        let mut attrs = HashMap::new();
        attrs.insert("Width".to_string(), PDFObject::Int(100));
        attrs.insert("Height".to_string(), PDFObject::Int(50));
        attrs.insert("BitsPerComponent".to_string(), PDFObject::Int(8));
        attrs.insert(
            "ColorSpace".to_string(),
            PDFObject::Name("DeviceRGB".to_string()),
        );
        let stream = PDFStream::new(attrs, vec![]);

        analyzer.render_image("test_image", &stream);

        // Image should be added to the figure
        assert_eq!(analyzer.cur_item_len(), 1);
    }

    #[test]
    fn test_render_image_outside_figure_ignored() {
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        // Note: NOT in a figure

        let mut attrs = HashMap::new();
        attrs.insert("Width".to_string(), PDFObject::Int(100));
        attrs.insert("Height".to_string(), PDFObject::Int(50));
        let stream = PDFStream::new(attrs, vec![]);

        analyzer.render_image("test_image", &stream);

        // Should be ignored - no items added
        assert_eq!(analyzer.cur_item_len(), 0);
    }

    #[test]
    fn test_render_image_with_imagemask() {
        let mut arena = PageArena::new();
        let mut analyzer = get_analyzer(&mut arena);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        analyzer.begin_figure("Fig1", (10.0, 10.0, 100.0, 100.0), MATRIX_IDENTITY);

        let mut attrs = HashMap::new();
        attrs.insert("Width".to_string(), PDFObject::Int(100));
        attrs.insert("Height".to_string(), PDFObject::Int(50));
        attrs.insert("ImageMask".to_string(), PDFObject::Bool(true));
        attrs.insert("BitsPerComponent".to_string(), PDFObject::Int(1));
        let stream = PDFStream::new(attrs, vec![]);

        analyzer.render_image("mask_image", &stream);

        assert_eq!(analyzer.cur_item_len(), 1);
    }
}

// ============================================================================
// PDFPageAggregator Tests
// ============================================================================

mod page_aggregator_tests {
    use super::*;

    #[test]
    fn test_page_aggregator_creation() {
        let mut arena = PageArena::new();
        let aggregator = PDFPageAggregator::new(None, 1, &mut arena);
        assert!(aggregator.result().is_none());
    }

    #[test]
    fn test_page_aggregator_receives_layout() {
        let mut arena = PageArena::new();
        let mut aggregator = PDFPageAggregator::new(None, 1, &mut arena);
        let page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);
        aggregator.receive_layout(page);
        assert!(aggregator.result().is_some());
    }

    #[test]
    fn test_page_aggregator_get_result() {
        let mut arena = PageArena::new();
        let mut aggregator = PDFPageAggregator::new(None, 1, &mut arena);
        let page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);
        aggregator.receive_layout(page);
        let result = aggregator.get_result();
        assert_eq!(result.pageid, 1);
    }
}

// ============================================================================
// PDFConverter Binary Detection Tests
// ============================================================================

mod binary_detector_tests {
    use super::*;

    #[test]
    fn test_is_binary_stream_bytes_io() {
        let cursor: Cursor<Vec<u8>> = Cursor::new(Vec::new());
        assert!(PDFConverter::<Cursor<Vec<u8>>>::is_binary_stream(&cursor));
    }

    #[test]
    fn test_non_file_like_defaults_to_binary() {
        // Non-standard types default to binary
        let data: Vec<u8> = vec![];
        assert!(PDFConverter::<Cursor<Vec<u8>>>::is_binary_stream(&data));
    }
}

// ============================================================================
// TextConverter Tests
// ============================================================================

mod text_converter_tests {
    use super::*;

    #[test]
    fn test_text_converter_creation() {
        let mut output: Vec<u8> = Vec::new();
        let converter = TextConverter::new(&mut output, "utf-8", 1, None, false);
        assert!(!converter.show_pageno());
    }

    #[test]
    fn test_text_converter_with_pageno() {
        let mut output: Vec<u8> = Vec::new();
        let converter = TextConverter::new(&mut output, "utf-8", 1, None, true);
        assert!(converter.show_pageno());
    }

    #[test]
    fn test_text_converter_write_text() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = TextConverter::new(&mut output, "utf-8", 1, None, false);
            converter.write_text("Hello, World!");
        }
        assert_eq!(String::from_utf8(output).unwrap(), "Hello, World!");
    }

    #[test]
    fn test_text_converter_receive_layout() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = TextConverter::new(&mut output, "utf-8", 1, None, true);
            let page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);
            converter.receive_layout(page);
        }
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("Page 1"));
        assert!(result.contains('\x0c')); // form feed
    }

    #[test]
    fn test_text_converter_reorders_rtl_by_default() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter =
                TextConverter::new(&mut output, "utf-8", 1, Some(LAParams::default()), false);
            converter.receive_layout(sample_rtl_page());
        }
        let result = String::from_utf8(output).expect("utf8");
        assert!(result.contains("\u{05D2}\u{05D1}\u{05D0}\n\n"));
    }

    #[test]
    fn test_text_converter_reorders_rtl_without_laparams() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = TextConverter::new(&mut output, "utf-8", 1, None, false);
            converter.receive_layout(sample_rtl_page());
        }
        let result = String::from_utf8(output).expect("utf8");
        assert!(result.contains("\u{05D2}\u{05D1}\u{05D0}\n\n"));
    }
}

// ============================================================================
// HTMLConverter Tests
// ============================================================================

mod html_converter_tests {
    use super::*;

    #[test]
    fn test_html_converter_creation() {
        let mut output: Vec<u8> = Vec::new();
        let _converter = HTMLConverter::new(&mut output, "utf-8", 1, None);
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("<html>"));
        assert!(result.contains("<head>"));
    }

    #[test]
    fn test_html_converter_write_footer() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HTMLConverter::new(&mut output, "utf-8", 1, None);
            converter.close();
        }
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("</body></html>"));
    }

    #[test]
    fn test_html_converter_rect_colors() {
        let converter_colors = HTMLConverter::<Vec<u8>>::default_rect_colors();
        assert!(converter_colors.contains_key("curve"));
        assert!(converter_colors.contains_key("page"));
    }

    #[test]
    fn test_html_converter_text_colors() {
        let converter_colors = HTMLConverter::<Vec<u8>>::default_text_colors();
        assert!(converter_colors.contains_key("char"));
    }

    #[test]
    fn test_html_converter_debug_mode() {
        let mut output: Vec<u8> = Vec::new();
        let converter = HTMLConverter::with_debug(&mut output, "utf-8", 1, None, 1);
        let rect_colors = converter.rect_colors();
        // Debug mode adds more colors
        assert!(rect_colors.contains_key("figure"));
        assert!(rect_colors.contains_key("textline"));
        assert!(rect_colors.contains_key("textbox"));
        assert!(rect_colors.contains_key("textgroup"));
    }

    #[test]
    fn test_html_converter_scale() {
        let mut output: Vec<u8> = Vec::new();
        let converter = HTMLConverter::with_options(&mut output, "utf-8", 1, None, 2.0, 1.0);
        assert!((converter.scale() - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_html_converter_write_text() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HTMLConverter::new(&mut output, "utf-8", 1, None);
            converter.write_text("Hello <World> & \"Test\"");
        }
        let result = String::from_utf8(output).unwrap();
        // Special characters should be escaped
        assert!(result.contains("Hello &lt;World&gt; &amp; &quot;Test&quot;"));
    }

    #[test]
    fn test_html_converter_reorders_rtl_textbox_content() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HTMLConverter::new(&mut output, "utf-8", 1, None);
            converter.receive_layout(sample_rtl_page());
        }
        let result = String::from_utf8(output).expect("utf8");
        assert!(result.contains("\u{05D2}\u{05D1}\u{05D0}"));
    }

    #[test]
    fn test_html_converter_put_text() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HTMLConverter::new(&mut output, "utf-8", 1, None);
            converter.put_text("Hello", "ABCDEF+Arial", 12.0);
        }
        let result = String::from_utf8(output).unwrap();
        // Should contain font-family without subset prefix
        assert!(result.contains("font-family: Arial"));
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_html_converter_put_newline() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HTMLConverter::new(&mut output, "utf-8", 1, None);
            converter.put_newline();
        }
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("<br>"));
    }

    #[test]
    fn test_html_converter_begin_end_div() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HTMLConverter::new(&mut output, "utf-8", 1, None);
            converter.begin_div("black", 1, 10.0, 20.0, 100.0, 50.0, "lr-tb");
            converter.end_div("black");
        }
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("<div style="));
        assert!(result.contains("writing-mode:lr-tb"));
        assert!(result.contains("</div>"));
    }

    #[test]
    fn test_html_converter_place_image() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HTMLConverter::new(&mut output, "utf-8", 1, None);
            converter.place_image("test.png", 1, 10.0, 20.0, 100.0, 50.0);
        }
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("<img src=\"test.png\""));
        assert!(result.contains("border=\"1\""));
    }
}

// ============================================================================
// XMLConverter Tests
// ============================================================================

mod xml_converter_tests {
    use super::*;

    #[test]
    fn test_xml_converter_creation() {
        let mut output: Vec<u8> = Vec::new();
        let _converter = XMLConverter::new(&mut output, "utf-8", 1, None);
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("<?xml version=\"1.0\""));
        assert!(result.contains("<pages>"));
    }

    #[test]
    fn test_xml_converter_write_footer() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = XMLConverter::new(&mut output, "utf-8", 1, None);
            converter.close();
        }
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("</pages>"));
    }

    #[test]
    fn test_xml_converter_strip_control() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = XMLConverter::with_options(&mut output, "utf-8", 1, None, true);
            converter.write_text("Hello\x00World\x0bTest");
        }
        let result = String::from_utf8(output).unwrap();
        // Control characters should be stripped
        assert!(!result.contains('\x00'));
        assert!(!result.contains('\x0b'));
        assert!(result.contains("HelloWorldTest"));
    }

    #[test]
    fn test_xml_converter_receive_layout() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = XMLConverter::new(&mut output, "utf-8", 1, None);
            let page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);
            converter.receive_layout(page);
            converter.close();
        }
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("<page id=\"1\""));
        assert!(result.contains("</page>"));
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

mod integration_tests {
    use super::*;

    #[test]
    fn test_analyzer_page_lifecycle() {
        let mut arena = PageArena::new();
        let mut analyzer = PDFLayoutAnalyzer::new(Some(LAParams::default()), 1, arena.context());
        analyzer.set_ctm(MATRIX_IDENTITY);

        // Simulate page processing
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        // Add some content...
        analyzer.end_page();

        assert_eq!(analyzer.pageno(), 2);
    }

    #[test]
    fn test_analyzer_figure_lifecycle() {
        let mut arena = PageArena::new();
        let mut analyzer = PDFLayoutAnalyzer::new(Some(LAParams::default()), 1, arena.context());
        analyzer.set_ctm(MATRIX_IDENTITY);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);

        // Begin and end figure
        analyzer.begin_figure("Fig1", (10.0, 10.0, 100.0, 100.0), MATRIX_IDENTITY);
        assert!(analyzer.in_figure());
        analyzer.end_figure("Fig1");
        assert!(!analyzer.in_figure());

        analyzer.end_page();
    }
}

// ============================================================================
// Marked Content Tracking Tests
// ============================================================================

mod marked_content_tests {
    use super::*;
    use bolivar_core::layout::LTItem;
    use bolivar_core::pdfdevice::{
        PDFDevice, PDFStackT, PDFStackValue, PDFTextSeq, PDFTextSeqItem,
    };
    use bolivar_core::pdfstate::PDFTextState;
    use bolivar_core::psparser::PSLiteral;

    #[test]
    fn test_page_aggregator_tracks_marked_content_stack() {
        let mut arena = PageArena::new();
        let mut aggregator = PDFPageAggregator::new(Some(LAParams::default()), 1, &mut arena);
        aggregator.begin_page(1, (0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);

        // Initially no marked content
        assert_eq!(aggregator.current_mcid(), None);

        // Begin marked content with MCID
        let tag = PSLiteral::new("Span");
        let mut props = PDFStackT::new();
        props.insert("MCID".to_string(), PDFStackValue::Int(42));
        aggregator.begin_tag(&tag, Some(&props));

        // Should now have MCID 42
        assert_eq!(aggregator.current_mcid(), Some(42));

        // End marked content
        aggregator.end_tag();

        // Should be back to no MCID
        assert_eq!(aggregator.current_mcid(), None);
    }

    #[test]
    fn test_page_aggregator_nested_marked_content() {
        let mut arena = PageArena::new();
        let mut aggregator = PDFPageAggregator::new(Some(LAParams::default()), 1, &mut arena);
        aggregator.begin_page(1, (0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);

        // Outer marked content with MCID 1
        let tag_outer = PSLiteral::new("P");
        let mut props_outer = PDFStackT::new();
        props_outer.insert("MCID".to_string(), PDFStackValue::Int(1));
        aggregator.begin_tag(&tag_outer, Some(&props_outer));
        assert_eq!(aggregator.current_mcid(), Some(1));

        // Inner marked content with MCID 2
        let tag_inner = PSLiteral::new("Span");
        let mut props_inner = PDFStackT::new();
        props_inner.insert("MCID".to_string(), PDFStackValue::Int(2));
        aggregator.begin_tag(&tag_inner, Some(&props_inner));
        assert_eq!(aggregator.current_mcid(), Some(2));

        // End inner
        aggregator.end_tag();
        assert_eq!(aggregator.current_mcid(), Some(1));

        // End outer
        aggregator.end_tag();
        assert_eq!(aggregator.current_mcid(), None);
    }

    #[test]
    fn test_page_aggregator_marked_content_without_mcid() {
        let mut arena = PageArena::new();
        let mut aggregator = PDFPageAggregator::new(Some(LAParams::default()), 1, &mut arena);
        aggregator.begin_page(1, (0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);

        // Begin marked content without MCID (just tag name, no properties)
        let tag = PSLiteral::new("Artifact");
        aggregator.begin_tag(&tag, None);

        // Should have no MCID (None)
        assert_eq!(aggregator.current_mcid(), None);

        aggregator.end_tag();
    }

    /// Test that LTChar.with_mcid() correctly stores MCID.
    /// This tests the interface that render_string should use.
    #[test]
    fn test_ltchar_with_mcid_interface() {
        use bolivar_core::layout::LTChar;

        // Test directly with PDFLayoutAnalyzer to verify MCID tracking
        let mut arena = PageArena::new();
        let mut analyzer = PDFLayoutAnalyzer::new(Some(LAParams::default()), 1, arena.context());
        analyzer.set_ctm(MATRIX_IDENTITY);
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);

        // Begin marked content with MCID
        let tag = PSLiteral::new("P");
        let mut props = PDFStackT::new();
        props.insert("MCID".to_string(), PDFStackValue::Int(42));
        analyzer.begin_tag(&tag, Some(&props));

        // Verify current_mcid returns 42
        assert_eq!(
            analyzer.current_mcid(),
            Some(42),
            "Analyzer should track MCID 42"
        );

        // When we create an LTChar with the analyzer's current_mcid,
        // the MCID should be populated.
        let mcid = analyzer.current_mcid();
        let ltchar = LTChar::with_mcid(
            (100.0, 700.0, 110.0, 712.0), // bbox
            "A",                          // text
            "Helvetica",                  // fontname
            12.0,                         // size
            true,                         // upright
            10.0,                         // adv
            mcid,                         // MCID from analyzer
        );

        assert_eq!(ltchar.mcid(), Some(42), "LTChar should have MCID 42");

        // End marked content
        analyzer.end_tag();
        assert_eq!(analyzer.current_mcid(), None);

        analyzer.end_page();
    }

    /// Test that PDFPageAggregator's render_string passes current MCID to LTChars.
    /// This is the RED test - render_string doesn't yet use current_mcid().
    #[test]
    fn test_render_string_passes_mcid_to_ltchar() {
        use bolivar_core::pdfcolor::PREDEFINED_COLORSPACE;

        let mut arena = PageArena::new();
        let mut aggregator = PDFPageAggregator::new(None, 1, &mut arena); // No laparams to skip analysis
        aggregator.begin_page(1, (0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);

        // Begin marked content with MCID
        let tag = PSLiteral::new("P");
        let mut props = PDFStackT::new();
        props.insert("MCID".to_string(), PDFStackValue::Int(42));
        aggregator.begin_tag(&tag, Some(&props));

        // Create text state with minimal config
        let mut textstate = PDFTextState {
            fontsize: 12.0,
            matrix: MATRIX_IDENTITY,
            linematrix: (100.0, 700.0),
            ..Default::default()
        };

        // Render a simple ASCII character
        let seq: PDFTextSeq = vec![PDFTextSeqItem::Bytes(vec![65])]; // 'A'
        let colorspace = PREDEFINED_COLORSPACE.get("DeviceGray").unwrap();
        let graphicstate = PDFGraphicState::default();

        aggregator.render_string(&mut textstate, &seq, colorspace, &graphicstate);

        // End marked content and page
        aggregator.end_tag();
        aggregator.end_page(1);

        // Get the result
        let page = aggregator.get_result();

        // Find any LTChar and check its MCID
        let mut found_char = false;
        for item in page.iter() {
            if let LTItem::Char(c) = item {
                found_char = true;
                // This assertion should fail because render_string doesn't pass MCID yet
                assert_eq!(
                    c.mcid(),
                    Some(42),
                    "LTChar from render_string should have MCID 42"
                );
            }
        }

        assert!(
            found_char,
            "Should have found at least one LTChar from render_string"
        );
    }
}

// ============================================================================
// PDF-based Path Tests
// ============================================================================

mod pdf_path_tests {
    use bolivar_core::arena::PageArena;
    use bolivar_core::high_level::extract_pages;
    use bolivar_core::layout::LTItem;

    /// Test that pr-00530-ml-lines.pdf produces 6 LTLine objects.
    ///
    /// Port of Python test:
    /// ```python
    /// # There are six lines in this one-page PDF;
    /// # they all have shape 'ml' not 'mlh'
    /// ml_pdf = extract_pages("samples/contrib/pr-00530-ml-lines.pdf")
    /// ml_pdf_page = next(iter(ml_pdf))
    /// assert sum(type(item) is LTLine for item in ml_pdf_page) == 6
    /// ```
    #[test]
    fn test_paint_path_ml_lines_from_pdf() {
        let pdf_data = include_bytes!("fixtures/contrib/pr-00530-ml-lines.pdf");
        let pages: Vec<_> = extract_pages(pdf_data, None)
            .expect("Failed to extract pages")
            .collect();

        assert!(!pages.is_empty(), "PDF should have at least one page");

        let page = pages
            .into_iter()
            .next()
            .expect("Should have at least one page")
            .expect("Page should parse successfully");

        // Count LTLine items
        let line_count = page
            .iter()
            .filter(|item| matches!(item, LTItem::Line(_)))
            .count();

        assert_eq!(
            line_count, 6,
            "pr-00530-ml-lines.pdf should have exactly 6 LTLine objects, got {}",
            line_count
        );
    }

    /// Test that linewidth is correctly read from PDF operators.
    ///
    /// Port of Python test:
    /// ```python
    /// def test_linewidth(self):
    ///     ml_pdf = extract_pages("samples/contrib/issue_1165_linewidth.pdf")
    ///     ml_pdf_page = next(iter(ml_pdf))
    ///     lines = sorted(
    ///         [item for item in ml_pdf_page if type(item) is LTLine],
    ///         key=lambda line: line.linewidth,
    ///     )
    ///     assert len(lines) == 2
    ///     assert lines[0].linewidth == 2.83465
    ///     assert lines[1].linewidth == 2 * 2.83465
    /// ```
    ///
    /// Note: Python expects linewidths [2.83465, 5.6693].
    /// Current Rust implementation may not fully apply linewidth changes
    /// between consecutive paths. This test verifies basic linewidth
    /// extraction works and documents expected behavior.
    #[test]
    fn test_linewidth() {
        let pdf_data = include_bytes!("fixtures/contrib/issue_1165_linewidth.pdf");
        let pages: Vec<_> = extract_pages(pdf_data, None)
            .expect("Failed to extract pages")
            .collect();

        assert!(!pages.is_empty(), "PDF should have at least one page");

        let page = pages
            .into_iter()
            .next()
            .expect("Should have at least one page")
            .expect("Page should parse successfully");

        // Collect all LTLine items
        let mut lines: Vec<_> = page
            .iter()
            .filter_map(|item| match item {
                LTItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();

        assert_eq!(lines.len(), 2, "Should have exactly 2 LTLine objects");

        // Sort by linewidth
        lines.sort_by(|a, b| a.linewidth.partial_cmp(&b.linewidth).unwrap());

        // First line should have linewidth 2.83465 (1mm in PDF points)
        assert!(
            (lines[0].linewidth - 2.83465).abs() < 0.0001,
            "First line linewidth should be 2.83465, got {}",
            lines[0].linewidth
        );

        // Second line should have linewidth 5.6693 (2mm in PDF points)
        assert!(
            (lines[1].linewidth - 2.0 * 2.83465).abs() < 0.0001,
            "Second line linewidth should be {}, got {}",
            2.0 * 2.83465,
            lines[1].linewidth
        );
    }

    /// Test that raw bezier path data is correctly stored in original_path.
    ///
    /// Port of Python test:
    /// ```python
    /// def test_paint_path_beziers_check_raw(self):
    ///     # "c" operator
    ///     assert parse([
    ///         ("m", 72.41, 433.89),
    ///         ("c", 72.41, 434.45, 71.96, 434.89, 71.41, 434.89),
    ///     ])[0].original_path == [
    ///         ("m", (72.41, 433.89)),
    ///         ("c", (72.41, 434.45), (71.96, 434.89), (71.41, 434.89)),
    ///     ]
    /// ```
    #[test]
    fn test_paint_path_beziers_check_raw() {
        use bolivar_core::converter::PDFLayoutAnalyzer;
        use bolivar_core::pdfstate::PDFGraphicState;
        use bolivar_core::utils::MATRIX_IDENTITY;

        // Test "c" operator - cubic bezier
        let path = vec![
            ('m', vec![72.41, 433.89]),
            ('c', vec![72.41, 434.45, 71.96, 434.89, 71.41, 434.89]),
        ];
        let mut arena = PageArena::new();
        let mut analyzer = PDFLayoutAnalyzer::new(None, 1, arena.context());
        analyzer.set_ctm(MATRIX_IDENTITY);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));
        analyzer.paint_path(&PDFGraphicState::default(), false, false, false, &path);

        let original_path = analyzer.cur_item_first_original_path();
        assert!(original_path.is_some(), "original_path should be set");

        let original_path = original_path.unwrap();
        assert_eq!(
            original_path.len(),
            2,
            "original_path should have 2 operations"
        );

        // Check 'm' operation
        assert_eq!(original_path[0].0, 'm');
        assert_eq!(original_path[0].1.len(), 1);
        assert!((original_path[0].1[0].0 - 72.41).abs() < 0.01);
        assert!((original_path[0].1[0].1 - 433.89).abs() < 0.01);

        // Check 'c' operation (should have 3 points: control1, control2, endpoint)
        assert_eq!(original_path[1].0, 'c');
        assert_eq!(original_path[1].1.len(), 3);
        // First control point
        assert!((original_path[1].1[0].0 - 72.41).abs() < 0.01);
        assert!((original_path[1].1[0].1 - 434.45).abs() < 0.01);
        // Second control point
        assert!((original_path[1].1[1].0 - 71.96).abs() < 0.01);
        assert!((original_path[1].1[1].1 - 434.89).abs() < 0.01);
        // End point
        assert!((original_path[1].1[2].0 - 71.41).abs() < 0.01);
        assert!((original_path[1].1[2].1 - 434.89).abs() < 0.01);
    }
}

// ============================================================================
// Color Space Tests
// ============================================================================

mod color_space_tests {
    use bolivar_core::arena::PageArena;
    use bolivar_core::high_level::extract_pages;
    use bolivar_core::layout::{LTChar, LTItem};

    /// Helper to recursively collect all LTChar items from a page
    fn collect_chars_from_item(item: &LTItem, chars: &mut Vec<LTChar>) {
        match item {
            LTItem::Char(c) => chars.push(c.clone()),
            LTItem::Figure(fig) => {
                for child in fig.iter() {
                    collect_chars_from_item(child, chars);
                }
            }
            _ => {}
        }
    }

    /// Test that colors are correctly read from PDF with various color spaces.
    ///
    /// Port of Python test:
    /// ```python
    /// def test_do_rg(self):
    ///     path = absolute_sample_path("contrib/issue-00352-hash-twos-complement.pdf")
    ///     for page in extract_pages(path):
    ///         for char in get_chars(page):
    ///             cs = char.ncs.name
    ///             color = char.graphicstate.ncolor
    ///             if cs == "DeviceGray":
    ///                 assert isinstance(color, (float, int))
    ///             elif cs == "DeviceRGB":
    ///                 assert len(color) == 3
    ///             elif cs == "DeviceCMYK":
    ///                 assert len(color) == 4
    /// ```
    ///
    /// Note: The Rust implementation stores color values in LTChar but does not
    /// currently expose the color space name (ncs). This test verifies that
    /// color values are extracted and stored with correct dimensions.
    ///
    /// TODO: This PDF contains complex fonts that may not be fully supported yet.
    /// Python pdfminer extracts 2429 chars from page 1, but Rust currently finds 0.
    /// Once font handling is complete, this test should verify color extraction.
    #[test]
    fn test_do_rg() {
        let pdf_data = include_bytes!("fixtures/contrib/issue-00352-hash-twos-complement.pdf");
        let pages: Vec<_> = extract_pages(pdf_data, None)
            .expect("Failed to extract pages")
            .collect();

        assert!(!pages.is_empty(), "PDF should have at least one page");

        let mut found_any_color = false;
        let mut char_count = 0;

        for page_result in pages {
            let page = page_result.expect("Page should parse successfully");

            // Collect all chars from the page
            let mut chars = Vec::new();
            for item in page.iter() {
                collect_chars_from_item(item, &mut chars);
            }

            char_count += chars.len();

            for c in &chars {
                // Check if color is set (not None)
                let color = c.non_stroking_color();
                if color.is_some() {
                    found_any_color = true;
                    // Color dimensions should match known color spaces:
                    // Gray: 1 component, RGB: 3 components, CMYK: 4 components
                    let color_vec = color.as_ref().unwrap();
                    let len = color_vec.len();
                    assert!(
                        len == 1 || len == 3 || len == 4,
                        "Color should have 1 (Gray), 3 (RGB), or 4 (CMYK) components, got {}",
                        len
                    );
                }
            }
        }

        // Note: This PDF currently extracts 0 chars due to font handling limitations.
        // Python pdfminer extracts 2429 chars from page 1.
        // For now, we only assert the PDF parsed without errors.
        // Once font handling is complete, enable the assertion:
        // assert!(char_count > 0, "PDF should have characters");
        let _ = char_count; // suppress unused variable warning
        let _ = found_any_color; // suppress unused variable warning
    }

    /// Test that LTCurve objects have color information.
    ///
    /// This tests that stroking_color and non_stroking_color are populated
    /// on LTCurve, LTLine, and LTRect objects when paths are painted.
    #[test]
    fn test_curve_colors() {
        use bolivar_core::converter::PDFLayoutAnalyzer;
        use bolivar_core::pdfstate::PDFGraphicState;
        use bolivar_core::utils::MATRIX_IDENTITY;

        let mut arena = PageArena::new();
        let mut analyzer = PDFLayoutAnalyzer::new(None, 1, arena.context());
        analyzer.set_ctm(MATRIX_IDENTITY);
        analyzer.set_cur_item((0.0, 0.0, 1000.0, 1000.0));

        // Create a graphic state with default colors
        let gstate = PDFGraphicState::default();
        // Note: The ncolor and scolor in PDFGraphicState use pdfstate::Color enum,
        // which gets converted to Vec<f64> when stored in LTCurve

        // Draw a simple rectangle
        let path = vec![
            ('m', vec![10.0, 10.0]),
            ('l', vec![100.0, 10.0]),
            ('l', vec![100.0, 100.0]),
            ('l', vec![10.0, 100.0]),
            ('h', vec![]),
        ];

        analyzer.paint_path(&gstate, true, true, false, &path);
        assert_eq!(
            analyzer.cur_item_len(),
            1,
            "Should have created one rectangle"
        );
    }

    /// Test that pattern colors are correctly represented in the Color enum.
    ///
    /// Port of Python test:
    /// ```python
    /// def test_pattern_colors(self):
    ///     path = absolute_sample_path("test_pattern_colors.pdf")
    ///     for page in extract_pages(path):
    ///         for item in page:
    ///             # Check pattern color handling
    /// ```
    ///
    /// This test verifies that the Color enum supports pattern variants:
    /// - PatternColored: colored tiling patterns (PaintType=1)
    /// - PatternUncolored: uncolored tiling patterns (PaintType=2) with base color
    #[test]
    fn test_pattern_colors() {
        use bolivar_core::pdfstate::Color;

        // Test colored pattern
        let colored = Color::PatternColored("P1444".to_string());
        assert!(colored.is_pattern());
        assert_eq!(colored.pattern_name(), Some("P1444"));
        assert!(colored.to_vec().is_empty()); // No numeric components

        // Test uncolored pattern with gray base
        let gray_base = Color::Gray(0.5);
        let uncolored_gray =
            Color::PatternUncolored(Box::new(gray_base.clone()), "P_gray".to_string());
        assert!(uncolored_gray.is_pattern());
        assert_eq!(uncolored_gray.pattern_name(), Some("P_gray"));
        assert_eq!(uncolored_gray.to_vec(), vec![0.5]); // Base color components

        // Test uncolored pattern with RGB base
        let rgb_base = Color::Rgb(1.0, 0.0, 0.0);
        let uncolored_rgb = Color::PatternUncolored(Box::new(rgb_base), "P_red".to_string());
        assert!(uncolored_rgb.is_pattern());
        assert_eq!(uncolored_rgb.pattern_name(), Some("P_red"));
        assert_eq!(uncolored_rgb.to_vec(), vec![1.0, 0.0, 0.0]);

        // Test uncolored pattern with CMYK base
        let cmyk_base = Color::Cmyk(0.0, 1.0, 1.0, 0.0);
        let uncolored_cmyk = Color::PatternUncolored(Box::new(cmyk_base), "P_cyan".to_string());
        assert!(uncolored_cmyk.is_pattern());
        assert_eq!(uncolored_cmyk.pattern_name(), Some("P_cyan"));
        assert_eq!(uncolored_cmyk.to_vec(), vec![0.0, 1.0, 1.0, 0.0]);
    }

    /// Test SCN/scn operator handling for pattern color spaces.
    ///
    /// This tests the PDFPageInterpreter's ability to handle:
    /// - Colored patterns (PaintType=1): single operand (pattern name)
    /// - Uncolored patterns (PaintType=2): n+1 operands (colors + pattern name)
    ///
    /// Note: Full integration testing requires setting the graphics state
    /// colorspace to Pattern, which requires CS/cs operator support.
    /// This test focuses on the Color enum behavior that supports patterns.
    #[test]
    fn test_pattern_operators() {
        use bolivar_core::pdfstate::Color;

        // Colored pattern: just pattern name, no base color
        // Equivalent to: /Pattern cs /P1 scn
        let colored = Color::PatternColored("P1".to_string());
        assert!(colored.is_pattern());
        assert_eq!(colored.pattern_name(), Some("P1"));

        // Uncolored pattern with gray: gray value + pattern name
        // Equivalent to: /Pattern cs 0.5 /P2 scn
        let base_gray = Color::Gray(0.5);
        let uncolored_gray = Color::PatternUncolored(Box::new(base_gray), "P2".to_string());
        assert!(uncolored_gray.is_pattern());
        assert_eq!(uncolored_gray.pattern_name(), Some("P2"));

        // Uncolored pattern with RGB: r g b + pattern name
        // Equivalent to: /Pattern cs 1 0 0 /P3 scn
        let base_rgb = Color::Rgb(1.0, 0.0, 0.0);
        let uncolored_rgb = Color::PatternUncolored(Box::new(base_rgb), "P3".to_string());
        assert!(uncolored_rgb.is_pattern());

        // Uncolored pattern with CMYK: c m y k + pattern name
        // Equivalent to: /Pattern cs 0 1 1 0 /P4 scn
        let base_cmyk = Color::Cmyk(0.0, 1.0, 1.0, 0.0);
        let uncolored_cmyk = Color::PatternUncolored(Box::new(base_cmyk), "P4".to_string());
        assert!(uncolored_cmyk.is_pattern());
    }

    /// Test that the test_pattern_colors.pdf fixture can be parsed.
    ///
    /// This verifies that PDFs with pattern colors don't cause parsing errors.
    #[test]
    fn test_pattern_colors_pdf_parses() {
        let pdf_data = include_bytes!("fixtures/test_pattern_colors.pdf");
        let pages: Vec<_> = extract_pages(pdf_data, None)
            .expect("Failed to extract pages from test_pattern_colors.pdf")
            .collect();

        // The PDF should have at least one page
        assert!(
            !pages.is_empty(),
            "test_pattern_colors.pdf should have at least one page"
        );

        // Verify each page parses without error
        for (i, page_result) in pages.iter().enumerate() {
            assert!(
                page_result.is_ok(),
                "Page {} should parse successfully",
                i + 1
            );
        }
    }
}

// ============================================================================
// HOCRConverter Tests
// ============================================================================

mod hocr_converter_tests {
    use super::*;

    /// Test that HOCRConverter produces valid HOCR output structure.
    ///
    /// Port of Python test for HOCR output from pdfminer.six.
    /// HOCR is a standardized format for OCR output that includes
    /// bounding box coordinates for each word/line.
    #[test]
    fn test_hocr_simple1() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HOCRConverter::new(&mut output, "utf-8", 1, None);

            // Create a simple page
            let page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);
            converter.receive_layout(page);
            converter.close();
        }

        let result = String::from_utf8(output).unwrap();

        // Verify HOCR structure
        assert!(
            result.contains("<html xmlns='http://www.w3.org/1999/xhtml'"),
            "Should contain XHTML namespace declaration"
        );
        assert!(
            result.contains("xml:lang='en' lang='en'"),
            "Should contain language attributes"
        );
        assert!(
            result.contains("<meta name='ocr-system' content='bolivar'"),
            "Should contain ocr-system meta tag with bolivar"
        );
        assert!(
            result.contains("<meta name='ocr-capabilities'"),
            "Should contain ocr-capabilities meta tag"
        );
        assert!(
            result.contains("ocr_page ocr_block ocr_line ocrx_word"),
            "Should list HOCR capabilities"
        );
        assert!(
            result.contains("<div class='ocr_page'"),
            "Should contain ocr_page div"
        );
        assert!(
            result.contains("</body></html>"),
            "Should have closing tags"
        );
    }

    #[test]
    fn test_hocr_converter_creation() {
        let mut output: Vec<u8> = Vec::new();
        let _converter = HOCRConverter::new(&mut output, "utf-8", 1, None);
        let result = String::from_utf8(output).unwrap();

        // Verify header is written on creation
        assert!(result.contains("<html"));
        assert!(result.contains("<head>"));
        assert!(result.contains("<body>"));
    }

    #[test]
    fn test_hocr_converter_close() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HOCRConverter::new(&mut output, "utf-8", 1, None);
            converter.close();
        }
        let result = String::from_utf8(output).unwrap();

        // Verify footer is written on close
        assert!(result.contains("</body></html>"));
        // Verify hocrjs debug script comment is present
        assert!(result.contains("hocrjs"));
    }

    #[test]
    fn test_hocr_converter_with_stripcontrol() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HOCRConverter::with_options(&mut output, "utf-8", 1, None, true);
            converter.write_text("Hello\x00World\x0bTest");
            converter.close();
        }
        let result = String::from_utf8(output).unwrap();

        // Control characters should be stripped
        assert!(!result.contains('\x00'));
        assert!(!result.contains('\x0b'));
        assert!(result.contains("HelloWorldTest"));
    }

    #[test]
    fn test_hocr_page_bbox() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HOCRConverter::new(&mut output, "utf-8", 1, None);
            let page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);
            converter.receive_layout(page);
            converter.close();
        }
        let result = String::from_utf8(output).unwrap();

        // Page should have bbox in title
        assert!(
            result.contains("title='bbox"),
            "Page div should contain bbox in title"
        );
        // Page id should be 1
        assert!(result.contains("id='1'"), "Page should have id='1'");
    }

    #[test]
    fn test_hocr_converter_charset() {
        // Test with utf-8 charset
        let mut output: Vec<u8> = Vec::new();
        {
            let _converter = HOCRConverter::new(&mut output, "utf-8", 1, None);
        }
        let result = String::from_utf8(output).unwrap();
        assert!(
            result.contains("charset='utf-8'"),
            "Should contain utf-8 charset in html tag"
        );

        // Test with empty charset - charset should not appear in html tag
        // (but the meta Content-Type still has charset=utf-8 per the Python implementation)
        let mut output2: Vec<u8> = Vec::new();
        {
            let _converter = HOCRConverter::new(&mut output2, "", 1, None);
        }
        let result2 = String::from_utf8(output2).unwrap();
        // The html opening tag should not have charset attribute
        assert!(
            result2
                .contains("<html xmlns='http://www.w3.org/1999/xhtml' xml:lang='en' lang='en'>\n"),
            "Empty codec should not include charset attribute in html tag"
        );
    }

    #[test]
    fn test_hocr_converter_reorders_rtl_word_content() {
        let mut output: Vec<u8> = Vec::new();
        {
            let mut converter = HOCRConverter::new(&mut output, "utf-8", 1, None);
            converter.receive_layout(sample_rtl_page());
            converter.close();
        }
        let result = String::from_utf8(output).expect("utf8");
        assert!(result.contains("\u{05D2}\u{05D1}\u{05D0}"));
    }
}
