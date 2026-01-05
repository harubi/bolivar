//! Text box clustering algorithms.
//!
//! Contains group_textboxes() for approximate grouping and
//! group_textboxes_exact() for exact pdfminer-compatible hierarchical grouping.

use std::collections::{BinaryHeap, binary_heap::PeekMut};

use crate::utils::{HasBBox, INF_F64, Plane, Rect};

use super::super::elements::{LTTextGroup, TextBoxType, TextGroupElement};
use super::super::params::LAParams;
use super::spatial_tree::{
    BestEntry, DynamicSpatialTree, FrontierEntry, GroupHeapEntry, PlaneElem, PyId, SpatialNode,
    calc_dist_lower_bound, expand_frontier, expand_frontier_best,
};

/// Groups text boxes hierarchically based on spatial proximity.
///
/// Uses a distance function to find the closest pairs of text boxes
/// and merges them into groups. Uses a heap for efficient access.
pub fn group_textboxes(_laparams: &LAParams, boxes: &[TextBoxType]) -> Vec<LTTextGroup> {
    if boxes.is_empty() {
        return Vec::new();
    }

    // Distance function: area of bounding rectangle minus areas of both boxes
    fn dist(obj1: &TextGroupElement, obj2: &TextGroupElement) -> f64 {
        let x0 = obj1.x0().min(obj2.x0());
        let y0 = obj1.y0().min(obj2.y0());
        let x1 = obj1.x1().max(obj2.x1());
        let y1 = obj1.y1().max(obj2.y1());
        obj2.width().mul_add(-obj2.height(), (x1 - x0).mul_add(y1 - y0, -(obj1.width() * obj1.height())))
    }

    // Heap entry: (distance, skip_isany, id1, id2, elements)
    // Python uses tuple: (skip_isany, d, id1, id2, obj1, obj2)
    #[derive(Clone)]
    struct HeapEntry {
        dist: f64, // Actual distance (not negated)
        skip_isany: bool,
        id1: usize,
        id2: usize,
        elem1: TextGroupElement,
        elem2: TextGroupElement,
    }

    impl PartialEq for HeapEntry {
        fn eq(&self, other: &Self) -> bool {
            self.dist == other.dist
        }
    }
    impl Eq for HeapEntry {}
    impl PartialOrd for HeapEntry {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for HeapEntry {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // BinaryHeap is max-heap, so larger values pop first.
            // Python's heapq is min-heap with tuple: (skip_isany, d, id1, id2)
            //
            // Python priorities (min-heap, smaller pops first):
            // 1. skip_isany: False < True -> False pops first
            // 2. dist: smaller distance pops first
            // 3. id1: smaller id pops first
            // 4. id2: smaller id pops first
            //
            // For max-heap, we reverse all comparisons:
            // 1. skip_isany: False > True (so False pops first)
            // 2. dist: smaller dist needs to be "greater" -> compare other.dist to self.dist
            // 3. id1: smaller id needs to be "greater" -> compare other.id1 to self.id1
            // 4. id2: smaller id needs to be "greater" -> compare other.id2 to self.id2

            // First compare skip_isany (False should pop first)
            match other.skip_isany.cmp(&self.skip_isany) {
                std::cmp::Ordering::Equal => {}
                ord => return ord, // True > False in reverse, so False pops first
            }

            // Then compare distance (smaller pops first)
            // Use total_cmp for consistent f64 ordering
            match other.dist.total_cmp(&self.dist) {
                std::cmp::Ordering::Equal => {}
                ord => return ord, // larger other.dist = self is "greater", pops first
            }

            // Tie-break by id1 (smaller pops first)
            match other.id1.cmp(&self.id1) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            }

            // Tie-break by id2 (smaller pops first)
            other.id2.cmp(&self.id2)
        }
    }

    // Build plane with all boxes
    let mut min_x0 = INF_F64;
    let mut min_y0 = INF_F64;
    let mut max_x1 = -INF_F64;
    let mut max_y1 = -INF_F64;
    for b in boxes {
        min_x0 = min_x0.min(b.x0());
        min_y0 = min_y0.min(b.y0());
        max_x1 = max_x1.max(b.x1());
        max_y1 = max_y1.max(b.y1());
    }
    let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
    let mut plane: Plane<TextGroupElement> = Plane::new(plane_bbox, 1);

    // Convert boxes to elements and compute initial distances
    let mut elements: Vec<TextGroupElement> = boxes
        .iter()
        .map(|b| TextGroupElement::Box(b.clone()))
        .collect();

    // Add elements to plane with extend() to build RTree for fast neighbor queries
    plane.extend(elements.iter().cloned());
    // Now plane.seq[i] == elements[i], so seq_index == element_id

    let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();
    let mut next_id = elements.len();

    // Initialize heap with k-nearest neighbor distances
    // K=20 captures ~96% of merges based on empirical measurement
    // (see examples/measure_neighbors.rs for analysis)
    const K_NEIGHBORS: usize = 20;
    for (i, elem) in elements.iter().enumerate() {
        for (j, neighbor) in plane.neighbors(elem.bbox(), K_NEIGHBORS) {
            if j > i {
                // Avoid duplicate pairs (i,j) and (j,i)
                let d = dist(elem, neighbor);
                heap.push(HeapEntry {
                    dist: d,
                    skip_isany: false,
                    id1: i,
                    id2: j,
                    elem1: elem.clone(),
                    elem2: neighbor.clone(),
                });
            }
        }
    }

    let mut done: Vec<bool> = vec![false; elements.len()];
    let mut result_elements: Vec<TextGroupElement> = Vec::new();

    while let Some(entry) = heap.pop() {
        // Skip if either object is already merged
        // With proper id1/id2 from iter_with_indices, this check is sufficient
        if done[entry.id1] || (entry.id2 != usize::MAX && done[entry.id2]) {
            continue;
        }

        // Check if there are any other objects between these two
        // Use find_with_indices and compare by index (not bbox) for correctness
        if !entry.skip_isany {
            let x0 = entry.elem1.x0().min(entry.elem2.x0());
            let y0 = entry.elem1.y0().min(entry.elem2.y0());
            let x1 = entry.elem1.x1().max(entry.elem2.x1());
            let y1 = entry.elem1.y1().max(entry.elem2.y1());
            let between: Vec<_> = plane
                .find_with_indices((x0, y0, x1, y1))
                .into_iter()
                .filter(|(idx, _)| !done[*idx])
                .collect();

            // Check if there's any element between that isn't id1 or id2
            let has_other = between
                .iter()
                .any(|(idx, _)| *idx != entry.id1 && *idx != entry.id2);

            if has_other {
                // Re-add with skip_isany=true
                let mut new_entry = entry.clone();
                new_entry.skip_isany = true;
                heap.push(new_entry);
                continue;
            }
        }

        // Merge the two elements into a group
        done[entry.id1] = true;
        // Only insert id2 if it's a real ID, not the usize::MAX placeholder
        if entry.id2 != usize::MAX {
            done[entry.id2] = true;
        }

        // Tombstone pattern: elements are tracked in `done` set instead of
        // being removed from plane. Query results are filtered against `done`.

        let is_vertical = entry.elem1.is_vertical() || entry.elem2.is_vertical();
        let group = LTTextGroup::new(vec![entry.elem1.clone(), entry.elem2.clone()], is_vertical);
        let group_elem = TextGroupElement::Group(Box::new(group));

        // Add distances to all remaining elements using iter_with_indices
        // The seq_index from Plane is the element ID for proper tie-breaking
        // Filter against done (tombstone pattern)
        for (other_id, other) in plane.iter_with_indices().filter(|(id, _)| !done[*id]) {
            let d = dist(&group_elem, other);
            heap.push(HeapEntry {
                dist: d,
                skip_isany: false,
                id1: next_id,
                id2: other_id,
                elem1: group_elem.clone(),
                elem2: other.clone(),
            });
        }

        // Add the new group to plane - next_id will be its seq_index
        plane.add(group_elem.clone());
        elements.push(group_elem);
        next_id += 1;
        done.push(false);
    }

    // Collect remaining elements as groups (filter against done - tombstone pattern)
    for (id, elem) in plane.iter_with_indices() {
        if !done[id] {
            result_elements.push(elem.clone());
        }
    }

    // Convert to LTTextGroup
    result_elements
        .into_iter()
        .map(|e| match e {
            TextGroupElement::Group(g) => *g,
            TextGroupElement::Box(b) => LTTextGroup::new(vec![TextGroupElement::Box(b)], false),
        })
        .collect()
}

