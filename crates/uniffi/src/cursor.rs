use std::sync::{Arc, Mutex};
use std::vec::IntoIter;

use bolivar_core::engine::{CancellationHandle, PageTables, Stream};
use bolivar_core::table::TableMetadata;

use crate::error::BolivarError;
use crate::extract::{TablePageContext, prepare_table_cursor, prepare_table_rows_cursor};
use crate::types::{PageTableRows, Table, TableOptions, table_from_core, usize_to_u32};
use bolivar_core::extract::ExtractOptions as CoreExtractOptions;
use bolivar_core::pdfdocument::PDFDocument;

struct TableCursorState {
    stream: Option<Stream<Vec<TableMetadata>>>,
    contexts: IntoIter<TablePageContext>,
    current_tables: IntoIter<TableMetadata>,
    current_context: Option<TablePageContext>,
}

impl TableCursorState {
    fn fail(&mut self, error: BolivarError) -> Result<Option<Table>, BolivarError> {
        self.stream.take();
        Err(error)
    }

    fn next(&mut self) -> Result<Option<Table>, BolivarError> {
        loop {
            if let Some(table) = self.current_tables.next() {
                let Some(context) = self.current_context.as_ref() else {
                    return self.fail(BolivarError::RuntimeError);
                };
                return Ok(Some(table_from_core(
                    context.page_number,
                    table,
                    &context.geometry,
                )));
            }

            let Some(stream) = self.stream.as_mut() else {
                return Ok(None);
            };
            match stream.next() {
                Some(Ok((page_index, tables))) => {
                    let Some(context) = self.contexts.next() else {
                        return self.fail(BolivarError::RuntimeError);
                    };
                    if page_index != context.page_index {
                        return self.fail(BolivarError::RuntimeError);
                    }
                    self.current_context = Some(context);
                    self.current_tables = tables.into_iter();
                }
                Some(Err(error)) => return self.fail(BolivarError::from(error)),
                None => {
                    if self.contexts.next().is_some() {
                        return self.fail(BolivarError::RuntimeError);
                    }
                    self.stream.take();
                    return Ok(None);
                }
            }
        }
    }
}

/// A closeable native cursor that yields one table at a time.
pub struct NativeTableCursor {
    cancellation: CancellationHandle,
    state: Mutex<TableCursorState>,
}

impl std::fmt::Debug for NativeTableCursor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeTableCursor")
            .finish_non_exhaustive()
    }
}

impl NativeTableCursor {
    pub(crate) fn open(
        document: Arc<PDFDocument>,
        options: CoreExtractOptions,
        table_options: Option<TableOptions>,
    ) -> Result<Arc<Self>, BolivarError> {
        let (stream, contexts) = prepare_table_cursor(document, options, table_options)?;
        let cancellation = stream.cancellation_handle();
        Ok(Arc::new(Self {
            cancellation,
            state: Mutex::new(TableCursorState {
                stream: Some(stream),
                contexts: contexts.into_iter(),
                current_tables: Vec::new().into_iter(),
                current_context: None,
            }),
        }))
    }

    pub fn next(&self) -> Result<Option<Table>, BolivarError> {
        self.state
            .lock()
            .map_err(|_| BolivarError::RuntimeError)?
            .next()
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

struct PageTableRowsCursorState {
    stream: Option<Stream<PageTables>>,
}

impl PageTableRowsCursorState {
    fn fail(&mut self, error: BolivarError) -> Result<Option<PageTableRows>, BolivarError> {
        self.stream.take();
        Err(error)
    }

    fn next(&mut self) -> Result<Option<PageTableRows>, BolivarError> {
        let Some(stream) = self.stream.as_mut() else {
            return Ok(None);
        };
        match stream.next() {
            Some(Ok((page_index, tables))) => Ok(Some(PageTableRows::from_tables(
                usize_to_u32(page_index.saturating_add(1)),
                tables,
            ))),
            Some(Err(error)) => self.fail(BolivarError::from(error)),
            None => {
                self.stream.take();
                Ok(None)
            }
        }
    }
}

/// A closeable native cursor that yields the raw table rows for one page.
pub struct NativePageTableRowsCursor {
    cancellation: CancellationHandle,
    state: Mutex<PageTableRowsCursorState>,
}

impl std::fmt::Debug for NativePageTableRowsCursor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativePageTableRowsCursor")
            .finish_non_exhaustive()
    }
}

impl NativePageTableRowsCursor {
    pub(crate) fn open(
        document: Arc<PDFDocument>,
        options: CoreExtractOptions,
        table_options: Option<TableOptions>,
    ) -> Result<Arc<Self>, BolivarError> {
        let stream = prepare_table_rows_cursor(document, options, table_options)?;
        let cancellation = stream.cancellation_handle();
        Ok(Arc::new(Self {
            cancellation,
            state: Mutex::new(PageTableRowsCursorState {
                stream: Some(stream),
            }),
        }))
    }

    pub fn next(&self) -> Result<Option<PageTableRows>, BolivarError> {
        self.state
            .lock()
            .map_err(|_| BolivarError::RuntimeError)?
            .next()
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}
