//! Text box clustering algorithms.
//!
//! Contains `group_textboxes_exact()` — best-first hierarchical grouping over
//! already-formed text boxes. Normal pages use a full pair heap. Unusually large
//! pages use a bounded-memory spatial frontier. This is algorithmically distinct
//! from [`super::grouping`], which forms lines and boxes upstream of this stage.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::utils::{HasBBox, INF_F64, Plane, Rect};

use super::super::params::LAParams;
use super::super::types::{LTTextGroup, TextBoxType, TextGroupElement};
use super::spatial::{
    BestEntry, DistKey, DynamicSpatialTree, FrontierBestParams, FrontierEntry, PlaneElem, PyId,
    SpatialNode, bbox_union, calc_dist_lower_bound, dist_key_from_geom, expand_frontier_best,
};

const FULL_HEAP_BOX_LIMIT: usize = 512;

/// Exact pdfminer-compatible hierarchical grouping.
pub fn group_textboxes_exact(_laparams: &LAParams, boxes: &[TextBoxType]) -> Vec<LTTextGroup> {
    let elements = boxes.iter().cloned().map(TextGroupElement::Box).collect();
    group_textboxes_exact_impl(elements)
}

pub(crate) fn group_textboxes_exact_owned(
    _laparams: &LAParams,
    boxes: Vec<TextBoxType>,
) -> Vec<LTTextGroup> {
    let elements = boxes.into_iter().map(TextGroupElement::Box).collect();
    group_textboxes_exact_impl(elements)
}

fn group_textboxes_exact_impl(elements: Vec<TextGroupElement>) -> Vec<LTTextGroup> {
    // The full heap is faster for normal pages. Keep the bounded-memory frontier
    // for unusually large pages, where all pairs would use quadratic memory.
    if elements.len() <= FULL_HEAP_BOX_LIMIT {
        return group_textboxes_full_heap_impl(elements);
    }

    group_textboxes_frontier_impl(elements)
}

/// One candidate pair of the full heap, packed so that integer order is pdfminer's order:
/// `(skip_isany, dist, id1, id2)`, lexicographic. In the full heap an element's id is its index,
/// so the pair needs no separate indices: the key is 16 bytes, not a 48-byte entry, and one
/// integer compare orders it. Ids take 31 bits each, ample for [`FULL_HEAP_BOX_LIMIT`] boxes
/// and the groups made from them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PairKey(u128);

impl PairKey {
    const SKIP: u128 = 1 << 127;

    const ID_BITS: u32 = 31;
    const ID_MASK: u128 = (1 << Self::ID_BITS) - 1;

    fn new(dist: DistKey, id1: usize, id2: usize) -> Self {
        debug_assert!(id1 as u128 <= Self::ID_MASK && id2 as u128 <= Self::ID_MASK);
        // Bits: skip flag at 127, distance at 62..=125, id1 at 31..=61, id2 at 0..=30. Flipping
        // the sign bit maps i64 order onto u64 order.
        let dist = (dist as u64 ^ (1 << 63)) as u128;
        Self((dist << (2 * Self::ID_BITS)) | ((id1 as u128) << Self::ID_BITS) | id2 as u128)
    }

    const fn skipping(self) -> Self {
        Self(self.0 | Self::SKIP)
    }

    const fn skips(self) -> bool {
        self.0 & Self::SKIP != 0
    }

    const fn id1(self) -> usize {
        ((self.0 >> Self::ID_BITS) & Self::ID_MASK) as usize
    }

    const fn id2(self) -> usize {
        (self.0 & Self::ID_MASK) as usize
    }
}

