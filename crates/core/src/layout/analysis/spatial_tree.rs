//! Spatial tree data structures for efficient distance computation.
//!
//! Contains SpatialNode and DynamicSpatialTree for lazy pair generation
//! in the exact pdfminer-compatible grouping algorithm.

use std::collections::BinaryHeap;

use crate::utils::{HasBBox, Rect};

// ============================================================================
// Types for exact pdfminer-compatible grouping algorithm
// ============================================================================

/// Monotonically increasing ID assigned in parse order (matches pdfminer's id() semantics)
pub type PyId = u64;
pub type DistKey = i64;

/// Tracks how pairs are oriented for correct pdfminer semantics
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairMode {
    /// Initial elements: pairs are (i, j) where i < j by py_id
    InitialIJ,
    /// Merged group: pairs are (group_id, other_id) in that order
    GroupOther { group_id: PyId, group_idx: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeKind {
    Initial,
    Dynamic,
}

#[derive(Clone, Copy, Debug)]
pub struct PlaneElem {
    pub bbox: Rect,
    pub idx: usize,
}

impl HasBBox for PlaneElem {
    fn x0(&self) -> f64 {
        self.bbox.0
    }

    fn y0(&self) -> f64 {
        self.bbox.1
    }

    fn x1(&self) -> f64 {
        self.bbox.2
    }

    fn y1(&self) -> f64 {
        self.bbox.3
    }
}

/// Heap entry for group_textboxes_exact algorithm.
/// Ordering matches pdfminer: (skip_isany, dist, id1, id2) lexicographic.
/// BinaryHeap is max-heap, so we reverse comparison to get min-heap behavior.
#[derive(Clone, Debug)]
pub struct GroupHeapEntry {
    pub skip_isany: bool,
    pub dist: DistKey,
    pub id1: PyId,
    pub id2: PyId,
    pub elem1_idx: usize,
    pub elem2_idx: usize,
}

impl PartialEq for GroupHeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.skip_isany == other.skip_isany
            && self.dist == other.dist
            && self.id1 == other.id1
            && self.id2 == other.id2
    }
}
impl Eq for GroupHeapEntry {}

impl PartialOrd for GroupHeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GroupHeapEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse for min-heap: smaller values should be "greater" so they pop first
        // pdfminer order: (skip_isany, dist, id1, id2) - False < True, smaller < larger
        other
            .skip_isany
            .cmp(&self.skip_isany)
            .then(other.dist.cmp(&self.dist))
            .then(other.id1.cmp(&self.id1))
            .then(other.id2.cmp(&self.id2))
    }
}

/// Frontier heap entry for lazy pair generation.
/// Lower bound key: (lb_dist, lb_id1, lb_id2) - skip_isany always false for frontier.
/// mode determines orientation semantics for lb_id1/lb_id2.
#[derive(Clone, Debug)]
pub struct FrontierEntry {
    pub lb_dist: DistKey,
    pub lb_id1: PyId,
    pub lb_id2: PyId,
    pub node_a: usize,
    pub node_b: usize,
    pub mode: PairMode,
    pub tree: TreeKind,
}

impl PartialEq for FrontierEntry {
    fn eq(&self, other: &Self) -> bool {
        self.lb_dist == other.lb_dist && self.lb_id1 == other.lb_id1 && self.lb_id2 == other.lb_id2
    }
}
impl Eq for FrontierEntry {}

impl PartialOrd for FrontierEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FrontierEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse for min-heap
        other
            .lb_dist
            .cmp(&self.lb_dist)
            .then(other.lb_id1.cmp(&self.lb_id1))
            .then(other.lb_id2.cmp(&self.lb_id2))
    }
}

#[derive(Clone, Debug)]
pub enum BestEntry {
    Frontier(FrontierEntry),
    Pair(GroupHeapEntry),
}

impl BestEntry {
    const fn key_parts(&self) -> (bool, DistKey, PyId, PyId, u8) {
        match self {
            Self::Frontier(entry) => (false, entry.lb_dist, entry.lb_id1, entry.lb_id2, 0),
            Self::Pair(entry) => (entry.skip_isany, entry.dist, entry.id1, entry.id2, 1),
        }
    }
}

