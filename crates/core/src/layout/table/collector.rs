//! Arena-backed collectors for table extraction.

use crate::arena::types::{ArenaItem, ArenaPage};
use crate::utils::Rect;

use super::clustering::bbox_overlap;
use super::edges::{clip_edge_to_bbox, curve_to_edges, rect_to_edges};
use super::geometry::to_top_left_bbox;
use super::types::{BBox, CharObj, EdgeObj, Orientation, PageGeometry};

/// Check if two rectangles are equal within epsilon.
fn rects_equal(a: Rect, b: Rect) -> bool {
    const EPS: f64 = 1e-6;
    (a.0 - b.0).abs() < EPS
        && (a.1 - b.1).abs() < EPS
        && (a.2 - b.2).abs() < EPS
        && (a.3 - b.3).abs() < EPS
}

fn arena_char_to_charobj(
    ch: &crate::arena::types::ArenaChar,
    geom: &PageGeometry,
    crop_bbox: Option<BBox>,
) -> Option<CharObj> {
    let bbox = to_top_left_bbox(ch.bbox.0, ch.bbox.1, ch.bbox.2, ch.bbox.3, geom);
    let bbox = if let Some(crop) = crop_bbox {
        bbox_overlap(bbox, crop)?
    } else {
        bbox
    };
    Some(CharObj {
        text: ch.text,
        x0: bbox.x0,
        x1: bbox.x1,
        top: bbox.top,
        bottom: bbox.bottom,
        doctop: geom.initial_doctop + bbox.top,
        width: bbox.width(),
        height: bbox.height(),
        size: ch.size,
        upright: ch.upright,
    })
}

pub(crate) fn collect_table_objects_from_arena(
    page: &ArenaPage,
    geom: &PageGeometry,
) -> (Vec<CharObj>, Vec<EdgeObj>) {
    let mut chars: Vec<CharObj> = Vec::new();
    let mut edges: Vec<EdgeObj> = Vec::new();

    fn visit_item(
        item: &ArenaItem,
        geom: &PageGeometry,
        crop_bbox: Option<BBox>,
        chars: &mut Vec<CharObj>,
        edges: &mut Vec<EdgeObj>,
    ) {
        match item {
            ArenaItem::Char(c) => {
                if let Some(obj) = arena_char_to_charobj(c, geom, crop_bbox) {
                    chars.push(obj);
                }
            }
            ArenaItem::Line(l) => {
                let bbox = to_top_left_bbox(l.p0.0, l.p0.1, l.p1.0, l.p1.1, geom);
                let edge = EdgeObj {
                    x0: bbox.x0,
                    x1: bbox.x1,
                    top: bbox.top,
                    bottom: bbox.bottom,
                    width: bbox.width(),
                    height: bbox.height(),
                    orientation: if bbox.top == bbox.bottom {
                        Some(Orientation::Horizontal)
                    } else {
                        Some(Orientation::Vertical)
                    },
                    object_type: "line",
                };
                if let Some(crop) = crop_bbox {
                    if let Some(edge) = clip_edge_to_bbox(edge, crop) {
                        edges.push(edge);
                    }
                } else {
                    edges.push(edge);
                }
            }
            ArenaItem::Rect(r) => {
                let bbox = to_top_left_bbox(r.bbox.0, r.bbox.1, r.bbox.2, r.bbox.3, geom);
                for edge in rect_to_edges(bbox) {
                    if let Some(crop) = crop_bbox {
                        if let Some(edge) = clip_edge_to_bbox(edge, crop) {
                            edges.push(edge);
                        }
                    } else {
                        edges.push(edge);
                    }
                }
            }
            ArenaItem::Curve(c) => {
                let mut pts = Vec::new();
                for p in &c.pts {
                    let tl = to_top_left_bbox(p.0, p.1, p.0, p.1, geom);
                    pts.push((tl.x0, tl.top));
                }
                for edge in curve_to_edges(&pts, "curve_edge") {
                    if let Some(crop) = crop_bbox {
                        if let Some(edge) = clip_edge_to_bbox(edge, crop) {
                            edges.push(edge);
                        }
                    } else {
                        edges.push(edge);
                    }
                }
            }
            ArenaItem::Figure(fig) => {
                for child in fig.items.iter() {
                    visit_item(child, geom, crop_bbox, chars, edges);
                }
            }
            _ => {}
        }
    }

    let crop_bbox = if rects_equal(geom.page_bbox, geom.mediabox) {
        None
    } else {
        Some(BBox {
            x0: geom.page_bbox.0,
            top: geom.page_bbox.1,
            x1: geom.page_bbox.2,
            bottom: geom.page_bbox.3,
        })
    };

    for item in page.items.iter() {
        visit_item(item, geom, crop_bbox, &mut chars, &mut edges);
    }

    (chars, edges)
}
