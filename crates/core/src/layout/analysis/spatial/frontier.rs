//! Frontier expansion for lazy pair generation.
//!
//! The frontier-based algorithm generates element pairs on-demand,
//! avoiding the O(n²) cost of precomputing all pairs.

use std::collections::BinaryHeap;

use super::distance::{calc_dist_lower_bound, dist_key_from_geom};
use super::tree::{DynamicSpatialTree, SpatialNode};
use super::types::{
    BestEntry, DistKey, FrontierEntry, GroupHeapEntry, NodeStats, PairMode, PyId, TreeKind,
};
use crate::utils::Rect;

impl FrontierEntry {
    /// Create frontier entry for InitialIJ mode (self-pair or cross-pair)
    pub const fn new_initial(
        lb_dist: DistKey,
        stats_a: &NodeStats,
        stats_b: &NodeStats,
        node_a: usize,
        node_b: usize,
    ) -> Option<Self> {
        if node_a == node_b {
            // Self-pair: need at least 2 elements
            if stats_a.second_min_py_id == PyId::MAX {
                return None; // Skip invalid self-pair
            }
            Some(Self {
                lb_dist,
                lb_id1: stats_a.min_py_id,
                lb_id2: stats_a.second_min_py_id,
                node_a,
                node_b,
                mode: PairMode::InitialIJ,
                tree: TreeKind::Initial,
            })
        } else {
            // Cross-pair: orient by smallest min
            let (lb_id1, lb_id2) = if stats_a.min_py_id < stats_b.min_py_id {
                (stats_a.min_py_id, stats_b.min_py_id)
            } else {
                (stats_b.min_py_id, stats_a.min_py_id)
            };
            Some(Self {
                lb_dist,
                lb_id1,
                lb_id2,
                node_a,
                node_b,
                mode: PairMode::InitialIJ,
                tree: TreeKind::Initial,
            })
        }
    }

    /// Create frontier entry for GroupOther mode
    pub const fn new_group_other(
        lb_dist: DistKey,
        group_id: PyId,
        group_idx: usize,
        other_stats: &NodeStats,
        node_a: usize,
        node_b: usize,
    ) -> Self {
        Self {
            lb_dist,
            lb_id1: group_id,
            lb_id2: other_stats.min_py_id,
            node_a,
            node_b,
            mode: PairMode::GroupOther {
                group_id,
                group_idx,
            },
            tree: TreeKind::Dynamic,
        }
    }

    /// Check if frontier entry could beat main heap entry (tie-safe with <=)
    pub fn could_beat(&self, main: &GroupHeapEntry) -> bool {
        // Frontier always has skip_isany=false, so if main has skip_isany=true, frontier wins
        if main.skip_isany {
            return true;
        }
        // Compare (dist, id1, id2) lexicographically with <= for tie-safety
        match self.lb_dist.cmp(&main.dist) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => {
                // Tie on dist - use <= to ensure we expand on ties
                (self.lb_id1, self.lb_id2) <= (main.id1, main.id2)
            }
        }
    }
}

