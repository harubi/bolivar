//! Layout analysis algorithms for grouping and clustering.
//!
//! - Grouping characters into text lines
//! - Grouping text lines into text boxes
//! - Hierarchical grouping of text boxes
//! - Exact pdfminer-compatible grouping with spatial indexing

mod analyze;
mod clustering;
mod grouping;
mod soa;
mod soa_layout;
pub mod spatial;

// Re-export public types and functions
pub use clustering::group_textboxes_exact;
pub use grouping::{group_objects, group_objects_arena, group_textlines, group_textlines_arena};
pub use spatial::{
    BestEntry, DynamicSpatialTree, FrontierEntry, GroupHeapEntry, NodeStats, PairMode, PlaneElem,
    PyId, SpatialNode, TreeKind, calc_dist_lower_bound, expand_frontier_best,
};
