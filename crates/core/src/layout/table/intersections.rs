//! Sweep-line algorithm for finding edge intersections.
//!
//! This module implements an efficient algorithm for finding all
//! intersections between horizontal and vertical edges, which is
//! the foundation for detecting table cell boundaries.

use std::collections::{BTreeMap, HashMap};
use std::simd::prelude::*;

use super::types::{EdgeObj, HEdgeId, KeyF64, KeyPoint, Orientation, VEdgeId, key_f64, key_point};

/// Storage for sorted vertical and horizontal edges.
pub struct EdgeStore {
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
pub struct IntersectionIdx {
    pub v: Vec<VEdgeId>,
    pub h: Vec<HEdgeId>,
}

#[derive(Clone, Debug)]
pub(crate) struct AoSoABlock {
    pub x0: [f64; 4],
    pub top: [f64; 4],
    pub bottom: [f64; 4],
    pub id: [usize; 4],
    pub mask: u8,
}

impl Default for AoSoABlock {
    fn default() -> Self {
        Self {
            x0: [0.0; 4],
            top: [0.0; 4],
            bottom: [0.0; 4],
            id: [0; 4],
            mask: 0,
        }
    }
}

impl AoSoABlock {
    fn insert(&mut self, id: usize, x0: f64, top: f64, bottom: f64) -> Option<u8> {
        for lane in 0..4 {
            let lane_bit = 1u8 << lane;
            if self.mask & lane_bit == 0 {
                self.x0[lane] = x0;
                self.top[lane] = top;
                self.bottom[lane] = bottom;
                self.id[lane] = id;
                self.mask |= lane_bit;
                return Some(lane_bit);
            }
        }
        None
    }

    fn remove(&mut self, lane_bit: u8) {
        self.mask &= !lane_bit;
    }
}

pub(crate) type BucketSlot = (usize, u8);

#[derive(Default, Debug)]
pub(crate) struct ActiveBucket {
    pub blocks: Vec<AoSoABlock>,
    len: usize,
}

impl ActiveBucket {
    pub fn insert(&mut self, id: usize, x0: f64, top: f64, bottom: f64) -> BucketSlot {
        for (block_idx, block) in self.blocks.iter_mut().enumerate() {
            if let Some(lane_bit) = block.insert(id, x0, top, bottom) {
                self.len += 1;
                return (block_idx, lane_bit);
            }
        }

        let mut block = AoSoABlock::default();
        let lane_bit = block
            .insert(id, x0, top, bottom)
            .expect("new block must accept insert");
        self.blocks.push(block);
        self.len += 1;
        (self.blocks.len() - 1, lane_bit)
    }

    pub fn remove(&mut self, slot: BucketSlot) {
        let (block_idx, lane_bit) = slot;
        if let Some(block) = self.blocks.get_mut(block_idx) {
            if block.mask & lane_bit != 0 {
                block.remove(lane_bit);
                self.len = self.len.saturating_sub(1);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[inline]
pub(crate) fn match_v_edges_simd4(
    tops: [f64; 4],
    bottoms: [f64; 4],
    x0s: [f64; 4],
    h_top: f64,
    x_min: f64,
    x_max: f64,
    y_tol: f64,
) -> [bool; 4] {
    let topv = Simd::<f64, 4>::from_array(tops);
    let botv = Simd::<f64, 4>::from_array(bottoms);
    let x0v = Simd::<f64, 4>::from_array(x0s);
    let htop = Simd::<f64, 4>::splat(h_top);
    let ytol = Simd::<f64, 4>::splat(y_tol);
    let xmin = Simd::<f64, 4>::splat(x_min);
    let xmax = Simd::<f64, 4>::splat(x_max);

    let y_ok = topv.simd_le(htop + ytol) & botv.simd_ge(htop - ytol);
    let x_ok = x0v.simd_ge(xmin) & x0v.simd_le(xmax);
    (y_ok & x_ok).to_array()
}

#[inline]
pub(crate) fn match_block_simd4(
    tops: [f64; 4],
    bottoms: [f64; 4],
    x0s: [f64; 4],
    h_top: f64,
    x_min: f64,
    x_max: f64,
    y_tol: f64,
    live_mask: u8,
) -> u8 {
    let topv = Simd::<f64, 4>::from_array(tops);
    let botv = Simd::<f64, 4>::from_array(bottoms);
    let x0v = Simd::<f64, 4>::from_array(x0s);
    let htop = Simd::<f64, 4>::splat(h_top);
    let ytol = Simd::<f64, 4>::splat(y_tol);
    let xmin = Simd::<f64, 4>::splat(x_min);
    let xmax = Simd::<f64, 4>::splat(x_max);

    let y_ok = topv.simd_le(htop + ytol) & botv.simd_ge(htop - ytol);
    let x_ok = x0v.simd_ge(xmin) & x0v.simd_le(xmax);
    (y_ok & x_ok).to_bitmask() as u8 & live_mask
}

/// Find all intersections between edges using a sweep-line algorithm.
///
/// Returns the edge store and a map from intersection points to the
/// edges that meet at each point.
pub fn edges_to_intersections(
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

    let mut active: BTreeMap<KeyF64, ActiveBucket> = BTreeMap::new();
    let mut active_slots: Vec<Option<(KeyF64, BucketSlot)>> = vec![None; v_sorted.len()];
    let mut pairs: HashMap<KeyPoint, Vec<(VEdgeId, HEdgeId)>> = HashMap::new();

    for event in events {
        match event.kind {
            EventKind::AddV => {
                let v = &v_sorted[event.idx];
                let key = key_f64(v.x0);
                let bucket = active.entry(key).or_default();
                let slot = bucket.insert(event.idx, v.x0, v.top, v.bottom);
                active_slots[event.idx] = Some((key, slot));
            }
            EventKind::RemoveV => {
                if let Some((key, slot)) = active_slots[event.idx].take() {
                    let mut remove_bucket = false;
                    if let Some(bucket) = active.get_mut(&key) {
                        bucket.remove(slot);
                        if bucket.is_empty() {
                            remove_bucket = true;
                        }
                    }
                    if remove_bucket {
                        active.remove(&key);
                    }
                }
            }
            EventKind::QueryH => {
                let h = &h_sorted[event.idx];
                let x_min = h.x0 - x_tol;
                let x_max = h.x1 + x_tol;
                for (_x0, bucket) in active.range(key_f64(x_min)..=key_f64(x_max)) {
                    for block in &bucket.blocks {
                        if block.mask == 0 {
                            continue;
                        }
                        let mask = match_block_simd4(
                            block.top,
                            block.bottom,
                            block.x0,
                            h.top,
                            x_min,
                            x_max,
                            y_tol,
                            block.mask,
                        );
                        if mask == 0 {
                            continue;
                        }
                        let mut lane_bit = 1u8;
                        for lane in 0..4 {
                            if mask & lane_bit != 0 {
                                let v_idx = block.id[lane];
                                let vertex = key_point(block.x0[lane], h.top);
                                pairs
                                    .entry(vertex)
                                    .or_default()
                                    .push((VEdgeId(v_idx), HEdgeId(event.idx)));
                            }
                            lane_bit <<= 1;
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
