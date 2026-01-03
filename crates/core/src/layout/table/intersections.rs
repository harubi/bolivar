//! Sweep-line algorithm for finding edge intersections.
//!
//! This module implements an efficient algorithm for finding all
//! intersections between horizontal and vertical edges, which is
//! the foundation for detecting table cell boundaries.

use std::collections::{BTreeMap, HashMap};

use super::types::{EdgeObj, HEdgeId, KeyF64, KeyPoint, Orientation, VEdgeId, key_f64, key_point};

/// Storage for sorted vertical and horizontal edges.
pub(crate) struct EdgeStore {
    pub v: Vec<EdgeObj>,
    pub h: Vec<EdgeObj>,
}

impl EdgeStore {
    pub fn v(&self, id: VEdgeId) -> &EdgeObj {
        &self.v[id.0]
    }

    pub fn h(&self, id: HEdgeId) -> &EdgeObj {
        &self.h[id.0]
    }
}

/// Index of edges meeting at an intersection point.
#[derive(Clone, Debug)]
pub(crate) struct IntersectionIdx {
    pub v: Vec<VEdgeId>,
    pub h: Vec<HEdgeId>,
}

/// Find all intersections between edges using a sweep-line algorithm.
///
/// Returns the edge store and a map from intersection points to the
/// edges that meet at each point.
pub(crate) fn edges_to_intersections(
    edges: &[EdgeObj],
    x_tol: f64,
    y_tol: f64,
) -> (EdgeStore, HashMap<KeyPoint, IntersectionIdx>) {
    enum EventKind {
        AddV,
        QueryH,
        RemoveV,
    }

    struct Event {
        y: f64,
        kind: EventKind,
        idx: usize,
    }

    let mut v_sorted: Vec<EdgeObj> = edges
        .iter()
        .filter(|e| e.orientation == Some(Orientation::Vertical))
        .cloned()
        .collect();
    let mut h_sorted: Vec<EdgeObj> = edges
        .iter()
        .filter(|e| e.orientation == Some(Orientation::Horizontal))
        .cloned()
        .collect();

    v_sorted.sort_by(|a, b| {
        a.x0.partial_cmp(&b.x0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.top
                    .partial_cmp(&b.top)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    h_sorted.sort_by(|a, b| {
        a.top
            .partial_cmp(&b.top)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut events = Vec::with_capacity(v_sorted.len() * 2 + h_sorted.len());
    for (idx, v) in v_sorted.iter().enumerate() {
        events.push(Event {
            y: v.top - y_tol,
            kind: EventKind::AddV,
            idx,
        });
        events.push(Event {
            y: v.bottom + y_tol,
            kind: EventKind::RemoveV,
            idx,
        });
    }
    for (idx, h) in h_sorted.iter().enumerate() {
        events.push(Event {
            y: h.top,
            kind: EventKind::QueryH,
            idx,
        });
    }

    let kind_order = |kind: &EventKind| match kind {
        EventKind::AddV => 0,
        EventKind::QueryH => 1,
        EventKind::RemoveV => 2,
    };

    events.sort_by(|a, b| {
        let y_cmp = a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal);
        if y_cmp != std::cmp::Ordering::Equal {
            return y_cmp;
        }
        let kind_cmp = kind_order(&a.kind).cmp(&kind_order(&b.kind));
        if kind_cmp != std::cmp::Ordering::Equal {
            return kind_cmp;
        }
        let (ax0, atop) = match a.kind {
            EventKind::AddV | EventKind::RemoveV => {
                let v = &v_sorted[a.idx];
                (v.x0, v.top)
            }
            EventKind::QueryH => {
                let h = &h_sorted[a.idx];
                (h.x0, h.top)
            }
        };
        let (bx0, btop) = match b.kind {
            EventKind::AddV | EventKind::RemoveV => {
                let v = &v_sorted[b.idx];
                (v.x0, v.top)
            }
            EventKind::QueryH => {
                let h = &h_sorted[b.idx];
                (h.x0, h.top)
            }
        };
        ax0.partial_cmp(&bx0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(atop.partial_cmp(&btop).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.idx.cmp(&b.idx))
    });

    let mut active: BTreeMap<KeyF64, Vec<usize>> = BTreeMap::new();
    let mut pairs: HashMap<KeyPoint, Vec<(VEdgeId, HEdgeId)>> = HashMap::new();

    for event in events {
        match event.kind {
            EventKind::AddV => {
                let v = &v_sorted[event.idx];
                active.entry(key_f64(v.x0)).or_default().push(event.idx);
            }
            EventKind::RemoveV => {
                let v = &v_sorted[event.idx];
                let key = key_f64(v.x0);
                if let Some(bucket) = active.get_mut(&key) {
                    if let Some(pos) = bucket.iter().position(|&idx| idx == event.idx) {
                        bucket.remove(pos);
                    }
                    if bucket.is_empty() {
                        active.remove(&key);
                    }
                }
            }
            EventKind::QueryH => {
                let h = &h_sorted[event.idx];
                let x_min = key_f64(h.x0 - x_tol);
                let x_max = key_f64(h.x1 + x_tol);
                for (_x0, v_indices) in active.range(x_min..=x_max) {
                    for &v_idx in v_indices {
                        let v = &v_sorted[v_idx];
                        if v.top <= h.top + y_tol
                            && v.bottom >= h.top - y_tol
                            && v.x0 >= h.x0 - x_tol
                            && v.x0 <= h.x1 + x_tol
                        {
                            let vertex = key_point(v.x0, h.top);
                            pairs
                                .entry(vertex)
                                .or_default()
                                .push((VEdgeId(v_idx), HEdgeId(event.idx)));
                        }
                    }
                }
            }
        }
    }

    let mut intersections: HashMap<KeyPoint, IntersectionIdx> = HashMap::with_capacity(pairs.len());
    for (vertex, mut pair_list) in pairs {
        pair_list.sort_by(|a, b| a.0.0.cmp(&b.0.0).then(a.1.0.cmp(&b.1.0)));
        let mut v = Vec::with_capacity(pair_list.len());
        let mut h = Vec::with_capacity(pair_list.len());
        for (v_idx, h_idx) in pair_list {
            v.push(v_idx);
            h.push(h_idx);
        }
        intersections.insert(vertex, IntersectionIdx { v, h });
    }
    (
        EdgeStore {
            v: v_sorted,
            h: h_sorted,
        },
        intersections,
    )
}
