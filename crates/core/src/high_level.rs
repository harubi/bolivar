//! High-level text extraction API - port of pdfminer.six high_level.py
//!
//! Provides the main public API for PDF text extraction:
//! - `extract_text()` - Extract all text from a PDF as a String
//! - `extract_text_to_fp()` - Extract text to a writer
//! - `extract_pages()` - Iterator over analyzed pages

use std::io::Write;

use crate::converter::{PDFPageAggregator, TextConverter};
use crate::error::{PdfError, Result};
use crate::layout::{LAParams, LTPage};
use crate::pdfdocument::PDFDocument;
use crate::pdfinterp::{PDFPageInterpreter, PDFResourceManager};
use crate::pdfpage::PDFPage;

/// Options for text extraction.
///
/// Port of the various optional parameters from pdfminer.six high_level functions.
#[derive(Debug, Clone)]
pub struct ExtractOptions {
    /// Password for encrypted PDFs.
    pub password: String,

    /// Zero-indexed page numbers to extract. None means all pages.
    pub page_numbers: Option<Vec<usize>>,

    /// Maximum number of pages to extract. 0 means no limit.
    pub maxpages: usize,

    /// Whether to cache resources (fonts, images).
    pub caching: bool,

    /// Layout analysis parameters. None uses default LAParams.
    pub laparams: Option<LAParams>,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            password: String::new(),
            page_numbers: None,
            maxpages: 0,
            caching: true,
            laparams: None,
        }
    }
}

/// Parse and return the text contained in PDF data.
///
/// This is the main text extraction function.
///
/// # Arguments
/// * `pdf_data` - PDF file contents as bytes
/// * `options` - Extraction options (None for defaults)
///
/// # Returns
/// A string containing all extracted text.
///
/// # Example
/// ```ignore
/// use bolivar_core::high_level::{extract_text, ExtractOptions};
///
/// let pdf_bytes = std::fs::read("document.pdf")?;
/// let text = extract_text(&pdf_bytes, None)?;
/// println!("{}", text);
/// ```
pub fn extract_text(pdf_data: &[u8], options: Option<ExtractOptions>) -> Result<String> {
    let options = options.unwrap_or_default();

    // Use LAParams or create default
    let laparams = options.laparams.unwrap_or_default();

    // Create output buffer
    let mut output = Vec::new();

    // Extract text to buffer
    extract_text_to_fp_inner(
        pdf_data,
        &mut output,
        &options.password,
        options.page_numbers.as_deref(),
        options.maxpages,
        options.caching,
        Some(&laparams),
    )?;

    // Convert to string
    String::from_utf8(output).map_err(|e| PdfError::DecodeError(e.to_string()))
}

/// Parse text from PDF data and write to a writer.
///
/// # Arguments
/// * `pdf_data` - PDF file contents as bytes
/// * `writer` - Output writer for extracted text
/// * `options` - Extraction options (None for defaults)
///
/// # Example
/// ```ignore
/// use bolivar_core::high_level::{extract_text_to_fp, ExtractOptions};
/// use std::fs::File;
///
/// let pdf_bytes = std::fs::read("document.pdf")?;
/// let mut output = File::create("output.txt")?;
/// extract_text_to_fp(&pdf_bytes, &mut output, None)?;
/// ```
pub fn extract_text_to_fp<W: Write>(
    pdf_data: &[u8],
    writer: &mut W,
    options: Option<ExtractOptions>,
) -> Result<()> {
    let options = options.unwrap_or_default();

    let laparams = options.laparams.as_ref();

    extract_text_to_fp_inner(
        pdf_data,
        writer,
        &options.password,
        options.page_numbers.as_deref(),
        options.maxpages,
        options.caching,
        laparams,
    )
}

/// Inner implementation of extract_text_to_fp.
fn extract_text_to_fp_inner<W: Write>(
    pdf_data: &[u8],
    writer: &mut W,
    password: &str,
    page_numbers: Option<&[usize]>,
    maxpages: usize,
    caching: bool,
    laparams: Option<&LAParams>,
) -> Result<()> {
    // Validate PDF header
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    // Parse PDF document
    let doc = PDFDocument::new(pdf_data, password)?;

    // Create resource manager
    let mut rsrcmgr = PDFResourceManager::with_caching(caching);

    // Get LAParams (use default if not provided)
    let default_laparams = LAParams::default();
    let laparams = laparams.unwrap_or(&default_laparams);

    // Create text converter
    let mut converter = TextConverter::new(writer, "utf-8", 1, Some(laparams.clone()), false);

    // Process pages
    let mut page_count = 0;
    for (page_idx, page_result) in PDFPage::create_pages(&doc).enumerate() {
        // Check page number filter
        if let Some(nums) = page_numbers {
            if !nums.contains(&page_idx) {
                continue;
            }
        }

        // Check maxpages limit
        if maxpages > 0 && page_count >= maxpages {
            break;
        }

        let page = page_result?;

        // Create aggregator for this page
        let mut aggregator = PDFPageAggregator::new(Some(laparams.clone()), page_idx as i32 + 1);

        // Process page content and get layout
        let ltpage = process_page(&page, &mut aggregator, &mut rsrcmgr, laparams, &doc)?;

        // Render to text
        converter.receive_layout(ltpage);

        page_count += 1;
    }

    Ok(())
}

