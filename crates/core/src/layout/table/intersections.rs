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

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActiveSlot {
    block: usize,
    lane: u8,
}

#[derive(Clone, Debug)]
struct AoSoABlock {
    tops: [f64; 4],
    bottoms: [f64; 4],
    x0s: [f64; 4],
    ids: [usize; 4],
    mask: u8,
}

impl Default for AoSoABlock {
    fn default() -> Self {
        Self {
            tops: [0.0; 4],
            bottoms: [0.0; 4],
            x0s: [0.0; 4],
            ids: [0; 4],
            mask: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ActiveBucket {
    blocks: Vec<AoSoABlock>,
    free: Vec<ActiveSlot>,
    len: usize,
}

impl ActiveBucket {
    pub(crate) fn insert(&mut self, v_idx: usize, v: &EdgeObj) -> ActiveSlot {
        if let Some(slot) = self.free.pop() {
            let lane = slot.lane as usize;
            let block = &mut self.blocks[slot.block];
            block.tops[lane] = v.top;
            block.bottoms[lane] = v.bottom;
            block.x0s[lane] = v.x0;
            block.ids[lane] = v_idx;
            block.mask |= 1u8 << lane;
            self.len += 1;
            return slot;
        }

        let mut block = AoSoABlock::default();
        block.tops[0] = v.top;
        block.bottoms[0] = v.bottom;
        block.x0s[0] = v.x0;
        block.ids[0] = v_idx;
        block.mask = 1;
        let slot = ActiveSlot {
            block: self.blocks.len(),
            lane: 0,
        };
        self.blocks.push(block);
        self.len += 1;
        slot
    }

    fn remove(&mut self, slot: ActiveSlot) {
        let lane = slot.lane as usize;
        if let Some(block) = self.blocks.get_mut(slot.block) {
            let bit = 1u8 << lane;
            if block.mask & bit != 0 {
                block.mask &= !bit;
                self.free.push(slot);
                self.len = self.len.saturating_sub(1);
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[cfg(test)]
    pub(crate) fn active_len(&self) -> usize {
        self.len
    }
}

pub(crate) fn remove_active_entry(
    active: &mut BTreeMap<KeyF64, ActiveBucket>,
    active_slots: &mut [Option<(KeyF64, ActiveSlot)>],
    v_idx: usize,
) {
    let Some((key, slot)) = active_slots.get_mut(v_idx).and_then(Option::take) else {
        return;
    };
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
    let mut active_slots: Vec<Option<(KeyF64, ActiveSlot)>> = vec![None; v_sorted.len()];
    let mut pairs: HashMap<KeyPoint, Vec<(VEdgeId, HEdgeId)>> = HashMap::new();

    for event in events {
        match event.kind {
            EventKind::AddV => {
                let v = &v_sorted[event.idx];
                let key = key_f64(v.x0);
                let bucket = active.entry(key).or_default();
                let slot = bucket.insert(event.idx, v);
                active_slots[event.idx] = Some((key, slot));
            }
            EventKind::RemoveV => {
                remove_active_entry(&mut active, &mut active_slots, event.idx);
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
                        let mask = match_v_edges_simd4(
                            block.tops,
                            block.bottoms,
                            block.x0s,
                            h.top,
                            x_min,
                            x_max,
                            y_tol,
                        );
                        let mut mask_bits = 0u8;
                        for lane in 0..4 {
                            if mask[lane] {
                                mask_bits |= 1u8 << lane;
                            }
                        }
                        mask_bits &= block.mask;
                        if mask_bits == 0 {
                            continue;
                        }
                        for lane in 0..4 {
                            if mask_bits & (1u8 << lane) != 0 {
                                let v_idx = block.ids[lane];
                                let vertex = key_point(v_sorted[v_idx].x0, h.top);
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
