//! Orchestration primitives for the unified PDF extraction pipeline.
//!
//! `engine` owns page selection, worker pool construction, per-page processing,
//! and result ordering. It does NOT know about specific extraction modes (text,
//! tables, words) — those live in `crate::extract`.

mod options;
mod plan;
pub(crate) mod processor;
mod runtime;

pub mod batch;
pub mod stream;

pub use batch::run_batch;
pub use options::{Cell, DocumentTables, ExtractOptions, PageTables, Row, Table};
pub use plan::{ExecutionPlan, select_pages, validate_geometry_count};
pub use processor::{
    aggregator_result, collector_result, process_page, process_page_with_cancellation,
};
pub use stream::{
    CancellationHandle, Stream, no_precheck, no_precheck_cancellable, run_stream,
    run_stream_cancellable,
};
