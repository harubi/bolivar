//! Port of pdfminer.six tests/test_layout.py
//!
//! Tests for layout analysis module including LAParams, LTComponent hierarchy,
//! text line grouping, neighbor finding, graphical elements, and layout analysis.

use bolivar_core::layout::{
    LAParams, LTAnno, LTChar, LTComponent, LTCurve, LTFigure, LTImage, LTItem, LTLayoutContainer,
    LTLine, LTPage, LTRect, LTTextBox, LTTextBoxHorizontal, LTTextBoxVertical, LTTextGroup,
    LTTextLine, LTTextLineHorizontal, LTTextLineVertical, TextLineType,
};
use bolivar_core::utils::{HasBBox, Plane, Rect};

// ============================================================================
// TestGroupTextLines - test grouping text lines into textboxes
// ============================================================================

#[test]
fn test_parent_with_wrong_bbox_returns_non_empty_neighbour_list() {
    // LTLayoutContainer.group_textlines() should return all the lines in
    // separate LTTextBoxes if they do not overlap. Even when the bounding box
    // of the parent container does not contain all the lines.
    let laparams = LAParams::default();
    let layout = LTLayoutContainer::new((0.0, 0.0, 50.0, 50.0));

    let mut line1 = LTTextLineHorizontal::new(laparams.word_margin);
    line1.set_bbox((0.0, 0.0, 50.0, 5.0));

    let mut line2 = LTTextLineHorizontal::new(laparams.word_margin);
    line2.set_bbox((0.0, 50.0, 50.0, 55.0));

    let lines: Vec<TextLineType> = vec![
        TextLineType::Horizontal(line1),
        TextLineType::Horizontal(line2),
    ];

    let textboxes = layout.group_textlines(&laparams, lines);

    assert_eq!(textboxes.len(), 2);
}

// ============================================================================
// TestFindNeighbors - test finding neighboring text lines
// ============================================================================

#[test]
fn test_find_neighbors_horizontal() {
    let laparams = LAParams::default();
    let mut plane: Plane<LTTextLineHorizontal> = Plane::new((0.0, 0.0, 50.0, 50.0), 1);

    let mut line = LTTextLineHorizontal::new(laparams.word_margin);
    line.set_bbox((10.0, 4.0, 20.0, 6.0));
    plane.add(line.clone());

    let mut left_aligned_above = LTTextLineHorizontal::new(laparams.word_margin);
    left_aligned_above.set_bbox((10.0, 6.0, 15.0, 8.0));
    plane.add(left_aligned_above.clone());

    let mut right_aligned_below = LTTextLineHorizontal::new(laparams.word_margin);
    right_aligned_below.set_bbox((15.0, 2.0, 20.0, 4.0));
    plane.add(right_aligned_below.clone());

    let mut centrally_aligned_overlapping = LTTextLineHorizontal::new(laparams.word_margin);
    centrally_aligned_overlapping.set_bbox((13.0, 5.0, 17.0, 7.0));
    plane.add(centrally_aligned_overlapping.clone());

    let mut not_aligned = LTTextLineHorizontal::new(laparams.word_margin);
    not_aligned.set_bbox((0.0, 6.0, 5.0, 8.0));
    plane.add(not_aligned);

    let mut wrong_height = LTTextLineHorizontal::new(laparams.word_margin);
    wrong_height.set_bbox((10.0, 6.0, 15.0, 10.0));
    plane.add(wrong_height);

    let neighbors = line.find_neighbors(&plane, laparams.line_margin);

    // Should include: line, left_aligned_above, right_aligned_below, centrally_aligned_overlapping
    // Should NOT include: not_aligned (wrong x position), wrong_height (different height)
    assert_eq!(neighbors.len(), 4);

    // Check that the expected neighbors are present
    let neighbor_bboxes: Vec<_> = neighbors.iter().map(|n| n.bbox()).collect();
    assert!(neighbor_bboxes.contains(&(10.0, 4.0, 20.0, 6.0))); // line itself
    assert!(neighbor_bboxes.contains(&(10.0, 6.0, 15.0, 8.0))); // left_aligned_above
    assert!(neighbor_bboxes.contains(&(15.0, 2.0, 20.0, 4.0))); // right_aligned_below
    assert!(neighbor_bboxes.contains(&(13.0, 5.0, 17.0, 7.0))); // centrally_aligned_overlapping
}

