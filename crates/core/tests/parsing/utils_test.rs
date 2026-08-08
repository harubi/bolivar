//! Port of pdfminer.six tests/test_utils.py
//!
//! Tests for utils module including Plane, matrix operations, and formatting functions.

use bolivar_core::utils::{
    HasBBox, Matrix, Plane, Point, Rect, apply_matrix_pt, apply_matrix_rect, choplist, decode_text,
    format_int_alpha, format_int_roman, mult_matrix, nunpack, shorten_str, translate_matrix,
};

// Helper struct that implements HasBBox for testing Plane
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LTComponent {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
}

impl LTComponent {
    fn new(bbox: (i64, i64, i64, i64)) -> Self {
        Self {
            x0: bbox.0,
            y0: bbox.1,
            x1: bbox.2,
            y1: bbox.3,
        }
    }
}

impl HasBBox for LTComponent {
    fn x0(&self) -> f64 {
        self.x0 as f64
    }
    fn y0(&self) -> f64 {
        self.y0 as f64
    }
    fn x1(&self) -> f64 {
        self.x1 as f64
    }
    fn y1(&self) -> f64 {
        self.y1 as f64
    }
}

fn given_plane_with_one_object(
    object_size: i64,
    gridsize: i32,
) -> (Plane<LTComponent>, LTComponent) {
    let bounding_box = (0.0, 0.0, 100.0, 100.0);
    let mut plane = Plane::new(bounding_box, gridsize);
    let obj = LTComponent::new((0, 0, object_size, object_size));
    plane.add(obj.clone());
    (plane, obj)
}

// ============================================================================
// TestPlane - 5 tests
// ============================================================================

#[test]
fn test_plane_find_nothing_in_empty_bbox() {
    let (plane, _) = given_plane_with_one_object(50, 50);
    let result = plane.find((50.0, 50.0, 100.0, 100.0));
    assert!(result.is_empty());
}

#[test]
fn test_plane_find_nothing_after_removing() {
    let (mut plane, obj) = given_plane_with_one_object(50, 50);
    plane.remove(&obj);
    let result = plane.find((0.0, 0.0, 100.0, 100.0));
    assert!(result.is_empty());
}

#[test]
fn test_plane_find_object_in_whole_plane() {
    let (plane, obj) = given_plane_with_one_object(50, 50);
    let result = plane.find((0.0, 0.0, 100.0, 100.0));
    assert_eq!(result.len(), 1);
    assert_eq!(*result[0], obj);
}

#[test]
fn test_plane_find_if_object_is_smaller_than_gridsize() {
    let (plane, obj) = given_plane_with_one_object(1, 100);
    let result = plane.find((0.0, 0.0, 100.0, 100.0));
    assert_eq!(result.len(), 1);
    assert_eq!(*result[0], obj);
}

#[test]
fn test_plane_find_object_if_much_larger_than_gridsize() {
    let (plane, obj) = given_plane_with_one_object(100, 10);
    let result = plane.find((0.0, 0.0, 100.0, 100.0));
    assert_eq!(result.len(), 1);
    assert_eq!(*result[0], obj);
}

#[test]
fn test_plane_remove_by_id_preserves_insertion_order() {
    let bbox = (0.0, 0.0, 100.0, 100.0);
    let mut plane = Plane::new(bbox, 10);

    let a = LTComponent::new((0, 0, 10, 10));
    let b = LTComponent::new((20, 0, 30, 10));
    let c = LTComponent::new((40, 0, 50, 10));

    plane.add(a);
    plane.add(b);
    plane.add(c);

    assert!(plane.remove_by_id(1));

    let ids: Vec<usize> = plane.iter_with_indices().map(|(i, _)| i).collect();
    assert_eq!(ids, vec![0, 2]);
}

