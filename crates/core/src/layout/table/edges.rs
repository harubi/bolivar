//! Edge detection, snapping, joining, and filtering for table extraction.
//!
//! This module processes raw edges extracted from PDF content into
//! clean, aligned edges suitable for table detection.

use std::cmp::Reverse;

use ordered_float::OrderedFloat;

use crate::cancellation::CancellationToken;
use crate::error::Result;
use crate::utils::Point;

use super::clustering::{bbox_from_words, bbox_overlap, cluster_objects_cancellable};
use super::types::{BBox, EdgeObj, Orientation, WordObj};

const CANCEL_INTERVAL: usize = 256;

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

fn edge_position(edge: &EdgeObj) -> f64 {
    match edge.orientation {
        Some(Orientation::Horizontal) => edge.top,
        Some(Orientation::Vertical) => edge.x0,
        None => 0.0,
    }
}

fn edge_start(edge: &EdgeObj) -> f64 {
    match edge.orientation {
        Some(Orientation::Horizontal) => edge.x0,
        Some(Orientation::Vertical) => edge.top,
        None => 0.0,
    }
}

fn snap_edges(
    edges: &mut [EdgeObj],
    x_tolerance: f64,
    y_tolerance: f64,
    cancellation: &CancellationToken,
) -> Result<()> {
    edges.sort_by_key(|edge| {
        (
            edge.orientation.expect("oriented edge"),
            OrderedFloat(edge_position(edge)),
        )
    });
    cancellation.check()?;

    let mut start = 0usize;
    while start < edges.len() {
        if start.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        let orientation = edges[start].orientation.expect("oriented edge");
        let tolerance = match orientation {
            Orientation::Horizontal => y_tolerance,
            Orientation::Vertical => x_tolerance,
        };
        if tolerance <= 0.0 {
            start += 1;
            continue;
        }

        let mut end = start + 1;
        let mut last = edge_position(&edges[start]);
        while end < edges.len() && edges[end].orientation == Some(orientation) {
            let position = edge_position(&edges[end]);
            if position > last + tolerance {
                break;
            }
            last = position;
            end += 1;
        }

        let mut position_sum = 0.0;
        for (index, edge) in edges[start..end].iter().enumerate() {
            if index.is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            position_sum += edge_position(edge);
        }
        let average = position_sum / ((end - start) as f64);
        for (index, edge) in edges[start..end].iter_mut().enumerate() {
            if index.is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            match orientation {
                Orientation::Horizontal => {
                    let offset = average - edge.top;
                    edge.top += offset;
                    edge.bottom += offset;
                }
                Orientation::Vertical => {
                    let offset = average - edge.x0;
                    edge.x0 += offset;
                    edge.x1 += offset;
                }
            }
        }
        start = end;
    }
    Ok(())
}

fn join_edge(
    previous: &mut EdgeObj,
    edge: &EdgeObj,
    join_x_tolerance: f64,
    join_y_tolerance: f64,
) -> bool {
    let orientation = edge.orientation.expect("oriented edge");
    if previous.orientation != Some(orientation) || edge_position(previous) != edge_position(edge) {
        return false;
    }

    let tolerance = match orientation {
        Orientation::Horizontal => join_x_tolerance,
        Orientation::Vertical => join_y_tolerance,
    };
    let previous_end = match orientation {
        Orientation::Horizontal => previous.x1,
        Orientation::Vertical => previous.bottom,
    };
    if edge_start(edge) > previous_end + tolerance {
        return false;
    }

    match orientation {
        Orientation::Horizontal if edge.x1 > previous.x1 => {
            previous.x1 = edge.x1;
            previous.width = previous.x1 - previous.x0;
        }
        Orientation::Vertical if edge.bottom > previous.bottom => {
            previous.bottom = edge.bottom;
            previous.height = previous.bottom - previous.top;
        }
        _ => {}
    }
    true
}

