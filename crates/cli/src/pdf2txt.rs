//! pdf2txt - Extract text and images from PDF files
//!
//! A command line tool for extracting text and images from PDF and
//! outputting it to plain text, html, xml or tags.
//!
//! Port of pdfminer.six tools/pdf2txt.py

use bolivar_core::converter::{
    HOCRConverter, HTMLConverter, PDFPageAggregator, TextConverter, XMLConverter,
};
use bolivar_core::error::{PdfError, Result};
use bolivar_core::high_level::{ExtractOptions, extract_pages};
use bolivar_core::image::ImageWriter;
use bolivar_core::layout::LAParams;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdfinterp::{PDFPageInterpreter, PDFResourceManager};
use bolivar_core::pdfpage::PDFPage;
use clap::{ArgAction, Parser, ValueEnum};
use std::cell::RefCell;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::rc::Rc;

/// Output type for the extracted content.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum OutputType {
    /// Plain text output (default)
    #[default]
    Text,
    /// HTML output with positioning
    Html,
    /// XML output with full structure
    Xml,
    /// Tagged output
    Tag,
    /// hOCR output
    Hocr,
}

/// Layout mode for HTML output.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum LayoutMode {
    /// Normal mode - each line positioned separately
    #[default]
    Normal,
    /// Exact mode - each character positioned separately
    Exact,
    /// Loose mode - normal with extra newlines
    Loose,
}

/// A command line tool for extracting text and images from PDF and
/// outputting it to plain text, html, xml or tags.
#[derive(Parser, Debug)]
#[command(name = "pdf2txt")]
#[command(author, version, about, long_about = None)]
#[command(disable_version_flag = true)]
struct Args {
    /// One or more paths to PDF files
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Print version information
    #[arg(short = 'v', long = "version", action = ArgAction::Version)]
    version: (),

    /// Use debug logging level
    #[arg(short = 'd', long, action = ArgAction::SetTrue)]
    debug: bool,

    /// Disable caching of resources (fonts, etc.)
    #[arg(short = 'C', long = "disable-caching", action = ArgAction::SetTrue)]
    disable_caching: bool,

    // === Parser options ===
    /// A space-separated list of page numbers to parse (1-indexed)
    #[arg(long = "page-numbers")]
    page_numbers: Option<String>,

    /// A comma-separated list of page numbers to parse (1-indexed, legacy)
    #[arg(short = 'p', long = "pagenos")]
    pagenos: Option<String>,

    /// The maximum number of pages to parse (0 = no limit)
    #[arg(short = 'm', long, default_value = "0")]
    maxpages: usize,

    /// The password to use for decrypting PDF file
    #[arg(short = 'P', long, default_value = "")]
    password: String,

    /// The number of degrees to rotate the PDF before processing
    #[arg(short = 'R', long, default_value = "0")]
    rotation: i32,

    // === Layout analysis options ===
    /// Disable layout analysis parameters
    #[arg(short = 'n', long = "no-laparams", action = ArgAction::SetTrue)]
    no_laparams: bool,

    /// Consider vertical text during layout analysis
    #[arg(short = 'V', long = "detect-vertical", action = ArgAction::SetTrue)]
    detect_vertical: bool,

    /// Line overlap ratio (relative to character height)
    #[arg(long = "line-overlap", default_value = "0.5")]
    line_overlap: f64,

    /// Character margin (relative to character width)
    #[arg(short = 'M', long = "char-margin", default_value = "2.0")]
    char_margin: f64,

    /// Word margin (relative to character width)
    #[arg(short = 'W', long = "word-margin", default_value = "0.1")]
    word_margin: f64,

    /// Line margin (relative to line height)
    #[arg(short = 'L', long = "line-margin", default_value = "0.5")]
    line_margin: f64,

    /// Boxes flow direction (-1.0 to 1.0, or "disabled"). Default: 0.5
    #[arg(short = 'F', long = "boxes-flow")]
    boxes_flow: Option<String>,

    /// Perform layout analysis on text in figures
    #[arg(short = 'A', long = "all-texts", action = ArgAction::SetTrue)]
    all_texts: bool,

    // === Output options ===
    /// Path to file where output is written, or "-" for stdout
    #[arg(short = 'o', long, default_value = "-")]
    outfile: String,

    /// Type of output to generate
    #[arg(short = 't', long = "output_type", value_enum, default_value = "text")]
    output_type: OutputType,

    /// Text encoding for output file
    #[arg(short = 'c', long, default_value = "utf-8")]
    codec: String,

    /// Directory to extract images to (if not given, images are not extracted)
    #[arg(short = 'O', long = "output-dir")]
    output_dir: Option<PathBuf>,

    /// Layout mode for HTML output
    #[arg(short = 'Y', long, value_enum, default_value = "normal")]
    layoutmode: LayoutMode,