#[test]
fn test_find_neighbors_vertical() {
    let laparams = LAParams::default();
    let mut plane: Plane<LTTextLineVertical> = Plane::new((0.0, 0.0, 50.0, 50.0), 1);

    let mut line = LTTextLineVertical::new(laparams.word_margin);
    line.set_bbox((4.0, 10.0, 6.0, 20.0));
    plane.add(line.clone());

    let mut bottom_aligned_right = LTTextLineVertical::new(laparams.word_margin);
    bottom_aligned_right.set_bbox((6.0, 10.0, 8.0, 15.0));
    plane.add(bottom_aligned_right.clone());

    let mut top_aligned_left = LTTextLineVertical::new(laparams.word_margin);
    top_aligned_left.set_bbox((2.0, 15.0, 4.0, 20.0));
    plane.add(top_aligned_left.clone());

    let mut centrally_aligned_overlapping = LTTextLineVertical::new(laparams.word_margin);
    centrally_aligned_overlapping.set_bbox((5.0, 13.0, 7.0, 17.0));
    plane.add(centrally_aligned_overlapping.clone());

    let mut not_aligned = LTTextLineVertical::new(laparams.word_margin);
    not_aligned.set_bbox((6.0, 0.0, 8.0, 5.0));
    plane.add(not_aligned);

    let mut wrong_width = LTTextLineVertical::new(laparams.word_margin);
    wrong_width.set_bbox((6.0, 10.0, 10.0, 15.0));
    plane.add(wrong_width);

    let neighbors = line.find_neighbors(&plane, laparams.line_margin);

    // Should include: line, bottom_aligned_right, top_aligned_left, centrally_aligned_overlapping
    // Should NOT include: not_aligned (wrong y position), wrong_width (different width)
    assert_eq!(neighbors.len(), 4);

    // Check that the expected neighbors are present
    let neighbor_bboxes: Vec<_> = neighbors.iter().map(|n| n.bbox()).collect();
    assert!(neighbor_bboxes.contains(&(4.0, 10.0, 6.0, 20.0))); // line itself
    assert!(neighbor_bboxes.contains(&(6.0, 10.0, 8.0, 15.0))); // bottom_aligned_right
    assert!(neighbor_bboxes.contains(&(2.0, 15.0, 4.0, 20.0))); // top_aligned_left
    assert!(neighbor_bboxes.contains(&(5.0, 13.0, 7.0, 17.0))); // centrally_aligned_overlapping
}

#[derive(Clone, Copy)]
struct TestBox {
    bbox: Rect,
}

impl HasBBox for TestBox {
    fn x0(&self) -> f64 {
        self.bbox.0
    }

    fn y0(&self) -> f64 {
        self.bbox.1
    }

    fn x1(&self) -> f64 {
        self.bbox.2
    }

    fn y1(&self) -> f64 {
        self.bbox.3
    }
}

#[test]
fn test_plane_any_with_indices_matches_find() {
    let mut plane = Plane::new((0.0, 0.0, 100.0, 100.0), 1);
    plane.extend(vec![
        TestBox {
            bbox: (0.0, 0.0, 10.0, 10.0),
        },
        TestBox {
            bbox: (20.0, 20.0, 30.0, 30.0),
        },
    ]);
    let q = (5.0, 5.0, 15.0, 15.0);
    assert!(plane.any_with_indices(q, |idx, _| idx == 0));
    assert!(!plane.any_with_indices(q, |idx, _| idx == 1));
}

// ============================================================================
// LAParams tests
// ============================================================================

#[test]
fn test_laparams_default() {
    let params = LAParams::default();
    assert_eq!(params.line_overlap, 0.5);
    assert_eq!(params.char_margin, 2.0);
    assert_eq!(params.line_margin, 0.5);
    assert_eq!(params.word_margin, 0.1);
    assert_eq!(params.boxes_flow, Some(0.5));
    assert!(!params.detect_vertical);
    assert!(!params.all_texts);
}

#[test]
fn test_laparams_custom() {
    let params = LAParams::new(0.3, 1.5, 0.4, 0.2, Some(0.7), true, true);
    assert_eq!(params.line_overlap, 0.3);
    assert_eq!(params.char_margin, 1.5);
    assert_eq!(params.line_margin, 0.4);
    assert_eq!(params.word_margin, 0.2);
    assert_eq!(params.boxes_flow, Some(0.7));
    assert!(params.detect_vertical);
    assert!(params.all_texts);
}

#[test]
fn test_laparams_boxes_flow_none() {
    let params = LAParams::new(0.5, 2.0, 0.5, 0.1, None, false, false);
    assert_eq!(params.boxes_flow, None);
}

#[test]
#[should_panic(expected = "boxes_flow")]
fn test_laparams_boxes_flow_too_low() {
    LAParams::new(0.5, 2.0, 0.5, 0.1, Some(-1.5), false, false);
}

#[test]
#[should_panic(expected = "boxes_flow")]
fn test_laparams_boxes_flow_too_high() {
    LAParams::new(0.5, 2.0, 0.5, 0.1, Some(1.5), false, false);
}

// ============================================================================
// LTComponent tests
// ============================================================================

#[test]
fn test_ltcomponent_bbox() {
    let comp = LTComponent::new((10.0, 20.0, 30.0, 40.0));
    assert_eq!(comp.x0(), 10.0);
    assert_eq!(comp.y0(), 20.0);
    assert_eq!(comp.x1(), 30.0);
    assert_eq!(comp.y1(), 40.0);
    assert_eq!(comp.width(), 20.0);
    assert_eq!(comp.height(), 20.0);
    assert_eq!(comp.bbox(), (10.0, 20.0, 30.0, 40.0));
}

#[test]
fn test_ltcomponent_set_bbox() {
    let mut comp = LTComponent::new((0.0, 0.0, 10.0, 10.0));
    comp.set_bbox((5.0, 5.0, 15.0, 25.0));
    assert_eq!(comp.x0(), 5.0);
    assert_eq!(comp.y0(), 5.0);
    assert_eq!(comp.x1(), 15.0);
    assert_eq!(comp.y1(), 25.0);
    assert_eq!(comp.width(), 10.0);
    assert_eq!(comp.height(), 20.0);
}