/// Merge edges by snapping and joining.
#[cfg(test)]
pub fn merge_edges(
    edges: Vec<EdgeObj>,
    snap_x_tolerance: f64,
    snap_y_tolerance: f64,
    join_x_tolerance: f64,
    join_y_tolerance: f64,
) -> Vec<EdgeObj> {
    merge_edges_cancellable(
        edges,
        snap_x_tolerance,
        snap_y_tolerance,
        join_x_tolerance,
        join_y_tolerance,
        &CancellationToken::new(),
    )
    .expect("a new cancellation token cannot be cancelled")
}

pub(crate) fn merge_edges_cancellable(
    edges: Vec<EdgeObj>,
    snap_x_tolerance: f64,
    snap_y_tolerance: f64,
    join_x_tolerance: f64,
    join_y_tolerance: f64,
    cancellation: &CancellationToken,
) -> Result<Vec<EdgeObj>> {
    cancellation.check()?;
    let mut edges = edges;
    let mut kept = 0usize;
    for read in 0..edges.len() {
        if read.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        if edges[read].orientation.is_none() {
            continue;
        }
        if kept != read {
            edges.swap(kept, read);
        }
        kept += 1;
    }
    edges.truncate(kept);

    if snap_x_tolerance > 0.0 || snap_y_tolerance > 0.0 {
        snap_edges(&mut edges, snap_x_tolerance, snap_y_tolerance, cancellation)?;
    }

    edges.sort_by_key(|edge| {
        (
            edge.orientation.expect("oriented edge"),
            OrderedFloat(edge_position(edge)),
            OrderedFloat(edge_start(edge)),
        )
    });
    cancellation.check()?;

    if !edges.is_empty() {
        let mut write = 0usize;
        for read in 1..edges.len() {
            if read.is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            let joined = {
                let (previous, current) = edges.split_at_mut(read);
                join_edge(
                    &mut previous[write],
                    &current[0],
                    join_x_tolerance,
                    join_y_tolerance,
                )
            };
            if joined {
                continue;
            }

            write += 1;
            if write != read {
                edges.swap(write, read);
            }
        }
        edges.truncate(write + 1);
    }

    cancellation.check()?;
    Ok(edges)
}

/// Convert a rectangle to four edges.
pub fn rect_to_edges(rect: BBox) -> [EdgeObj; 4] {
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
    [top, bottom, left, right]
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

pub(super) fn words_to_edges_h_cancellable(
    words: &[WordObj],
    word_threshold: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<EdgeObj>> {
    let clusters = cluster_objects_cancellable(words, |word| word.top, 1.0, false, cancellation)?;
    let mut rects = Vec::new();
    for (index, cluster) in clusters.into_iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        if cluster.len() >= word_threshold {
            rects.push(bbox_from_words(&cluster));
        }
    }
    if rects.is_empty() {
        return Ok(Vec::new());
    }

    let mut min_x0 = f64::INFINITY;
    let mut max_x1 = f64::NEG_INFINITY;
    for (index, rect) in rects.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        min_x0 = min_x0.min(rect.x0);
        max_x1 = max_x1.max(rect.x1);
    }

    let mut edges = Vec::with_capacity(rects.len() * 2);
    for (index, rect) in rects.into_iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        edges.push(EdgeObj {
            x0: min_x0,
            x1: max_x1,
            top: rect.top,
            bottom: rect.top,
            width: max_x1 - min_x0,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "word_edge",
        });
        edges.push(EdgeObj {
            x0: min_x0,
            x1: max_x1,
            top: rect.bottom,
            bottom: rect.bottom,
            width: max_x1 - min_x0,
            height: 0.0,
            orientation: Some(Orientation::Horizontal),
            object_type: "word_edge",
        });
    }
    Ok(edges)
}

