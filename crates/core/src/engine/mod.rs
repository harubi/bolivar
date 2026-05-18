//! Orchestration primitives for the unified PDF extraction pipeline.
//!
//! `engine` owns page selection, worker pool construction, per-page processing,
//! and result ordering. It does NOT know about specific extraction modes (text,
//! tables, words) — those live in `crate::extract`.

mod options;
mod plan;
pub(crate) mod processor;

pub mod batch;
pub mod stream;

pub use batch::run_batch;
pub use options::{Cell, DocumentTables, ExtractOptions, PageTables, Row, Table};
pub use plan::{ExecutionPlan, select_pages, validate_geometry_count};
pub use processor::{aggregator_result, collector_result, process_page};
pub use stream::{DEFAULT_STREAM_BUFFER_CAPACITY, Stream, no_precheck, run_stream};