#[test]
fn test_ltcomponent_is_empty() {
    let zero_width = LTComponent::new((10.0, 10.0, 10.0, 20.0));
    assert!(zero_width.is_empty());

    let zero_height = LTComponent::new((10.0, 10.0, 20.0, 10.0));
    assert!(zero_height.is_empty());

    let normal = LTComponent::new((10.0, 10.0, 20.0, 20.0));
    assert!(!normal.is_empty());
}

#[test]
fn test_ltcomponent_hoverlap() {
    let comp1 = LTComponent::new((0.0, 0.0, 10.0, 10.0));
    let comp2 = LTComponent::new((5.0, 0.0, 15.0, 10.0));
    let comp3 = LTComponent::new((20.0, 0.0, 30.0, 10.0));

    assert!(comp1.is_hoverlap(&comp2));
    assert!(!comp1.is_hoverlap(&comp3));
    assert_eq!(comp1.hoverlap(&comp2), 5.0);
    assert_eq!(comp1.hoverlap(&comp3), 0.0);
}

#[test]
fn test_ltcomponent_voverlap() {
    let comp1 = LTComponent::new((0.0, 0.0, 10.0, 10.0));
    let comp2 = LTComponent::new((0.0, 5.0, 10.0, 15.0));
    let comp3 = LTComponent::new((0.0, 20.0, 10.0, 30.0));

    assert!(comp1.is_voverlap(&comp2));
    assert!(!comp1.is_voverlap(&comp3));
    assert_eq!(comp1.voverlap(&comp2), 5.0);
    assert_eq!(comp1.voverlap(&comp3), 0.0);
}

#[test]
fn test_ltcomponent_hdistance() {
    let comp1 = LTComponent::new((0.0, 0.0, 10.0, 10.0));
    let comp2 = LTComponent::new((15.0, 0.0, 25.0, 10.0));
    let comp3 = LTComponent::new((5.0, 0.0, 15.0, 10.0));

    assert_eq!(comp1.hdistance(&comp2), 5.0);
    assert_eq!(comp1.hdistance(&comp3), 0.0); // overlapping
}

#[test]
fn test_ltcomponent_vdistance() {
    let comp1 = LTComponent::new((0.0, 0.0, 10.0, 10.0));
    let comp2 = LTComponent::new((0.0, 15.0, 10.0, 25.0));
    let comp3 = LTComponent::new((0.0, 5.0, 10.0, 15.0));

    assert_eq!(comp1.vdistance(&comp2), 5.0);
    assert_eq!(comp1.vdistance(&comp3), 0.0); // overlapping
}

// ============================================================================
// LTAnno tests
// ============================================================================

#[test]
fn test_ltanno_text() {
    let anno = LTAnno::new(" ");
    assert_eq!(anno.get_text(), " ");

    let newline = LTAnno::new("\n");
    assert_eq!(newline.get_text(), "\n");
}

// ============================================================================
// LTTextLine tests
// ============================================================================

#[test]
fn test_lttextline_horizontal_is_empty() {
    let line = LTTextLineHorizontal::new(0.1);
    assert!(line.is_empty());
}

#[test]
fn test_lttextline_vertical_is_empty() {
    let line = LTTextLineVertical::new(0.1);
    assert!(line.is_empty());
}

// ============================================================================
// LTTextBox tests
// ============================================================================

#[test]
fn test_lttextbox_horizontal_writing_mode() {
    let textbox = LTTextBoxHorizontal::new();
    assert_eq!(textbox.get_writing_mode(), "lr-tb");
}

#[test]
fn test_lttextbox_vertical_writing_mode() {
    let textbox = LTTextBoxVertical::new();
    assert_eq!(textbox.get_writing_mode(), "tb-rl");
}

#[test]
fn test_lttextbox_index() {
    let mut textbox = LTTextBoxHorizontal::new();
    assert_eq!(textbox.index(), -1);
    textbox.set_index(5);
    assert_eq!(textbox.index(), 5);
}

// ============================================================================
// LTCurve, LTLine, LTRect tests
// ============================================================================

#[test]
fn test_ltcurve_basic() {
    let pts = vec![(0.0, 0.0), (10.0, 10.0), (20.0, 0.0)];
    let curve = LTCurve::new(1.0, pts, true, false, false, None, None);

    assert_eq!(curve.x0(), 0.0);
    assert_eq!(curve.y0(), 0.0);
    assert_eq!(curve.x1(), 20.0);
    assert_eq!(curve.y1(), 10.0);
    assert_eq!(curve.linewidth, 1.0);
    assert!(curve.stroke);
    assert!(!curve.fill);
}

#[test]
fn test_ltcurve_get_pts() {
    let pts = vec![(1.5, 2.5), (3.5, 4.5)];
    let curve = LTCurve::new(1.0, pts, false, false, false, None, None);

    let pts_str = curve.get_pts();
    assert!(pts_str.contains("1.500,2.500"));
    assert!(pts_str.contains("3.500,4.500"));
}

#[test]
fn test_ltline_basic() {
    let line = LTLine::new(
        2.0,
        (0.0, 0.0),
        (100.0, 50.0),
        true,
        false,
        false,
        None,
        None,
    );

    assert_eq!(line.x0(), 0.0);
    assert_eq!(line.y0(), 0.0);
    assert_eq!(line.x1(), 100.0);
    assert_eq!(line.y1(), 50.0);
    assert_eq!(line.p0(), (0.0, 0.0));
    assert_eq!(line.p1(), (100.0, 50.0));
    assert_eq!(line.linewidth, 2.0);
}