impl PartialEq for BestEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key_parts() == other.key_parts()
    }
}
impl Eq for BestEntry {}

impl PartialOrd for BestEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BestEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let (skip_a, dist_a, id1_a, id2_a, kind_a) = self.key_parts();
        let (skip_b, dist_b, id1_b, id2_b, kind_b) = other.key_parts();
        // Reverse for min-heap: smaller values should be "greater" so they pop first
        skip_b
            .cmp(&skip_a)
            .then(dist_b.cmp(&dist_a))
            .then(id1_b.cmp(&id1_a))
            .then(id2_b.cmp(&id2_a))
            .then(kind_b.cmp(&kind_a))
    }
}

/// Cached statistics for a spatial tree node.
/// Used for computing lower bounds on distances between node pairs.
#[derive(Clone, Debug)]
pub struct NodeStats {
    pub bbox: Rect,
    pub min_w: f64,
    pub min_h: f64,
    pub max_area: f64,
    pub min_py_id: PyId,
    pub second_min_py_id: PyId,
}

#[inline(always)]
pub const fn f64_total_key(x: f64) -> DistKey {
    let mut bits = x.to_bits() as i64;
    bits ^= (((bits >> 63) as u64) >> 1) as i64;
    bits
}

#[inline(always)]
pub fn dist_key_from_geom(a: Rect, area_a: f64, b: Rect, area_b: f64) -> DistKey {
    let x0 = a.0.min(b.0);
    let y0 = a.1.min(b.1);
    let x1 = a.2.max(b.2);
    let y1 = a.3.max(b.3);
    f64_total_key((x1 - x0).mul_add(y1 - y0, -area_a) - area_b)
}

impl NodeStats {
    /// Create stats for a single element
    pub fn from_bbox_and_id(bbox: Rect, py_id: PyId) -> Self {
        let w = bbox.2 - bbox.0;
        let h = bbox.3 - bbox.1;
        Self {
            bbox,
            min_w: w,
            min_h: h,
            max_area: w * h,
            min_py_id: py_id,
            second_min_py_id: PyId::MAX,
        }
    }

    /// Merge two node stats
    pub fn merge(&self, other: &Self) -> Self {
        // Track the two smallest py_ids across both nodes
        let mut ids = [
            self.min_py_id,
            self.second_min_py_id,
            other.min_py_id,
            other.second_min_py_id,
        ];
        ids.sort();

        Self {
            bbox: (
                self.bbox.0.min(other.bbox.0),
                self.bbox.1.min(other.bbox.1),
                self.bbox.2.max(other.bbox.2),
                self.bbox.3.max(other.bbox.3),
            ),
            min_w: self.min_w.min(other.min_w),
            min_h: self.min_h.min(other.min_h),
            max_area: self.max_area.max(other.max_area),
            min_py_id: ids[0],
            second_min_py_id: ids[1],
        }
    }
}

/// Lightweight node for frontier expansion.
/// Does NOT replace Plane - only used for computing lower bounds.
#[derive(Clone, Debug)]
pub struct SpatialNode {
    pub stats: NodeStats,
    pub count: usize,
    pub element_indices: Vec<usize>,
    pub left_child: Option<usize>,
    pub right_child: Option<usize>,
}

/// Leaf threshold for spatial tree
const LEAF_THRESHOLD: usize = 8;

impl SpatialNode {
    /// Build a tree from bboxes and py_ids, returning root index
    pub fn build(elements: &[(Rect, PyId)], arena: &mut Vec<Self>) -> usize {
        Self::build_range(elements, (0..elements.len()).collect(), arena)
    }

