//! pdf2txt - Extract text and images from PDF files
//!
//! A command line tool for extracting text and images from PDF and
//! outputting it to plain text, html, xml or tags.
//!
//! Port of pdfminer.six tools/pdf2txt.py

use bolivar_core::api::stream::extract_tables_stream_from_doc_with_settings;
use bolivar_core::converter::{HOCRConverter, HTMLConverter, TextConverter, XMLConverter};
use bolivar_core::error::{PdfError, Result};
use bolivar_core::high_level::{
    ExtractOptions, extract_pages_with_document, extract_pages_with_images_with_document,
};
use bolivar_core::layout::LAParams;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::table::{ExplicitLine, TableProbePolicy, TableSettings, TextDir, TextSettings};
use clap::{ArgAction, Parser, ValueEnum};
use memmap2::Mmap;
use serde::Deserialize;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

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

/// Output format for table extraction.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum TableFormat {
    /// CSV format (default)
    #[default]
    Csv,
    /// JSON format
    Json,
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

    // === Table extraction options ===
    /// Extract tables instead of text
    #[arg(long = "extract-tables", action = ArgAction::SetTrue)]
    extract_tables: bool,

    /// Output format for table extraction (csv or json)
    #[arg(long = "table-format", value_enum, default_value = "csv")]
    table_format: TableFormat,

    /// Table settings JSON file path
    #[arg(long = "table-settings-json")]
    table_settings_json: Option<PathBuf>,

    /// Inline table settings JSON
    #[arg(long = "table-settings")]
    table_settings: Option<String>,

    /// Table vertical strategy
    #[arg(long = "table-vertical-strategy")]
    table_vertical_strategy: Option<String>,

    /// Table horizontal strategy
    #[arg(long = "table-horizontal-strategy")]
    table_horizontal_strategy: Option<String>,

    /// Explicit vertical lines (comma-separated)
    #[arg(long = "table-explicit-vertical-lines")]
    table_explicit_vertical_lines: Option<String>,

    /// Explicit horizontal lines (comma-separated)
    #[arg(long = "table-explicit-horizontal-lines")]
    table_explicit_horizontal_lines: Option<String>,

    /// Snap X tolerance
    #[arg(long = "table-snap-x-tolerance")]
    table_snap_x_tolerance: Option<f64>,

    /// Snap Y tolerance
    #[arg(long = "table-snap-y-tolerance")]
    table_snap_y_tolerance: Option<f64>,

    /// Join X tolerance
    #[arg(long = "table-join-x-tolerance")]
    table_join_x_tolerance: Option<f64>,

    /// Join Y tolerance
    #[arg(long = "table-join-y-tolerance")]
    table_join_y_tolerance: Option<f64>,

    /// Intersection X tolerance
    #[arg(long = "table-intersection-x-tolerance")]
    table_intersection_x_tolerance: Option<f64>,

    /// Intersection Y tolerance
    #[arg(long = "table-intersection-y-tolerance")]
    table_intersection_y_tolerance: Option<f64>,

    /// Edge minimum length
    #[arg(long = "table-edge-min-length")]
    table_edge_min_length: Option<f64>,

    /// Edge minimum length prefilter
    #[arg(long = "table-edge-min-length-prefilter")]
    table_edge_min_length_prefilter: Option<f64>,

    /// Minimum words for vertical strategy
    #[arg(long = "table-min-words-vertical")]
    table_min_words_vertical: Option<usize>,

    /// Minimum words for horizontal strategy
    #[arg(long = "table-min-words-horizontal")]
    table_min_words_horizontal: Option<usize>,
}

#[derive(Default, Deserialize)]
struct TextSettingsPatch {
    x_tolerance: Option<f64>,
    y_tolerance: Option<f64>,
    x_tolerance_ratio: Option<f64>,
    y_tolerance_ratio: Option<f64>,
    keep_blank_chars: Option<bool>,
    use_text_flow: Option<bool>,
    vertical_ttb: Option<bool>,
    horizontal_ltr: Option<bool>,
    line_dir: Option<String>,
    char_dir: Option<String>,
    line_dir_rotated: Option<String>,
    char_dir_rotated: Option<String>,
    split_at_punctuation: Option<String>,
    expand_ligatures: Option<bool>,
    layout: Option<bool>,
}

