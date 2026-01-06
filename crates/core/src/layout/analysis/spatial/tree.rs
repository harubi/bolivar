//! Spatial tree data structures.
//!
//! Binary space partition trees for efficient nearest-neighbor queries
//! in the pdfminer-compatible text grouping algorithm.

use super::distance::bbox_expand_area;
use super::types::{NodeStats, PyId};
use crate::utils::Rect;

/// Leaf threshold for spatial tree nodes
const LEAF_THRESHOLD: usize = 8;

/// Node in a spatial binary tree.
///
/// Internal nodes have left/right children; leaf nodes store element indices.
#[derive(Clone, Debug)]
pub struct SpatialNode {
    pub stats: NodeStats,
    pub count: usize,
    pub element_indices: Vec<usize>,
    pub left_child: Option<usize>,
    pub right_child: Option<usize>,
}

impl SpatialNode {
    /// Build a tree from bboxes and py_ids, returning root index
    pub fn build(elements: &[(Rect, PyId)], arena: &mut Vec<Self>) -> usize {
        Self::build_range(elements, (0..elements.len()).collect(), arena)
    }

    fn build_range(elements: &[(Rect, PyId)], indices: Vec<usize>, arena: &mut Vec<Self>) -> usize {
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

    /// Check if this is a leaf node
    pub const fn is_leaf(&self) -> bool {
        self.left_child.is_none()
    }

    /// Get element count in this subtree
    pub const fn element_count(&self) -> usize {
        self.count
    }
}

/// Dynamic spatial tree supporting insertions.
///
/// Used for incrementally building the tree as groups are merged.
pub struct DynamicSpatialTree {
    pub nodes: Vec<SpatialNode>,
    parents: Vec<Option<usize>>,
    pub root: usize,
    elem_leaf: Vec<usize>,
}

impl DynamicSpatialTree {
    /// Build from initial elements
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

    /// Insert a new element into the tree
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

        let left_expand = bbox_expand_area(self.nodes[left].stats.bbox, bbox);
        let right_expand = bbox_expand_area(self.nodes[right].stats.bbox, bbox);

        if left_expand <= right_expand {
            self.choose_leaf(left, bbox)
        } else {
            self.choose_leaf(right, bbox)
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
        let left_indices: Vec<usize> = sorted[..mid].to_vec();
        let right_indices: Vec<usize> = sorted[mid..].to_vec();

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

    /// Check if element is contained in subtree rooted at node
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