    /// Scale factor for HTML output
    #[arg(short = 's', long, default_value = "1.0")]
    scale: f64,

    /// Remove control characters from XML output
    #[arg(short = 'S', long = "strip-control", action = ArgAction::SetTrue)]
    strip_control: bool,
}

/// Parse boxes_flow value - either a float or "disabled".
fn parse_boxes_flow(s: &str) -> std::result::Result<Option<f64>, String> {
    let s = s.trim().to_lowercase();
    if s == "disabled" {
        return Ok(None);
    }

    match s.parse::<f64>() {
        Ok(v) => {
            if (-1.0..=1.0).contains(&v) {
                Ok(Some(v))
            } else {
                Err(format!(
                    "boxes_flow must be between -1.0 and 1.0, got {}",
                    v
                ))
            }
        }
        Err(_) => Err(format!("invalid float value: {}", s)),
    }
}

/// Infer output type from file extension.
fn infer_output_type(path: &str) -> Option<OutputType> {
    let path_lower = path.to_lowercase();
    if path_lower.ends_with(".htm") || path_lower.ends_with(".html") {
        Some(OutputType::Html)
    } else if path_lower.ends_with(".xml") {
        Some(OutputType::Xml)
    } else if path_lower.ends_with(".tag") {
        Some(OutputType::Tag)
    } else {
        None
    }
}

/// Build LAParams from command line arguments.
fn build_laparams(args: &Args) -> Result<Option<LAParams>> {
    if args.no_laparams {
        return Ok(None);
    }

    // boxes_flow: None means not specified (use default 0.5),
    // Some("disabled") means disabled, Some(v) means explicit value
    let boxes_flow = match args.boxes_flow.as_deref() {
        None => Some(0.5),
        Some(s) => parse_boxes_flow(s).map_err(PdfError::DecodeError)?,
    };

    Ok(Some(LAParams::new(
        args.line_overlap,
        args.char_margin,
        args.line_margin,
        args.word_margin,
        boxes_flow,
        args.detect_vertical,
        args.all_texts,
    )))
}

/// Parse page numbers from either --page-numbers or -p option.
fn parse_page_numbers(args: &Args) -> Option<Vec<usize>> {
    // --page-numbers takes precedence
    if let Some(ref nums) = args.page_numbers {
        let nums: Vec<usize> = nums
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .map(|n| n.saturating_sub(1))
            .collect();
        if !nums.is_empty() {
            return Some(nums);
        }
    }

    // Legacy -p option: comma-separated
    if let Some(ref pagenos) = args.pagenos {
        let nums: Vec<usize> = pagenos
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .map(|n| n.saturating_sub(1))
            .collect();
        if !nums.is_empty() {
            return Some(nums);
        }
    }

    None
}

fn for_each_page_with_images<F>(
    pdf_data: &[u8],
    options: &ExtractOptions,
    image_writer: Rc<RefCell<ImageWriter>>,
    mut on_page: F,
) -> Result<()>
where
    F: FnMut(bolivar_core::layout::LTPage),
{
    if pdf_data.len() < 8 || !pdf_data.starts_with(b"%PDF-") {
        return Err(PdfError::SyntaxError("Invalid PDF header".to_string()));
    }

    let doc = PDFDocument::new(pdf_data, &options.password)?;
    let mut rsrcmgr = PDFResourceManager::with_caching(options.caching);
    let laparams = options.laparams.clone().unwrap_or_default();

    let mut page_count = 0;
    for (page_idx, page_result) in PDFPage::create_pages(&doc).enumerate() {
        if let Some(ref nums) = options.page_numbers {
            if !nums.contains(&page_idx) {
                continue;
            }
        }

        if options.maxpages > 0 && page_count >= options.maxpages {
            break;
        }

        let page = page_result?;
        let mut aggregator = PDFPageAggregator::new_with_imagewriter(
            Some(laparams.clone()),
            page_idx as i32 + 1,
            Some(image_writer.clone()),
        );
        let mut interpreter = PDFPageInterpreter::new(&mut rsrcmgr, &mut aggregator);
        interpreter.process_page(&page, Some(&doc));

        let ltpage = aggregator.get_result().clone();
        on_page(ltpage);
        page_count += 1;
    }

    Ok(())
}

