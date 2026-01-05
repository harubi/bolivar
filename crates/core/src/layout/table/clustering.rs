//! Spatial clustering algorithms for table extraction.
//!
//! These functions group objects (edges, characters, words) based on
//! spatial proximity using tolerance-based clustering.

use std::collections::HashMap;

use super::types::{BBox, CharObj, EdgeObj, KeyF64, Orientation, WordObj, key_f64};

/// Cluster a list of f64 values based on tolerance.
pub fn cluster_list(mut xs: Vec<f64>, tolerance: f64) -> Vec<Vec<f64>> {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if tolerance == 0.0 || xs.len() < 2 {
        return xs.into_iter().map(|x| vec![x]).collect();
    }
    let mut groups: Vec<Vec<f64>> = Vec::new();
    let mut current: Vec<f64> = Vec::new();
    let mut last = xs[0];
    current.push(xs[0]);
    for x in xs.into_iter().skip(1) {
        if x <= last + tolerance {
            current.push(x);
        } else {
            groups.push(current);
            current = vec![x];
        }
        last = x;
    }
    groups.push(current);
    groups
}

/// Create a mapping from values to their cluster indices.
pub fn make_cluster_dict(values: Vec<f64>, tolerance: f64) -> HashMap<KeyF64, usize> {
    let mut unique: Vec<f64> = values;
    unique.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    unique.dedup_by(|a, b| (*a - *b).abs() == 0.0);
    let clusters = cluster_list(unique, tolerance);
    let mut dict = HashMap::new();
    for (i, cluster) in clusters.into_iter().enumerate() {
        for val in cluster {
            dict.insert(key_f64(val), i);
        }
    }
    dict
}

/// Cluster objects based on a key function and tolerance.
pub fn cluster_objects<T: Clone, F: Fn(&T) -> f64>(
    xs: &[T],
    key_fn: F,
    tolerance: f64,
    preserve_order: bool,
) -> Vec<Vec<T>> {
    let values: Vec<f64> = xs.iter().map(&key_fn).collect();
    let cluster_dict = make_cluster_dict(values, tolerance);

    let mut cluster_tuples: Vec<(T, usize)> = if preserve_order {
        xs.iter()
            .map(|x| {
                (
                    x.clone(),
                    *cluster_dict.get(&key_f64(key_fn(x))).unwrap_or(&0),
                )
            })
            .collect()
    } else {
        let mut tuples: Vec<(T, usize)> = xs
            .iter()
            .map(|x| {
                (
                    x.clone(),
                    *cluster_dict.get(&key_f64(key_fn(x))).unwrap_or(&0),
                )
            })
            .collect();
        tuples.sort_by(|a, b| a.1.cmp(&b.1));
        tuples
    };

    let mut groups: Vec<Vec<T>> = Vec::new();
    let mut current: Vec<T> = Vec::new();
    let mut last_idx: Option<usize> = None;
    for (item, idx) in cluster_tuples.drain(..) {
        if last_idx.is_none() || last_idx.unwrap() == idx {
            current.push(item);
        } else {
            groups.push(current);
            current = vec![item];
        }
        last_idx = Some(idx);
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// Move an edge along an axis by a given value.
pub fn move_edge(edge: &EdgeObj, axis: Orientation, value: f64) -> EdgeObj {
    match axis {
        Orientation::Horizontal => EdgeObj {
            x0: edge.x0 + value,
            x1: edge.x1 + value,
            ..edge.clone()
        },
        Orientation::Vertical => EdgeObj {
            top: edge.top + value,
            bottom: edge.bottom + value,
            ..edge.clone()
        },
    }
}

/// Compute a bounding box from a slice of character references.
pub fn bbox_from_chars(chars: &[&CharObj]) -> BBox {
    let mut x0 = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for c in chars {
        x0 = x0.min(c.x0);
        top = top.min(c.top);
        x1 = x1.max(c.x1);
        bottom = bottom.max(c.bottom);
    }
    BBox {
        x0,
        top,
        x1,
        bottom,
    }
}

/// Compute a bounding box from a slice of words.
pub fn bbox_from_words(words: &[WordObj]) -> BBox {
    let mut x0 = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for w in words {
        x0 = x0.min(w.x0);
        top = top.min(w.top);
        x1 = x1.max(w.x1);
        bottom = bottom.max(w.bottom);
    }
    BBox {
        x0,
        top,
        x1,
        bottom,
    }
}

/// Compute the overlap between two bounding boxes.
pub fn bbox_overlap(a: BBox, b: BBox) -> Option<BBox> {
    let o_left = a.x0.max(b.x0);
    let o_right = a.x1.min(b.x1);
    let o_top = a.top.max(b.top);
    let o_bottom = a.bottom.min(b.bottom);
    let o_width = o_right - o_left;
    let o_height = o_bottom - o_top;
    if o_height >= 0.0 && o_width >= 0.0 && (o_height + o_width) > 0.0 {
        Some(BBox {
            x0: o_left,
            top: o_top,
            x1: o_right,
            bottom: o_bottom,
        })
    } else {
        None
    }
}

/// Check if two bounding boxes have a strict overlap (both dimensions > 0).
pub fn bbox_overlap_strict(a: BBox, b: BBox) -> bool {
    match bbox_overlap(a, b) {
        Some(overlap) => overlap.width() > 0.0 && overlap.height() > 0.0,
        None => false,
    }
}
