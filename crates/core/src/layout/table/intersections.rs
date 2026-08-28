//! Sweep-line algorithm for finding edge intersections.
//!
//! This module implements an efficient algorithm for finding all
//! intersections between horizontal and vertical edges, which is
//! the foundation for detecting table cell boundaries.

use std::collections::HashMap;

use super::types::{EdgeObj, HEdgeId, KeyPoint, Orientation, VEdgeId, key_point};
use crate::cancellation::CancellationToken;
use crate::error::Result;

const CANCEL_INTERVAL: usize = 256;

/// Storage for sorted vertical and horizontal edges.
pub struct EdgeStore {
    pub v: Vec<EdgeObj>,
    pub h: Vec<EdgeObj>,
}

#[cfg(test)]
impl EdgeStore {
    pub fn v(&self, id: VEdgeId) -> &EdgeObj {
        &self.v[id.index()]
    }

    pub fn h(&self, id: HEdgeId) -> &EdgeObj {
        &self.h[id.index()]
    }
}

/// Index of edges meeting at an intersection point.
#[derive(Clone, Debug, Default)]
pub struct IntersectionIdx {
    pub v: Vec<VEdgeId>,
    pub h: Vec<HEdgeId>,
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
        slot
    }

    fn remove(&mut self, slot: ActiveSlot) -> Option<usize> {
        let lane = slot.lane as usize;
        let block = self.blocks.get_mut(slot.block)?;
        let bit = 1u8 << lane;
        if block.mask & bit == 0 {
            return None;
        }

        block.mask &= !bit;
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
        Some(removed_block)
    }

    #[cfg(test)]
    pub(crate) fn active_len(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| block.mask.count_ones() as usize)
            .sum()
    }
}

const EDGES_PER_BUCKET: usize = 32;

fn bucket_count(vertical_edges: usize) -> usize {
    vertical_edges.div_ceil(EDGES_PER_BUCKET)
}

fn edge_bucket(edge_index: usize) -> usize {
    edge_index / EDGES_PER_BUCKET
}

fn query_buckets(edges: &[EdgeObj], x_min: f64, x_max: f64) -> Option<(usize, usize)> {
    let first_edge = edges.partition_point(|edge| edge.x0 < x_min);
    let after_last = edges.partition_point(|edge| edge.x0 <= x_max);
    if first_edge == after_last {
        return None;
    }

    Some((edge_bucket(first_edge), edge_bucket(after_last - 1)))
}

#[cfg(test)]
pub(crate) fn bucket_count_for_edges(edges: &[EdgeObj]) -> usize {
    bucket_count(
        edges
            .iter()
            .filter(|edge| edge.orientation == Some(Orientation::Vertical))
            .count(),
    )
}

pub(crate) fn remove_active_entry(
    active: &mut [ActiveBucket],
    active_slots: &mut [Option<(usize, ActiveSlot)>],
    v_idx: usize,
) {
    let Some((bucket_idx, slot)) = active_slots.get_mut(v_idx).and_then(Option::take) else {
        return;
    };
    let Some(bucket) = active.get_mut(bucket_idx) else {
        return;
    };
    let Some(to) = bucket.remove(slot) else {
        return;
    };
    let Some(block) = bucket.blocks.get(to) else {
        return;
    };
    for lane in 0..4 {
        if block.mask & (1u8 << lane) != 0 {
            let moved_idx = block.ids[lane];
            if let Some((_, moved_slot)) = active_slots.get_mut(moved_idx).and_then(Option::as_mut)
            {
                moved_slot.block = to;
            }
        }
    }
}

/// Find all intersections between edges using a sweep-line algorithm.
///
/// Returns the edge store and a map from intersection points to the
/// edges that meet at each point.
#[cfg(test)]
pub fn edges_to_intersections(
    edges: &[EdgeObj],
    x_tol: f64,
    y_tol: f64,
) -> (EdgeStore, HashMap<KeyPoint, IntersectionIdx>) {
    find_intersections(edges, x_tol, y_tol, &CancellationToken::new())
        .expect("a new cancellation token cannot be cancelled")
}

