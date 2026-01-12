//! Coordinate transformation utilities for table extraction.
//!
//! PDF uses a bottom-left origin coordinate system, while table extraction
//! uses a top-left origin. These functions handle the conversion.

use super::types::{BBox, PageGeometry};
use crate::utils::{Matrix, Rect, apply_matrix_rects};

/// Get the page height from the mediabox.
pub fn page_height(geom: &PageGeometry) -> f64 {
    geom.mediabox.3 - geom.mediabox.1
}

/// Get the x and y offsets from the mediabox origin.
pub const fn mb_offsets(geom: &PageGeometry) -> (f64, f64) {
    (geom.mediabox.0, geom.mediabox.1)
}

/// Convert a bounding box from bottom-left to top-left origin.
pub fn to_top_left_bbox(x0: f64, y0: f64, x1: f64, y1: f64, geom: &PageGeometry) -> BBox {
    let (mb_x0, mb_top) = mb_offsets(geom);
    let top = (page_height(geom) - y1) + mb_top;
    let bottom = (page_height(geom) - y0) + mb_top;
    BBox {
        x0: x0 + mb_x0,
        x1: x1 + mb_x0,
        top,
        bottom,
    }
}

/// Applies a matrix to a slice of rectangles using batched transform.
pub fn transform_bboxes_batch(m: Matrix, rects: &[Rect]) -> Vec<Rect> {
    apply_matrix_rects(m, rects)
}

/// Convert a list of bottom-left rects to top-left bboxes in a single batch.
pub fn to_top_left_bboxes_batch(rects: &[Rect], geom: &PageGeometry) -> Vec<BBox> {
    if rects.is_empty() {
        return Vec::new();
    }
    let (mb_x0, mb_top) = mb_offsets(geom);
    let m = (1.0, 0.0, 0.0, -1.0, mb_x0, page_height(geom) + mb_top);
    transform_bboxes_batch(m, rects)
        .into_iter()
        .map(|(x0, y0, x1, y1)| BBox {
            x0,
            x1,
            top: y0,
            bottom: y1,
        })
        .collect()
}
