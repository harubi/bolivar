//! Sweep-line algorithm for finding edge intersections.
//!
//! This module implements an efficient algorithm for finding all
//! intersections between horizontal and vertical edges, which is
//! the foundation for detecting table cell boundaries.

use std::collections::HashMap;
use std::simd::prelude::*;

use super::types::{EdgeObj, HEdgeId, KeyPoint, Orientation, VEdgeId, key_point};

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
    active_blocks: Vec<usize>,
    len: usize,
}

impl ActiveBucket {
    pub(crate) fn insert(&mut self, v_idx: usize, v: &EdgeObj) -> ActiveSlot {
        if let Some(slot) = self.free.pop() {
            let lane = slot.lane as usize;
            let block = &mut self.blocks[slot.block];
            let was_empty = block.mask == 0;
            block.tops[lane] = v.top;
            block.bottoms[lane] = v.bottom;
            block.x0s[lane] = v.x0;
            block.ids[lane] = v_idx;
            block.mask |= 1u8 << lane;
            if was_empty {
                self.active_blocks.push(slot.block);
            }
            self.len += 1;
            return slot;
        }

        let mut block = AoSoABlock::default();
        block.tops[0] = v.top;
        block.bottoms[0] = v.bottom;
        block.x0s[0] = v.x0;
        block.ids[0] = v_idx;
        block.mask = 1;
        let block_idx = self.blocks.len();
        let slot = ActiveSlot {
            block: block_idx,
            lane: 0,
        };
        self.blocks.push(block);
        self.active_blocks.push(block_idx);
        for lane in (1..4).rev() {
            self.free.push(ActiveSlot {
                block: block_idx,
                lane: lane as u8,
            });
        }
        self.len += 1;
        slot
    }

