//! Shared execution engine for CPU-bound PDF work.

use std::sync::Arc;

use once_cell::sync::OnceCell;
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::error::{PdfError, Result};

use super::plan::default_worker_count;

pub(crate) struct Engine {
    pool: ThreadPool,
    worker_count: usize,
}

impl Engine {
    pub(crate) fn new(worker_count: usize) -> Result<Arc<Self>> {
        let worker_count = worker_count.max(1);
        let pool = ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .thread_name(|index| format!("bolivar-worker-{index}"))
            .build()
            .map_err(|error| PdfError::RuntimeError(error.to_string()))?;
        Ok(Arc::new(Self { pool, worker_count }))
    }

    pub(crate) const fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub(crate) fn install<R: Send>(&self, work: impl FnOnce() -> R + Send) -> R {
        self.pool.install(work)
    }

    pub(crate) fn spawn(&self, work: impl FnOnce() + Send + 'static) {
        self.pool.spawn(work);
    }
}

pub(crate) fn shared_engine() -> Result<Arc<Engine>> {
    static ENGINE: OnceCell<Arc<Engine>> = OnceCell::new();
    ENGINE
        .get_or_try_init(|| Engine::new(default_worker_count()))
        .map(Arc::clone)
}
