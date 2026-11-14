//! Coordinate transformation utilities for table extraction.
//!
//! PDF uses a bottom-left origin coordinate system, while table extraction
//! uses a top-left origin. These functions handle the conversion.

use super::types::{BBox, PageGeometry};
use crate::document::page::PDFPage;
use crate::utils::{Matrix, Rect, apply_matrix_rects};

fn normalize_rect(rect: [f64; 4]) -> Rect {
    (
        rect[0].min(rect[2]),
        rect[1].min(rect[3]),
        rect[0].max(rect[2]),
        rect[1].max(rect[3]),
    )
}

impl PageGeometry {
    /// The page's geometry in layout space, with pdfplumber's semantics: the
    /// crop box (or media box) as the page region, and the box axes swapped
    /// for 90 and 270 degree pages so crops and PDF-space conversions line up
    /// with the coordinates layout emits.
    pub fn for_page(page: &PDFPage) -> Self {
        let rotated = |b: Rect| -> Rect {
            match page.rotate.rem_euclid(360) {
                90 | 270 => (b.1, b.0, b.3, b.2),
                _ => b,
            }
        };
        let raw_mediabox = normalize_rect(page.mediabox.unwrap_or([0.0; 4]));
        let raw_page_bbox = page.cropbox.map_or(raw_mediabox, normalize_rect);
        let mediabox = rotated(raw_mediabox);
        let page_bbox = rotated(raw_page_bbox);
        Self {
            page_bbox,
            mediabox,
            initial_doctop: 0.0,
            force_crop: page_bbox != mediabox,
        }
    }

    /// This geometry restricted to a pdfplumber-space crop, clamped to the
    /// page. A degenerate, non-finite or out-of-bounds crop leaves the page
    /// uncropped, as pdfplumber's `_sanitize_bbox` does.
    pub fn cropped(self, crop: Rect) -> Self {
        if !(crop.0.is_finite() && crop.1.is_finite() && crop.2.is_finite() && crop.3.is_finite()) {
            return self;
        }
        let (px0, py0, px1, py1) = self.page_bbox;
        let x0 = crop.0.max(px0).min(px1);
        let y0 = crop.1.max(py0).min(py1);
        let x1 = crop.2.max(px0).min(px1);
        let y1 = crop.3.max(py0).min(py1);
        if x1 <= x0 || y1 <= y0 {
            return self;
        }
        let clamped = (x0, y0, x1, y1);
        if clamped == self.page_bbox {
            return self;
        }
        Self {
            page_bbox: clamped,
            force_crop: true,
            ..self
        }
    }
}

impl BBox {
    /// This box (pdfplumber space, y down from the top) in PDF space, with
    /// y0 <= y1.
    pub fn to_pdf_space(self, geometry: &PageGeometry) -> Rect {
        let page_top = geometry.mediabox.3;
        let raw_y0 = page_top - self.bottom;
        let raw_y1 = page_top - self.top;
        (self.x0, raw_y0.min(raw_y1), self.x1, raw_y0.max(raw_y1))
    }
}

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

#[cfg(test)]
mod page_geometry_tests {
    use super::*;
    use crate::pdftypes::PDFDict;

    fn page(mediabox: [f64; 4], cropbox: Option<[f64; 4]>, rotate: i64) -> PDFPage {
        PDFPage {
            pageid: 1,
            attrs: PDFDict::default(),
            label: None,
            mediabox: Some(mediabox),
            cropbox,
            bleedbox: None,
            trimbox: None,
            artbox: None,
            rotate,
            annots: None,
            resources: PDFDict::default(),
            contents: Vec::new(),
            user_unit: 1.0,
        }
    }

    #[test]
    fn page_geometry_uses_cropbox_and_mediabox() {
        let geometry = PageGeometry::for_page(&page(
            [0.0, 0.0, 200.0, 200.0],
            Some([50.0, 50.0, 150.0, 150.0]),
            0,
        ));
        assert_eq!(geometry.mediabox, (0.0, 0.0, 200.0, 200.0));
        assert_eq!(geometry.page_bbox, (50.0, 50.0, 150.0, 150.0));
        assert!(geometry.force_crop);
    }

    #[test]
    fn rotated_pages_swap_axes() {
        let geometry = PageGeometry::for_page(&page([0.0, 0.0, 200.0, 300.0], None, 90));
        assert_eq!(geometry.mediabox, (0.0, 0.0, 300.0, 200.0));
    }

    #[test]
    fn table_boxes_convert_to_pdf_space() {
        let geometry = PageGeometry {
            page_bbox: (50.0, 50.0, 150.0, 150.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let bbox = BBox {
            x0: 10.0,
            top: 20.0,
            x1: 40.0,
            bottom: 80.0,
        };
        assert_eq!(bbox.to_pdf_space(&geometry), (10.0, 120.0, 40.0, 180.0));
        let inverted = BBox {
            x0: 10.0,
            top: 120.0,
            x1: 20.0,
            bottom: 40.0,
        };
        let full = PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            ..geometry
        };
        assert_eq!(inverted.to_pdf_space(&full), (10.0, 80.0, 20.0, 160.0));
    }

    #[test]
    fn crops_clamp_and_fall_back_when_degenerate() {
        let geometry = PageGeometry::for_page(&page([0.0, 0.0, 100.0, 100.0], None, 0));
        let clamped = geometry.clone().cropped((-10.0, 10.0, 50.0, 200.0));
        assert_eq!(clamped.page_bbox, (0.0, 10.0, 50.0, 100.0));
        assert!(clamped.force_crop);
        assert_eq!(
            geometry.clone().cropped((50.0, 50.0, 10.0, 10.0)).page_bbox,
            geometry.page_bbox
        );
        assert_eq!(
            geometry
                .clone()
                .cropped((f64::NAN, 0.0, 1.0, 1.0))
                .page_bbox,
            geometry.page_bbox
        );
    }
}