    fn build_range(
        elements: &[(Rect, PyId)],
        indices: Vec<usize>,
        arena: &mut Vec<Self>,
    ) -> usize {
        let count = indices.len();
        let stats = indices
            .iter()
            .map(|&i| NodeStats::from_bbox_and_id(elements[i].0, elements[i].1))
            .reduce(|a, b| a.merge(&b))
            .unwrap();

        let node_idx = arena.len();
        if count <= LEAF_THRESHOLD {
            arena.push(Self {
                stats,
                count,
                element_indices: indices,
                left_child: None,
                right_child: None,
            });
            return node_idx;
        }

        arena.push(Self {
            stats,
            count,
            element_indices: Vec::new(),
            left_child: None,
            right_child: None,
        });

        // Split if > threshold
        if count > LEAF_THRESHOLD {
            let (x0, y0, x1, y1) = arena[node_idx].stats.bbox;
            let width = x1 - x0;
            let height = y1 - y0;

            // Sort by center along longer axis
            let mut sorted = indices;
            if width >= height {
                sorted.sort_by(|&a, &b| {
                    let ca = (elements[a].0.0 + elements[a].0.2) / 2.0;
                    let cb = (elements[b].0.0 + elements[b].0.2) / 2.0;
                    ca.partial_cmp(&cb).unwrap()
                });
            } else {
                sorted.sort_by(|&a, &b| {
                    let ca = (elements[a].0.1 + elements[a].0.3) / 2.0;
                    let cb = (elements[b].0.1 + elements[b].0.3) / 2.0;
                    ca.partial_cmp(&cb).unwrap()
                });
            }

            let mid = sorted.len() / 2;
            let left_indices = sorted[..mid].to_vec();
            let right_indices = sorted[mid..].to_vec();

            let left_idx = Self::build_range(elements, left_indices, arena);
            let right_idx = Self::build_range(elements, right_indices, arena);

            arena[node_idx].left_child = Some(left_idx);
            arena[node_idx].right_child = Some(right_idx);
            arena[node_idx].count = arena[left_idx].count + arena[right_idx].count;
        }

        node_idx
    }

    pub const fn is_leaf(&self) -> bool {
        self.left_child.is_none()
    }

    pub const fn element_count(&self) -> usize {
        self.count
    }
}

pub struct DynamicSpatialTree {
    pub nodes: Vec<SpatialNode>,
    parents: Vec<Option<usize>>,
    pub root: usize,
    elem_leaf: Vec<usize>,
}

impl DynamicSpatialTree {
    pub fn build(elements: &[(Rect, PyId)]) -> Self {
        let mut nodes = Vec::new();
        let mut parents = Vec::new();
        let root = Self::build_range(
            elements,
            (0..elements.len()).collect(),
            &mut nodes,
            &mut parents,
            None,
        );
        let mut tree = Self {
            nodes,
            parents,
            root,
            elem_leaf: vec![0; elements.len()],
        };
        tree.rebuild_elem_leaf();
        tree
    }

    fn build_range(
        elements: &[(Rect, PyId)],
        indices: Vec<usize>,
        nodes: &mut Vec<SpatialNode>,
        parents: &mut Vec<Option<usize>>,
        parent: Option<usize>,
    ) -> usize {
        let count = indices.len();
        let stats = indices
            .iter()
            .map(|&i| NodeStats::from_bbox_and_id(elements[i].0, elements[i].1))
            .reduce(|a, b| a.merge(&b))
            .unwrap();

        let node_idx = nodes.len();
        if count <= LEAF_THRESHOLD {
            nodes.push(SpatialNode {
                stats,
                count,
                element_indices: indices,
                left_child: None,
                right_child: None,
            });
            parents.push(parent);
            return node_idx;
        }

        nodes.push(SpatialNode {
            stats,
            count,
            element_indices: Vec::new(),
            left_child: None,
            right_child: None,
        });
        parents.push(parent);

        if count > LEAF_THRESHOLD {
            let (x0, y0, x1, y1) = nodes[node_idx].stats.bbox;
            let width = x1 - x0;
            let height = y1 - y0;

            let mut sorted = indices;
            if width >= height {
                sorted.sort_by(|&a, &b| {
                    let ca = (elements[a].0.0 + elements[a].0.2) / 2.0;
                    let cb = (elements[b].0.0 + elements[b].0.2) / 2.0;
                    ca.partial_cmp(&cb).unwrap()
                });
            } else {
                sorted.sort_by(|&a, &b| {
                    let ca = (elements[a].0.1 + elements[a].0.3) / 2.0;
                    let cb = (elements[b].0.1 + elements[b].0.3) / 2.0;
                    ca.partial_cmp(&cb).unwrap()
                });
            }

            let mid = sorted.len() / 2;
            let left_indices = sorted[..mid].to_vec();
            let right_indices = sorted[mid..].to_vec();

            let left_idx =
                Self::build_range(elements, left_indices, nodes, parents, Some(node_idx));
            let right_idx =
                Self::build_range(elements, right_indices, nodes, parents, Some(node_idx));

            nodes[node_idx].left_child = Some(left_idx);
            nodes[node_idx].right_child = Some(right_idx);
            let merged = nodes[left_idx].stats.merge(&nodes[right_idx].stats);
            nodes[node_idx].stats = merged;
            nodes[node_idx].count = nodes[left_idx].count + nodes[right_idx].count;
        }

        node_idx
    }

