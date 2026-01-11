//! Core types for spatial tree algorithms.
//!
//! Data structures for the pdfminer-compatible grouping algorithm:
//! - Heap entries for priority queue processing
//! - Node statistics for lower bound computation

use super::distance::bbox_union;
use crate::utils::{HasBBox, Rect};

/// Monotonically increasing ID assigned in parse order (matches pdfminer's id() semantics)
pub type PyId = u64;

/// Distance key for total ordering of f64 distances
pub type DistKey = i64;

/// Tracks how pairs are oriented for correct pdfminer semantics
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairMode {
    /// Initial elements: pairs are (i, j) where i < j by py_id
    InitialIJ,
    /// Merged group: pairs are (group_id, other_id) in that order
    GroupOther { group_id: PyId, group_idx: usize },
}

/// Which tree a frontier entry belongs to
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeKind {
    Initial,
    Dynamic,
}

/// Element in the spatial plane with bounding box and index
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
///
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
///
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

/// Union of frontier and pair entries for single-heap algorithm
#[derive(Clone, Debug)]
pub enum BestEntry {
    Frontier(FrontierEntry),
    Pair(GroupHeapEntry),
}

impl BestEntry {
    pub const fn key_parts(&self) -> (bool, DistKey, PyId, PyId, u8) {
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
///
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
            bbox: bbox_union(self.bbox, other.bbox),
            min_w: self.min_w.min(other.min_w),
            min_h: self.min_h.min(other.min_h),
            max_area: self.max_area.max(other.max_area),
            min_py_id: ids[0],
            second_min_py_id: ids[1],
        }
    }
}
