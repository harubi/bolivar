//! Cooperative cancellation for long-running extraction work.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{PdfError, Result};

/// A small, cloneable cancellation signal.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(PdfError::Cancelled)
        } else {
            Ok(())
        }
    }
}