/// Expand a frontier entry for the single-heap best-first algorithm.
///
/// If both nodes are leaves, emits concrete pairs to the heap.
/// Otherwise, splits the larger node and re-enqueues child frontiers.
pub fn expand_frontier_best(
    entry: FrontierEntry,
    initial_nodes: &[SpatialNode],
    dynamic_tree: &DynamicSpatialTree,
    bboxes: &[Rect],
    areas: &[f64],
    py_ids: &[PyId],
    done: &[bool],
    best_heap: &mut BinaryHeap<BestEntry>,
) {
    let nodes = match entry.tree {
        TreeKind::Initial => initial_nodes,
        TreeKind::Dynamic => &dynamic_tree.nodes,
    };
    let node_a = &nodes[entry.node_a];
    let node_b = &nodes[entry.node_b];

    if node_a.is_leaf() && node_b.is_leaf() {
        // Emit concrete pairs
        match entry.mode {
            PairMode::InitialIJ => {
                if entry.node_a == entry.node_b {
                    // Self-pair: emit (i, j) where py_ids[i] < py_ids[j]
                    for i in 0..node_a.element_indices.len() {
                        for j in (i + 1)..node_a.element_indices.len() {
                            let ei = node_a.element_indices[i];
                            let ej = node_a.element_indices[j];
                            if done[ei] || done[ej] {
                                continue;
                            }
                            let d =
                                dist_key_from_geom(bboxes[ei], areas[ei], bboxes[ej], areas[ej]);
                            best_heap.push(BestEntry::Pair(GroupHeapEntry {
                                skip_isany: false,
                                dist: d,
                                id1: py_ids[ei],
                                id2: py_ids[ej],
                                elem1_idx: ei,
                                elem2_idx: ej,
                            }));
                        }
                    }
                } else {
                    // Cross-pair: emit with i<j orientation
                    for &ei in &node_a.element_indices {
                        for &ej in &node_b.element_indices {
                            if done[ei] || done[ej] {
                                continue;
                            }
                            let d =
                                dist_key_from_geom(bboxes[ei], areas[ei], bboxes[ej], areas[ej]);
                            // Maintain i<j orientation
                            let (id1, id2, idx1, idx2) = if py_ids[ei] < py_ids[ej] {
                                (py_ids[ei], py_ids[ej], ei, ej)
                            } else {
                                (py_ids[ej], py_ids[ei], ej, ei)
                            };
                            best_heap.push(BestEntry::Pair(GroupHeapEntry {
                                skip_isany: false,
                                dist: d,
                                id1,
                                id2,
                                elem1_idx: idx1,
                                elem2_idx: idx2,
                            }));
                        }
                    }
                }
            }
            PairMode::GroupOther {
                group_id,
                group_idx,
            } => {
                // GroupOther: emit (group_id, other_id) in that order - NO min/max
                // node_a is the "group" side, node_b is "other" side
                if !dynamic_tree.contains_elem(entry.node_a, group_idx) {
                    return;
                }
                if done[group_idx] {
                    return;
                }
                for &ej in &node_b.element_indices {
                    if ej == group_idx || done[ej] {
                        continue;
                    }
                    let d = dist_key_from_geom(
                        bboxes[group_idx],
                        areas[group_idx],
                        bboxes[ej],
                        areas[ej],
                    );
                    best_heap.push(BestEntry::Pair(GroupHeapEntry {
                        skip_isany: false,
                        dist: d,
                        id1: group_id,
                        id2: py_ids[ej],
                        elem1_idx: group_idx,
                        elem2_idx: ej,
                    }));
                }
            }
        }
    } else {
        // Split the larger node and re-enqueue
        let (split_node_idx, other_node_idx, split_is_a) =
            if node_a.element_count() >= node_b.element_count() && !node_a.is_leaf() {
                (entry.node_a, entry.node_b, true)
            } else if !node_b.is_leaf() {
                (entry.node_b, entry.node_a, false)
            } else {
                // node_a is larger but is leaf, node_b is not leaf
                (entry.node_b, entry.node_a, false)
            };

        let split_node = &nodes[split_node_idx];
        let other_node = &nodes[other_node_idx];

        if let (Some(left), Some(right)) = (split_node.left_child, split_node.right_child) {
            let left_node = &nodes[left];
            let right_node = &nodes[right];

            match entry.mode {
                PairMode::InitialIJ => {
                    if split_node_idx == other_node_idx {
                        // Self-pair split: push (left, left), (right, right), (left, right)
                        if let Some(e) = FrontierEntry::new_initial(
                            calc_dist_lower_bound(&left_node.stats, &left_node.stats),
                            &left_node.stats,
                            &left_node.stats,
                            left,
                            left,
                        ) {
                            best_heap.push(BestEntry::Frontier(e));
                        }
                        if let Some(e) = FrontierEntry::new_initial(
                            calc_dist_lower_bound(&right_node.stats, &right_node.stats),
                            &right_node.stats,
                            &right_node.stats,
                            right,
                            right,
                        ) {
                            best_heap.push(BestEntry::Frontier(e));
                        }
                        if let Some(e) = FrontierEntry::new_initial(
                            calc_dist_lower_bound(&left_node.stats, &right_node.stats),
                            &left_node.stats,
                            &right_node.stats,
                            left,
                            right,
                        ) {
                            best_heap.push(BestEntry::Frontier(e));
                        }
                    } else {
                        // Cross-pair split: push child pairs with other node
                        for &child_idx in &[left, right] {
                            let child_node = &nodes[child_idx];
                            let lb = calc_dist_lower_bound(&child_node.stats, &other_node.stats);
                            if let Some(e) = FrontierEntry::new_initial(
                                lb,
                                &child_node.stats,
                                &other_node.stats,
                                child_idx,
                                other_node_idx,
                            ) {
                                best_heap.push(BestEntry::Frontier(e));
                            }
                        }
                    }
                }
                PairMode::GroupOther {
                    group_id,
                    group_idx,
                } => {
                    // GroupOther split: maintain mode
                    if split_is_a {
                        // Only keep the child that contains group_idx
                        let group_child = if dynamic_tree.contains_elem(left, group_idx) {
                            left
                        } else {
                            right
                        };
                        let lb =
                            calc_dist_lower_bound(&nodes[group_child].stats, &other_node.stats);
                        let e = FrontierEntry::new_group_other(
                            lb,
                            group_id,
                            group_idx,
                            &other_node.stats,
                            group_child,
                            other_node_idx,
                        );
                        best_heap.push(BestEntry::Frontier(e));
                    } else {
                        for &child_idx in &[left, right] {
                            let child_node = &nodes[child_idx];
                            let lb = calc_dist_lower_bound(
                                &nodes[entry.node_a].stats,
                                &child_node.stats,
                            );
                            let e = FrontierEntry::new_group_other(
                                lb,
                                group_id,
                                group_idx,
                                &child_node.stats,
                                entry.node_a,
                                child_idx,
                            );
                            best_heap.push(BestEntry::Frontier(e));
                        }
                    }
                }
            }
        }
    }
}
