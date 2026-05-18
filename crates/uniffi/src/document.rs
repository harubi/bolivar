use bolivar_core::extract::{
    ExtractOptions as CoreExtractOptions,
    extract_text_with_document as core_extract_text_with_document,
};
use bolivar_core::pdfdocument::PDFDocument;
use std::sync::Arc;

use crate::error::BolivarError;
use crate::extract::{
    core_extract_options, extract_layout_pages_core, extract_tables_core, open_pdf_document,
    read_pdf_bytes,
};
use crate::types::{ExtractOptions, LayoutPage, PageSummary, Table, summary_from_layout_page};

pub struct NativePdfDocument {
    doc: Arc<PDFDocument>,
    options: Option<ExtractOptions>,
}

impl std::fmt::Debug for NativePdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePdfDocument").finish_non_exhaustive()
    }
}

impl NativePdfDocument {
    pub fn from_path(path: String, options: Option<ExtractOptions>) -> Result<Self, BolivarError> {
        let pdf_data = read_pdf_bytes(path)?;
        Self::from_bytes(pdf_data, options)
    }

    pub fn from_bytes(
        pdf_data: Vec<u8>,
        options: Option<ExtractOptions>,
    ) -> Result<Self, BolivarError> {
        let doc = open_pdf_document(&pdf_data, &options)?;
        Ok(Self { doc, options })
    }

    fn core_options(&self) -> Result<CoreExtractOptions, BolivarError> {
        core_extract_options(self.options.clone())
    }

    pub fn extract_text(&self) -> Result<String, BolivarError> {
        let options = self.core_options()?;
        core_extract_text_with_document(self.doc.as_ref(), options).map_err(BolivarError::from)
    }

    pub fn extract_page_summaries(&self) -> Result<Vec<PageSummary>, BolivarError> {
        Ok(self
            .extract_layout_pages()?
            .into_iter()
            .map(summary_from_layout_page)
            .collect())
    }

    pub fn extract_layout_pages(&self) -> Result<Vec<LayoutPage>, BolivarError> {
        let options = self.core_options()?;
        extract_layout_pages_core(Arc::clone(&self.doc), options)
    }

    pub fn extract_tables(&self) -> Result<Vec<Table>, BolivarError> {
        let options = self.core_options()?;
        extract_tables_core(Arc::clone(&self.doc), options)
    }
}
