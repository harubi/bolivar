use bolivar_core::extract::{
    ExtractOptions as CoreExtractOptions,
    extract_text_with_document as core_extract_text_with_document,
};
use bolivar_core::pdfdocument::PDFDocument;
use std::sync::Arc;

use crate::error::BolivarError;
use crate::extract::{
    core_extract_options, extract_layout_pages_core, extract_raw_document_core,
    extract_raw_page_core, extract_table_rows_with_core, extract_tables_core,
    extract_tables_with_core, open_pdf_document, read_pdf_bytes,
};
use crate::metadata::metadata_from_document;
use crate::types::{
    ExtractOptions, LayoutPage, PageSummary, PageTableRows, RawDocument, RawDocumentMetadata,
    RawPage, Table, TableOptions, summary_from_ltpage,
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
    pub fn from_path(path: String, options: Option<ExtractOptions>) -> Result<Self, BolivarError> {
        let pdf_data = read_pdf_bytes(path)?;
        Self::from_bytes(pdf_data, options)
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

    pub fn extract_page_summaries(&self) -> Result<Vec<PageSummary>, BolivarError> {
        let stream = bolivar_core::extract::extract_pages_stream_from_doc(
            Arc::clone(&self.doc),
            self.core_options(),
        )
        .map_err(BolivarError::from)?;
        let mut summaries = Vec::new();
        for page in stream {
            let (_, page) = page.map_err(BolivarError::from)?;
            summaries.push(summary_from_ltpage(&page));
        }
        Ok(summaries)
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

    pub fn extract_tables(&self) -> Result<Vec<Table>, BolivarError> {
        let options = self.core_options();
        extract_tables_core(Arc::clone(&self.doc), options)
    }

    pub fn extract_tables_with(
        &self,
        table_options: Option<TableOptions>,
    ) -> Result<Vec<Table>, BolivarError> {
        let options = self.core_options();
        extract_tables_with_core(Arc::clone(&self.doc), options, table_options)
    }

    pub fn extract_table_rows_with(
        &self,
        table_options: Option<TableOptions>,
    ) -> Result<Vec<PageTableRows>, BolivarError> {
        let options = self.core_options();
        extract_table_rows_with_core(Arc::clone(&self.doc), options, table_options)
    }
}
