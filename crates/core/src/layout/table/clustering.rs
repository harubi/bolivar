//! Spatial clustering algorithms for table extraction.
//!
//! These functions group objects (edges, characters, words) based on
//! spatial proximity using tolerance-based clustering.

use std::collections::HashMap;

use crate::cancellation::CancellationToken;
use crate::error::Result;

use super::types::{BBox, KeyF64, WordObj, key_f64};

const CANCEL_INTERVAL: usize = 256;

fn cluster_list_cancellable(
    mut xs: Vec<f64>,
    tolerance: f64,
    cancellation: &CancellationToken,
) -> Result<Vec<Vec<f64>>> {
    cancellation.check()?;
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cancellation.check()?;
    if tolerance == 0.0 || xs.len() < 2 {
        return Ok(xs.into_iter().map(|x| vec![x]).collect());
    }
    let mut groups: Vec<Vec<f64>> = Vec::new();
    let mut current: Vec<f64> = Vec::new();
    let mut last = xs[0];
    current.push(xs[0]);
    for (index, x) in xs.into_iter().skip(1).enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        if x <= last + tolerance {
            current.push(x);
        } else {
            groups.push(current);
            current = vec![x];
        }
        last = x;
    }
    groups.push(current);
    Ok(groups)
}

fn make_cluster_dict_cancellable(
    values: Vec<f64>,
    tolerance: f64,
    cancellation: &CancellationToken,
) -> Result<HashMap<KeyF64, usize>> {
    cancellation.check()?;
    let mut unique: Vec<f64> = values;
    unique.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cancellation.check()?;
    unique.dedup_by(|a, b| (*a - *b).abs() == 0.0);
    let clusters = cluster_list_cancellable(unique, tolerance, cancellation)?;
    let mut dict = HashMap::new();
    let mut count = 0usize;
    for (cluster_index, cluster) in clusters.into_iter().enumerate() {
        for value in cluster {
            if count.is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            count += 1;
            dict.insert(key_f64(value), cluster_index);
        }
    }
    Ok(dict)
}

/// Cluster objects based on a key function and tolerance.
pub fn cluster_objects<T: Clone, F: Fn(&T) -> f64>(
    xs: &[T],
    key_fn: F,
    tolerance: f64,
    preserve_order: bool,
) -> Vec<Vec<T>> {
    cluster_objects_cancellable(
        xs,
        key_fn,
        tolerance,
        preserve_order,
        &CancellationToken::new(),
    )
    .expect("a new cancellation token cannot be cancelled")
}

pub(super) fn cluster_objects_cancellable<T: Clone, F: Fn(&T) -> f64>(
    xs: &[T],
    key_fn: F,
    tolerance: f64,
    preserve_order: bool,
    cancellation: &CancellationToken,
) -> Result<Vec<Vec<T>>> {
    cancellation.check()?;
    let mut values = Vec::with_capacity(xs.len());
    for (index, item) in xs.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        values.push(key_fn(item));
    }
    let cluster_dict = make_cluster_dict_cancellable(values, tolerance, cancellation)?;

    let mut cluster_tuples = Vec::with_capacity(xs.len());
    for (index, item) in xs.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        cluster_tuples.push((
            item.clone(),
            *cluster_dict.get(&key_f64(key_fn(item))).unwrap_or(&0),
        ));
    }
    if !preserve_order {
        cluster_tuples.sort_by_key(|tuple| tuple.1);
        cancellation.check()?;
    }

    let mut groups: Vec<Vec<T>> = Vec::new();
    let mut current: Vec<T> = Vec::new();
    let mut last_idx: Option<usize> = None;
    for (index, (item, cluster_index)) in cluster_tuples.drain(..).enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        if last_idx.is_none() || last_idx == Some(cluster_index) {
            current.push(item);
        } else {
            groups.push(current);
            current = vec![item];
        }
        last_idx = Some(cluster_index);
    }
    if !current.is_empty() {
        groups.push(current);
    }
    Ok(groups)
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
