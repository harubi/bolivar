//! Layout Analysis Module - grouping and clustering algorithms
//!
//! Contains the layout analysis algorithms for:
//! - Grouping characters into text lines
//! - Grouping text lines into text boxes
//! - Hierarchical grouping of text boxes
//! - Exact pdfminer-compatible grouping

use std::collections::{BinaryHeap, binary_heap::PeekMut};

use crate::utils::{HasBBox, INF_F64, Plane, Rect, fsplit, uniq};

use super::elements::{
    IndexAssigner, LTAnno, LTChar, LTFigure, LTItem, LTLayoutContainer, LTPage, LTTextBox,
    LTTextBoxHorizontal, LTTextBoxVertical, LTTextGroup, LTTextLineHorizontal, LTTextLineVertical,
    TextBoxType, TextGroupElement, TextLineElement, TextLineType,
};
use super::params::LAParams;

// ============================================================================
// Types for exact pdfminer-compatible grouping algorithm
// ============================================================================

/// Monotonically increasing ID assigned in parse order (matches pdfminer's id() semantics)
pub type PyId = u64;
type DistKey = i64;

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
struct PlaneElem {
    bbox: Rect,
    idx: usize,
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
fn f64_total_key(x: f64) -> DistKey {
    let mut bits = x.to_bits() as i64;
    bits ^= (((bits >> 63) as u64) >> 1) as i64;
    bits
}

#[inline(always)]
fn dist_key_from_geom(a: Rect, area_a: f64, b: Rect, area_b: f64) -> DistKey {
    let x0 = a.0.min(b.0);
    let y0 = a.1.min(b.1);
    let x1 = a.2.max(b.2);
    let y1 = a.3.max(b.3);
    f64_total_key((x1 - x0) * (y1 - y0) - area_a - area_b)
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
    pub fn build(elements: &[(Rect, PyId)], arena: &mut Vec<SpatialNode>) -> usize {
        Self::build_range(elements, (0..elements.len()).collect(), arena)
    }

    fn build_range(
        elements: &[(Rect, PyId)],
        indices: Vec<usize>,
        arena: &mut Vec<SpatialNode>,
    ) -> usize {
        let count = indices.len();
        let stats = indices
            .iter()
            .map(|&i| NodeStats::from_bbox_and_id(elements[i].0, elements[i].1))
            .reduce(|a, b| a.merge(&b))
            .unwrap();

        let node_idx = arena.len();
        if count <= LEAF_THRESHOLD {
            arena.push(SpatialNode {
                stats,
                count,
                element_indices: indices,
                left_child: None,
                right_child: None,
            });
            return node_idx;
        }

        arena.push(SpatialNode {
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

    pub fn is_leaf(&self) -> bool {
        self.left_child.is_none()
    }

    pub fn element_count(&self) -> usize {
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

fn bbox_union(a: Rect, b: Rect) -> Rect {
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
    pub fn new_initial(
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
    pub fn new_group_other(
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

// ============================================================================
// Layout Analysis Implementation for LTLayoutContainer
// ============================================================================

impl LTLayoutContainer {
    /// Groups character objects into text lines.
    ///
    /// This is the core character-to-line grouping algorithm from pdfminer.
    /// It groups LTChar objects based on horizontal/vertical alignment and proximity.
    ///
    /// # Algorithm (Python lines 702-777)
    /// - For each pair of consecutive characters, check if they are:
    ///   - horizontally aligned (halign): on same line, close enough horizontally
    ///   - vertically aligned (valign): on same column, close enough vertically
    /// - Group characters into horizontal or vertical text lines accordingly
    pub fn group_objects(&self, laparams: &LAParams, objs: &[LTChar]) -> Vec<TextLineType> {
        let mut result = Vec::new();
        if objs.is_empty() {
            return result;
        }

        let mut obj_iter = objs.iter().peekable();
        let mut current_line: Option<TextLineType> = None;

        // Get first object
        let mut obj0 = match obj_iter.next() {
            Some(o) => o,
            None => return result,
        };

        for obj1 in obj_iter {
            // Check horizontal alignment:
            //   +------+ - - -
            //   | obj0 | - - +------+   -
            //   |      |     | obj1 |   | (line_overlap)
            //   +------+ - - |      |   -
            //          - - - +------+
            //          |<--->|
            //        (char_margin)
            let halign = obj0.is_voverlap(obj1)
                && obj0.height().min(obj1.height()) * laparams.line_overlap < obj0.voverlap(obj1)
                && obj0.hdistance(obj1) < obj0.width().max(obj1.width()) * laparams.char_margin;

            // Check vertical alignment:
            //   +------+
            //   | obj0 |
            //   |      |
            //   +------+ - - -
            //     |    |     | (char_margin)
            //     +------+ - -
            //     | obj1 |
            //     |      |
            //     +------+
            //     |<-->|
            //   (line_overlap)
            let valign = laparams.detect_vertical
                && obj0.is_hoverlap(obj1)
                && obj0.width().min(obj1.width()) * laparams.line_overlap < obj0.hoverlap(obj1)
                && obj0.vdistance(obj1) < obj0.height().max(obj1.height()) * laparams.char_margin;

            match &mut current_line {
                Some(TextLineType::Horizontal(line)) if halign => {
                    // Continue horizontal line
                    Self::add_char_to_horizontal_line(line, obj1.clone(), laparams.word_margin);
                }
                Some(TextLineType::Vertical(line)) if valign => {
                    // Continue vertical line
                    Self::add_char_to_vertical_line(line, obj1.clone(), laparams.word_margin);
                }
                Some(line) => {
                    // End current line (obj0 was already added to it)
                    line.analyze();
                    result.push(line.clone());
                    current_line = None;
                    // Don't create single-char line from obj0 - it's already in current_line
                    // Just continue to next iteration where obj1 becomes obj0
                }
                None => {
                    if valign && !halign {
                        // Start new vertical line
                        let mut line = LTTextLineVertical::new(laparams.word_margin);
                        Self::add_char_to_vertical_line(
                            &mut line,
                            obj0.clone(),
                            laparams.word_margin,
                        );
                        Self::add_char_to_vertical_line(
                            &mut line,
                            obj1.clone(),
                            laparams.word_margin,
                        );
                        current_line = Some(TextLineType::Vertical(line));
                    } else if halign && !valign {
                        // Start new horizontal line
                        let mut line = LTTextLineHorizontal::new(laparams.word_margin);
                        Self::add_char_to_horizontal_line(
                            &mut line,
                            obj0.clone(),
                            laparams.word_margin,
                        );
                        Self::add_char_to_horizontal_line(
                            &mut line,
                            obj1.clone(),
                            laparams.word_margin,
                        );
                        current_line = Some(TextLineType::Horizontal(line));
                    } else {
                        // Neither aligned - output single-char line
                        let mut line = LTTextLineHorizontal::new(laparams.word_margin);
                        Self::add_char_to_horizontal_line(
                            &mut line,
                            obj0.clone(),
                            laparams.word_margin,
                        );
                        line.analyze();
                        result.push(TextLineType::Horizontal(line));
                    }
                }
            }

            obj0 = obj1;
        }

        // Handle remaining line or last character
        match current_line {
            Some(mut line) => {
                line.analyze();
                result.push(line);
            }
            None => {
                // Last character wasn't part of a line
                let mut line = LTTextLineHorizontal::new(laparams.word_margin);
                Self::add_char_to_horizontal_line(&mut line, obj0.clone(), laparams.word_margin);
                line.analyze();
                result.push(TextLineType::Horizontal(line));
            }
        }

        result
    }

    /// Helper to add a character to a horizontal line, inserting word spaces as needed.
    fn add_char_to_horizontal_line(line: &mut LTTextLineHorizontal, ch: LTChar, word_margin: f64) {
        let margin = word_margin * ch.width().max(ch.height());
        if line.x1_tracker < ch.x0() - margin && line.x1_tracker != INF_F64 {
            line.elements.push(TextLineElement::Anno(LTAnno::new(" ")));
        }
        line.x1_tracker = ch.x1();

        // Expand bounding box
        line.component.x0 = line.component.x0.min(ch.x0());
        line.component.y0 = line.component.y0.min(ch.y0());
        line.component.x1 = line.component.x1.max(ch.x1());
        line.component.y1 = line.component.y1.max(ch.y1());

        line.elements.push(TextLineElement::Char(ch));
    }

    /// Helper to add a character to a vertical line, inserting word spaces as needed.
    fn add_char_to_vertical_line(line: &mut LTTextLineVertical, ch: LTChar, word_margin: f64) {
        let margin = word_margin * ch.width().max(ch.height());
        if ch.y1() + margin < line.y0_tracker && line.y0_tracker != -INF_F64 {
            line.elements.push(TextLineElement::Anno(LTAnno::new(" ")));
        }
        line.y0_tracker = ch.y0();

        // Expand bounding box
        line.component.x0 = line.component.x0.min(ch.x0());
        line.component.y0 = line.component.y0.min(ch.y0());
        line.component.x1 = line.component.x1.max(ch.x1());
        line.component.y1 = line.component.y1.max(ch.y1());

        line.elements.push(TextLineElement::Char(ch));
    }

    /// Groups text lines into text boxes based on neighbor relationships.
    pub fn group_textlines(
        &self,
        laparams: &LAParams,
        lines: Vec<TextLineType>,
    ) -> Vec<TextBoxType> {
        if lines.is_empty() {
            return Vec::new();
        }

        // Compute bounding box that covers all lines (may be outside container bbox)
        let mut min_x0 = INF_F64;
        let mut min_y0 = INF_F64;
        let mut max_x1 = -INF_F64;
        let mut max_y1 = -INF_F64;

        for line in &lines {
            min_x0 = min_x0.min(line.x0());
            min_y0 = min_y0.min(line.y0());
            max_x1 = max_x1.max(line.x1());
            max_y1 = max_y1.max(line.y1());
        }

        // Create plane with expanded bbox
        let plane_bbox = (min_x0 - 1.0, min_y0 - 1.0, max_x1 + 1.0, max_y1 + 1.0);
        let mut plane: Plane<TextLineType> = Plane::new(plane_bbox, 1);

        // Add lines to plane (keep original lines with elements intact)
        for line in &lines {
            plane.add(line.clone());
        }
        let line_types = lines;

        // Group lines into boxes - MUST match Python's exact logic:
        // Python: boxes: Dict[LTTextLine, LTTextBox] = {}
        // Each line maps to its current box. When merging, ALL lines from
        // existing boxes are added to the new box.

        // line_to_box_id: maps line_index -> box_id (which box contains this line)
        // box_contents: maps box_id -> Vec<line_index> (lines in each box)
        let mut line_to_box_id: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let mut box_contents: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        let mut next_box_id: usize = 0;

        for (i, line) in line_types.iter().enumerate() {
            // Use different search strategy for horizontal vs vertical text
            let (d, search_bbox) = match line {
                TextLineType::Horizontal(_) => {
                    let d = laparams.line_margin * line.height();
                    (d, (line.x0(), line.y0() - d, line.x1(), line.y1() + d))
                }
                TextLineType::Vertical(_) => {
                    let d = laparams.line_margin * line.width();
                    (d, (line.x0() - d, line.y0(), line.x1() + d, line.y1()))
                }
            };
            // Use find_with_indices to get (seq_index, neighbor) pairs
            // Since we added lines to plane in order, seq_index == line_types index
            let neighbors = plane.find_with_indices(search_bbox);

            // Start with current line
            let mut members: Vec<usize> = vec![i];

            for (j, neighbor) in neighbors {
                // Python uses NON-STRICT comparison (<= tolerance)
                // See layout.py:543-560 - _is_left_aligned_with, _is_same_height_as, etc.
                let is_aligned = match (line, neighbor) {
                    (TextLineType::Horizontal(l1), TextLineType::Horizontal(l2)) => {
                        let tolerance = d;
                        let height_diff = (l2.height() - l1.height()).abs();
                        let same_height = height_diff <= tolerance; // Python: <=
                        let left_diff = (l2.x0() - l1.x0()).abs();
                        let left_aligned = left_diff <= tolerance; // Python: <=
                        let right_diff = (l2.x1() - l1.x1()).abs();
                        let right_aligned = right_diff <= tolerance; // Python: <=
                        let center1 = (l1.x0() + l1.x1()) / 2.0;
                        let center2 = (l2.x0() + l2.x1()) / 2.0;
                        let center_diff = (center2 - center1).abs();
                        let centrally_aligned = center_diff <= tolerance; // Python: <=
                        same_height && (left_aligned || right_aligned || centrally_aligned)
                    }
                    (TextLineType::Vertical(l1), TextLineType::Vertical(l2)) => {
                        let tolerance = d;
                        let same_width = (l2.width() - l1.width()).abs() <= tolerance; // Python: <=
                        let lower_aligned = (l2.y0() - l1.y0()).abs() <= tolerance; // Python: <=
                        let upper_aligned = (l2.y1() - l1.y1()).abs() <= tolerance; // Python: <=
                        let center1 = (l1.y0() + l1.y1()) / 2.0;
                        let center2 = (l2.y0() + l2.y1()) / 2.0;
                        let centrally_aligned = (center2 - center1).abs() <= tolerance; // Python: <=
                        same_width && (lower_aligned || upper_aligned || centrally_aligned)
                    }
                    _ => false,
                };

                if is_aligned {
                    // j is the direct index from plane, no need to search by bbox!
                    // Add neighbor to members
                    members.push(j);
                    // CRITICAL: If neighbor is already in a box, merge ALL lines from that box
                    // This matches Python's: members.extend(boxes.pop(obj1))
                    if let Some(&existing_box_id) = line_to_box_id.get(&j) {
                        if let Some(existing_members) = box_contents.remove(&existing_box_id) {
                            members.extend(existing_members);
                        }
                    }
                }
            }

            // Create new box with all members (matching Python: box = LTTextBox(); for obj in uniq(members): box.add(obj); boxes[obj] = box)
            let box_id = next_box_id;
            next_box_id += 1;

            let unique_members: Vec<usize> = uniq(members);
            for &m in &unique_members {
                line_to_box_id.insert(m, box_id);
            }
            box_contents.insert(box_id, unique_members);
        }

        // CRITICAL: Python iterates through original 'lines' in order and yields boxes
        // as their first line is encountered. We must do the same - NOT iterate the HashMap!
        let mut result: Vec<TextBoxType> = Vec::new();
        let mut done: Vec<bool> = vec![false; next_box_id];

        // Iterate through lines in ORIGINAL ORDER (like Python's "for line in lines:")
        for (i, _line) in line_types.iter().enumerate() {
            // Look up which box this line belongs to
            let box_id = match line_to_box_id.get(&i) {
                Some(&id) => id,
                None => continue,
            };

            // Skip if we've already processed this box
            if done[box_id] {
                continue;
            }
            done[box_id] = true;

            // Get all members of this box
            let member_indices = match box_contents.get(&box_id) {
                Some(members) => members,
                None => continue,
            };

            let unique_members: Vec<usize> = uniq(member_indices.clone());

            // Determine box type from first line in group
            if unique_members.is_empty() {
                continue;
            }
            let first_line = &line_types[unique_members[0]];
            let is_vertical = matches!(first_line, TextLineType::Vertical(_));

            if is_vertical {
                let mut textbox = LTTextBoxVertical::new();
                for idx in unique_members {
                    if let TextLineType::Vertical(line) = &line_types[idx] {
                        textbox.add(line.clone());
                    }
                }
                if !textbox.is_empty() {
                    result.push(TextBoxType::Vertical(textbox));
                }
            } else {
                let mut textbox = LTTextBoxHorizontal::new();
                for idx in unique_members {
                    if let TextLineType::Horizontal(line) = &line_types[idx] {
                        textbox.add(line.clone());
                    }
                }
                if !textbox.is_empty() {
                    result.push(TextBoxType::Horizontal(textbox));
                }
            }
        }

        result
    }

    /// Groups text boxes hierarchically based on spatial proximity.
    ///
    /// Uses a distance function to find the closest pairs of text boxes
    /// and merges them into groups. Uses a heap for efficient access.
    pub fn group_textboxes(&self, _laparams: &LAParams, boxes: &[TextBoxType]) -> Vec<LTTextGroup> {
        if boxes.is_empty() {
            return Vec::new();
        }

        // Distance function: area of bounding rectangle minus areas of both boxes
        fn dist(obj1: &TextGroupElement, obj2: &TextGroupElement) -> f64 {
            let x0 = obj1.x0().min(obj2.x0());
            let y0 = obj1.y0().min(obj2.y0());
            let x1 = obj1.x1().max(obj2.x1());
            let y1 = obj1.y1().max(obj2.y1());
            (x1 - x0) * (y1 - y0) - obj1.width() * obj1.height() - obj2.width() * obj2.height()
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
            let group =
                LTTextGroup::new(vec![entry.elem1.clone(), entry.elem2.clone()], is_vertical);
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

    /// Exact pdfminer-compatible grouping using a certified lazy all-pairs algorithm.
    /// Uses existing Plane.find() for isany queries. Two-heap approach: main heap
    /// holds exact (dist, id) pairs; frontier heap holds spatial node-pairs with
    /// lower-bound keys for lazy pair generation.
    pub fn group_textboxes_exact(
        &self,
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
                Self::expand_frontier(
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

    /// Expand a frontier entry - either emit concrete pairs or split and re-enqueue
    fn expand_frontier(
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
                                let d = dist_key_from_geom(
                                    bboxes[ei], areas[ei], bboxes[ej], areas[ej],
                                );
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
                                let d = dist_key_from_geom(
                                    bboxes[ei], areas[ei], bboxes[ej], areas[ej],
                                );
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
                                let lb =
                                    calc_dist_lower_bound(&child_node.stats, &other_node.stats);
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

    /// Performs layout analysis on the container's items.
    ///
    /// This is the main entry point for layout analysis. It:
    /// 1. Separates text characters from other objects
    /// 2. Groups characters into text lines
    /// 3. Groups text lines into text boxes
    /// 4. Optionally groups text boxes hierarchically (if boxes_flow is set)
    /// 5. Assigns reading order indices to text boxes
    pub fn analyze(&mut self, laparams: &LAParams) {
        // Separate text objects from other objects
        let (textobjs, otherobjs): (Vec<_>, Vec<_>) =
            self.items.iter().cloned().partition(|obj| obj.is_char());

        if textobjs.is_empty() {
            return;
        }

        // Extract LTChar objects
        let chars: Vec<LTChar> = textobjs
            .into_iter()
            .filter_map(|item| match item {
                LTItem::Char(c) => Some(c),
                _ => None,
            })
            .collect();

        // Group characters into text lines
        let textlines = self.group_objects(laparams, &chars);

        // Separate empty lines
        let (empties, textlines): (Vec<_>, Vec<_>) =
            fsplit(|l: &TextLineType| l.is_empty(), textlines);

        // Group lines into text boxes
        let mut textboxes = self.group_textlines(laparams, textlines);

        if laparams.boxes_flow.is_none() {
            // Analyze each textbox (sorts internal lines)
            // Python: for textbox in textboxes: textbox.analyze(laparams)
            for tb in &mut textboxes {
                match tb {
                    TextBoxType::Horizontal(h) => h.analyze(),
                    TextBoxType::Vertical(v) => v.analyze(),
                }
            }

            // Simple sorting without hierarchical grouping
            textboxes.sort_by(|a, b| {
                let key_a = match a {
                    TextBoxType::Vertical(v) => {
                        (0, (-v.x1() * 1000.0) as i64, (-v.y0() * 1000.0) as i64)
                    }
                    TextBoxType::Horizontal(h) => {
                        (1, (-h.y0() * 1000.0) as i64, (h.x0() * 1000.0) as i64)
                    }
                };
                let key_b = match b {
                    TextBoxType::Vertical(v) => {
                        (0, (-v.x1() * 1000.0) as i64, (-v.y0() * 1000.0) as i64)
                    }
                    TextBoxType::Horizontal(h) => {
                        (1, (-h.y0() * 1000.0) as i64, (h.x0() * 1000.0) as i64)
                    }
                };
                key_a.cmp(&key_b)
            });
        } else {
            // Hierarchical grouping (exact pdfminer-compatible)
            let mut groups = self.group_textboxes_exact(laparams, &textboxes);

            // Analyze and assign indices (analyze recursively sorts elements within groups)
            let mut assigner = IndexAssigner::new();
            for group in groups.iter_mut() {
                group.analyze(laparams);
                assigner.run(group);
            }

            // Extract textboxes with assigned indices from the groups
            textboxes = groups.iter().flat_map(|g| g.collect_textboxes()).collect();

            self.groups = Some(groups);

            // Sort textboxes by their assigned index
            textboxes.sort_by(|a, b| {
                let idx_a = match a {
                    TextBoxType::Horizontal(h) => h.index(),
                    TextBoxType::Vertical(v) => v.index(),
                };
                let idx_b = match b {
                    TextBoxType::Horizontal(h) => h.index(),
                    TextBoxType::Vertical(v) => v.index(),
                };
                idx_a.cmp(&idx_b)
            });
        }

        // Rebuild items list: textboxes + other objects + empty lines
        self.items.clear();
        for tb in textboxes {
            self.items.push(LTItem::TextBox(tb));
        }
        for other in otherobjs {
            self.items.push(other);
        }
        for empty in empties {
            self.items.push(LTItem::TextLine(empty));
        }
    }
}

// ============================================================================
// Analysis methods for LTFigure and LTPage
// ============================================================================

impl LTFigure {
    /// Performs layout analysis on the figure.
    ///
    /// Only performs analysis if all_texts is enabled in laparams.
    pub fn analyze(&mut self, laparams: &LAParams) {
        if !laparams.all_texts {
            return;
        }
        self.container.analyze(laparams);
    }
}

impl LTPage {
    /// Performs layout analysis on the page.
    pub fn analyze(&mut self, laparams: &LAParams) {
        self.container.analyze(laparams);
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