fn group_textboxes_full_heap_impl(elements: Vec<TextGroupElement>) -> Vec<LTTextGroup> {
    if elements.is_empty() {
        return Vec::new();
    }

    let mut elements: Vec<Option<TextGroupElement>> = elements.into_iter().map(Some).collect();
    let reserve_extra = elements.len().saturating_sub(1);
    elements.reserve_exact(reserve_extra);

    let mut bboxes: Vec<Rect> = elements
        .iter()
        .map(|element| element.as_ref().expect("element missing").bbox())
        .collect();
    let mut areas: Vec<f64> = bboxes
        .iter()
        .map(|bbox| (bbox.2 - bbox.0) * (bbox.3 - bbox.1))
        .collect();
    bboxes.reserve_exact(reserve_extra);
    areas.reserve_exact(reserve_extra);

    let mut min_x0 = INF_F64;
    let mut min_y0 = INF_F64;
    let mut max_x1 = -INF_F64;
    let mut max_y1 = -INF_F64;
    for bbox in &bboxes {
        min_x0 = min_x0.min(bbox.0);
        min_y0 = min_y0.min(bbox.1);
        max_x1 = max_x1.max(bbox.2);
        max_y1 = max_y1.max(bbox.3);
    }
    let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
    let mut plane: Plane<PlaneElem> = Plane::new(plane_bbox, 1);
    plane.extend(
        bboxes
            .iter()
            .enumerate()
            .map(|(idx, &bbox)| PlaneElem { bbox, idx }),
    );

    // Every initial pair, heapified at once: O(n) rather than one O(log n) push per pair.
    // Keys are unique, so the pop order does not depend on how the heap was built.
    let n = elements.len();
    let mut pairs = Vec::with_capacity(n * (n - 1) / 2);
    for elem1_idx in 0..n {
        for elem2_idx in (elem1_idx + 1)..n {
            let dist = dist_key_from_geom(
                bboxes[elem1_idx],
                areas[elem1_idx],
                bboxes[elem2_idx],
                areas[elem2_idx],
            );
            pairs.push(Reverse(PairKey::new(dist, elem1_idx, elem2_idx)));
        }
    }
    let mut best_heap = BinaryHeap::from(pairs);

    let mut done = vec![false; elements.len()];
    done.reserve_exact(reserve_extra);

    while let Some(Reverse(best)) = best_heap.pop() {
        let (elem1_idx, elem2_idx) = (best.id1(), best.id2());
        if done[elem1_idx] || done[elem2_idx] {
            continue;
        }

        if !best.skips() {
            let union = bbox_union(bboxes[elem1_idx], bboxes[elem2_idx]);
            let has_between = plane.any_with_indices(union, |_, element| {
                element.idx != elem1_idx && element.idx != elem2_idx
            });
            if has_between {
                best_heap.push(Reverse(best.skipping()));
                continue;
            }
        }

        done[elem1_idx] = true;
        done[elem2_idx] = true;
        plane.remove_by_id(elem1_idx);
        plane.remove_by_id(elem2_idx);

        let left = elements[elem1_idx].take().expect("missing elem1");
        let right = elements[elem2_idx].take().expect("missing elem2");
        let is_vertical = left.is_vertical() || right.is_vertical();
        let group = LTTextGroup::new(vec![left, right], is_vertical);
        let group_elem = TextGroupElement::Group(Box::new(group));

        let new_idx = elements.len();
        let new_bbox = bbox_union(bboxes[elem1_idx], bboxes[elem2_idx]);
        let new_area = (new_bbox.2 - new_bbox.0) * (new_bbox.3 - new_bbox.1);

        elements.push(Some(group_elem));
        bboxes.push(new_bbox);
        areas.push(new_area);
        done.push(false);

        for other_idx in 0..new_idx {
            if done[other_idx] {
                continue;
            }
            let dist = dist_key_from_geom(new_bbox, new_area, bboxes[other_idx], areas[other_idx]);
            best_heap.push(Reverse(PairKey::new(dist, new_idx, other_idx)));
        }
        plane.add(PlaneElem {
            bbox: new_bbox,
            idx: new_idx,
        });
    }

    elements
        .into_iter()
        .enumerate()
        .filter(|(id, _)| !done[*id])
        .filter_map(|(_, element)| element)
        .map(|element| match element {
            TextGroupElement::Group(group) => *group,
            TextGroupElement::Box(text_box) => {
                LTTextGroup::new(vec![TextGroupElement::Box(text_box)], false)
            }
        })
        .collect()
}