    fn remove(&mut self, slot: ActiveSlot) -> Option<(usize, usize)> {
        let lane = slot.lane as usize;
        if let Some(block) = self.blocks.get_mut(slot.block) {
            let bit = 1u8 << lane;
            if block.mask & bit != 0 {
                block.mask &= !bit;
                self.len = self.len.saturating_sub(1);
                if block.mask != 0 {
                    self.free.push(slot);
                    return None;
                }
                let removed_block = slot.block;
                self.free.retain(|entry| entry.block != removed_block);
                if let Some(pos) = self
                    .active_blocks
                    .iter()
                    .position(|&block_idx| block_idx == removed_block)
                {
                    self.active_blocks.swap_remove(pos);
                }
                let last_idx = self.blocks.len().saturating_sub(1);
                if removed_block == last_idx {
                    self.blocks.pop();
                    return None;
                }
                self.blocks.swap_remove(removed_block);
                for entry in &mut self.active_blocks {
                    if *entry == last_idx {
                        *entry = removed_block;
                    }
                }
                for entry in &mut self.free {
                    if entry.block == last_idx {
                        entry.block = removed_block;
                    }
                }
                return Some((last_idx, removed_block));
            }
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn active_len(&self) -> usize {
        self.len
    }
}

fn bucket_params_for_edges(edges: &[EdgeObj], x_tol: f64) -> Option<(f64, f64, usize)> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut v_count = 0usize;
    for edge in edges {
        if edge.orientation == Some(Orientation::Vertical) {
            min_x = min_x.min(edge.x0);
            max_x = max_x.max(edge.x0);
            v_count += 1;
        }
    }
    if v_count == 0 || !min_x.is_finite() || !max_x.is_finite() {
        return None;
    }
    let min_x = min_x - x_tol;
    let max_x = max_x + x_tol;
    let span = (max_x - min_x).max(0.0);
    let bucket_width = if x_tol > 0.0 {
        (x_tol * 2.0).max(1e-6)
    } else {
        (span / v_count as f64).max(1.0)
    };
    if !bucket_width.is_finite() || bucket_width <= 0.0 {
        return None;
    }
    let bucket_count = ((span / bucket_width).floor() as usize).saturating_add(1);
    Some((min_x, bucket_width, bucket_count))
}

fn bucket_index(x: f64, min_x: f64, bucket_width: f64, bucket_count: usize) -> usize {
    if bucket_count == 0 {
        return 0;
    }
    if x <= min_x {
        return 0;
    }
    let raw = ((x - min_x) / bucket_width).floor();
    if !raw.is_finite() {
        return 0;
    }
    let idx = raw as isize;
    if idx < 0 {
        0
    } else if (idx as usize) >= bucket_count {
        bucket_count - 1
    } else {
        idx as usize
    }
}

fn bucket_range(
    x_min: f64,
    x_max: f64,
    min_x: f64,
    bucket_width: f64,
    bucket_count: usize,
) -> Option<(usize, usize)> {
    if bucket_count == 0 {
        return None;
    }
    let start = bucket_index(x_min, min_x, bucket_width, bucket_count);
    let end = bucket_index(x_max, min_x, bucket_width, bucket_count);
    Some((start.min(end), start.max(end)))
}

pub(crate) fn bucket_count_for_edges(edges: &[EdgeObj], x_tol: f64) -> usize {
    bucket_params_for_edges(edges, x_tol)
        .map(|(_, _, count)| count)
        .unwrap_or(0)
}

pub(crate) fn remove_active_entry(
    active: &mut [ActiveBucket],
    active_slots: &mut [Option<(usize, ActiveSlot)>],
    v_idx: usize,
) {
    let Some((bucket_idx, slot)) = active_slots.get_mut(v_idx).and_then(Option::take) else {
        return;
    };
    if let Some(bucket) = active.get_mut(bucket_idx) {
        if let Some((_from, to)) = bucket.remove(slot) {
            if let Some(block) = bucket.blocks.get(to) {
                for lane in 0..4 {
                    if block.mask & (1u8 << lane) != 0 {
                        let moved_idx = block.ids[lane];
                        if let Some((_, moved_slot)) =
                            active_slots.get_mut(moved_idx).and_then(Option::as_mut)
                        {
                            moved_slot.block = to;
                        }
                    }
                }
            }
        }
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

    let bucket_params = bucket_params_for_edges(&v_sorted, x_tol);
    if bucket_params.is_none() {
        return (
            EdgeStore {
                v: v_sorted,
                h: h_sorted,
            },
            HashMap::new(),
        );
    }
    let (min_x, bucket_width, bucket_count) = bucket_params.unwrap();
    let mut active = vec![ActiveBucket::default(); bucket_count];
    let mut active_slots: Vec<Option<(usize, ActiveSlot)>> = vec![None; v_sorted.len()];
    let mut pairs: HashMap<KeyPoint, Vec<(VEdgeId, HEdgeId)>> = HashMap::new();

    for event in events {
        match event.kind {
            EventKind::AddV => {
                let v = &v_sorted[event.idx];
                let bucket_idx = bucket_index(v.x0, min_x, bucket_width, bucket_count);
                let bucket = &mut active[bucket_idx];
                let slot = bucket.insert(event.idx, v);
                active_slots[event.idx] = Some((bucket_idx, slot));
            }
            EventKind::RemoveV => {
                remove_active_entry(&mut active, &mut active_slots, event.idx);
            }
            EventKind::QueryH => {
                let h = &h_sorted[event.idx];
                let x_min = h.x0 - x_tol;
                let x_max = h.x1 + x_tol;
                let Some((start, end)) =
                    bucket_range(x_min, x_max, min_x, bucket_width, bucket_count)
                else {
                    continue;
                };
                for bucket in &active[start..=end] {
                    for &block_idx in &bucket.active_blocks {
                        let block = &bucket.blocks[block_idx];
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

#[cfg(test)]
mod tests {
    use super::{ActiveBucket, EdgeObj, Orientation};

    fn make_v_edge(x: f64, top: f64, bottom: f64) -> EdgeObj {
        EdgeObj {
            x0: x,
            x1: x,
            top,
            bottom,
            width: 0.0,
            height: bottom - top,
            orientation: Some(Orientation::Vertical),
            object_type: "test",
        }
    }

    #[test]
    fn aosoa_fills_blocks_before_new() {
        let mut bucket = ActiveBucket::default();
        let v = make_v_edge(1.0, 0.0, 10.0);
        let s0 = bucket.insert(0, &v);
        let s1 = bucket.insert(1, &v);
        let s2 = bucket.insert(2, &v);
        let s3 = bucket.insert(3, &v);

        assert_eq!(bucket.blocks.len(), 1);
        assert_eq!(bucket.blocks[0].mask, 0b1111);
        assert_eq!(s0.block, 0);
        assert_eq!(s1.block, 0);
        assert_eq!(s2.block, 0);
        assert_eq!(s3.block, 0);
    }

    #[test]
    fn aosoa_removes_empty_block_and_updates_free() {
        let mut bucket = ActiveBucket::default();
        let v = make_v_edge(2.0, 0.0, 10.0);
        let slots: Vec<_> = (0..6).map(|i| bucket.insert(i, &v)).collect();

        assert_eq!(bucket.blocks.len(), 2);

        bucket.remove(slots[5]);
        for slot in slots.iter().take(4) {
            bucket.remove(*slot);
        }

        assert_eq!(bucket.blocks.len(), 1);
        assert!(
            bucket
                .free
                .iter()
                .all(|slot| slot.block < bucket.blocks.len())
        );
    }
}