#[derive(Default, Deserialize)]
struct TableSettingsPatch {
    vertical_strategy: Option<String>,
    horizontal_strategy: Option<String>,
    explicit_vertical_lines: Option<Vec<f64>>,
    explicit_horizontal_lines: Option<Vec<f64>>,
    snap_x_tolerance: Option<f64>,
    snap_y_tolerance: Option<f64>,
    join_x_tolerance: Option<f64>,
    join_y_tolerance: Option<f64>,
    join_tolerance: Option<f64>,
    intersection_x_tolerance: Option<f64>,
    intersection_y_tolerance: Option<f64>,
    edge_min_length: Option<f64>,
    edge_min_length_prefilter: Option<f64>,
    min_words_vertical: Option<usize>,
    min_words_horizontal: Option<usize>,
    text_settings: Option<TextSettingsPatch>,
}

fn parse_text_dir(value: &str) -> Result<TextDir> {
    TextDir::from_str(value)
        .map_err(|_| PdfError::DecodeError(format!("invalid text direction: {value}")))
}

fn apply_text_settings_patch(settings: &mut TextSettings, patch: TextSettingsPatch) -> Result<()> {
    if let Some(v) = patch.x_tolerance {
        settings.x_tolerance = v;
    }
    if let Some(v) = patch.y_tolerance {
        settings.y_tolerance = v;
    }
    if let Some(v) = patch.x_tolerance_ratio {
        settings.x_tolerance_ratio = Some(v);
    }
    if let Some(v) = patch.y_tolerance_ratio {
        settings.y_tolerance_ratio = Some(v);
    }
    if let Some(v) = patch.keep_blank_chars {
        settings.keep_blank_chars = v;
    }
    if let Some(v) = patch.use_text_flow {
        settings.use_text_flow = v;
    }
    if let Some(v) = patch.vertical_ttb {
        settings.vertical_ttb = v;
    }
    if let Some(v) = patch.horizontal_ltr {
        settings.horizontal_ltr = v;
    }
    if let Some(v) = patch.line_dir {
        settings.line_dir = parse_text_dir(&v)?;
    }
    if let Some(v) = patch.char_dir {
        settings.char_dir = parse_text_dir(&v)?;
    }
    if let Some(v) = patch.line_dir_rotated {
        settings.line_dir_rotated = Some(parse_text_dir(&v)?);
    }
    if let Some(v) = patch.char_dir_rotated {
        settings.char_dir_rotated = Some(parse_text_dir(&v)?);
    }
    if let Some(v) = patch.split_at_punctuation {
        settings.split_at_punctuation = v;
    }
    if let Some(v) = patch.expand_ligatures {
        settings.expand_ligatures = v;
    }
    if let Some(v) = patch.layout {
        settings.layout = v;
    }
    Ok(())
}

fn apply_table_settings_patch(
    settings: &mut TableSettings,
    patch: TableSettingsPatch,
) -> Result<()> {
    if let Some(v) = patch.vertical_strategy {
        settings.vertical_strategy = v;
    }
    if let Some(v) = patch.horizontal_strategy {
        settings.horizontal_strategy = v;
    }
    if let Some(lines) = patch.explicit_vertical_lines {
        settings.explicit_vertical_lines = lines.into_iter().map(ExplicitLine::Coord).collect();
    }
    if let Some(lines) = patch.explicit_horizontal_lines {
        settings.explicit_horizontal_lines = lines.into_iter().map(ExplicitLine::Coord).collect();
    }
    if let Some(v) = patch.snap_x_tolerance {
        settings.snap_x_tolerance = v;
    }
    if let Some(v) = patch.snap_y_tolerance {
        settings.snap_y_tolerance = v;
    }
    if let Some(v) = patch.join_tolerance {
        settings.join_x_tolerance = v;
        settings.join_y_tolerance = v;
    }
    if let Some(v) = patch.join_x_tolerance {
        settings.join_x_tolerance = v;
    }
    if let Some(v) = patch.join_y_tolerance {
        settings.join_y_tolerance = v;
    }
    if let Some(v) = patch.intersection_x_tolerance {
        settings.intersection_x_tolerance = v;
    }
    if let Some(v) = patch.intersection_y_tolerance {
        settings.intersection_y_tolerance = v;
    }
    if let Some(v) = patch.edge_min_length {
        settings.edge_min_length = v;
    }
    if let Some(v) = patch.edge_min_length_prefilter {
        settings.edge_min_length_prefilter = v;
    }
    if let Some(v) = patch.min_words_vertical {
        settings.min_words_vertical = v;
    }
    if let Some(v) = patch.min_words_horizontal {
        settings.min_words_horizontal = v;
    }
    if let Some(patch) = patch.text_settings {
        apply_text_settings_patch(&mut settings.text_settings, patch)?;
    }
    Ok(())
}

