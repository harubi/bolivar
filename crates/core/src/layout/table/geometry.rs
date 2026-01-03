//! Coordinate transformation utilities for table extraction.
//!
//! PDF uses a bottom-left origin coordinate system, while table extraction
//! uses a top-left origin. These functions handle the conversion.

use super::types::{BBox, PageGeometry};

/// Get the page height from the mediabox.
pub(crate) fn page_height(geom: &PageGeometry) -> f64 {
    geom.mediabox.3 - geom.mediabox.1
}

/// Get the x and y offsets from the mediabox origin.
pub(crate) fn mb_offsets(geom: &PageGeometry) -> (f64, f64) {
    (geom.mediabox.0, geom.mediabox.1)
}

/// Convert a y-coordinate from bottom-left to top-left origin.
pub(crate) fn to_top_left_y(y: f64, geom: &PageGeometry) -> f64 {
    let (.., mb_top) = mb_offsets(geom);
    page_height(geom) - y + mb_top
}

/// Convert a bounding box from bottom-left to top-left origin.
pub(crate) fn to_top_left_bbox(x0: f64, y0: f64, x1: f64, y1: f64, geom: &PageGeometry) -> BBox {
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
