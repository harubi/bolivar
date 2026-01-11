//! Distance calculations for spatial tree algorithms.
//!
//! Functions for computing distances and lower bounds between bounding boxes.

use super::types::{DistKey, NodeStats};
use crate::utils::Rect;

/// Convert f64 to a sortable integer key preserving total ordering.
///
/// This allows f64 values to be used in BinaryHeap while maintaining
/// correct ordering including handling of -0.0, infinities, and NaN.
#[inline(always)]
pub const fn f64_total_key(x: f64) -> DistKey {
    let mut bits = x.to_bits() as i64;
    bits ^= (((bits >> 63) as u64) >> 1) as i64;
    bits
}

/// Compute distance key from two bounding boxes.
///
/// Distance is defined as: union_area - area_a - area_b
/// This measures the "gap" between elements.
#[inline(always)]
pub fn dist_key_from_geom(a: Rect, area_a: f64, b: Rect, area_b: f64) -> DistKey {
    let x0 = a.0.min(b.0);
    let y0 = a.1.min(b.1);
    let x1 = a.2.max(b.2);
    let y1 = a.3.max(b.3);
    f64_total_key((x1 - x0).mul_add(y1 - y0, -area_a) - area_b)
}

/// Calculate area of a bounding box
pub fn bbox_area(bbox: Rect) -> f64 {
    let w = bbox.2 - bbox.0;
    let h = bbox.3 - bbox.1;
    w * h
}

/// Calculate union of two bounding boxes
pub const fn bbox_union(a: Rect, b: Rect) -> Rect {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

/// Calculate area expansion when adding a bbox to current bbox
pub fn bbox_expand_area(current: Rect, add: Rect) -> f64 {
    let union = bbox_union(current, add);
    bbox_area(union) - bbox_area(current)
}

/// Calculate lower bound on dist() for any pair between two nodes.
///
/// Uses tight geometric bound: max(min_w) not min(min_w).
/// The minimum union bbox must span at least the larger of the two smallest elements.
pub fn calc_dist_lower_bound(a: &NodeStats, b: &NodeStats) -> DistKey {
    // Gap between bounding boxes
    let gap_x = (a.bbox.0.max(b.bbox.0) - a.bbox.2.min(b.bbox.2)).max(0.0);
    let gap_y = (a.bbox.1.max(b.bbox.1) - a.bbox.3.min(b.bbox.3)).max(0.0);

    // TIGHTER bound: use max(min_w), max(min_h) - the minimum union bbox
    // must span at least the larger of the two smallest elements
    let w_lb = gap_x + a.min_w.max(b.min_w);
    let h_lb = gap_y + a.min_h.max(b.min_h);

    // Geometric lower bound: min_union_area - max_area_a - max_area_b
    let geometric_lb = w_lb * h_lb - a.max_area - b.max_area;

    // Clamp: dist(a,b) >= -min(area(a), area(b))
    let clamped = geometric_lb.max(-a.max_area.min(b.max_area));

    f64_total_key(clamped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f64_total_key_matches_total_cmp() {
        let vals = [
            f64::NEG_INFINITY,
            -1.0,
            -0.0,
            0.0,
            1.0,
            f64::INFINITY,
            f64::NAN,
        ];
        for &a in &vals {
            for &b in &vals {
                assert_eq!(f64_total_key(a).cmp(&f64_total_key(b)), a.total_cmp(&b));
            }
        }
    }

    #[test]
    fn test_dist_key_from_geom_matches_manual_formula() {
        let a = (0.0, 0.0, 10.0, 10.0);
        let b = (20.0, 0.0, 30.0, 10.0);
        let area_a = 100.0;
        let area_b = 100.0;
        let expected = f64_total_key((30.0 - 0.0) * (10.0 - 0.0) - area_a - area_b);
        assert_eq!(dist_key_from_geom(a, area_a, b, area_b), expected);
    }

    #[test]
    fn bbox_union_matches_expected() {
        let a = (0.0, 1.0, 5.0, 6.0);
        let b = (-1.0, 2.0, 7.0, 4.0);
        assert_eq!(bbox_union(a, b), (-1.0, 1.0, 7.0, 6.0));
    }
}