    pub fn insert(
        &mut self,
        elem_idx: usize,
        bbox: Rect,
        py_id: PyId,
        elements: &[(Rect, PyId)],
    ) -> usize {
        if self.nodes.is_empty() {
            let stats = NodeStats::from_bbox_and_id(bbox, py_id);
            self.nodes.push(SpatialNode {
                stats,
                count: 1,
                element_indices: vec![elem_idx],
                left_child: None,
                right_child: None,
            });
            self.parents.push(None);
            self.root = 0;
            if elem_idx >= self.elem_leaf.len() {
                self.elem_leaf.push(0);
            } else {
                self.elem_leaf[elem_idx] = 0;
            }
            return 0;
        }

        let leaf = self.choose_leaf(self.root, bbox);
        self.nodes[leaf].element_indices.push(elem_idx);
        self.nodes[leaf].count = self.nodes[leaf].element_indices.len();
        self.nodes[leaf].stats = self.nodes[leaf]
            .stats
            .merge(&NodeStats::from_bbox_and_id(bbox, py_id));

        let mut leaf_idx = leaf;
        if self.nodes[leaf].element_indices.len() > LEAF_THRESHOLD {
            leaf_idx = self.split_leaf(leaf, elem_idx, elements);
        }

        if elem_idx >= self.elem_leaf.len() {
            self.elem_leaf.push(leaf_idx);
        } else {
            self.elem_leaf[elem_idx] = leaf_idx;
        }

        let parent = self.parents[leaf_idx];
        self.update_ancestors(parent);
        leaf_idx
    }

    fn choose_leaf(&self, node_idx: usize, bbox: Rect) -> usize {
        let node = &self.nodes[node_idx];
        if node.is_leaf() {
            return node_idx;
        }
        let left = node.left_child.unwrap();
        let right = node.right_child.unwrap();
        let left_bbox = self.nodes[left].stats.bbox;
        let right_bbox = self.nodes[right].stats.bbox;

        let left_expand = bbox_expand_area(left_bbox, bbox);
        let right_expand = bbox_expand_area(right_bbox, bbox);

        if left_expand < right_expand {
            self.choose_leaf(left, bbox)
        } else if right_expand < left_expand {
            self.choose_leaf(right, bbox)
        } else {
            let left_area = bbox_area(left_bbox);
            let right_area = bbox_area(right_bbox);
            if left_area <= right_area {
                self.choose_leaf(left, bbox)
            } else {
                self.choose_leaf(right, bbox)
            }
        }
    }