#[test]
fn test_ltrect_basic() {
    let rect = LTRect::new(
        1.5,
        (10.0, 20.0, 30.0, 40.0),
        false,
        true,
        false,
        None,
        None,
    );

    assert_eq!(rect.x0(), 10.0);
    assert_eq!(rect.y0(), 20.0);
    assert_eq!(rect.x1(), 30.0);
    assert_eq!(rect.y1(), 40.0);
    assert_eq!(rect.linewidth, 1.5);
    assert!(!rect.stroke);
    assert!(rect.fill);
}

// ============================================================================
// LTImage tests
// ============================================================================

#[test]
fn test_ltimage_basic() {
    let image = LTImage::new(
        "Image1",
        (0.0, 0.0, 100.0, 100.0),
        (Some(200), Some(200)),
        false,
        8,
        vec!["DeviceRGB".to_string()],
    );

    assert_eq!(image.name, "Image1");
    assert_eq!(image.x0(), 0.0);
    assert_eq!(image.y0(), 0.0);
    assert_eq!(image.x1(), 100.0);
    assert_eq!(image.y1(), 100.0);
    assert_eq!(image.srcsize, (Some(200), Some(200)));
    assert!(!image.imagemask);
    assert_eq!(image.bits, 8);
    assert_eq!(image.colorspace, vec!["DeviceRGB"]);
}

// ============================================================================
// group_objects tests - character to line grouping
// ============================================================================

#[test]
fn test_group_objects_single_char() {
    let laparams = LAParams::default();
    let layout = LTLayoutContainer::new((0.0, 0.0, 100.0, 100.0));

    let char1 = LTChar::new((10.0, 10.0, 20.0, 20.0), "A", "Helvetica", 10.0, true, 10.0);

    let lines = layout.group_objects(&laparams, &[char1]);

    assert_eq!(lines.len(), 1);
    match &lines[0] {
        TextLineType::Horizontal(line) => {
            assert_eq!(line.get_text(), "A\n");
        }
        _ => panic!("Expected horizontal line"),
    }
}

#[test]
fn test_group_objects_horizontal_line() {
    let laparams = LAParams::default();
    let layout = LTLayoutContainer::new((0.0, 0.0, 100.0, 100.0));

    // Three characters on the same horizontal line, close together
    let char1 = LTChar::new((10.0, 10.0, 20.0, 20.0), "A", "Helvetica", 10.0, true, 10.0);
    let char2 = LTChar::new((21.0, 10.0, 31.0, 20.0), "B", "Helvetica", 10.0, true, 10.0);
    let char3 = LTChar::new((32.0, 10.0, 42.0, 20.0), "C", "Helvetica", 10.0, true, 10.0);

    let lines = layout.group_objects(&laparams, &[char1, char2, char3]);

    assert_eq!(lines.len(), 1);
    match &lines[0] {
        TextLineType::Horizontal(line) => {
            assert_eq!(line.get_text(), "ABC\n");
        }
        _ => panic!("Expected horizontal line"),
    }
}

#[test]
fn test_group_objects_word_spacing() {
    let laparams = LAParams::default();
    let layout = LTLayoutContainer::new((0.0, 0.0, 100.0, 100.0));

    // Two characters with a small gap between them (larger than word_margin but
    // still within char_margin for line grouping). Gap = 2 char widths is within
    // char_margin=2.0 (so same line), but > word_margin=0.1 (so space inserted).
    // char width = 10, char_margin=2.0 means gap up to 20 allowed for same line
    // word_margin=0.1 means gap > 1 triggers space
    let char1 = LTChar::new((10.0, 10.0, 20.0, 20.0), "A", "Helvetica", 10.0, true, 10.0);
    let char2 = LTChar::new((25.0, 10.0, 35.0, 20.0), "B", "Helvetica", 10.0, true, 10.0);

    let lines = layout.group_objects(&laparams, &[char1, char2]);

    assert_eq!(lines.len(), 1);
    match &lines[0] {
        TextLineType::Horizontal(line) => {
            let text = line.get_text();
            assert!(
                text.contains(" "),
                "Expected space between words: '{}'",
                text
            );
        }
        _ => panic!("Expected horizontal line"),
    }
}

#[test]
fn test_group_objects_multiple_lines() {
    let laparams = LAParams::default();
    let layout = LTLayoutContainer::new((0.0, 0.0, 100.0, 100.0));

    // Two separate lines (far apart vertically)
    let char1 = LTChar::new((10.0, 80.0, 20.0, 90.0), "A", "Helvetica", 10.0, true, 10.0);
    let char2 = LTChar::new((10.0, 10.0, 20.0, 20.0), "B", "Helvetica", 10.0, true, 10.0);

    let lines = layout.group_objects(&laparams, &[char1, char2]);

    assert_eq!(lines.len(), 2);
}

// ============================================================================
// LTPage tests
// ============================================================================

#[test]
fn test_ltpage_basic() {
    let page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);

    assert_eq!(page.pageid, 1);
    assert_eq!(page.rotate, 0.0);
    assert_eq!(page.x0(), 0.0);
    assert_eq!(page.y0(), 0.0);
    assert_eq!(page.x1(), 612.0);
    assert_eq!(page.y1(), 792.0);
}