fn parse_table_settings_json_str(input: &str) -> Result<TableSettingsPatch> {
    serde_json::from_str(input)
        .map_err(|e| PdfError::DecodeError(format!("table settings json error: {e}")))
}

fn parse_table_settings_json_file(path: &PathBuf) -> Result<TableSettingsPatch> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| PdfError::DecodeError(format!("table settings read error: {e}")))?;
    parse_table_settings_json_str(&content)
}

fn parse_f64_list(input: &str) -> Result<Vec<f64>> {
    let mut out = Vec::new();
    for part in input.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = trimmed
            .parse::<f64>()
            .map_err(|e| PdfError::DecodeError(format!("invalid float value '{trimmed}': {e}")))?;
        out.push(value);
    }
    Ok(out)
}

fn build_table_settings(args: &Args, json_file_override: Option<&str>) -> Result<TableSettings> {
    let mut settings = TableSettings::default();
    let mut has_override = false;

    if let Some(override_str) = json_file_override {
        has_override = true;
        let patch = parse_table_settings_json_str(override_str)?;
        apply_table_settings_patch(&mut settings, patch)?;
    } else if let Some(ref path) = args.table_settings_json {
        has_override = true;
        let patch = parse_table_settings_json_file(path)?;
        apply_table_settings_patch(&mut settings, patch)?;
    }

    if let Some(ref inline) = args.table_settings {
        has_override = true;
        let patch = parse_table_settings_json_str(inline)?;
        apply_table_settings_patch(&mut settings, patch)?;
    }

    if let Some(ref v) = args.table_vertical_strategy {
        has_override = true;
        settings.vertical_strategy = v.clone();
    }
    if let Some(ref v) = args.table_horizontal_strategy {
        has_override = true;
        settings.horizontal_strategy = v.clone();
    }
    if let Some(ref v) = args.table_explicit_vertical_lines {
        has_override = true;
        let lines = parse_f64_list(v)?;
        settings.explicit_vertical_lines = lines.into_iter().map(ExplicitLine::Coord).collect();
    }
    if let Some(ref v) = args.table_explicit_horizontal_lines {
        has_override = true;
        let lines = parse_f64_list(v)?;
        settings.explicit_horizontal_lines = lines.into_iter().map(ExplicitLine::Coord).collect();
    }
    if let Some(v) = args.table_snap_x_tolerance {
        has_override = true;
        settings.snap_x_tolerance = v;
    }
    if let Some(v) = args.table_snap_y_tolerance {
        has_override = true;
        settings.snap_y_tolerance = v;
    }
    if let Some(v) = args.table_join_x_tolerance {
        has_override = true;
        settings.join_x_tolerance = v;
    }
    if let Some(v) = args.table_join_y_tolerance {
        has_override = true;
        settings.join_y_tolerance = v;
    }
    if let Some(v) = args.table_intersection_x_tolerance {
        has_override = true;
        settings.intersection_x_tolerance = v;
    }
    if let Some(v) = args.table_intersection_y_tolerance {
        has_override = true;
        settings.intersection_y_tolerance = v;
    }
    if let Some(v) = args.table_edge_min_length {
        has_override = true;
        settings.edge_min_length = v;
    }
    if let Some(v) = args.table_edge_min_length_prefilter {
        has_override = true;
        settings.edge_min_length_prefilter = v;
    }
    if let Some(v) = args.table_min_words_vertical {
        has_override = true;
        settings.min_words_vertical = v;
    }
    if let Some(v) = args.table_min_words_horizontal {
        has_override = true;
        settings.min_words_horizontal = v;
    }
    if has_override {
        settings.probe_policy = TableProbePolicy::Never;
    }

    Ok(settings)
}

/// Parse `boxes_flow` value - either a float or "disabled".
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
                Err(format!("boxes_flow must be between -1.0 and 1.0, got {v}"))
            }
        }
        Err(_) => Err(format!("invalid float value: {s}")),
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

