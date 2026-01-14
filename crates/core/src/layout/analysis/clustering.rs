//! Text box clustering algorithms.
//!
//! Contains group_textboxes_exact() for exact pdfminer-compatible hierarchical grouping.

use std::collections::BinaryHeap;

use crate::utils::{HasBBox, INF_F64, Plane, Rect};

use super::super::params::LAParams;
use super::super::types::{LTTextGroup, TextBoxType, TextGroupElement};
use super::spatial::{
    BestEntry, DynamicSpatialTree, FrontierBestParams, FrontierEntry, PlaneElem, PyId, SpatialNode,
    bbox_union, calc_dist_lower_bound, expand_frontier_best,
};

/// Exact pdfminer-compatible grouping using a single-heap best-first algorithm.
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
    loop {
        let Some(entry) = best_heap.pop() else {
            break;
        };

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

    #[test]
    fn bbox_union_expected() {
        let a = (0.0, 1.0, 2.0, 3.0);
        let b = (-1.0, 2.0, 5.0, 4.0);
        let out = bbox_union(a, b);
        assert_eq!(out, (-1.0, 1.0, 5.0, 4.0));
    }
}
