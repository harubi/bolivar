use bolivar_core::extract::{
    ExtractOptions as CoreExtractOptions,
    extract_text_with_document as core_extract_text_with_document,
};
use bolivar_core::pdfdocument::PDFDocument;
use std::sync::Arc;

use crate::cursor::{NativePageSummaryCursor, NativePageTableRowsCursor, NativeTableCursor};
use crate::error::BolivarError;
use crate::extract::{
    core_extract_options, extract_layout_pages_core, extract_raw_document_core,
    extract_raw_page_core, open_pdf_document, validate_input_path,
};
use crate::metadata::metadata_from_document;
use crate::types::{
    ExtractOptions, LayoutPage, RawDocument, RawDocumentMetadata, RawPage, TableOptions,
    cache_capacity,
};

pub struct NativePdfDocument {
    doc: Arc<PDFDocument>,
    options: CoreExtractOptions,
}

impl std::fmt::Debug for NativePdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePdfDocument").finish_non_exhaustive()
    }
}

impl NativePdfDocument {
    /// Open a memory-mapped path. The source must stay stable until this document and its cursors close.
    pub fn from_path(path: String, options: Option<ExtractOptions>) -> Result<Self, BolivarError> {
        validate_input_path(&path)?;
        let mut options = core_extract_options(options)?;
        let doc = PDFDocument::new_from_path_with_cache_and_fallback(
            path,
            &options.password,
            cache_capacity(options.caching),
            true,
        )
        .map(Arc::new)
        .map_err(BolivarError::from)?;
        options.password = String::new();
        Ok(Self { doc, options })
    }

    pub fn from_bytes(
        pdf_data: Vec<u8>,
        options: Option<ExtractOptions>,
    ) -> Result<Self, BolivarError> {
        let mut options = core_extract_options(options)?;
        let doc = open_pdf_document(pdf_data, &options)?;
        options.password = String::new();
        Ok(Self { doc, options })
    }

    fn core_options(&self) -> CoreExtractOptions {
        self.options.clone()
    }

    pub fn extract_text(&self) -> Result<String, BolivarError> {
        let options = self.core_options();
        core_extract_text_with_document(self.doc.as_ref(), options).map_err(BolivarError::from)
    }

    pub fn page_summaries(&self) -> Result<Arc<NativePageSummaryCursor>, BolivarError> {
        NativePageSummaryCursor::open(Arc::clone(&self.doc), self.core_options())
    }

    pub fn extract_layout_pages(&self) -> Result<Vec<LayoutPage>, BolivarError> {
        let options = self.core_options();
        extract_layout_pages_core(Arc::clone(&self.doc), options)
    }

    pub fn extract_raw_document(&self) -> Result<RawDocument, BolivarError> {
        let options = self.core_options();
        extract_raw_document_core(Arc::clone(&self.doc), options)
    }

    pub fn extract_raw_page(&self, page_number: u32) -> Result<RawPage, BolivarError> {
        let options = self.core_options();
        extract_raw_page_core(Arc::clone(&self.doc), options, page_number)
    }

    pub fn metadata(&self) -> Result<RawDocumentMetadata, BolivarError> {
        Ok(metadata_from_document(self.doc.as_ref()))
    }

    pub fn tables(
        &self,
        table_options: Option<TableOptions>,
    ) -> Result<Arc<NativeTableCursor>, BolivarError> {
        NativeTableCursor::open(Arc::clone(&self.doc), self.core_options(), table_options)
    }

    pub fn table_rows(
        &self,
        table_options: Option<TableOptions>,
    ) -> Result<Arc<NativePageTableRowsCursor>, BolivarError> {
        NativePageTableRowsCursor::open(Arc::clone(&self.doc), self.core_options(), table_options)
    }
}