    fn split_leaf(&mut self, node_idx: usize, elem_idx: usize, elements: &[(Rect, PyId)]) -> usize {
        let indices = std::mem::take(&mut self.nodes[node_idx].element_indices);
        let (x0, y0, x1, y1) = self.nodes[node_idx].stats.bbox;
        let width = x1 - x0;
        let height = y1 - y0;

        let mut sorted = indices;
        if width >= height {
            sorted.sort_by(|&a, &b| {
                let ca = (elements[a].0.0 + elements[a].0.2) / 2.0;
                let cb = (elements[b].0.0 + elements[b].0.2) / 2.0;
                ca.partial_cmp(&cb).unwrap()
            });
        } else {
            sorted.sort_by(|&a, &b| {
                let ca = (elements[a].0.1 + elements[a].0.3) / 2.0;
                let cb = (elements[b].0.1 + elements[b].0.3) / 2.0;
                ca.partial_cmp(&cb).unwrap()
            });
        }

        let mid = sorted.len() / 2;
        let right_indices = sorted.split_off(mid);
        let left_indices = sorted;

        let left_stats = left_indices
            .iter()
            .map(|&i| NodeStats::from_bbox_and_id(elements[i].0, elements[i].1))
            .reduce(|a, b| a.merge(&b))
            .unwrap();
        let right_stats = right_indices
            .iter()
            .map(|&i| NodeStats::from_bbox_and_id(elements[i].0, elements[i].1))
            .reduce(|a, b| a.merge(&b))
            .unwrap();

        let left_idx = self.nodes.len();
        self.nodes.push(SpatialNode {
            stats: left_stats,
            count: left_indices.len(),
            element_indices: left_indices,
            left_child: None,
            right_child: None,
        });
        self.parents.push(Some(node_idx));

        let right_idx = self.nodes.len();
        self.nodes.push(SpatialNode {
            stats: right_stats,
            count: right_indices.len(),
            element_indices: right_indices,
            left_child: None,
            right_child: None,
        });
        self.parents.push(Some(node_idx));

        self.nodes[node_idx].left_child = Some(left_idx);
        self.nodes[node_idx].right_child = Some(right_idx);
        let merged = self.nodes[left_idx]
            .stats
            .merge(&self.nodes[right_idx].stats);
        self.nodes[node_idx].stats = merged;
        self.nodes[node_idx].count = self.nodes[left_idx].count + self.nodes[right_idx].count;

        for &idx in &self.nodes[left_idx].element_indices {
            if idx >= self.elem_leaf.len() {
                self.elem_leaf.push(left_idx);
            } else {
                self.elem_leaf[idx] = left_idx;
            }
        }
        for &idx in &self.nodes[right_idx].element_indices {
            if idx >= self.elem_leaf.len() {
                self.elem_leaf.push(right_idx);
            } else {
                self.elem_leaf[idx] = right_idx;
            }
        }

        if self.nodes[left_idx].element_indices.contains(&elem_idx) {
            left_idx
        } else {
            right_idx
        }
    }

    fn update_ancestors(&mut self, mut node_idx: Option<usize>) {
        while let Some(idx) = node_idx {
            let left = self.nodes[idx].left_child.unwrap();
            let right = self.nodes[idx].right_child.unwrap();
            let merged = self.nodes[left].stats.merge(&self.nodes[right].stats);
            self.nodes[idx].stats = merged;
            self.nodes[idx].count = self.nodes[left].count + self.nodes[right].count;
            node_idx = self.parents[idx];
        }
    }

    fn rebuild_elem_leaf(&mut self) {
        for (node_idx, node) in self.nodes.iter().enumerate() {
            if node.is_leaf() {
                for &elem in &node.element_indices {
                    if elem < self.elem_leaf.len() {
                        self.elem_leaf[elem] = node_idx;
                    }
                }
            }
        }
    }

    pub fn contains_elem(&self, node_idx: usize, elem_idx: usize) -> bool {
        if elem_idx >= self.elem_leaf.len() {
            return false;
        }
        let mut cur = self.elem_leaf[elem_idx];
        loop {
            if cur == node_idx {
                return true;
            }
            match self.parents[cur] {
                Some(parent) => cur = parent,
                None => return false,
            }
        }
    }
}

fn bbox_area(bbox: Rect) -> f64 {
    let w = bbox.2 - bbox.0;
    let h = bbox.3 - bbox.1;
    w * h
}