#[test]
fn test_plane_find_excludes_touching_edges() {
    let bbox = (0.0, 0.0, 100.0, 100.0);
    let mut plane = Plane::new(bbox, 10);
    let obj = LTComponent::new((0, 0, 10, 10));
    plane.add(obj.clone());

    // Query that touches the object's right edge at x=10 should NOT intersect.
    let result = plane.find((10.0, 0.0, 20.0, 10.0));
    assert!(result.is_empty());
}

// ============================================================================
// TestFunctions - shorten_str - 3 tests
// ============================================================================

#[test]
fn test_shorten_str() {
    let s = shorten_str("Hello there World", 15);
    assert_eq!(s, "Hello ... World");
}

#[test]
fn test_shorten_short_str_is_same() {
    let s = "Hello World";
    assert_eq!(shorten_str(s, 50), s);
}

#[test]
fn test_shorten_to_really_short() {
    assert_eq!(shorten_str("Hello World", 5), "Hello");
}

// ============================================================================
// TestFunctions - format_int_alpha - 1 test with many assertions
// ============================================================================

#[test]
fn test_format_int_alpha() {
    assert_eq!(format_int_alpha(1), "a");
    assert_eq!(format_int_alpha(2), "b");
    assert_eq!(format_int_alpha(26), "z");
    assert_eq!(format_int_alpha(27), "aa");
    assert_eq!(format_int_alpha(28), "ab");
    assert_eq!(format_int_alpha(26 * 2), "az");
    assert_eq!(format_int_alpha(26 * 2 + 1), "ba");
    assert_eq!(format_int_alpha(26 * 27), "zz");
    assert_eq!(format_int_alpha(26 * 27 + 1), "aaa");
}

// ============================================================================
// TestFunctions - format_int_roman - 1 test with many assertions
// ============================================================================

#[test]
fn test_format_int_roman() {
    assert_eq!(format_int_roman(1), "i");
    assert_eq!(format_int_roman(2), "ii");
    assert_eq!(format_int_roman(3), "iii");
    assert_eq!(format_int_roman(4), "iv");
    assert_eq!(format_int_roman(5), "v");
    assert_eq!(format_int_roman(6), "vi");
    assert_eq!(format_int_roman(7), "vii");
    assert_eq!(format_int_roman(8), "viii");
    assert_eq!(format_int_roman(9), "ix");
    assert_eq!(format_int_roman(10), "x");
    assert_eq!(format_int_roman(11), "xi");
    assert_eq!(format_int_roman(20), "xx");
    assert_eq!(format_int_roman(40), "xl");
    assert_eq!(format_int_roman(45), "xlv");
    assert_eq!(format_int_roman(50), "l");
    assert_eq!(format_int_roman(90), "xc");
    assert_eq!(format_int_roman(91), "xci");
    assert_eq!(format_int_roman(100), "c");
}

// ============================================================================
// test_mult_matrix - parametrized test cases - 4 tests
// ============================================================================

#[test]
fn test_mult_matrix_identity_identity() {
    let m0: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let m1: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let expected: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    assert_eq!(mult_matrix(m0, m1), expected);
}

#[test]
fn test_mult_matrix_with_identity() {
    let m0: Matrix = (1.0, 2.0, 3.0, 2.0, -4.0, 1.0);
    let m1: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let expected: Matrix = (1.0, 2.0, 3.0, 2.0, -4.0, 1.0);
    assert_eq!(mult_matrix(m0, m1), expected);
}

#[test]
fn test_mult_matrix_general() {
    let m0: Matrix = (1.0, 2.0, 3.0, 2.0, -4.0, 1.0);
    let m1: Matrix = (3.0, 4.0, 1.0, 2.0, -2.0, 1.0);
    let expected: Matrix = (5.0, 8.0, 11.0, 16.0, -13.0, -13.0);
    assert_eq!(mult_matrix(m0, m1), expected);
}