/// Exact pdfminer-compatible grouping using a single-heap best-first algorithm.
pub fn group_textboxes_exact(laparams: &LAParams, boxes: &[TextBoxType]) -> Vec<LTTextGroup> {
    group_textboxes_exact_single_heap(laparams, boxes)
}

/// Exact pdfminer-compatible grouping using a single-heap best-first algorithm.
pub fn group_textboxes_exact_single_heap(
    _laparams: &LAParams,
    boxes: &[TextBoxType],
) -> Vec<LTTextGroup> {
    if boxes.is_empty() {
        return Vec::new();
    }

    // 1. Build elements with py_ids in PARSE ORDER
    let mut elements: Vec<TextGroupElement> = boxes
        .iter()
        .map(|b| TextGroupElement::Box(b.clone()))
        .collect();
    let mut py_ids: Vec<PyId> = (0..elements.len() as PyId).collect();
    let mut next_py_id = elements.len() as PyId;
    let reserve_extra = elements.len().saturating_sub(1);
    elements.reserve_exact(reserve_extra);
    py_ids.reserve_exact(reserve_extra);
    let mut bboxes: Vec<Rect> = elements.iter().map(|e| e.bbox()).collect();
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
                expand_frontier_best(
                    entry,
                    &initial_nodes,
                    &dynamic_tree,
                    &bboxes,
                    &areas,
                    &py_ids,
                    &done,
                    &mut best_heap,
                );
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
                    let x0 = bbox_a.0.min(bbox_b.0);
                    let y0 = bbox_a.1.min(bbox_b.1);
                    let x1 = bbox_a.2.max(bbox_b.2);
                    let y1 = bbox_a.3.max(bbox_b.3);

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

                let is_vertical = elements[best.elem1_idx].is_vertical()
                    || elements[best.elem2_idx].is_vertical();
                let group = LTTextGroup::new(
                    vec![
                        elements[best.elem1_idx].clone(),
                        elements[best.elem2_idx].clone(),
                    ],
                    is_vertical,
                );
                let group_elem = TextGroupElement::Group(Box::new(group));

                let new_idx = elements.len();
                let new_py_id = next_py_id;
                next_py_id += 1;

                let bbox_a = bboxes[best.elem1_idx];
                let bbox_b = bboxes[best.elem2_idx];
                let x0 = bbox_a.0.min(bbox_b.0);
                let y0 = bbox_a.1.min(bbox_b.1);
                let x1 = bbox_a.2.max(bbox_b.2);
                let y1 = bbox_a.3.max(bbox_b.3);
                let new_bbox = (x0, y0, x1, y1);
                let new_area = (x1 - x0) * (y1 - y0);

                plane.add(PlaneElem {
                    bbox: new_bbox,
                    idx: new_idx,
                });
                elements.push(group_elem);
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
        .iter()
        .enumerate()
        .filter(|(id, _)| !done[*id])
        .map(|(_, elem)| match elem {
            TextGroupElement::Group(g) => g.as_ref().clone(),
            TextGroupElement::Box(b) => {
                LTTextGroup::new(vec![TextGroupElement::Box(b.clone())], false)
            }
        })
        .collect()
}

