//! Edge detection, snapping, joining, and filtering for table extraction.
//!
//! This module processes raw edges extracted from PDF content into
//! clean, aligned edges suitable for table detection.

use std::collections::BTreeMap;

use ordered_float::OrderedFloat;

use crate::utils::Point;

use super::clustering::{bbox_from_words, bbox_overlap, cluster_objects, move_edge};
use super::types::{BBox, EdgeObj, Orientation, WordObj};

/// Clip an edge to a bounding box, returning None if no overlap.
pub fn clip_edge_to_bbox(edge: EdgeObj, crop: BBox) -> Option<EdgeObj> {
    let bbox = BBox {
        x0: edge.x0,
        top: edge.top,
        x1: edge.x1,
        bottom: edge.bottom,
    };
    let overlap = bbox_overlap(bbox, crop)?;
    Some(EdgeObj {
        x0: overlap.x0,
        x1: overlap.x1,
        top: overlap.top,
        bottom: overlap.bottom,
        width: overlap.width(),
        height: overlap.height(),
        orientation: edge.orientation,
        object_type: edge.object_type,
    })
}

/// Snap edges to align with nearby edges of the same orientation.
pub fn snap_edges(edges: &[EdgeObj], x_tolerance: f64, y_tolerance: f64) -> Vec<EdgeObj> {
    let mut v_edges: Vec<EdgeObj> = edges
        .iter()
        .filter(|e| e.orientation == Some(Orientation::Vertical))
        .cloned()
        .collect();
    let mut h_edges: Vec<EdgeObj> = edges
        .iter()
        .filter(|e| e.orientation == Some(Orientation::Horizontal))
        .cloned()
        .collect();

    if x_tolerance > 0.0 {
        let clusters = cluster_objects(&v_edges, |e| e.x0, x_tolerance, false);
        let mut snapped: Vec<EdgeObj> = Vec::new();
        for cluster in clusters {
            let avg = cluster.iter().map(|e| e.x0).sum::<f64>() / (cluster.len() as f64);
            for e in cluster {
                snapped.push(move_edge(&e, Orientation::Horizontal, avg - e.x0));
            }
        }
        v_edges = snapped;
    }

    if y_tolerance > 0.0 {
        let clusters = cluster_objects(&h_edges, |e| e.top, y_tolerance, false);
        let mut snapped: Vec<EdgeObj> = Vec::new();
        for cluster in clusters {
            let avg = cluster.iter().map(|e| e.top).sum::<f64>() / (cluster.len() as f64);
            for e in cluster {
                snapped.push(move_edge(&e, Orientation::Vertical, avg - e.top));
            }
        }
        h_edges = snapped;
    }

    v_edges.into_iter().chain(h_edges).collect()
}