fn group_textboxes_frontier_impl(elements: Vec<TextGroupElement>) -> Vec<LTTextGroup> {
    if elements.is_empty() {
        return Vec::new();
    }

    // 1. Build elements with py_ids in PARSE ORDER
    let mut elements: Vec<Option<TextGroupElement>> = elements.into_iter().map(Some).collect();
    let mut py_ids: Vec<PyId> = (0..elements.len() as PyId).collect();
    let mut next_py_id = elements.len() as PyId;
    let reserve_extra = elements.len().saturating_sub(1);
    elements.reserve_exact(reserve_extra);
    py_ids.reserve_exact(reserve_extra);
    let mut bboxes: Vec<Rect> = elements
        .iter()
        .map(|e| e.as_ref().expect("element missing").bbox())
        .collect();
    let mut areas: Vec<f64> = bboxes.iter().map(|b| (b.2 - b.0) * (b.3 - b.1)).collect();
    bboxes.reserve_exact(reserve_extra);
    areas.reserve_exact(reserve_extra);

    // 2. Build Plane for isany queries (uses existing infrastructure)
    let mut min_x0 = INF_F64;
    let mut min_y0 = INF_F64;
    let mut max_x1 = -INF_F64;
    let mut max_y1 = -INF_F64;
    for bbox in &bboxes {
        min_x0 = min_x0.min(bbox.0);
        min_y0 = min_y0.min(bbox.1);
        max_x1 = max_x1.max(bbox.2);
        max_y1 = max_y1.max(bbox.3);
    }
    let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
    let mut plane: Plane<PlaneElem> = Plane::new(plane_bbox, 1);
    plane.extend(
        bboxes
            .iter()
            .enumerate()
            .map(|(idx, &bbox)| PlaneElem { bbox, idx }),
    );

    // 3. Build lightweight spatial tree for frontier
    let mut bbox_ids: Vec<(Rect, PyId)> = bboxes
        .iter()
        .enumerate()
        .map(|(i, &bbox)| (bbox, i as PyId))
        .collect();
    bbox_ids.reserve_exact(reserve_extra);
    let mut initial_nodes: Vec<SpatialNode> = Vec::new();
    let root_idx = SpatialNode::build(&bbox_ids, &mut initial_nodes);
    let mut dynamic_tree = DynamicSpatialTree::build(&bbox_ids);

    // 4. Initialize best-first heap (frontier + exact pairs)
    let mut best_heap: BinaryHeap<BestEntry> = BinaryHeap::new();

    // Seed heap with root vs root (may be None if single element)
    let root = &initial_nodes[root_idx];
    let lb = calc_dist_lower_bound(&root.stats, &root.stats);
    if let Some(entry) =
        FrontierEntry::new_initial(lb, &root.stats, &root.stats, root_idx, root_idx)
    {
        best_heap.push(BestEntry::Frontier(entry));
    }

    // 5. Track active elements (tombstone pattern via done set)
    let mut done: Vec<bool> = vec![false; elements.len()];
    done.reserve_exact(reserve_extra);

    // 6. Main loop
    while let Some(entry) = best_heap.pop() {
        match entry {
            BestEntry::Frontier(entry) => {
                let mut params = FrontierBestParams {
                    initial_nodes: &initial_nodes,
                    dynamic_tree: &dynamic_tree,
                    bboxes: &bboxes,
                    areas: &areas,
                    py_ids: &py_ids,
                    done: &done,
                    best_heap: &mut best_heap,
                };
                expand_frontier_best(entry, &mut params);
            }
            BestEntry::Pair(mut best) => {
                // Skip if either element is already merged
                if done[best.elem1_idx] || done[best.elem2_idx] {
                    continue;
                }

                // isany check using allocation-free Plane query
                if !best.skip_isany {
                    let bbox_a = bboxes[best.elem1_idx];
                    let bbox_b = bboxes[best.elem2_idx];
                    let (x0, y0, x1, y1) = bbox_union(bbox_a, bbox_b);

                    let has_between = plane.any_with_indices((x0, y0, x1, y1), |_, elem| {
                        let idx = elem.idx;
                        !done[idx] && idx != best.elem1_idx && idx != best.elem2_idx
                    });

                    if has_between {
                        best.skip_isany = true;
                        best_heap.push(BestEntry::Pair(best));
                        continue;
                    }
                }

                // Merge!
                done[best.elem1_idx] = true;
                done[best.elem2_idx] = true;

                let left = elements[best.elem1_idx].take().expect("missing elem1");
                let right = elements[best.elem2_idx].take().expect("missing elem2");
                let is_vertical = left.is_vertical() || right.is_vertical();
                let group = LTTextGroup::new(vec![left, right], is_vertical);
                let group_elem = TextGroupElement::Group(Box::new(group));

                let new_idx = elements.len();
                let new_py_id = next_py_id;
                next_py_id += 1;

                let bbox_a = bboxes[best.elem1_idx];
                let bbox_b = bboxes[best.elem2_idx];
                let new_bbox = bbox_union(bbox_a, bbox_b);
                let x0 = new_bbox.0;
                let y0 = new_bbox.1;
                let x1 = new_bbox.2;
                let y1 = new_bbox.3;
                let new_area = (x1 - x0) * (y1 - y0);

                plane.add(PlaneElem {
                    bbox: new_bbox,
                    idx: new_idx,
                });
                elements.push(Some(group_elem));
                bboxes.push(new_bbox);
                areas.push(new_area);
                py_ids.push(new_py_id);
                done.push(false);
                bbox_ids.push((new_bbox, new_py_id));

                let group_leaf = dynamic_tree.insert(new_idx, new_bbox, new_py_id, &bbox_ids);
                let group_stats = &dynamic_tree.nodes[group_leaf].stats;
                let root_stats = &dynamic_tree.nodes[dynamic_tree.root].stats;
                let lb = calc_dist_lower_bound(group_stats, root_stats);
                let entry = FrontierEntry::new_group_other(
                    lb,
                    new_py_id,
                    new_idx,
                    root_stats,
                    group_leaf,
                    dynamic_tree.root,
                );
                best_heap.push(BestEntry::Frontier(entry));
            }
        }
    }

    // Collect remaining elements as groups
    elements
        .into_iter()
        .enumerate()
        .filter(|(id, _)| !done[*id])
        .filter_map(|(_, elem)| elem)
        .map(|elem| match elem {
            TextGroupElement::Group(g) => *g,
            TextGroupElement::Box(b) => LTTextGroup::new(vec![TextGroupElement::Box(b)], false),
        })
        .collect()
}