#[hotpath::measure]
pub(crate) fn find_intersections(
    edges: &[EdgeObj],
    x_tol: f64,
    y_tol: f64,
    cancellation: &CancellationToken,
) -> Result<(EdgeStore, HashMap<KeyPoint, IntersectionIdx>)> {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    cancellation.check()?;
    let mut v_sorted: Vec<EdgeObj> = Vec::new();
    let mut h_sorted: Vec<EdgeObj> = Vec::new();
    for (index, edge) in edges.iter().enumerate() {
        if index.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        match edge.orientation {
            Some(Orientation::Vertical) => v_sorted.push(edge.clone()),
            Some(Orientation::Horizontal) => h_sorted.push(edge.clone()),
            None => {}
        }
    }

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
    cancellation.check()?;

    if v_sorted.is_empty() || h_sorted.is_empty() {
        return Ok((
            EdgeStore {
                v: v_sorted,
                h: h_sorted,
            },
            HashMap::new(),
        ));
    }

    let mut events = Vec::with_capacity(v_sorted.len() * 2 + h_sorted.len());
    for (idx, v) in v_sorted.iter().enumerate() {
        if idx.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
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
        if idx.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        events.push(Event {
            y: h.top,
            kind: EventKind::QueryH,
            idx,
        });
    }

    events.sort_by(|a, b| {
        let y_cmp = a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal);
        if y_cmp != std::cmp::Ordering::Equal {
            return y_cmp;
        }
        let kind_cmp = a.kind.cmp(&b.kind);
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
    cancellation.check()?;

    let mut active = vec![ActiveBucket::default(); bucket_count(v_sorted.len())];
    let mut active_slots: Vec<Option<(usize, ActiveSlot)>> = vec![None; v_sorted.len()];
    let mut intersections: HashMap<KeyPoint, IntersectionIdx> = HashMap::new();

    for (event_count, event) in events.into_iter().enumerate() {
        if event_count.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        match event.kind {
            EventKind::AddV => {
                let v = &v_sorted[event.idx];
                let bucket_idx = edge_bucket(event.idx);
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
                let Some((start, end)) = query_buckets(&v_sorted, x_min, x_max) else {
                    continue;
                };
                for (bucket_offset, bucket) in active[start..=end].iter().enumerate() {
                    if bucket_offset.is_multiple_of(CANCEL_INTERVAL) {
                        cancellation.check()?;
                    }
                    for &block_idx in &bucket.active_blocks {
                        let block = &bucket.blocks[block_idx];
                        let mut mask_bits = 0u8;
                        for lane in 0..4 {
                            if block.tops[lane] <= h.top + y_tol
                                && block.bottoms[lane] >= h.top - y_tol
                                && block.x0s[lane] >= x_min
                                && block.x0s[lane] <= x_max
                            {
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
                                let intersection = intersections.entry(vertex).or_default();
                                intersection.v.push(VEdgeId::from_index(v_idx));
                                intersection.h.push(HEdgeId::from_index(event.idx));
                            }
                        }
                    }
                }
            }
        }
    }

    for (intersection_count, intersection) in intersections.values_mut().enumerate() {
        if intersection_count.is_multiple_of(CANCEL_INTERVAL) {
            cancellation.check()?;
        }
        // Keep each vertical/horizontal pair aligned while restoring stable ID order.
        for index in 1..intersection.v.len() {
            if index.is_multiple_of(CANCEL_INTERVAL) {
                cancellation.check()?;
            }
            let v_id = intersection.v[index];
            let h_id = intersection.h[index];
            let mut insert_at = index;
            while insert_at > 0
                && (intersection.v[insert_at - 1], intersection.h[insert_at - 1]) > (v_id, h_id)
            {
                intersection.v[insert_at] = intersection.v[insert_at - 1];
                intersection.h[insert_at] = intersection.h[insert_at - 1];
                insert_at -= 1;
            }
            intersection.v[insert_at] = v_id;
            intersection.h[insert_at] = h_id;
        }
    }
    cancellation.check()?;
    Ok((
        EdgeStore {
            v: v_sorted,
            h: h_sorted,
        },
        intersections,
    ))
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