const fn bbox_union(a: Rect, b: Rect) -> Rect {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

fn bbox_expand_area(current: Rect, add: Rect) -> f64 {
    let union = bbox_union(current, add);
    bbox_area(union) - bbox_area(current)
}

/// Calculate lower bound on dist() for any pair between two nodes.
/// Uses TIGHTER geometric bound: max(min_w) not min(min_w).
pub fn calc_dist_lower_bound(a: &NodeStats, b: &NodeStats) -> DistKey {
    // Gap between bounding boxes
    let gap_x = (a.bbox.0.max(b.bbox.0) - a.bbox.2.min(b.bbox.2)).max(0.0);
    let gap_y = (a.bbox.1.max(b.bbox.1) - a.bbox.3.min(b.bbox.3)).max(0.0);

    // TIGHTER bound: use max(min_w), max(min_h) - the minimum union bbox
    // must span at least the larger of the two smallest elements
    let w_lb = gap_x + a.min_w.max(b.min_w);
    let h_lb = gap_y + a.min_h.max(b.min_h);

    // Geometric lower bound: min_union_area - max_area_a - max_area_b
    let geometric_lb = w_lb * h_lb - a.max_area - b.max_area;

    // Clamp: dist(a,b) >= -min(area(a), area(b))
    let clamped = geometric_lb.max(-a.max_area.min(b.max_area));

    f64_total_key(clamped)
}

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

/// Expand a frontier entry - either emit concrete pairs or split and re-enqueue
pub fn expand_frontier(
    entry: FrontierEntry,
    initial_nodes: &[SpatialNode],
    dynamic_tree: &DynamicSpatialTree,
    bboxes: &[Rect],
    areas: &[f64],
    py_ids: &[PyId],
    done: &[bool],
    main_heap: &mut BinaryHeap<GroupHeapEntry>,
    frontier: &mut BinaryHeap<FrontierEntry>,
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
                            // py_ids equal indices for initial elements, so i<j means py_ids[i]<py_ids[j]
                            main_heap.push(GroupHeapEntry {
                                skip_isany: false,
                                dist: d,
                                id1: py_ids[ei],
                                id2: py_ids[ej],
                                elem1_idx: ei,
                                elem2_idx: ej,
                            });
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
                            main_heap.push(GroupHeapEntry {
                                skip_isany: false,
                                dist: d,
                                id1,
                                id2,
                                elem1_idx: idx1,
                                elem2_idx: idx2,
                            });
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
                    main_heap.push(GroupHeapEntry {
                        skip_isany: false,
                        dist: d,
                        id1: group_id,
                        id2: py_ids[ej],
                        elem1_idx: group_idx,
                        elem2_idx: ej,
                    });
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
                        // (left, left)
                        if let Some(e) = FrontierEntry::new_initial(
                            calc_dist_lower_bound(&left_node.stats, &left_node.stats),
                            &left_node.stats,
                            &left_node.stats,
                            left,
                            left,
                        ) {
                            frontier.push(e);
                        }
                        // (right, right)
                        if let Some(e) = FrontierEntry::new_initial(
                            calc_dist_lower_bound(&right_node.stats, &right_node.stats),
                            &right_node.stats,
                            &right_node.stats,
                            right,
                            right,
                        ) {
                            frontier.push(e);
                        }
                        // (left, right)
                        if let Some(e) = FrontierEntry::new_initial(
                            calc_dist_lower_bound(&left_node.stats, &right_node.stats),
                            &left_node.stats,
                            &right_node.stats,
                            left,
                            right,
                        ) {
                            frontier.push(e);
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
                                frontier.push(e);
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
                        frontier.push(e);
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
                            frontier.push(e);
                        }
                    }
                }
            }
        }
    }
}

/// Expand a frontier entry for the single-heap best-first algorithm.
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

#[cfg(test)]
mod tests {
    use super::{dist_key_from_geom, f64_total_key};

    #[test]
    fn test_f64_total_key_matches_total_cmp() {
        let vals = [
            f64::NEG_INFINITY,
            -1.0,
            -0.0,
            0.0,
            1.0,
            f64::INFINITY,
            f64::NAN,
        ];
        for &a in &vals {
            for &b in &vals {
                assert_eq!(f64_total_key(a).cmp(&f64_total_key(b)), a.total_cmp(&b));
            }
        }
    }

    #[test]
    fn test_dist_key_from_geom_matches_manual_formula() {
        let a = (0.0, 0.0, 10.0, 10.0);
        let b = (20.0, 0.0, 30.0, 10.0);
        let area_a = 100.0;
        let area_b = 100.0;
        let expected = f64_total_key((30.0 - 0.0) * (10.0 - 0.0) - area_a - area_b);
        assert_eq!(dist_key_from_geom(a, area_a, b, area_b), expected);
    }
}
