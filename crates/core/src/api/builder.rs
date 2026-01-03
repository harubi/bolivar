//! Builder pattern for PDF text extraction.
//!
//! Provides a fluent API for configuring and executing PDF extraction.
//!
//! # Example
//! ```ignore
//! use bolivar_core::api::ExtractorBuilder;
//!
//! let text = ExtractorBuilder::new("document.pdf")
//!     .password("secret")
//!     .pages(0..5)
//!     .parallel(4)
//!     .extract_text()?;
//! ```

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::layout::LAParams;

use super::high_level::{ExtractOptions, PageIterator, extract_pages, extract_text};

/// A builder for configuring PDF text extraction.
///
/// This provides a fluent API that wraps the underlying `ExtractOptions`
/// and extraction functions.
#[derive(Debug, Clone)]
pub struct ExtractorBuilder {
    source: PathBuf,
    password: Option<String>,
    pages: Option<Range<usize>>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    threads: Option<usize>,
    laparams: Option<LAParams>,
}

impl ExtractorBuilder {
    /// Creates a new ExtractorBuilder for the given PDF file path.
    ///
    /// # Arguments
    /// * `source` - Path to the PDF file to extract from.
    ///
    /// # Example
    /// ```ignore
    /// let builder = ExtractorBuilder::new("document.pdf");
    /// ```
    pub fn new(source: impl AsRef<Path>) -> Self {
        Self {
            source: source.as_ref().to_path_buf(),
            password: None,
            pages: None,
            page_numbers: None,
            maxpages: 0,
            caching: true,
            threads: None,
            laparams: None,
        }
    }

    /// Sets the password for encrypted PDFs.
    ///
    /// # Arguments
    /// * `pwd` - The password string.
    ///
    /// # Example
    /// ```ignore
    /// let builder = ExtractorBuilder::new("encrypted.pdf")
    ///     .password("secret");
    /// ```
    pub fn password(mut self, pwd: &str) -> Self {
        self.password = Some(pwd.to_string());
        self
    }

    /// Sets a range of pages to extract (zero-indexed).
    ///
    /// This converts the range to a list of page numbers internally.
    /// Note: This replaces any previously set page_numbers.
    ///
    /// # Arguments
    /// * `range` - A range of page indices (e.g., `0..5` for first 5 pages).
    ///
    /// # Example
    /// ```ignore
    /// let builder = ExtractorBuilder::new("document.pdf")
    ///     .pages(0..10);  // First 10 pages
    /// ```
    pub fn pages(mut self, range: Range<usize>) -> Self {
        self.pages = Some(range);
        self.page_numbers = None; // Clear page_numbers when using range
        self
    }

    /// Sets specific page numbers to extract (zero-indexed).
    ///
    /// Use this when you need non-contiguous pages.
    /// Note: This replaces any previously set pages range.
    ///
    /// # Arguments
    /// * `numbers` - A vector of page indices.
    ///
    /// # Example
    /// ```ignore
    /// let builder = ExtractorBuilder::new("document.pdf")
    ///     .page_numbers(vec![0, 2, 5, 10]);  // Specific pages
    /// ```
    pub fn page_numbers(mut self, numbers: Vec<usize>) -> Self {
        self.page_numbers = Some(numbers);
        self.pages = None; // Clear pages range when using specific numbers
        self
    }

    /// Sets the maximum number of pages to extract.
    ///
    /// # Arguments
    /// * `max` - Maximum page count (0 means no limit).
    ///
    /// # Example
    /// ```ignore
    /// let builder = ExtractorBuilder::new("document.pdf")
    ///     .maxpages(50);  // At most 50 pages
    /// ```
    pub fn maxpages(mut self, max: usize) -> Self {
        self.maxpages = max;
        self
    }

    /// Enables or disables parallel processing with the specified thread count.
    ///
    /// # Arguments
    /// * `thread_count` - Number of threads to use. Use 1 or less for sequential processing.
    ///
    /// # Example
    /// ```ignore
    /// let builder = ExtractorBuilder::new("document.pdf")
    ///     .parallel(4);  // Use 4 threads
    /// ```
    pub fn parallel(mut self, thread_count: usize) -> Self {
        self.threads = if thread_count > 1 {
            Some(thread_count)
        } else {
            None
        };
        self
    }

    /// Sets whether to cache resources (fonts, images).
    ///
    /// # Arguments
    /// * `enabled` - Whether caching is enabled (default: true).
    ///
    /// # Example
    /// ```ignore
    /// let builder = ExtractorBuilder::new("document.pdf")
    ///     .caching(false);  // Disable caching
    /// ```
    pub fn caching(mut self, enabled: bool) -> Self {
        self.caching = enabled;
        self
    }

    /// Sets the layout analysis parameters.
    ///
    /// # Arguments
    /// * `params` - Layout analysis parameters.
    ///
    /// # Example
    /// ```ignore
    /// use bolivar_core::layout::LAParams;
    ///
    /// let params = LAParams {
    ///     char_margin: 3.0,
    ///     ..Default::default()
    /// };
    /// let builder = ExtractorBuilder::new("document.pdf")
    ///     .laparams(params);
    /// ```
    pub fn laparams(mut self, params: LAParams) -> Self {
        self.laparams = Some(params);
        self
    }

