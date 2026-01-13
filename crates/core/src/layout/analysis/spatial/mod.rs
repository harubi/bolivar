//! Spatial indexing for layout element grouping.
//!
//! Uses a frontier-based algorithm to efficiently generate element pairs
//! for the pdfminer-compatible text grouping algorithm.
//!
//! - `types` - Core data structures (heap entries, node statistics)
//! - `distance` - Distance and lower bound calculations
//! - `tree` - Spatial tree structures (static and dynamic)
//! - `frontier` - Lazy pair generation via frontier expansion

mod distance;
mod frontier;
mod tree;
mod types;

pub use distance::{
    bbox_area, bbox_expand_area, bbox_union, calc_dist_lower_bound, dist_key_from_geom,
    f64_total_key,
};
pub use frontier::{FrontierBestParams, expand_frontier_best};
pub use tree::{DynamicSpatialTree, SpatialNode};
pub use types::{
    BestEntry, DistKey, FrontierEntry, GroupHeapEntry, NodeStats, PairMode, PlaneElem, PyId,
    TreeKind,
};