/// Build `LAParams` from command line arguments.
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

/// Escape a string for RFC 4180 compliant CSV output.
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Process a single PDF file.
fn process_file<W: Write>(
    path: &PathBuf,
    writer: &mut W,
    args: &Args,
    output_type: OutputType,
) -> Result<()> {
    // Read PDF file via mmap
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| PdfError::Io(io::Error::other(format!("Failed to mmap PDF: {e}"))))?;

    // Build options
    let options = ExtractOptions {
        password: args.password.clone(),
        page_numbers: parse_page_numbers(args),
        maxpages: args.maxpages,
        caching: !args.disable_caching,
        laparams: build_laparams(args)?,
    };

    // Create PDFDocument from mmap
    let doc = Arc::new(PDFDocument::new_from_mmap(mmap, &options.password)?);

    // Handle table extraction mode
    if args.extract_tables {
        let settings = build_table_settings(args, None)?;
        let stream =
            extract_tables_stream_from_doc_with_settings(Arc::clone(&doc), options, settings)?;

        match args.table_format {
            TableFormat::Csv => {
                for item in stream {
                    let (page_idx, tables) = item?;
                    let page_num = page_idx + 1;
                    for (table_idx, table) in tables.iter().enumerate() {
                        writeln!(writer, "--- Page {} Table {} ---", page_num, table_idx + 1)?;
                        for row in table {
                            let cells: Vec<String> = row
                                .iter()
                                .map(|cell| csv_escape(cell.as_deref().unwrap_or("")))
                                .collect();
                            writeln!(writer, "{}", cells.join(","))?;
                        }
                        writeln!(writer)?;
                    }
                }
            }
            TableFormat::Json => {
                writeln!(writer, "{{")?;
                writeln!(writer, "  \"pages\": [")?;

                let mut iter = stream.peekable();
                while let Some(item) = iter.next() {
                    let (page_idx, tables) = item?;
                    let page_num = page_idx + 1;
                    let page_tables: Vec<serde_json::Value> = tables
                        .iter()
                        .map(|table| {
                            let rows: Vec<serde_json::Value> = table
                                .iter()
                                .map(|row| {
                                    serde_json::Value::Array(
                                        row.iter()
                                            .map(|cell| match cell {
                                                Some(s) => serde_json::Value::String(s.clone()),
                                                None => serde_json::Value::Null,
                                            })
                                            .collect(),
                                    )
                                })
                                .collect();
                            serde_json::json!({ "rows": rows })
                        })
                        .collect();
                    let page_json = serde_json::json!({
                        "page": page_num,
                        "tables": page_tables
                    });

                    let trailing_comma = iter.peek().is_some();
                    let page_json_str =
                        serde_json::to_string_pretty(&page_json).expect("json serialize");
                    let lines: Vec<&str> = page_json_str.lines().collect();
                    for (idx, line) in lines.iter().enumerate() {
                        if idx + 1 == lines.len() && trailing_comma {
                            writeln!(writer, "    {line},")?;
                        } else {
                            writeln!(writer, "    {line}")?;
                        }
                    }
                }

                writeln!(writer, "  ]")?;
                writeln!(writer, "}}")?;
            }
        }

        return Ok(());
    }

    if let Some(ref output_dir) = args.output_dir {
        let output_dir = output_dir.to_string_lossy();
        let pages =
            extract_pages_with_images_with_document(&doc, options.clone(), output_dir.as_ref())?;
        match output_type {
            OutputType::Text => {
                let laparams = build_laparams(args)?;
                let mut converter = TextConverter::new(writer, &args.codec, 1, laparams, false);
                for page in pages {
                    converter.receive_layout(page);
                }
            }
            OutputType::Html => {
                let laparams = build_laparams(args)?;
                let mut converter =
                    HTMLConverter::with_options(writer, &args.codec, 1, laparams, args.scale, 1.0);
                for page in pages {
                    converter.receive_layout(page);
                }
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
                for page in pages {
                    converter.receive_layout(page);
                }
                converter.close();
            }
            OutputType::Tag => {
                let laparams = build_laparams(args)?;
                let mut converter = TextConverter::new(writer, &args.codec, 1, laparams, false);
                for page in pages {
                    converter.receive_layout(page);
                }
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
                for page in pages {
                    converter.receive_layout(page);
                }
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
            for page in extract_pages_with_document(&doc, options)? {
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
            for page in extract_pages_with_document(&doc, options)? {
                converter.receive_layout(page);
            }
            converter.close();
        }
        OutputType::Xml => {
            let laparams = build_laparams(args)?;
            let mut converter =
                XMLConverter::with_options(writer, &args.codec, 1, laparams, args.strip_control);
            for page in extract_pages_with_document(&doc, options)? {
                converter.receive_layout(page);
            }
            converter.close();
        }
        OutputType::Tag => {
            // Tag output - fall back to text for now
            let laparams = build_laparams(args)?;
            let mut converter = TextConverter::new(writer, &args.codec, 1, laparams, false);
            for page in extract_pages_with_document(&doc, options)? {
                converter.receive_layout(page);
            }
        }
        OutputType::Hocr => {
            let laparams = build_laparams(args)?;
            let mut converter =
                HOCRConverter::with_options(writer, &args.codec, 1, laparams, args.strip_control);
            for page in extract_pages_with_document(&doc, options)? {
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
        eprintln!("{e}");
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

#[cfg(test)]
mod tests {
    use super::*;
    use bolivar_core::table::{ExplicitLine, TableProbePolicy};

    fn test_args_with_table_settings(_json_file: &str, _json_inline: &str) -> Args {
        Args {
            files: vec![PathBuf::from("dummy.pdf")],
            version: (),
            debug: false,
            disable_caching: false,
            page_numbers: None,
            pagenos: None,
            maxpages: 0,
            password: String::new(),
            rotation: 0,
            no_laparams: false,
            detect_vertical: false,
            line_overlap: 0.5,
            char_margin: 2.0,
            word_margin: 0.1,
            line_margin: 0.5,
            boxes_flow: None,
            all_texts: false,
            outfile: "-".to_string(),
            output_type: OutputType::Text,
            codec: "utf-8".to_string(),
            output_dir: None,
            layoutmode: LayoutMode::Normal,
            scale: 1.0,
            strip_control: false,
            extract_tables: false,
            table_format: TableFormat::Csv,
            table_settings_json: None,
            table_settings: Some(_json_inline.to_string()),
            table_vertical_strategy: None,
            table_horizontal_strategy: None,
            table_explicit_vertical_lines: None,
            table_explicit_horizontal_lines: None,
            table_snap_x_tolerance: None,
            table_snap_y_tolerance: None,
            table_join_x_tolerance: None,
            table_join_y_tolerance: None,
            table_intersection_x_tolerance: None,
            table_intersection_y_tolerance: None,
            table_edge_min_length: None,
            table_edge_min_length_prefilter: None,
            table_min_words_vertical: None,
            table_min_words_horizontal: None,
        }
    }

    #[test]
    fn table_settings_merge_precedence() {
        let json_file = r#"{"vertical_strategy":"explicit","snap_x_tolerance":9}"#;
        let json_inline = r#"{"vertical_strategy":"lines","snap_x_tolerance":5}"#;
        let mut args = test_args_with_table_settings(json_file, json_inline);
        args.table_vertical_strategy = Some("text".to_string());
        let settings = build_table_settings(&args, Some(json_file)).unwrap();
        assert_eq!(settings.vertical_strategy, "text");
        assert!((settings.snap_x_tolerance - 5.0).abs() < 1e-9);
    }

    #[test]
    fn table_settings_explicit_lines_parse() {
        let mut args = test_args_with_table_settings("", "");
        args.table_settings = None;
        args.table_explicit_vertical_lines = Some("10, 20, 30".to_string());
        let settings = build_table_settings(&args, None).unwrap();
        assert_eq!(settings.explicit_vertical_lines.len(), 3);
        assert!(matches!(
            settings.explicit_vertical_lines[0],
            ExplicitLine::Coord(v) if (v - 10.0).abs() < 1e-9
        ));
    }

    #[test]
    fn table_settings_override_disables_probe() {
        let mut args = test_args_with_table_settings("", r#"{"snap_x_tolerance":8}"#);
        args.table_settings_json = None;
        let settings = build_table_settings(&args, None).unwrap();
        assert_eq!(settings.probe_policy, TableProbePolicy::Never);
    }
}
