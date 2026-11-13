//! Page geometry for table extraction: crops, rotation, and the conversion
//! from pdfplumber space (top-down) to PDF space (bottom-up).

use std::sync::Arc;

use bolivar_core::extract::ExtractOptions as CoreOptions;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::table::{BBox as TableBBox, PageGeometry, TableSettings};

use crate::abi::FfiResult;
use crate::options::Tables;

pub type Rect = (f64, f64, f64, f64);

fn normalize_rect(rect: [f64; 4]) -> Rect {
    (
        rect[0].min(rect[2]),
        rect[1].min(rect[3]),
        rect[0].max(rect[2]),
        rect[1].max(rect[3]),
    )
}

/// The page's geometry in layout space (post-rotation, pdfplumber
/// semantics): 90 and 270 degree pages swap their box axes so crops and
/// PDF-space conversions line up with the coordinates layout emits.
pub fn page_geometry(page: &PDFPage) -> PageGeometry {
    let rotated = |b: Rect| -> Rect {
        match page.rotate.rem_euclid(360) {
            90 | 270 => (b.1, b.0, b.3, b.2),
            _ => b,
        }
    };
    let raw_mediabox = normalize_rect(page.mediabox.unwrap_or([0.0; 4]));
    let raw_page_bbox = normalize_rect(page.cropbox.unwrap_or([
        raw_mediabox.0,
        raw_mediabox.1,
        raw_mediabox.2,
        raw_mediabox.3,
    ]));
    let mediabox = rotated(raw_mediabox);
    let page_bbox = rotated(raw_page_bbox);
    PageGeometry {
        page_bbox,
        mediabox,
        initial_doctop: 0.0,
        force_crop: page_bbox != mediabox,
    }
}

/// A table box (pdfplumber space) in PDF space, with y0 <= y1.
pub fn table_bbox_in_pdf_space(bbox: TableBBox, geometry: &PageGeometry) -> Rect {
    let page_top = geometry.mediabox.3;
    let raw_y0 = page_top - bbox.bottom;
    let raw_y1 = page_top - bbox.top;
    let (y0, y1) = if raw_y0 <= raw_y1 {
        (raw_y0, raw_y1)
    } else {
        (raw_y1, raw_y0)
    };
    (bbox.x0, y0, bbox.x1, y1)
}

/// Clamp a pdfplumber-space crop to the page region; degenerate or
/// out-of-bounds crops fall back to the uncropped page (mirrors the
/// decision-layer `_sanitize_bbox` the Python binding relies on).
fn cropped_geometry(geometry: PageGeometry, crop: Rect) -> PageGeometry {
    if !(crop.0.is_finite() && crop.1.is_finite() && crop.2.is_finite() && crop.3.is_finite()) {
        return geometry;
    }
    let (px0, py0, px1, py1) = geometry.page_bbox;
    let x0 = crop.0.max(px0).min(px1);
    let y0 = crop.1.max(py0).min(py1);
    let x1 = crop.2.max(px0).min(px1);
    let y1 = crop.3.max(py0).min(py1);
    if x1 <= x0 || y1 <= y0 {
        return geometry;
    }
    let clamped = (x0, y0, x1, y1);
    if clamped == geometry.page_bbox {
        return geometry;
    }
    PageGeometry {
        page_bbox: clamped,
        force_crop: true,
        ..geometry
    }
}

/// One selected page of a table extraction.
#[derive(Debug, Clone)]
pub struct TablePage {
    pub page_index: usize,
    pub geometry: PageGeometry,
}

/// What a table stream needs: options with the table page limit applied,
/// settings, and each selected page with its (cropped) geometry.
pub struct TablePlan {
    pub options: CoreOptions,
    pub settings: TableSettings,
    pub pages: Vec<TablePage>,
}

pub fn table_plan(
    doc: &Arc<PDFDocument>,
    mut options: CoreOptions,
    tables: Tables,
) -> FfiResult<TablePlan> {
    if let Some(max_pages) = tables.max_pages {
        options.maxpages = max_pages;
    }
    let selected = bolivar_core::engine::select_pages(
        doc.page_tree_len(),
        options.page_numbers.clone(),
        options.maxpages,
    );
    let mut pages = Vec::with_capacity(selected.len());
    for page_index in selected {
        let pdf_page = doc.get_page_cached(page_index)?;
        let geometry = page_geometry(pdf_page.as_ref());
        let crop = if page_index == 0 {
            tables.first_page_crop.or(tables.crop)
        } else {
            tables.crop
        };
        pages.push(TablePage {
            page_index,
            geometry: match crop {
                Some(crop) => cropped_geometry(geometry, crop),
                None => geometry,
            },
        });
    }
    Ok(TablePlan {
        options,
        settings: tables.settings,
        pages,
    })
}

impl TablePlan {
    pub fn geometries(&self) -> Vec<PageGeometry> {
        self.pages
            .iter()
            .map(|page| page.geometry.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bolivar_core::pdftypes::PDFDict;

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
        let geometry = page_geometry(&page(
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
        let geometry = page_geometry(&page([0.0, 0.0, 200.0, 300.0], None, 90));
        assert_eq!(geometry.mediabox, (0.0, 0.0, 300.0, 200.0));
    }

    #[test]
    fn table_bbox_conversion_uses_layout_coordinate_convention() {
        let geometry = PageGeometry {
            page_bbox: (50.0, 50.0, 150.0, 150.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let bbox = TableBBox {
            x0: 10.0,
            top: 20.0,
            x1: 40.0,
            bottom: 80.0,
        };
        assert_eq!(
            table_bbox_in_pdf_space(bbox, &geometry),
            (10.0, 120.0, 40.0, 180.0)
        );
    }

    #[test]
    fn table_bbox_conversion_normalizes_inverted_vertical_bounds() {
        let geometry = PageGeometry {
            page_bbox: (0.0, 0.0, 200.0, 200.0),
            mediabox: (0.0, 0.0, 200.0, 200.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let bbox = TableBBox {
            x0: 10.0,
            top: 120.0,
            x1: 20.0,
            bottom: 40.0,
        };
        assert_eq!(
            table_bbox_in_pdf_space(bbox, &geometry),
            (10.0, 80.0, 20.0, 160.0)
        );
    }

    #[test]
    fn crops_clamp_and_fall_back_when_degenerate() {
        let geometry = page_geometry(&page([0.0, 0.0, 100.0, 100.0], None, 0));
        let clamped = cropped_geometry(geometry.clone(), (-10.0, 10.0, 50.0, 200.0));
        assert_eq!(clamped.page_bbox, (0.0, 10.0, 50.0, 100.0));
        assert!(clamped.force_crop);
        assert_eq!(
            cropped_geometry(geometry.clone(), (50.0, 50.0, 10.0, 10.0)).page_bbox,
            geometry.page_bbox
        );
        assert_eq!(
            cropped_geometry(geometry.clone(), (f64::NAN, 0.0, 1.0, 1.0)).page_bbox,
            geometry.page_bbox
        );
    }
}
