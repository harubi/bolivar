//! Tests for converter module - output converters for PDF content.
//!
//! Port of tests from pdfminer.six tests/test_converter.py

use bolivar_core::converter::{
    HTMLConverter, LTContainer, PDFConverter, PDFLayoutAnalyzer, PDFPageAggregator, TextConverter,
    XMLConverter,
};
use bolivar_core::layout::{LAParams, LTPage};
use bolivar_core::pdfcolor::PDFColorSpace;
use bolivar_core::pdffont::{FontWidthDict, PDFFont};
use bolivar_core::pdfstate::PDFGraphicState;
use bolivar_core::pdftypes::{PDFObject, PDFStream};
use bolivar_core::utils::MATRIX_IDENTITY;
use std::collections::HashMap;
use std::io::Cursor;

// ============================================================================
// PDFLayoutAnalyzer Tests
// ============================================================================

mod layout_analyzer_tests {
    use super::*;

    fn get_analyzer() -> PDFLayoutAnalyzer {
        let mut analyzer = PDFLayoutAnalyzer::new(None, 1);
        analyzer.set_ctm(MATRIX_IDENTITY);
        analyzer
    }

    #[test]
    fn test_paint_path_simple_line() {
        // Test path: m(6,7) l(7,7) - single line segment
        let path = vec![('m', vec![6.0, 7.0]), ('l', vec![7.0, 7.0])];
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 100.0, 100.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 100.0, 100.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 100.0, 100.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));
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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 1000.0, 1000.0)));

        let mut graphicstate = PDFGraphicState::default();
        graphicstate.dash = Some((vec![1.0, 1.0], 0.0));

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
        let mut analyzer = get_analyzer();
        analyzer.set_cur_item(LTContainer::new((0.0, 0.0, 100.0, 100.0)));

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
        let mut analyzer = get_analyzer();
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
        let mut analyzer = get_analyzer();
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
        let mut analyzer = get_analyzer();
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
        let mut analyzer = get_analyzer();
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
        let mut analyzer = get_analyzer();
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
        let mut analyzer = get_analyzer();
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
        let mut analyzer = get_analyzer();
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
        let aggregator = PDFPageAggregator::new(None, 1);
        assert!(aggregator.result().is_none());
    }

    #[test]
    fn test_page_aggregator_receives_layout() {
        let mut aggregator = PDFPageAggregator::new(None, 1);
        let page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);
        aggregator.receive_layout(page);
        assert!(aggregator.result().is_some());
    }

    #[test]
    fn test_page_aggregator_get_result() {
        let mut aggregator = PDFPageAggregator::new(None, 1);
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
        let mut analyzer = PDFLayoutAnalyzer::new(Some(LAParams::default()), 1);
        analyzer.set_ctm(MATRIX_IDENTITY);

        // Simulate page processing
        analyzer.begin_page((0.0, 0.0, 612.0, 792.0), MATRIX_IDENTITY);
        // Add some content...
        analyzer.end_page();

        assert_eq!(analyzer.pageno(), 2);
    }

    #[test]
    fn test_analyzer_figure_lifecycle() {
        let mut analyzer = PDFLayoutAnalyzer::new(Some(LAParams::default()), 1);
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