/// Join collinear edges that are within tolerance of each other.
pub fn join_edge_group(
    edges: &[EdgeObj],
    orientation: Orientation,
    tolerance: f64,
) -> Vec<EdgeObj> {
    let mut sorted = edges.to_vec();
    sorted.sort_by(|a, b| {
        let a_min = if orientation == Orientation::Horizontal {
            a.x0
        } else {
            a.top
        };
        let b_min = if orientation == Orientation::Horizontal {
            b.x0
        } else {
            b.top
        };
        a_min
            .partial_cmp(&b_min)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut joined: Vec<EdgeObj> = Vec::new();
    if sorted.is_empty() {
        return joined;
    }
    joined.push(sorted[0].clone());
    for e in sorted.into_iter().skip(1) {
        let last = joined.last_mut().unwrap();
        let e_min = if orientation == Orientation::Horizontal {
            e.x0
        } else {
            e.top
        };
        let e_max = if orientation == Orientation::Horizontal {
            e.x1
        } else {
            e.bottom
        };
        let last_max = if orientation == Orientation::Horizontal {
            last.x1
        } else {
            last.bottom
        };
        if e_min <= last_max + tolerance {
            if e_max > last_max {
                if orientation == Orientation::Horizontal {
                    last.x1 = e.x1;
                    last.width = last.x1 - last.x0;
                } else {
                    last.bottom = e.bottom;
                    last.height = last.bottom - last.top;
                }
            }
        } else {
            joined.push(e);
        }
    }
    joined
}

/// Merge edges by snapping and joining.
pub fn merge_edges(
    edges: Vec<EdgeObj>,
    snap_x_tolerance: f64,
    snap_y_tolerance: f64,
    join_x_tolerance: f64,
    join_y_tolerance: f64,
) -> Vec<EdgeObj> {
    let mut edges = edges;
    if snap_x_tolerance > 0.0 || snap_y_tolerance > 0.0 {
        edges = snap_edges(&edges, snap_x_tolerance, snap_y_tolerance);
    }

    // Group by orientation and position (match pdfplumber exact grouping)
    let mut grouped: BTreeMap<(Orientation, OrderedFloat<f64>), Vec<EdgeObj>> = BTreeMap::new();
    for e in &edges {
        let orientation = match e.orientation {
            Some(o) => o,
            None => continue,
        };
        let key_val = match orientation {
            Orientation::Horizontal => e.top,
            Orientation::Vertical => e.x0,
        };
        let key = (orientation, OrderedFloat(key_val));
        grouped.entry(key).or_default().push(e.clone());
    }

    let mut merged: Vec<EdgeObj> = Vec::new();
    for ((orientation, _), group) in grouped {
        let tol = if orientation == Orientation::Horizontal {
            join_x_tolerance
        } else {
            join_y_tolerance
        };
        merged.extend(join_edge_group(&group, orientation, tol));
    }

    merged
}

/// Filter edges by orientation, type, and minimum length.
pub fn filter_edges(
    edges: Vec<EdgeObj>,
    orientation: Option<Orientation>,
    edge_type: Option<&str>,
    min_length: f64,
) -> Vec<EdgeObj> {
    edges
        .into_iter()
        .filter(|e| {
            let dim = if e.orientation == Some(Orientation::Vertical) {
                e.height
            } else {
                e.width
            };
            let et_ok = match edge_type {
                Some(t) => e.object_type == t,
                None => true,
            };
            let orient_ok = match orientation {
                Some(o) => e.orientation == Some(o),
                None => true,
            };
            et_ok && orient_ok && dim >= min_length
        })
        .collect()
}

/// Convert a line object to an edge with orientation.
pub fn line_to_edge(line: &EdgeObj) -> EdgeObj {
    let orientation = if (line.top - line.bottom).abs() < f64::EPSILON {
        Some(Orientation::Horizontal)
    } else {
        Some(Orientation::Vertical)
    };
    EdgeObj {
        orientation,
        ..line.clone()
    }
}

/// Convert a rectangle to four edges.
pub fn rect_to_edges(rect: BBox) -> Vec<EdgeObj> {
    let top = EdgeObj {
        x0: rect.x0,
        x1: rect.x1,
        top: rect.top,
        bottom: rect.top,
        width: rect.x1 - rect.x0,
        height: 0.0,
        orientation: Some(Orientation::Horizontal),
        object_type: "rect_edge",
    };
    let bottom = EdgeObj {
        x0: rect.x0,
        x1: rect.x1,
        top: rect.bottom,
        bottom: rect.bottom,
        width: rect.x1 - rect.x0,
        height: 0.0,
        orientation: Some(Orientation::Horizontal),
        object_type: "rect_edge",
    };
    let left = EdgeObj {
        x0: rect.x0,
        x1: rect.x0,
        top: rect.top,
        bottom: rect.bottom,
        width: 0.0,
        height: rect.bottom - rect.top,
        orientation: Some(Orientation::Vertical),
        object_type: "rect_edge",
    };
    let right = EdgeObj {
        x0: rect.x1,
        x1: rect.x1,
        top: rect.top,
        bottom: rect.bottom,
        width: 0.0,
        height: rect.bottom - rect.top,
        orientation: Some(Orientation::Vertical),
        object_type: "rect_edge",
    };
    vec![top, bottom, left, right]
}

/// Convert a curve (series of points) to edges.
pub fn curve_to_edges(points: &[Point], object_type: &'static str) -> Vec<EdgeObj> {
    let mut edges = Vec::new();
    for pair in points.windows(2) {
        let p0 = pair[0];
        let p1 = pair[1];
        let x0 = p0.0.min(p1.0);
        let x1 = p0.0.max(p1.0);
        let top = p0.1.min(p1.1);
        let bottom = p0.1.max(p1.1);
        let orientation = if (p0.0 - p1.0).abs() < f64::EPSILON {
            Some(Orientation::Vertical)
        } else if (p0.1 - p1.1).abs() < f64::EPSILON {
            Some(Orientation::Horizontal)
        } else {
            None
        };
        edges.push(EdgeObj {
            x0,
            x1,
            top,
            bottom,
            width: (x1 - x0).abs(),
            height: (bottom - top).abs(),
            orientation,
            object_type,
        });
    }
    edges
}

/// Generate horizontal edges from word clusters.
pub fn words_to_edges_h(words: &[WordObj], word_threshold: usize) -> Vec<EdgeObj> {
    let clusters = cluster_objects(words, |w| w.top, 1.0, false);
    let large_clusters = clusters
        .into_iter()
        .filter(|c| c.len() >= word_threshold)
        .collect::<Vec<_>>();
    let mut rects: Vec<BBox> = large_clusters.iter().map(|c| bbox_from_words(c)).collect();
    if rects.is_empty() {
        return Vec::new();
    }
    let min_x0 = rects.iter().map(|r| r.x0).fold(f64::INFINITY, f64::min);
    let max_x1 = rects.iter().map(|r| r.x1).fold(f64::NEG_INFINITY, f64::max);

    let mut edges = Vec::new();
    for r in rects.drain(..) {
        edges.push(EdgeObj {
            x0: min_x0,
            x1: max_x1,
            top: r.top,
            bottom: r.top,
            width: max_x1 - min_x0,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "word_edge",
        });
        edges.push(EdgeObj {
            x0: min_x0,
            x1: max_x1,
            top: r.bottom,
            bottom: r.bottom,
            width: max_x1 - min_x0,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "word_edge",
        });
    }
    edges
}

/// Generate vertical edges from word clusters.
pub fn words_to_edges_v(words: &[WordObj], word_threshold: usize) -> Vec<EdgeObj> {
    let by_x0 = cluster_objects(words, |w| w.x0, 1.0, false);
    let by_x1 = cluster_objects(words, |w| w.x1, 1.0, false);
    let by_center = cluster_objects(words, |w| (w.x0 + w.x1) / 2.0, 1.0, false);

    let mut clusters = Vec::new();
    clusters.extend(by_x0);
    clusters.extend(by_x1);
    clusters.extend(by_center);

    clusters.sort_by(|a, b| b.len().cmp(&a.len()));
    let large_clusters: Vec<Vec<WordObj>> = clusters
        .into_iter()
        .filter(|c| c.len() >= word_threshold)
        .collect();

    let bboxes: Vec<BBox> = large_clusters.iter().map(|c| bbox_from_words(c)).collect();

    let mut condensed: Vec<BBox> = Vec::new();
    'outer: for bbox in bboxes {
        for c in &condensed {
            if bbox_overlap(bbox, *c).is_some() {
                continue 'outer;
            }
        }
        condensed.push(bbox);
    }

    if condensed.is_empty() {
        return Vec::new();
    }

    condensed.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal));

    let max_x1 = condensed
        .iter()
        .map(|r| r.x1)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_top = condensed
        .iter()
        .map(|r| r.top)
        .fold(f64::INFINITY, f64::min);
    let max_bottom = condensed
        .iter()
        .map(|r| r.bottom)
        .fold(f64::NEG_INFINITY, f64::max);

    let mut edges = Vec::new();
    for r in condensed {
        edges.push(EdgeObj {
            x0: r.x0,
            x1: r.x0,
            top: min_top,
            bottom: max_bottom,
            width: 0.0,
            height: max_bottom - min_top,
            orientation: Some(Orientation::Vertical),
            object_type: "word_edge",
        });
    }
    edges.push(EdgeObj {
        x0: max_x1,
        x1: max_x1,
        top: min_top,
        bottom: max_bottom,
        width: 0.0,
        height: max_bottom - min_top,
        orientation: Some(Orientation::Vertical),
        object_type: "word_edge",
    });
    edges
}