#[test]
fn test_mult_matrix_cancellation() {
    let m0: Matrix = (1.0, -1.0, 1.0, -1.0, 1.0, -1.0);
    let m1: Matrix = (1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
    let expected: Matrix = (0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    assert_eq!(mult_matrix(m0, m1), expected);
}

// ============================================================================
// test_translate_matrix - parametrized test cases - 6 tests
// ============================================================================

#[test]
fn test_translate_matrix_origin() {
    let m: Matrix = (1.0, 2.0, 3.0, 2.0, -4.0, 1.0);
    let p: Point = (0.0, 0.0);
    let expected: Matrix = (1.0, 2.0, 3.0, 2.0, -4.0, 1.0);
    assert_eq!(translate_matrix(m, p), expected);
}

#[test]
fn test_translate_matrix_identity_translate() {
    let m: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let p: Point = (12.0, -32.0);
    let expected: Matrix = (1.0, 0.0, 0.0, 1.0, 12.0, -32.0);
    assert_eq!(translate_matrix(m, p), expected);
}

#[test]
fn test_translate_matrix_identity_with_offset() {
    let m: Matrix = (1.0, 0.0, 0.0, 1.0, 3.0, -3.0);
    let p: Point = (12.0, -32.0);
    let expected: Matrix = (1.0, 0.0, 0.0, 1.0, 15.0, -35.0);
    assert_eq!(translate_matrix(m, p), expected);
}

#[test]
fn test_translate_matrix_scale() {
    let m: Matrix = (2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
    let p: Point = (1.0, -1.0);
    let expected: Matrix = (2.0, 0.0, 0.0, 2.0, 2.0, -2.0);
    assert_eq!(translate_matrix(m, p), expected);
}

#[test]
fn test_translate_matrix_rotate_x() {
    let m: Matrix = (0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
    let p: Point = (1.0, 0.0);
    let expected: Matrix = (0.0, 1.0, -1.0, 0.0, 0.0, 1.0);
    assert_eq!(translate_matrix(m, p), expected);
}

#[test]
fn test_translate_matrix_rotate_y() {
    let m: Matrix = (0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
    let p: Point = (0.0, 1.0);
    let expected: Matrix = (0.0, 1.0, -1.0, 0.0, -1.0, 0.0);
    assert_eq!(translate_matrix(m, p), expected);
}

// ============================================================================
// test_apply_matrix_pt - parametrized test cases - 3 tests
// ============================================================================

#[test]
fn test_apply_matrix_pt_identity_origin() {
    let m: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let p: Point = (0.0, 0.0);
    let expected: Point = (0.0, 0.0);
    assert_eq!(apply_matrix_pt(m, p), expected);
}

#[test]
fn test_apply_matrix_pt_identity() {
    let m: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let p: Point = (33.0, 21.0);
    let expected: Point = (33.0, 21.0);
    assert_eq!(apply_matrix_pt(m, p), expected);
}

#[test]
fn test_apply_matrix_pt_with_translation() {
    let m: Matrix = (1.0, 2.0, 3.0, 2.0, -4.0, 1.0);
    let p: Point = (0.0, 0.0);
    let expected: Point = (-4.0, 1.0);
    assert_eq!(apply_matrix_pt(m, p), expected);
}

// ============================================================================
// test_apply_matrix_rect - parametrized test cases - many tests
// ============================================================================

fn approx_rect_eq(a: Rect, b: Rect) -> bool {
    const EPS: f64 = 1e-6;
    (a.0 - b.0).abs() < EPS
        && (a.1 - b.1).abs() < EPS
        && (a.2 - b.2).abs() < EPS
        && (a.3 - b.3).abs() < EPS
}

#[test]
fn test_apply_matrix_rect_identity_1() {
    let m: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let r: Rect = (0.0, 0.0, 100.0, 200.0);
    let expected: Rect = (0.0, 0.0, 100.0, 200.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_identity_2() {
    let m: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let r: Rect = (20.0, 30.0, 40.0, 50.0);
    let expected: Rect = (20.0, 30.0, 40.0, 50.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_translate_x() {
    let m: Matrix = (1.0, 0.0, 0.0, 1.0, 5.0, 0.0);
    let r: Rect = (0.0, 1.0, 2.0, 3.0);
    let expected: Rect = (5.0, 1.0, 7.0, 3.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_translate_y() {
    let m: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 7.0);
    let r: Rect = (0.0, 2.0, 4.0, 6.0);
    let expected: Rect = (0.0, 9.0, 4.0, 13.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_scale_x() {
    let m: Matrix = (2.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let r: Rect = (0.0, 1.0, 2.0, 3.0);
    let expected: Rect = (0.0, 1.0, 4.0, 3.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_scale_y() {
    let m: Matrix = (1.0, 0.0, 0.0, 2.0, 0.0, 0.0);
    let r: Rect = (0.0, 1.0, 2.0, 3.0);
    let expected: Rect = (0.0, 2.0, 2.0, 6.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_rotate_90() {
    let m: Matrix = (0.0, 1.0, 1.0, 0.0, 0.0, 0.0);
    let r: Rect = (3.0, 4.0, 7.0, 6.0);
    let expected: Rect = (4.0, 3.0, 6.0, 7.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_rotate_180() {
    let m: Matrix = (-1.0, 0.0, 0.0, -1.0, 0.0, 0.0);
    let r: Rect = (3.0, 4.0, 7.0, 6.0);
    let expected: Rect = (-7.0, -6.0, -3.0, -4.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_rotate_270() {
    let m: Matrix = (0.0, -1.0, 1.0, 0.0, 0.0, 0.0);
    let r: Rect = (3.0, 4.0, 7.0, 6.0);
    let expected: Rect = (4.0, -7.0, 6.0, -3.0);
    assert_eq!(apply_matrix_rect(m, r), expected);
}

#[test]
fn test_apply_matrix_rect_rotate_10_degrees() {
    let angle = 10.0_f64.to_radians();
    let m: Matrix = (
        angle.cos(),
        angle.sin(),
        -angle.sin(),
        angle.cos(),
        0.0,
        0.0,
    );
    let r: Rect = (3.0, 4.0, 7.0, 6.0);
    let result = apply_matrix_rect(m, r);
    let expected: Rect = (1.91253419, 4.46017555, 6.19906156, 7.12438376);
    assert!(
        approx_rect_eq(result, expected),
        "expected {:?}, got {:?}",
        expected,
        result
    );
}

#[test]
fn test_apply_matrix_rect_skew_1() {
    let m: Matrix = (
        1.0,
        5.0_f64.to_radians().tan(),
        7.0_f64.to_radians().tan(),
        1.0,
        0.0,
        0.0,
    );
    let r: Rect = (3.0, 4.0, 7.0, 6.0);
    let result = apply_matrix_rect(m, r);
    let expected: Rect = (
        3.4911382436116183,
        4.262465990577772,
        7.736707365417428,
        6.612420644681468,
    );
    assert!(
        approx_rect_eq(result, expected),
        "expected {:?}, got {:?}",
        expected,
        result
    );
}

#[test]
fn test_apply_matrix_rect_skew_2() {
    let m: Matrix = (
        1.0,
        (-11.0_f64).to_radians().tan(),
        (-9.0_f64).to_radians().tan(),
        1.0,
        0.0,
        0.0,
    );
    let r: Rect = (3.0, 4.0, 7.0, 6.0);
    let result = apply_matrix_rect(m, r);
    let expected: Rect = (
        2.0496933580527825,
        2.6393378360359705,
        6.366462238701855,
        5.416859072586845,
    );
    assert!(
        approx_rect_eq(result, expected),
        "expected {:?}, got {:?}",
        expected,
        result
    );
}

// ============================================================================
// Additional tests for coverage
// ============================================================================

#[test]
fn test_choplist_basic() {
    let data = vec![1, 2, 3, 4, 5, 6];
    let result: Vec<Vec<i32>> = choplist(2, data).collect();
    assert_eq!(result, vec![vec![1, 2], vec![3, 4], vec![5, 6]]);
}

#[test]
fn test_choplist_incomplete() {
    let data = vec![1, 2, 3, 4, 5];
    let result: Vec<Vec<i32>> = choplist(2, data).collect();
    // Incomplete final chunk is discarded
    assert_eq!(result, vec![vec![1, 2], vec![3, 4]]);
}

#[test]
fn test_nunpack_empty() {
    assert_eq!(nunpack(&[], 0), 0);
    assert_eq!(nunpack(&[], 42), 42);
}

#[test]
fn test_nunpack_single() {
    assert_eq!(nunpack(&[1], 0), 1);
    assert_eq!(nunpack(&[255], 0), 255);
}

#[test]
fn test_nunpack_multi() {
    assert_eq!(nunpack(&[1, 2], 0), 258); // 0x0102
    assert_eq!(nunpack(&[0, 0, 1], 0), 1);
    assert_eq!(nunpack(&[1, 0, 0], 0), 65536); // 0x010000
}

#[test]
fn test_decode_text_pdfdocencoding() {
    // ASCII range works as expected
    assert_eq!(decode_text(b"Hello"), "Hello");
}

#[test]
fn test_decode_text_utf16be() {
    // UTF-16BE with BOM
    let data = b"\xfe\xff\x00H\x00e\x00l\x00l\x00o";
    assert_eq!(decode_text(data), "Hello");
}

#[test]
fn test_plane_len_and_is_empty() {
    let mut plane: Plane<LTComponent> = Plane::new((0.0, 0.0, 100.0, 100.0), 50);
    assert!(plane.is_empty());
    assert_eq!(plane.len(), 0);

    let obj = LTComponent::new((10, 10, 20, 20));
    plane.add(obj);
    assert!(!plane.is_empty());
    assert_eq!(plane.len(), 1);
}

#[test]
fn test_plane_contains() {
    let (plane, obj) = given_plane_with_one_object(50, 50);
    assert!(plane.contains(&obj));

    let other = LTComponent::new((99, 99, 100, 100));
    assert!(!plane.contains(&other));
}

#[test]
fn test_plane_iter() {
    let (plane, obj) = given_plane_with_one_object(50, 50);
    let items: Vec<_> = plane.iter().collect();
    assert_eq!(items.len(), 1);
    assert_eq!(*items[0], obj);
}

#[test]
fn test_plane_extend() {
    let mut plane: Plane<LTComponent> = Plane::new((0.0, 0.0, 100.0, 100.0), 50);
    let objs = vec![
        LTComponent::new((0, 0, 10, 10)),
        LTComponent::new((20, 20, 30, 30)),
    ];
    plane.extend(objs);
    assert_eq!(plane.len(), 2);
}

#[test]
fn test_plane_neighbors() {
    let mut plane: Plane<LTComponent> = Plane::new((0.0, 0.0, 100.0, 100.0), 50);

    // Create objects at known positions:
    // A at (0,0)-(10,10), center = (5, 5)
    // B at (20,0)-(30,10), center = (25, 5) - distance ~20 from A
    // C at (50,0)-(60,10), center = (55, 5) - distance ~50 from A
    // D at (80,0)-(90,10), center = (85, 5) - distance ~80 from A
    let objs = vec![
        LTComponent::new((0, 0, 10, 10)),  // A - closest to query point
        LTComponent::new((20, 0, 30, 10)), // B - 2nd closest
        LTComponent::new((50, 0, 60, 10)), // C - 3rd closest
        LTComponent::new((80, 0, 90, 10)), // D - 4th closest
    ];
    plane.extend(objs);

    // Query for 2 nearest neighbors from center of A's bbox
    let query_bbox = (0.0, 0.0, 10.0, 10.0);
    let neighbors = plane.neighbors(query_bbox, 2);

    // Should return A and B (the 2 closest)
    assert_eq!(neighbors.len(), 2);

    // Verify we got the right items (indices 0 and 1)
    let indices: Vec<usize> = neighbors.iter().map(|(idx, _)| *idx).collect();
    assert!(indices.contains(&0)); // A
    assert!(indices.contains(&1)); // B
}