#[test]
fn test_ltpage_add_items() {
    let mut page = LTPage::new(1, (0.0, 0.0, 612.0, 792.0), 0.0);

    let char1 = LTChar::new((10.0, 10.0, 20.0, 20.0), "A", "Helvetica", 10.0, true, 10.0);
    page.add(LTItem::Char(char1));

    let rect = LTRect::new(
        1.0,
        (50.0, 50.0, 100.0, 100.0),
        true,
        false,
        false,
        None,
        None,
    );
    page.add(LTItem::Rect(rect));

    let items: Vec<_> = page.iter().collect();
    assert_eq!(items.len(), 2);
}

#[test]
fn test_ltpage_analyze() {
    let mut page = LTPage::new(1, (0.0, 0.0, 100.0, 100.0), 0.0);

    // Add some characters
    let char1 = LTChar::new((10.0, 10.0, 20.0, 20.0), "H", "Helvetica", 10.0, true, 10.0);
    let char2 = LTChar::new((21.0, 10.0, 31.0, 20.0), "i", "Helvetica", 10.0, true, 10.0);
    page.add(LTItem::Char(char1));
    page.add(LTItem::Char(char2));

    let laparams = LAParams::default();
    page.analyze(&laparams);

    // After analysis, items should be reorganized into text boxes
    let items: Vec<_> = page.iter().collect();
    assert!(!items.is_empty());
}

// ============================================================================
// LTFigure tests
// ============================================================================

#[test]
fn test_ltfigure_basic() {
    let matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0); // identity matrix
    let figure = LTFigure::new("Figure1", (0.0, 0.0, 100.0, 50.0), matrix);

    assert_eq!(figure.name, "Figure1");
    // With identity matrix, bbox should be (x, y, x+w, y+h) = (0, 0, 100, 50)
    assert_eq!(figure.x0(), 0.0);
    assert_eq!(figure.y0(), 0.0);
    assert_eq!(figure.x1(), 100.0);
    assert_eq!(figure.y1(), 50.0);
}

#[test]
fn test_ltfigure_with_transform() {
    // Translation matrix: move by (10, 20)
    let matrix = (1.0, 0.0, 0.0, 1.0, 10.0, 20.0);
    let figure = LTFigure::new("Figure2", (0.0, 0.0, 100.0, 50.0), matrix);

    // Transformed bbox should be shifted
    assert_eq!(figure.x0(), 10.0);
    assert_eq!(figure.y0(), 20.0);
    assert_eq!(figure.x1(), 110.0);
    assert_eq!(figure.y1(), 70.0);
}

#[test]
fn test_ltfigure_analyze_respects_all_texts() {
    let matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let mut figure = LTFigure::new("Figure", (0.0, 0.0, 100.0, 100.0), matrix);

    let char1 = LTChar::new((10.0, 10.0, 20.0, 20.0), "X", "Helvetica", 10.0, true, 10.0);
    figure.add(LTItem::Char(char1));

    // With all_texts=false (default), analyze should not process figure contents
    let laparams = LAParams::default();
    assert!(!laparams.all_texts);
    figure.analyze(&laparams);

    // With all_texts=true, it should analyze
    let laparams_all = LAParams::new(0.5, 2.0, 0.5, 0.1, Some(0.5), false, true);
    assert!(laparams_all.all_texts);
    figure.analyze(&laparams_all);
}

// ============================================================================
// LTTextGroup tests
// ============================================================================

#[test]
fn test_lttextgroup_basic() {
    use bolivar_core::layout::TextGroupElement;

    let mut box1 = LTTextBoxHorizontal::new();
    let mut line1 = LTTextLineHorizontal::new(0.1);
    line1.set_bbox((0.0, 0.0, 50.0, 10.0));
    box1.add(line1);

    let mut box2 = LTTextBoxHorizontal::new();
    let mut line2 = LTTextLineHorizontal::new(0.1);
    line2.set_bbox((0.0, 20.0, 50.0, 30.0));
    box2.add(line2);

    use bolivar_core::layout::TextBoxType;
    let elements = vec![
        TextGroupElement::Box(TextBoxType::Horizontal(box1)),
        TextGroupElement::Box(TextBoxType::Horizontal(box2)),
    ];

    let group = LTTextGroup::new(elements, false);

    assert!(!group.is_vertical());
    assert_eq!(group.elements().len(), 2);
    // Group bbox should encompass both boxes
    assert_eq!(group.x0(), 0.0);
    assert_eq!(group.y0(), 0.0);
    assert_eq!(group.x1(), 50.0);
    assert_eq!(group.y1(), 30.0);
}

// ============================================================================
// LTItem tests
// ============================================================================

#[test]
fn test_ltitem_is_char() {
    let char_item = LTItem::Char(LTChar::new(
        (0.0, 0.0, 10.0, 10.0),
        "A",
        "Helvetica",
        10.0,
        true,
        10.0,
    ));
    assert!(char_item.is_char());

    let rect_item = LTItem::Rect(LTRect::new(
        1.0,
        (0.0, 0.0, 10.0, 10.0),
        false,
        false,
        false,
        None,
        None,
    ));
    assert!(!rect_item.is_char());
}