pub(super) fn words_to_edges_v_cancellable(
    words: &[WordObj],
    word_threshold: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<EdgeObj>> {
    let by_x0 = cluster_objects_cancellable(words, |word| word.x0, 1.0, false, cancellation)?;
    let by_x1 = cluster_objects_cancellable(words, |word| word.x1, 1.0, false, cancellation)?;
    let by_center = cluster_objects_cancellable(
        words,
        |word| (word.x0 + word.x1) / 2.0,
        1.0,
        false,
        cancellation,
    )?;

    let mut clusters = Vec::new();
    clusters.extend(by_x0);
    clusters.extend(by_x1);
    clusters.extend(by_center);

    clusters.sort_by_key(|c| Reverse(c.len()));
    cancellation.check()?;
    let mut bboxes = Vec::new();
    for (index, cluster) in clusters.into_iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        if cluster.len() >= word_threshold {
            bboxes.push(bbox_from_words(&cluster));
        }
    }

    let mut condensed: Vec<BBox> = Vec::new();
    'outer: for (bbox_index, bbox) in bboxes.into_iter().enumerate() {
        if bbox_index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        for (index, existing) in condensed.iter().enumerate() {
            if index.is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            if bbox_overlap(bbox, *existing).is_some() {
                continue 'outer;
            }
        }
        condensed.push(bbox);
    }

    if condensed.is_empty() {
        return Ok(Vec::new());
    }

    condensed.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal));
    cancellation.check()?;

    let mut max_x1 = f64::NEG_INFINITY;
    let mut min_top = f64::INFINITY;
    let mut max_bottom = f64::NEG_INFINITY;
    for (index, rect) in condensed.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        max_x1 = max_x1.max(rect.x1);
        min_top = min_top.min(rect.top);
        max_bottom = max_bottom.max(rect.bottom);
    }

    let mut edges = Vec::with_capacity(condensed.len() + 1);
    for (index, rect) in condensed.into_iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        edges.push(EdgeObj {
            x0: rect.x0,
            x1: rect.x0,
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
    Ok(edges)
}

#[cfg(test)]
mod tests {
    use super::{merge_edges, merge_edges_cancellable};
    use crate::cancellation::CancellationToken;
    use crate::error::PdfError;
    use crate::layout::table::{EdgeObj, Orientation};

    fn edge(
        orientation: Orientation,
        object_type: &'static str,
        width: f64,
        height: f64,
    ) -> EdgeObj {
        EdgeObj {
            x0: 0.0,
            x1: width,
            top: 0.0,
            bottom: height,
            width,
            height,
            orientation: Some(orientation),
            object_type,
        }
    }

    #[test]
    fn merge_edges_honors_cancellation() {
        let token = CancellationToken::new();
        token.cancel();

        let result = merge_edges_cancellable(Vec::new(), 3.0, 3.0, 3.0, 3.0, &token);

        assert!(matches!(result, Err(PdfError::Cancelled)));
    }

    #[test]
    fn merge_edges_snaps_then_joins_in_order() {
        let edges = vec![
            EdgeObj {
                x0: 11.0,
                x1: 11.0,
                top: 8.0,
                bottom: 15.0,
                width: 0.0,
                height: 7.0,
                orientation: Some(Orientation::Vertical),
                object_type: "line",
            },
            EdgeObj {
                x0: 10.0,
                x1: 10.0,
                top: 0.0,
                bottom: 10.0,
                width: 0.0,
                height: 10.0,
                orientation: Some(Orientation::Vertical),
                object_type: "line",
            },
        ];

        let merged = merge_edges(edges, 3.0, 3.0, 3.0, 3.0);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].x0, 10.5);
        assert_eq!((merged[0].top, merged[0].bottom), (0.0, 15.0));
    }

    #[test]
    fn merge_edges_uses_chained_snap_clusters() {
        let edges = vec![
            edge(Orientation::Vertical, "line", 0.0, 2.0),
            EdgeObj {
                x0: 2.0,
                x1: 2.0,
                top: 4.0,
                bottom: 6.0,
                width: 0.0,
                height: 2.0,
                orientation: Some(Orientation::Vertical),
                object_type: "line",
            },
            EdgeObj {
                x0: 4.0,
                x1: 4.0,
                top: 8.0,
                bottom: 10.0,
                width: 0.0,
                height: 2.0,
                orientation: Some(Orientation::Vertical),
                object_type: "line",
            },
        ];

        let merged = merge_edges(edges, 2.0, 0.0, 0.0, 0.0);

        assert!(merged.iter().all(|item| item.x0 == 2.0));
    }
}
