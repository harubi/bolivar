//! Layout Analysis Module - grouping and clustering algorithms
//!
//! Contains the layout analysis algorithms for:
//! - Grouping characters into text lines
//! - Grouping text lines into text boxes
//! - Hierarchical grouping of text boxes
//! - Exact pdfminer-compatible grouping

mod analyze;
mod clustering;
mod grouping;
mod spatial_tree;

// Re-export public types and functions
pub use clustering::group_textboxes_exact;
pub use grouping::{group_objects, group_objects_arena, group_textlines, group_textlines_arena};
pub use spatial_tree::{
    BestEntry, DynamicSpatialTree, FrontierEntry, GroupHeapEntry, NodeStats, PairMode, PlaneElem,
    PyId, SpatialNode, TreeKind, calc_dist_lower_bound, expand_frontier_best,
};