#[test]
fn test_ltitem_bbox() {
    let char_item = LTItem::Char(LTChar::new(
        (5.0, 10.0, 15.0, 20.0),
        "A",
        "Helvetica",
        10.0,
        true,
        10.0,
    ));
    assert_eq!(char_item.x0(), 5.0);
    assert_eq!(char_item.y0(), 10.0);
    assert_eq!(char_item.x1(), 15.0);
    assert_eq!(char_item.y1(), 20.0);
}

// ============================================================================
// MCID (Marked Content ID) support tests
// ============================================================================

#[test]
fn test_ltchar_mcid_defaults_to_none() {
    let char1 = LTChar::new((10.0, 10.0, 20.0, 20.0), "A", "Helvetica", 10.0, true, 10.0);
    assert_eq!(char1.mcid(), None);
}

#[test]
fn test_ltchar_with_mcid() {
    let char1 = LTChar::with_mcid(
        (10.0, 10.0, 20.0, 20.0),
        "A",
        "Helvetica",
        10.0,
        true,
        10.0,
        Some(42),
    );
    assert_eq!(char1.mcid(), Some(42));
}

#[test]
fn test_ltchar_tag_defaults_to_none() {
    let char1 = LTChar::new((10.0, 10.0, 20.0, 20.0), "A", "Helvetica", 10.0, true, 10.0);
    assert_eq!(char1.tag(), None);
}

#[test]
fn test_ltchar_with_marked_content() {
    let char1 = LTChar::builder((10.0, 10.0, 20.0, 20.0), "A", "Helvetica", 10.0)
        .upright(true)
        .adv(10.0)
        .mcid(Some(42))
        .tag(Some("P".to_string()))
        .build();
    assert_eq!(char1.mcid(), Some(42));
    assert_eq!(char1.tag(), Some("P".to_string()));
}

// ============================================================================
// GroupHeapEntry tests - exact pdfminer-compatible grouping
// ============================================================================

#[test]
fn test_group_heap_entry_ordering() {
    use bolivar_core::layout::GroupHeapEntry;
    use std::collections::BinaryHeap;

    let e1 = GroupHeapEntry {
        skip_isany: false,
        dist: 10,
        id1: 5,
        id2: 10,
        elem1_idx: 0,
        elem2_idx: 1,
    };
    let e2 = GroupHeapEntry {
        skip_isany: false,
        dist: 10,
        id1: 3,
        id2: 20,
        elem1_idx: 0,
        elem2_idx: 2,
    };
    // e2 should pop first (same dist, smaller id1)
    let mut heap = BinaryHeap::new();
    heap.push(e1);
    heap.push(e2);
    let first = heap.pop().unwrap();
    assert_eq!(first.id1, 3, "Smaller id1 should pop first on tie");
}

#[test]
fn test_frontier_entry_ordering() {
    use bolivar_core::layout::{FrontierEntry, PairMode, TreeKind};
    use std::collections::BinaryHeap;

    let f1 = FrontierEntry {
        lb_dist: 10,
        lb_id1: 5,
        lb_id2: 10,
        node_a: 0,
        node_b: 1,
        mode: PairMode::InitialIJ,
        tree: TreeKind::Initial,
    };
    let f2 = FrontierEntry {
        lb_dist: 10,
        lb_id1: 3,
        lb_id2: 20,
        node_a: 0,
        node_b: 2,
        mode: PairMode::InitialIJ,
        tree: TreeKind::Initial,
    };
    // f2 should pop first (same dist, smaller lb_id1)
    let mut heap = BinaryHeap::new();
    heap.push(f1);
    heap.push(f2);
    let first = heap.pop().unwrap();
    assert_eq!(first.lb_id1, 3, "Smaller lb_id1 should pop first on tie");
}

#[test]
fn test_frontier_could_beat() {
    use bolivar_core::layout::{FrontierEntry, GroupHeapEntry, PairMode, TreeKind};

    let frontier = FrontierEntry {
        lb_dist: 10,
        lb_id1: 3,
        lb_id2: 5,
        node_a: 0,
        node_b: 1,
        mode: PairMode::InitialIJ,
        tree: TreeKind::Initial,
    };
    let main_higher_id = GroupHeapEntry {
        skip_isany: false,
        dist: 10,
        id1: 5,
        id2: 10,
        elem1_idx: 0,
        elem2_idx: 1,
    };
    let main_lower_id = GroupHeapEntry {
        skip_isany: false,
        dist: 10,
        id1: 1,
        id2: 2,
        elem1_idx: 0,
        elem2_idx: 1,
    };
    let main_equal_id = GroupHeapEntry {
        skip_isany: false,
        dist: 10,
        id1: 3,
        id2: 5,
        elem1_idx: 0,
        elem2_idx: 1,
    };

    assert!(
        frontier.could_beat(&main_higher_id),
        "Frontier with lower id should beat"
    );
    assert!(
        !frontier.could_beat(&main_lower_id),
        "Frontier with higher id should not beat"
    );
    assert!(
        frontier.could_beat(&main_equal_id),
        "Frontier with equal id should beat (tie-safe <=)"
    );
}

// ============================================================================
// NodeStats tests - spatial tree node statistics
// ============================================================================