    /// Builds the `ExtractOptions` from this builder's configuration.
    fn build_options(&self) -> ExtractOptions {
        // Convert pages range to page_numbers if set
        let page_numbers = if let Some(ref range) = self.pages {
            Some(range.clone().collect())
        } else {
            self.page_numbers.clone()
        };

        ExtractOptions {
            password: self.password.clone().unwrap_or_default(),
            page_numbers,
            maxpages: self.maxpages,
            caching: self.caching,
            laparams: self.laparams.clone(),
            threads: self.threads,
        }
    }

    /// Extracts all text from the PDF as a single string.
    ///
    /// # Returns
    /// A `Result<String>` containing all extracted text.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or the PDF is invalid.
    ///
    /// # Example
    /// ```ignore
    /// let text = ExtractorBuilder::new("document.pdf")
    ///     .extract_text()?;
    /// println!("{}", text);
    /// ```
    pub fn extract_text(self) -> Result<String> {
        let pdf_data = std::fs::read(&self.source)?;
        let options = self.build_options();
        extract_text(&pdf_data, Some(options))
    }

    /// Extracts pages as an iterator of `LTPage` objects.
    ///
    /// Each page is analyzed according to the configured layout parameters.
    ///
    /// # Returns
    /// A `Result<PageIterator>` that yields `Result<LTPage>` for each page.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or the PDF is invalid.
    ///
    /// # Example
    /// ```ignore
    /// for page_result in ExtractorBuilder::new("document.pdf").extract_pages()? {
    ///     let page = page_result?;
    ///     println!("Page {}: {:?}", page.pageid, page.bbox());
    /// }
    /// ```
    pub fn extract_pages(self) -> Result<PageIterator> {
        let pdf_data = std::fs::read(&self.source)?;
        let options = self.build_options();
        extract_pages(&pdf_data, Some(options))
    }
}

/// Creates an ExtractorBuilder from raw PDF bytes instead of a file path.
///
/// This is useful when you already have the PDF data in memory.
#[derive(Debug, Clone)]
pub struct ExtractorBuilderFromBytes {
    data: Vec<u8>,
    password: Option<String>,
    pages: Option<Range<usize>>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    caching: bool,
    threads: Option<usize>,
    laparams: Option<LAParams>,
}