/// Exact pdfminer-compatible grouping using a certified lazy all-pairs algorithm.
///
/// Uses existing Plane.find() for isany queries. Two-heap approach: main heap
/// holds exact (dist, id) pairs; frontier heap holds spatial node-pairs with
/// lower-bound keys for lazy pair generation.
pub fn group_textboxes_exact_dual_heap(
    _laparams: &LAParams,
    boxes: &[TextBoxType],
) -> Vec<LTTextGroup> {
    if boxes.is_empty() {
        return Vec::new();
    }

    // 1. Build elements with py_ids in PARSE ORDER
    let mut elements: Vec<TextGroupElement> = boxes
        .iter()
        .map(|b| TextGroupElement::Box(b.clone()))
        .collect();
    let mut py_ids: Vec<PyId> = (0..elements.len() as PyId).collect();
    let mut next_py_id = elements.len() as PyId;
    let reserve_extra = elements.len().saturating_sub(1);
    elements.reserve_exact(reserve_extra);
    py_ids.reserve_exact(reserve_extra);
    let mut bboxes: Vec<Rect> = elements.iter().map(|e| e.bbox()).collect();
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

    // 4. Initialize heaps
    let mut main_heap: BinaryHeap<GroupHeapEntry> = BinaryHeap::new();
    let mut frontier: BinaryHeap<FrontierEntry> = BinaryHeap::new();

    // Seed frontier with root vs root (may be None if single element)
    let root = &initial_nodes[root_idx];
    let lb = calc_dist_lower_bound(&root.stats, &root.stats);
    if let Some(entry) =
        FrontierEntry::new_initial(lb, &root.stats, &root.stats, root_idx, root_idx)
    {
        frontier.push(entry);
    }

    // 5. Track active elements (tombstone pattern via done set)
    let mut done: Vec<bool> = vec![false; elements.len()];
    done.reserve_exact(reserve_extra);

    // 6. Main loop
    loop {
        // Expand frontier while it could beat main heap
        while let Some(frontier_entry) = frontier.peek() {
            let should_expand = match main_heap.peek() {
                None => true,
                Some(main_entry) => frontier_entry.could_beat(main_entry),
            };

            if !should_expand {
                break;
            }

            let entry = frontier.pop().unwrap();
            expand_frontier(
                entry,
                &initial_nodes,
                &dynamic_tree,
                &bboxes,
                &areas,
                &py_ids,
                &done,
                &mut main_heap,
                &mut frontier,
            );
        }

        let Some(mut best) = main_heap.peek_mut() else {
            break;
        };

        // Skip if either element is already merged
        if done[best.elem1_idx] || done[best.elem2_idx] {
            PeekMut::pop(best);
            continue;
        }

        // isany check using allocation-free Plane query
        if !best.skip_isany {
            let bbox_a = bboxes[best.elem1_idx];
            let bbox_b = bboxes[best.elem2_idx];
            let x0 = bbox_a.0.min(bbox_b.0);
            let y0 = bbox_a.1.min(bbox_b.1);
            let x1 = bbox_a.2.max(bbox_b.2);
            let y1 = bbox_a.3.max(bbox_b.3);

            let has_between = plane.any_with_indices((x0, y0, x1, y1), |_, elem| {
                let idx = elem.idx;
                !done[idx] && idx != best.elem1_idx && idx != best.elem2_idx
            });

            if has_between {
                best.skip_isany = true;
                continue;
            }
        }

        let best = PeekMut::pop(best);

        // Merge!
        done[best.elem1_idx] = true;
        done[best.elem2_idx] = true;

        let is_vertical =
            elements[best.elem1_idx].is_vertical() || elements[best.elem2_idx].is_vertical();
        let group = LTTextGroup::new(
            vec![
                elements[best.elem1_idx].clone(),
                elements[best.elem2_idx].clone(),
            ],
            is_vertical,
        );
        let group_elem = TextGroupElement::Group(Box::new(group));

        let new_idx = elements.len();
        let new_py_id = next_py_id;
        next_py_id += 1;

        let bbox_a = bboxes[best.elem1_idx];
        let bbox_b = bboxes[best.elem2_idx];
        let x0 = bbox_a.0.min(bbox_b.0);
        let y0 = bbox_a.1.min(bbox_b.1);
        let x1 = bbox_a.2.max(bbox_b.2);
        let y1 = bbox_a.3.max(bbox_b.3);
        let new_bbox = (x0, y0, x1, y1);
        let new_area = (x1 - x0) * (y1 - y0);

        plane.add(PlaneElem {
            bbox: new_bbox,
            idx: new_idx,
        });
        elements.push(group_elem);
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
        frontier.push(entry);
    }

    // Collect remaining elements as groups
    elements
        .iter()
        .enumerate()
        .filter(|(id, _)| !done[*id])
        .map(|(_, elem)| match elem {
            TextGroupElement::Group(g) => g.as_ref().clone(),
            TextGroupElement::Box(b) => {
                LTTextGroup::new(vec![TextGroupElement::Box(b.clone())], false)
            }
        })
        .collect()
}