#[test]
fn test_node_stats_merge_tracks_second_min() {
    use bolivar_core::layout::{NodeStats, PyId};

    // Single element has second_min = MAX
    let stats1 = NodeStats::from_bbox_and_id((0.0, 0.0, 10.0, 10.0), 5);
    assert_eq!(stats1.min_py_id, 5);
    assert_eq!(stats1.second_min_py_id, PyId::MAX);

    // Merge two: second_min should be the larger of the two min_py_ids
    let stats2 = NodeStats::from_bbox_and_id((20.0, 0.0, 30.0, 10.0), 3);
    let merged = stats1.merge(&stats2);
    assert_eq!(merged.min_py_id, 3);
    assert_eq!(merged.second_min_py_id, 5);
}

fn f64_total_key_for_test(x: f64) -> i64 {
    let mut bits = x.to_bits() as i64;
    bits ^= (((bits >> 63) as u64) >> 1) as i64;
    bits
}

#[test]
fn test_calc_lower_bound_non_overlapping() {
    use bolivar_core::layout::{NodeStats, calc_dist_lower_bound};

    let stats_a = NodeStats::from_bbox_and_id((0.0, 0.0, 10.0, 10.0), 0);
    let stats_b = NodeStats::from_bbox_and_id((100.0, 100.0, 110.0, 110.0), 1);
    let lb = calc_dist_lower_bound(&stats_a, &stats_b);
    assert!(
        lb > f64_total_key_for_test(0.0),
        "Non-overlapping boxes should have positive LB"
    );
}

#[test]
fn test_calc_lower_bound_overlapping() {
    use bolivar_core::layout::{NodeStats, calc_dist_lower_bound};

    let stats_a = NodeStats::from_bbox_and_id((0.0, 0.0, 50.0, 50.0), 0);
    let stats_b = NodeStats::from_bbox_and_id((25.0, 25.0, 75.0, 75.0), 1);
    let lb = calc_dist_lower_bound(&stats_a, &stats_b);
    assert!(
        lb >= f64_total_key_for_test(-2500.0),
        "LB should be clamped"
    );
}

#[test]
fn test_frontier_new_initial_skips_single_element() {
    use bolivar_core::layout::{FrontierEntry, NodeStats, PyId};

    let stats_single = NodeStats::from_bbox_and_id((0.0, 0.0, 10.0, 10.0), 5);
    assert_eq!(stats_single.second_min_py_id, PyId::MAX);

    // Self-pair with single element should return None
    let entry = FrontierEntry::new_initial(
        f64_total_key_for_test(0.0),
        &stats_single,
        &stats_single,
        0,
        0,
    );
    assert!(entry.is_none(), "Should skip self-pair with < 2 elements");
}

// ============================================================================
// SpatialNode tests - lightweight tree for frontier expansion
// ============================================================================

#[test]
fn test_spatial_node_build_and_split() {
    use bolivar_core::layout::{PyId, SpatialNode};
    use bolivar_core::utils::Rect;

    // Create test elements with known positions and py_ids
    let bboxes: Vec<(Rect, PyId)> = (0..20)
        .map(|i| {
            (
                (i as f64 * 10.0, 0.0, i as f64 * 10.0 + 5.0, 5.0),
                i as PyId,
            )
        })
        .collect();

    let mut nodes_arena: Vec<SpatialNode> = Vec::new();
    let root_idx = SpatialNode::build(&bboxes, &mut nodes_arena);

    let root = &nodes_arena[root_idx];
    assert_eq!(root.element_count(), 20);
    assert_eq!(root.stats.min_py_id, 0);
    // With 20 elements > LEAF_THRESHOLD=8, root should have children
    assert!(!root.is_leaf(), "Root with 20 elements should be split");
}

#[test]
fn test_spatial_node_internal_has_no_indices() {
    use bolivar_core::layout::{PyId, SpatialNode};
    use bolivar_core::utils::Rect;

    let bboxes: Vec<(Rect, PyId)> = (0..20)
        .map(|i| ((i as f64, 0.0, i as f64 + 0.5, 1.0), i as PyId))
        .collect();
    let mut arena = Vec::new();
    let root = SpatialNode::build(&bboxes, &mut arena);

    assert!(!arena[root].is_leaf());
    assert!(arena[root].element_indices.is_empty());
    assert_eq!(arena[root].element_count(), 20);
}

#[test]
fn test_spatial_tree_handles_nan() {
    use bolivar_core::layout::{PyId, SpatialNode};
    use bolivar_core::utils::Rect;

    let bboxes: Vec<(Rect, PyId)> = (0..9)
        .map(|i| {
            let x0 = if i == 4 { f64::NAN } else { i as f64 };
            let x1 = x0 + 0.5;
            ((x0, 0.0, x1, 1.0), i as PyId)
        })
        .collect();

    let result = std::panic::catch_unwind(|| {
        let mut arena = Vec::new();
        SpatialNode::build(&bboxes, &mut arena);
    });

    assert!(result.is_ok(), "Spatial tree should tolerate NaN bboxes");
}

// ============================================================================
// group_textboxes_exact tests - exact pdfminer-compatible grouping
// ============================================================================

