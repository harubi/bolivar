//! Port of pdfminer.six tests/test_layout.py
//!
//! Tests for layout analysis module including LAParams, LTComponent hierarchy,
//! text line grouping, neighbor finding, graphical elements, and layout analysis.

use bolivar::layout::{
    LAParams, LTAnno, LTChar, LTComponent, LTCurve, LTFigure, LTImage, LTItem, LTLayoutContainer,
    LTLine, LTPage, LTRect, LTTextBox, LTTextBoxHorizontal, LTTextBoxVertical, LTTextGroup,
    LTTextLine, LTTextLineHorizontal, LTTextLineVertical, TextLineType,
};
use bolivar::utils::{HasBBox, Plane};

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
    use bolivar::layout::TextGroupElement;

    let mut box1 = LTTextBoxHorizontal::new();
    let mut line1 = LTTextLineHorizontal::new(0.1);
    line1.set_bbox((0.0, 0.0, 50.0, 10.0));
    box1.add(line1);

    let mut box2 = LTTextBoxHorizontal::new();
    let mut line2 = LTTextLineHorizontal::new(0.1);
    line2.set_bbox((0.0, 20.0, 50.0, 30.0));
    box2.add(line2);

    use bolivar::layout::TextBoxType;
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