/// Process a single PDF file.
fn process_file<W: Write>(
    path: &PathBuf,
    writer: &mut W,
    args: &Args,
    output_type: OutputType,
) -> Result<()> {
    // Read PDF file
    let pdf_data = std::fs::read(path)?;

    // Build options
    let options = ExtractOptions {
        password: args.password.clone(),
        page_numbers: parse_page_numbers(args),
        maxpages: args.maxpages,
        caching: !args.disable_caching,
        laparams: build_laparams(args)?,
        threads: std::thread::available_parallelism().ok().map(|n| n.get()),
    };

    if let Some(ref output_dir) = args.output_dir {
        let image_writer = Rc::new(RefCell::new(ImageWriter::new(output_dir)?));
        match output_type {
            OutputType::Text => {
                let laparams = build_laparams(args)?;
                let mut converter = TextConverter::new(writer, &args.codec, 1, laparams, false);
                for_each_page_with_images(&pdf_data, &options, image_writer, |page| {
                    converter.receive_layout(page);
                })?;
            }
            OutputType::Html => {
                let laparams = build_laparams(args)?;
                let mut converter =
                    HTMLConverter::with_options(writer, &args.codec, 1, laparams, args.scale, 1.0);
                for_each_page_with_images(&pdf_data, &options, image_writer, |page| {
                    converter.receive_layout(page);
                })?;
                converter.close();
            }
            OutputType::Xml => {
                let laparams = build_laparams(args)?;
                let mut converter = XMLConverter::with_options(
                    writer,
                    &args.codec,
                    1,
                    laparams,
                    args.strip_control,
                );
                for_each_page_with_images(&pdf_data, &options, image_writer, |page| {
                    converter.receive_layout(page);
                })?;
                converter.close();
            }
            OutputType::Tag => {
                let laparams = build_laparams(args)?;
                let mut converter = TextConverter::new(writer, &args.codec, 1, laparams, false);
                for_each_page_with_images(&pdf_data, &options, image_writer, |page| {
                    converter.receive_layout(page);
                })?;
            }
            OutputType::Hocr => {
                let laparams = build_laparams(args)?;
                let mut converter = HOCRConverter::with_options(
                    writer,
                    &args.codec,
                    1,
                    laparams,
                    args.strip_control,
                );
                for_each_page_with_images(&pdf_data, &options, image_writer, |page| {
                    converter.receive_layout(page);
                })?;
                converter.close();
            }
        }

        return Ok(());
    }

    // Process based on output type
    match output_type {
        OutputType::Text => {
            let laparams = build_laparams(args)?;
            let mut converter = TextConverter::new(writer, &args.codec, 1, laparams, false);
            for page_result in extract_pages(&pdf_data, Some(options.clone()))? {
                let page = page_result?;
                converter.receive_layout(page);
            }
        }
        OutputType::Html => {
            let laparams = build_laparams(args)?;
            let mut converter = HTMLConverter::with_options(
                writer,
                &args.codec,
                1,
                laparams,
                args.scale,
                1.0, // fontscale
            );
            for page_result in extract_pages(&pdf_data, Some(options.clone()))? {
                let page = page_result?;
                converter.receive_layout(page);
            }
            converter.close();
        }
        OutputType::Xml => {
            let laparams = build_laparams(args)?;
            let mut converter =
                XMLConverter::with_options(writer, &args.codec, 1, laparams, args.strip_control);
            for page_result in extract_pages(&pdf_data, Some(options.clone()))? {
                let page = page_result?;
                converter.receive_layout(page);
            }
            converter.close();
        }
        OutputType::Tag => {
            // Tag output - fall back to text for now
            let laparams = build_laparams(args)?;
            let mut converter = TextConverter::new(writer, &args.codec, 1, laparams, false);
            for page_result in extract_pages(&pdf_data, Some(options.clone()))? {
                let page = page_result?;
                converter.receive_layout(page);
            }
        }
        OutputType::Hocr => {
            let laparams = build_laparams(args)?;
            let mut converter =
                HOCRConverter::with_options(writer, &args.codec, 1, laparams, args.strip_control);
            for page_result in extract_pages(&pdf_data, Some(options.clone()))? {
                let page = page_result?;
                converter.receive_layout(page);
            }
            converter.close();
        }
    }

    Ok(())
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Set up logging if debug mode
    if args.debug {
        // Debug logging would be set up here
        eprintln!("Debug mode enabled");
    }

    if let Err(e) = build_laparams(&args) {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    // Determine output type (may be inferred from output filename)
    let output_type = if matches!(args.output_type, OutputType::Text) && args.outfile != "-" {
        infer_output_type(&args.outfile).unwrap_or(args.output_type)
    } else {
        args.output_type
    };

    // Open output file or use stdout
    let mut output: Box<dyn Write> = if args.outfile == "-" {
        Box::new(BufWriter::new(io::stdout()))
    } else {
        let file = File::create(&args.outfile)
            .map_err(|e| format!("Failed to create output file {}: {}", args.outfile, e))?;
        Box::new(BufWriter::new(file))
    };

    // Process each input file
    for path in &args.files {
        if !path.exists() {
            eprintln!("Error: File not found: {}", path.display());
            std::process::exit(1);
        }

        if let Err(e) = process_file(path, &mut output, &args, output_type) {
            eprintln!("Error processing {}: {}", path.display(), e);
            std::process::exit(1);
        }
    }

    // Ensure output is flushed
    output.flush()?;

    Ok(())
}