#[test]
fn test_group_textboxes_exact_two_boxes() {
    use bolivar_core::layout::{
        LAParams, LTLayoutContainer, LTTextBoxHorizontal, LTTextLineHorizontal, TextBoxType,
    };

    let container = LTLayoutContainer::new((0.0, 0.0, 200.0, 100.0));

    // Create box1 with line
    let mut box1 = LTTextBoxHorizontal::new();
    let mut line1 = LTTextLineHorizontal::new(0.1);
    line1.set_bbox((0.0, 0.0, 50.0, 20.0));
    box1.add(line1);

    // Create box2 with line
    let mut box2 = LTTextBoxHorizontal::new();
    let mut line2 = LTTextLineHorizontal::new(0.1);
    line2.set_bbox((60.0, 0.0, 110.0, 20.0));
    box2.add(line2);

    let boxes = vec![TextBoxType::Horizontal(box1), TextBoxType::Horizontal(box2)];

    let groups = container.group_textboxes_exact(&LAParams::default(), &boxes);
    assert_eq!(
        groups.len(),
        1,
        "Two nearby boxes should merge into one group"
    );
}

#[test]
fn test_group_textboxes_exact_three_boxes_stable() {
    use bolivar_core::layout::{
        LAParams, LTLayoutContainer, LTTextBoxHorizontal, LTTextLineHorizontal, TextBoxType,
    };

    let container = LTLayoutContainer::new((0.0, 0.0, 200.0, 100.0));

    let mut b1 = LTTextBoxHorizontal::new();
    let mut l1 = LTTextLineHorizontal::new(0.1);
    l1.set_bbox((0.0, 0.0, 40.0, 20.0));
    b1.add(l1);

    let mut b2 = LTTextBoxHorizontal::new();
    let mut l2 = LTTextLineHorizontal::new(0.1);
    l2.set_bbox((45.0, 0.0, 85.0, 20.0));
    b2.add(l2);

    let mut b3 = LTTextBoxHorizontal::new();
    let mut l3 = LTTextLineHorizontal::new(0.1);
    l3.set_bbox((90.0, 0.0, 130.0, 20.0));
    b3.add(l3);

    let boxes = vec![
        TextBoxType::Horizontal(b1),
        TextBoxType::Horizontal(b2),
        TextBoxType::Horizontal(b3),
    ];

    let groups = container.group_textboxes_exact(&LAParams::default(), &boxes);
    assert!(!groups.is_empty());
}

/// Test that analyze() uses exact grouping (now default).
#[test]
fn test_analyze_uses_exact_grouping() {
    use bolivar_core::layout::{LAParams, LTChar, LTItem, LTLayoutContainer};

    // Create container with some characters that will be grouped
    let mut container = LTLayoutContainer::new((0.0, 0.0, 200.0, 100.0));

    // Add characters that should form text lines and boxes
    // LTChar::new(bbox, text, fontname, size, upright, adv)
    let char1 = LTChar::new((0.0, 0.0, 10.0, 12.0), "A", "TestFont", 12.0, true, 10.0);
    let char2 = LTChar::new((10.0, 0.0, 20.0, 12.0), "B", "TestFont", 12.0, true, 10.0);
    container.add(LTItem::Char(char1));
    container.add(LTItem::Char(char2));

    let laparams = LAParams::default();
    container.analyze(&laparams);

    // Verify analyze completed without panic - exact grouping path was used
    // The groups field should be set when boxes_flow is Some (default)
    assert!(
        container.groups.is_some(),
        "Groups should be set after analyze with boxes_flow"
    );
}

// ============================================================================
// PDF-based empty character tests (issue-449)
// ============================================================================

#[test]
fn test_pdf_with_empty_characters_horizontal() {
    // Regression test for issue #449
    // See: https://github.com/pdfminer/pdfminer.six/pull/689
    // The page aggregator should separate the 3 horizontal lines
    use bolivar_core::high_level::extract_pages;
    use bolivar_core::layout::TextBoxType;

    let pdf_data = include_bytes!("fixtures/contrib/issue-449-horizontal.pdf");
    let pages: Vec<_> = extract_pages(pdf_data, None)
        .expect("Failed to extract pages")
        .collect();

    let page = pages
        .into_iter()
        .next()
        .expect("Should have at least one page")
        .expect("Page extraction should succeed");

    let textboxes: Vec<_> = page
        .iter()
        .filter(|item| matches!(item, LTItem::TextBox(TextBoxType::Horizontal(_))))
        .collect();

    assert_eq!(textboxes.len(), 3, "Should have 3 horizontal textboxes");
}

#[test]
fn test_pdf_with_empty_characters_vertical() {
    // Regression test for issue #449
    // See: https://github.com/pdfminer/pdfminer.six/pull/689
    use bolivar_core::high_level::{ExtractOptions, extract_pages};
    use bolivar_core::layout::TextBoxType;

    let pdf_data = include_bytes!("fixtures/contrib/issue-449-vertical.pdf");

    let laparams = LAParams {
        detect_vertical: true,
        ..Default::default()
    };

    let options = ExtractOptions {
        laparams: Some(laparams),
        ..Default::default()
    };

    let pages: Vec<_> = extract_pages(pdf_data, Some(options))
        .expect("Failed to extract pages")
        .collect();

    let page = pages
        .into_iter()
        .next()
        .expect("Should have at least one page")
        .expect("Page extraction should succeed");

    let textboxes: Vec<_> = page
        .iter()
        .filter(|item| matches!(item, LTItem::TextBox(TextBoxType::Vertical(_))))
        .collect();

    assert_eq!(textboxes.len(), 3, "Should have 3 vertical textboxes");
}