impl ExtractorBuilderFromBytes {
    /// Creates a new ExtractorBuilderFromBytes from raw PDF data.
    ///
    /// # Arguments
    /// * `data` - PDF file contents as bytes.
    ///
    /// # Example
    /// ```ignore
    /// let pdf_bytes = std::fs::read("document.pdf")?;
    /// let builder = ExtractorBuilderFromBytes::new(pdf_bytes);
    /// ```
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            password: None,
            pages: None,
            page_numbers: None,
            maxpages: 0,
            caching: true,
            threads: None,
            laparams: None,
        }
    }

    /// Sets the password for encrypted PDFs.
    pub fn password(mut self, pwd: &str) -> Self {
        self.password = Some(pwd.to_string());
        self
    }

    /// Sets a range of pages to extract (zero-indexed).
    pub fn pages(mut self, range: Range<usize>) -> Self {
        self.pages = Some(range);
        self.page_numbers = None;
        self
    }

    /// Sets specific page numbers to extract (zero-indexed).
    pub fn page_numbers(mut self, numbers: Vec<usize>) -> Self {
        self.page_numbers = Some(numbers);
        self.pages = None;
        self
    }

    /// Sets the maximum number of pages to extract.
    pub fn maxpages(mut self, max: usize) -> Self {
        self.maxpages = max;
        self
    }

    /// Enables or disables parallel processing with the specified thread count.
    pub fn parallel(mut self, thread_count: usize) -> Self {
        self.threads = if thread_count > 1 {
            Some(thread_count)
        } else {
            None
        };
        self
    }

    /// Sets whether to cache resources (fonts, images).
    pub fn caching(mut self, enabled: bool) -> Self {
        self.caching = enabled;
        self
    }

    /// Sets the layout analysis parameters.
    pub fn laparams(mut self, params: LAParams) -> Self {
        self.laparams = Some(params);
        self
    }

    /// Builds the `ExtractOptions` from this builder's configuration.
    fn build_options(&self) -> ExtractOptions {
        let page_numbers = if let Some(ref range) = self.pages {
            Some(range.clone().collect())
        } else {
            self.page_numbers.clone()
        };

        ExtractOptions {
            password: self.password.clone().unwrap_or_default(),
            page_numbers,
            maxpages: self.maxpages,
            caching: self.caching,
            laparams: self.laparams.clone(),
            threads: self.threads,
        }
    }

    /// Extracts all text from the PDF as a single string.
    pub fn extract_text(self) -> Result<String> {
        let options = self.build_options();
        extract_text(&self.data, Some(options))
    }

    /// Extracts pages as an iterator of `LTPage` objects.
    pub fn extract_pages(self) -> Result<PageIterator> {
        let options = self.build_options();
        extract_pages(&self.data, Some(options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_pdf() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");

        let mut offsets: Vec<usize> = Vec::new();
        let mut push_obj = |buf: &mut Vec<u8>, obj: &str, offsets: &mut Vec<usize>| {
            offsets.push(buf.len());
            buf.extend_from_slice(obj.as_bytes());
        };

        // 1: Catalog
        push_obj(
            &mut out,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
            &mut offsets,
        );

        // 2: Pages
        push_obj(
            &mut out,
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>\nendobj\n",
            &mut offsets,
        );

        // 3: Page 1
        push_obj(
            &mut out,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 5 0 R >>\nendobj\n",
            &mut offsets,
        );

        // 4: Page 2
        push_obj(
            &mut out,
            "4 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 6 0 R >>\nendobj\n",
            &mut offsets,
        );

        // 5: Contents for page 1
        push_obj(
            &mut out,
            "5 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n",
            &mut offsets,
        );

        // 6: Contents for page 2
        push_obj(
            &mut out,
            "6 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n",
            &mut offsets,
        );

        let xref_pos = out.len();
        let obj_count = offsets.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", obj_count + 1).as_bytes(),
        );
        for offset in offsets {
            out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }
        out.extend_from_slice(b"trailer\n<< /Size ");
        out.extend_from_slice((obj_count + 1).to_string().as_bytes());
        out.extend_from_slice(b" /Root 1 0 R >>\nstartxref\n");
        out.extend_from_slice(xref_pos.to_string().as_bytes());
        out.extend_from_slice(b"\n%%EOF");

        out
    }

    #[test]
    fn test_builder_new() {
        let builder = ExtractorBuilder::new("test.pdf");
        assert_eq!(builder.source, PathBuf::from("test.pdf"));
        assert!(builder.password.is_none());
        assert!(builder.pages.is_none());
        assert!(builder.laparams.is_none());
    }

    #[test]
    fn test_builder_password() {
        let builder = ExtractorBuilder::new("test.pdf").password("secret");
        assert_eq!(builder.password, Some("secret".to_string()));
    }

    #[test]
    fn test_builder_pages() {
        let builder = ExtractorBuilder::new("test.pdf").pages(0..5);
        assert_eq!(builder.pages, Some(0..5));
    }

    #[test]
    fn test_builder_page_numbers() {
        let builder = ExtractorBuilder::new("test.pdf").page_numbers(vec![0, 2, 5]);
        assert_eq!(builder.page_numbers, Some(vec![0, 2, 5]));
    }

    #[test]
    fn test_builder_parallel() {
        let builder = ExtractorBuilder::new("test.pdf").parallel(4);
        assert_eq!(builder.threads, Some(4));

        // Test that parallel(1) disables parallelism
        let builder2 = ExtractorBuilder::new("test.pdf").parallel(1);
        assert!(builder2.threads.is_none());
    }

    #[test]
    fn test_builder_laparams() {
        let params = LAParams {
            char_margin: 3.0,
            ..Default::default()
        };
        let builder = ExtractorBuilder::new("test.pdf").laparams(params.clone());
        assert_eq!(builder.laparams, Some(params));
    }

    #[test]
    fn test_builder_build_options() {
        let builder = ExtractorBuilder::new("test.pdf")
            .password("pwd")
            .pages(0..3)
            .maxpages(10)
            .caching(false)
            .parallel(2);

        let options = builder.build_options();
        assert_eq!(options.password, "pwd");
        assert_eq!(options.page_numbers, Some(vec![0, 1, 2]));
        assert_eq!(options.maxpages, 10);
        assert!(!options.caching);
        assert_eq!(options.threads, Some(2));
    }

    #[test]
    fn test_builder_chaining() {
        // Test that all methods can be chained
        let _builder = ExtractorBuilder::new("test.pdf")
            .password("secret")
            .pages(0..10)
            .maxpages(5)
            .caching(true)
            .parallel(4)
            .laparams(LAParams::default());
    }

    #[test]
    fn test_builder_from_bytes_extract_text() {
        let pdf_data = build_minimal_pdf();
        let result = ExtractorBuilderFromBytes::new(pdf_data).extract_text();
        assert!(result.is_ok());
    }

    #[test]
    fn test_builder_from_bytes_extract_pages() {
        let pdf_data = build_minimal_pdf();
        let result = ExtractorBuilderFromBytes::new(pdf_data).extract_pages();
        assert!(result.is_ok());

        let pages: Vec<_> = result.unwrap().collect();
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn test_builder_from_bytes_with_page_range() {
        let pdf_data = build_minimal_pdf();
        let result = ExtractorBuilderFromBytes::new(pdf_data)
            .pages(0..1)
            .extract_pages();
        assert!(result.is_ok());

        let pages: Vec<_> = result.unwrap().collect();
        assert_eq!(pages.len(), 1);
    }
}