#[cfg(test)]
mod textboxes_exact_tests {
    use super::*;
    use crate::layout::{LTTextBoxHorizontal, LTTextLineHorizontal};

    fn horizontal_box(bbox: Rect) -> TextGroupElement {
        let mut line = LTTextLineHorizontal::new(0.1);
        line.set_bbox(bbox);
        let mut text_box = LTTextBoxHorizontal::new();
        text_box.add(line);
        TextGroupElement::Box(TextBoxType::Horizontal(text_box))
    }

    #[test]
    fn bbox_union_expected() {
        let a = (0.0, 1.0, 2.0, 3.0);
        let b = (-1.0, 2.0, 5.0, 4.0);
        let out = bbox_union(a, b);
        assert_eq!(out, (-1.0, 1.0, 5.0, 4.0));
    }

    #[test]
    fn full_heap_matches_bounded_frontier() {
        let elements: Vec<_> = (0..32)
            .map(|index| {
                let column = (index % 4) as f64;
                let row = (index / 4) as f64;
                let x0 = 20.0 + column * 120.0 + (index % 3) as f64;
                let y0 = 700.0 - row * 18.0 - (index % 2) as f64;
                horizontal_box((x0, y0, x0 + 72.0, y0 + 9.0))
            })
            .collect();

        let full_heap = group_textboxes_full_heap_impl(elements.clone());
        let bounded_frontier = group_textboxes_frontier_impl(elements);

        assert_eq!(format!("{full_heap:?}"), format!("{bounded_frontier:?}"));
    }
}

#[cfg(test)]
mod pair_key_tests {
    use super::*;

    /// Packed keys order exactly as pdfminer's (skip_isany, dist, id1, id2) tuples, and give
    /// their fields back.
    #[test]
    fn packed_order_is_pdfminers_tuple_order() {
        let dists = [
            i64::MIN,
            i64::MIN + 1,
            -7,
            -1,
            0,
            1,
            7,
            i64::MAX - 1,
            i64::MAX,
        ];
        let ids = [0, 1, 2, 511, 1023, PairKey::ID_MASK as usize];
        let mut keys = Vec::new();
        for skip in [false, true] {
            for &dist in &dists {
                for &id1 in &ids {
                    for &id2 in &ids {
                        let key = PairKey::new(dist, id1, id2);
                        let key = if skip { key.skipping() } else { key };
                        assert_eq!((key.skips(), key.id1(), key.id2()), (skip, id1, id2));
                        keys.push(((skip, dist, id1, id2), key));
                    }
                }
            }
        }
        // A fixed pseudo-random walk over pairs, plus every pair of the edge cases above.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..200_000 {
            let a = (next() % keys.len() as u64) as usize;
            let b = (next() % keys.len() as u64) as usize;
            assert_eq!(
                keys[a].0.cmp(&keys[b].0),
                keys[a].1.cmp(&keys[b].1),
                "{:?} {:?}",
                keys[a].0,
                keys[b].0
            );
        }
        for (ta, ka) in &keys {
            for (tb, kb) in &keys {
                assert_eq!(ta.cmp(tb), ka.cmp(kb));
            }
        }
    }
}