/// Process a PDF page and return its layout.
///
/// Uses PDFPageInterpreter to execute the page's content stream,
/// which populates the device (aggregator) with layout items.
fn process_page(
    page: &PDFPage,
    aggregator: &mut PDFPageAggregator,
    rsrcmgr: &mut PDFResourceManager,
    _laparams: &LAParams,
    doc: &PDFDocument,
) -> Result<LTPage> {
    // Create interpreter with resource manager and aggregator as device
    let mut interpreter = PDFPageInterpreter::new(rsrcmgr, aggregator);

    // Process page - this executes the content stream and populates the device
    interpreter.process_page(page, Some(doc));

    // Get the analyzed result from aggregator
    Ok(aggregator.get_result().clone())
}

/// Iterator over analyzed pages.
///
/// Yields LTPage objects for each page in the PDF.
///
/// Note: Due to lifetime constraints with the underlying PDF document,
/// this iterator pre-processes pages into a vector. For very large PDFs,
/// consider using extract_text_to_fp with streaming output instead.
pub struct PageIterator {
    /// Pre-processed pages
    pages: std::vec::IntoIter<Result<LTPage>>,
}

impl Iterator for PageIterator {
    type Item = Result<LTPage>;

    fn next(&mut self) -> Option<Self::Item> {
        self.pages.next()
    }
}

impl PageIterator {
    /// Returns the number of remaining pages.
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Returns true if there are no more pages.
    pub fn is_empty(&self) -> bool {
        self.pages.len() == 0
    }
}

/// Extract and yield LTPage objects from PDF data.
///
/// Returns an iterator over analyzed pages.
///
/// # Arguments
/// * `pdf_data` - PDF file contents as bytes
/// * `options` - Extraction options (None for defaults)
///
/// # Example
/// ```ignore
/// use bolivar_core::high_level::{extract_pages, ExtractOptions};
/// use bolivar_core::layout::LTTextBox;
///
/// let pdf_bytes = std::fs::read("document.pdf")?;
/// for page_result in extract_pages(&pdf_bytes, None)? {
///     let page = page_result?;
///     for item in page.iter() {
///         // Process layout items
///     }
/// }
/// ```
pub fn extract_pages(pdf_data: &[u8], options: Option<ExtractOptions>) -> Result<PageIterator> {
    let options = options.unwrap_or_default();

    // Validate PDF header
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    // Parse PDF document
    let doc = PDFDocument::new(pdf_data, &options.password)?;

    // Create resource manager
    let mut rsrcmgr = PDFResourceManager::with_caching(options.caching);

    // Get LAParams
    let laparams = options.laparams.unwrap_or_default();

    // Collect pages into a vector
    // This is necessary because PDFPage borrows from PDFDocument
    let mut pages: Vec<Result<LTPage>> = Vec::new();
    let mut page_count = 0;

    for (page_idx, page_result) in PDFPage::create_pages(&doc).enumerate() {
        // Check page number filter
        if let Some(ref nums) = options.page_numbers {
            if !nums.contains(&page_idx) {
                continue;
            }
        }

        // Check maxpages limit
        if options.maxpages > 0 && page_count >= options.maxpages {
            break;
        }

        match page_result {
            Ok(page) => {
                // Create aggregator for this page
                let mut aggregator =
                    PDFPageAggregator::new(Some(laparams.clone()), page_idx as i32 + 1);

                // Process page content using interpreter
                match process_page(&page, &mut aggregator, &mut rsrcmgr, &laparams, &doc) {
                    Ok(ltpage) => {
                        pages.push(Ok(ltpage));
                        page_count += 1;
                    }
                    Err(e) => {
                        pages.push(Err(e));
                    }
                }
            }
            Err(e) => {
                pages.push(Err(e));
            }
        }
    }

    Ok(PageIterator {
        pages: pages.into_iter(),
    })
}
