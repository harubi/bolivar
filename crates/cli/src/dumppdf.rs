//! dumppdf - Extract PDF structure in XML format
//!
//! A command line tool for dumping PDF internal structure as XML.
//!
//! Port of pdfminer.six tools/dumppdf.py

use bolivar_core::error::Result;
use bolivar_core::pdfdocument::PDFDocument;
use bolivar_core::pdfpage::PDFPage;
use bolivar_core::pdftypes::PDFObject;
use clap::{ArgAction, ArgGroup, Parser};
use memmap2::Mmap;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

/// Escape special characters for XML output.
fn escape(s: &[u8]) -> String {
    let mut result = String::new();
    for &byte in s {
        match byte {
            b'&' => result.push_str("&amp;"),
            b'<' => result.push_str("&lt;"),
            b'>' => result.push_str("&gt;"),
            b'"' => result.push_str("&quot;"),
            b'\'' => result.push_str("&#39;"),
            b'\\' => result.push_str("&#92;"),
            0..=31 | 127..=255 => {
                result.push_str(&format!("&#{byte};"));
            }
            _ => result.push(byte as char),
        }
    }
    result
}

/// Escape a string for XML output.
fn escape_str(s: &str) -> String {
    escape(s.as_bytes())
}

/// Stream codec for output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamCodec {
    None,
    Raw,
    Binary,
    Text,
}

/// Dump a PDF object as XML.
fn dumpxml<W: Write>(out: &mut W, obj: &PDFObject, codec: StreamCodec) -> Result<()> {
    match obj {
        PDFObject::Null => {
            write!(out, "<null />")?;
        }
        PDFObject::Bool(b) => {
            write!(
                out,
                "<boolean>{}</boolean>",
                if *b { "true" } else { "false" }
            )?;
        }
        PDFObject::Int(n) => {
            write!(out, "<number>{n}</number>")?;
        }
        PDFObject::Real(n) => {
            write!(out, "<number>{n}</number>")?;
        }
        PDFObject::String(s) => {
            write!(out, r#"<string size="{}">{}</string>"#, s.len(), escape(s))?;
        }
        PDFObject::Name(name) => {
            write!(out, "<literal>{}</literal>", escape_str(name))?;
        }
        PDFObject::Array(arr) => {
            writeln!(out, r#"<list size="{}">"#, arr.len())?;
            for item in arr {
                dumpxml(out, item, codec)?;
                writeln!(out)?;
            }
            write!(out, "</list>")?;
        }
        PDFObject::Dict(dict) => {
            writeln!(out, r#"<dict size="{}">"#, dict.len())?;
            for (k, v) in dict {
                writeln!(out, "<key>{}</key>", escape_str(k))?;
                write!(out, "<value>")?;
                dumpxml(out, v, codec)?;
                writeln!(out, "</value>")?;
            }
            write!(out, "</dict>")?;
        }
        PDFObject::Stream(stream) => {
            match codec {
                StreamCodec::Raw => {
                    // Write raw stream data
                    out.write_all(stream.get_rawdata())?;
                }
                StreamCodec::Binary => {
                    // Write decoded stream data
                    out.write_all(stream.get_data())?;
                }
                StreamCodec::Text | StreamCodec::None => {
                    writeln!(out, "<stream>")?;
                    writeln!(out, "<props>")?;
                    dumpxml(out, &PDFObject::Dict(stream.attrs.clone()), codec)?;
                    writeln!(out)?;
                    writeln!(out, "</props>")?;
                    if codec == StreamCodec::Text {
                        let data = stream.get_data();
                        writeln!(
                            out,
                            r#"<data size="{}">{}</data>"#,
                            data.len(),
                            escape(data)
                        )?;
                    }
                    write!(out, "</stream>")?;
                }
            }
        }
        PDFObject::Ref(objref) => {
            write!(out, r#"<ref id="{}" />"#, objref.objid)?;
        }
    }
    Ok(())
}

/// Dump all trailers from the document.
///
/// Iterates over all xref trailers, optionally skipping fallback xrefs.
fn dumptrailers<W: Write>(out: &mut W, doc: &PDFDocument, show_fallback_xref: bool) -> Result<()> {
    let mut any_non_fallback = false;

    for (is_fallback, trailer) in doc.get_trailers() {
        if !is_fallback {
            any_non_fallback = true;
        }
        if !is_fallback || show_fallback_xref {
            writeln!(out, "<trailer>")?;
            dumpxml(out, &PDFObject::Dict(trailer.clone()), StreamCodec::None)?;
            writeln!(out)?;
            writeln!(out, "</trailer>")?;
            writeln!(out)?;
        }
    }

    // Warn if all xrefs are fallback and --show-fallback-xref not set
    if !any_non_fallback && !show_fallback_xref {
        eprintln!(
            "Warning: This PDF does not have a valid xref. Use --show-fallback-xref \
             to display the content of a fallback xref that contains all objects."
        );
    }

    Ok(())
}

/// Dump all objects from the document.
///
/// Iterates over all xref entries to enumerate ALL objects.
fn dumpallobjs<W: Write>(
    out: &mut W,
    doc: &PDFDocument,
    codec: StreamCodec,
    show_fallback_xref: bool,
) -> Result<()> {
    write!(out, "<pdf>")?;

    // Iterate over ALL object IDs from xrefs (not just pages)
    let mut visited = HashSet::new();

    for objid in doc.get_objids() {
        if visited.contains(&objid) {
            continue;
        }
        visited.insert(objid);

        match doc.getobj(objid) {
            Ok(obj) => {
                writeln!(out, r#"<object id="{objid}">"#)?;
                dumpxml(out, &obj, codec)?;
                writeln!(out)?;
                writeln!(out, "</object>")?;
                writeln!(out)?;
            }
            Err(e) => {
                eprintln!("not found: object {objid} - {e:?}");
            }
        }
    }

    // Dump trailers
    dumptrailers(out, doc, show_fallback_xref)?;

    write!(out, "</pdf>")?;
    Ok(())
}

/// Dump outline/table of contents.
fn dumpoutline<W: Write>(out: &mut W, doc: &PDFDocument) -> Result<()> {
    // Build page ID to page number mapping
    let mut pages = HashMap::new();
    for (pageno, page_result) in PDFPage::create_pages(doc).enumerate() {
        if let Ok(p) = page_result {
            pages.insert(p.pageid, pageno + 1);
        }
    }

    writeln!(out, "<outlines>")?;

    // Check for Outlines in catalog
    if let Some(outlines_ref) = doc.catalog().get("Outlines")
        && let Ok(outlines) = doc.resolve(outlines_ref)
        && let Ok(dict) = outlines.as_dict()
    {
        // Traverse outline tree
        if let Some(first_ref) = dict.get("First") {
            dump_outline_item(out, doc, first_ref, &pages, 0)?;
        }
    }

    writeln!(out, "</outlines>")?;
    Ok(())
}

/// Dump a single outline item and its siblings.
fn dump_outline_item<W: Write>(
    out: &mut W,
    doc: &PDFDocument,
    item_ref: &PDFObject,
    pages: &HashMap<u32, usize>,
    level: usize,
) -> Result<()> {
    let item = doc.resolve(item_ref)?;
    let dict = match item.as_dict() {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };

    // Get title
    let title = if let Some(title_obj) = dict.get("Title") {
        if let Ok(title_resolved) = doc.resolve(title_obj) {
            if let Ok(s) = title_resolved.as_string() {
                String::from_utf8_lossy(s).to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Try to get page number from Dest or A
    let mut pageno = None;
    if let Some(dest) = dict.get("Dest") {
        pageno = resolve_dest_to_pageno(doc, dest, pages);
    } else if let Some(action) = dict.get("A")
        && let Ok(a) = doc.resolve(action)
        && let Ok(action_dict) = a.as_dict()
        && let Some(PDFObject::Name(subtype)) = action_dict.get("S")
        && subtype == "GoTo"
        && let Some(d) = action_dict.get("D")
    {
        pageno = resolve_dest_to_pageno(doc, d, pages);
    }

    writeln!(
        out,
        r#"<outline level="{}" title="{}">"#,
        level,
        escape_str(&title)
    )?;

    if let Some(dest) = dict.get("Dest") {
        write!(out, "<dest>")?;
        if let Ok(resolved) = doc.resolve(dest) {
            dumpxml(out, &resolved, StreamCodec::None)?;
        }
        writeln!(out, "</dest>")?;
    }

    if let Some(pn) = pageno {
        writeln!(out, "<pageno>{pn}</pageno>")?;
    }

    writeln!(out, "</outline>")?;

    // Recurse to children
    if let Some(first) = dict.get("First") {
        dump_outline_item(out, doc, first, pages, level + 1)?;
    }

    // Process siblings
    if let Some(next) = dict.get("Next") {
        dump_outline_item(out, doc, next, pages, level)?;
    }

    Ok(())
}

/// Resolve a destination to its full form.
///
/// Handles named destinations (strings/names) via `doc.get_dest()` lookup.
fn resolve_dest(doc: &PDFDocument, dest: &PDFObject) -> Option<PDFObject> {
    let resolved = doc.resolve(dest).ok()?;

    match &resolved {
        PDFObject::String(s) => {
            // Named destination - look up via Names/Dests
            doc.get_dest(s).ok()
        }
        PDFObject::Name(name) => {
            // Named destination as name literal
            doc.get_dest(name.as_bytes()).ok()
        }
        PDFObject::Dict(dict) => {
            // Destination dict with "D" key
            if let Some(d) = dict.get("D") {
                doc.resolve(d).ok()
            } else {
                Some(resolved)
            }
        }
        _ => Some(resolved),
    }
}

/// Resolve a destination to a page number.
fn resolve_dest_to_pageno(
    doc: &PDFDocument,
    dest: &PDFObject,
    pages: &HashMap<u32, usize>,
) -> Option<usize> {
    let resolved = resolve_dest(doc, dest)?;

    match &resolved {
        PDFObject::Array(arr) => {
            if arr.is_empty() {
                None
            } else if let PDFObject::Ref(objref) = &arr[0] {
                pages.get(&objref.objid).copied()
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract embedded files.
fn extractembedded(doc: &PDFDocument, extractdir: &str) -> Result<()> {
    std::fs::create_dir_all(extractdir)?;

    // Look for embedded files in Names/EmbeddedFiles
    if let Some(names_ref) = doc.catalog().get("Names")
        && let Ok(names) = doc.resolve(names_ref)
        && let Ok(names_dict) = names.as_dict()
        && let Some(ef_ref) = names_dict.get("EmbeddedFiles")
        && let Ok(ef) = doc.resolve(ef_ref)
    {
        extract_embedded_files_from_tree(doc, &ef, extractdir)?;
    }

    Ok(())
}

/// Extract embedded files from a name tree.
fn extract_embedded_files_from_tree(
    doc: &PDFDocument,
    tree: &PDFObject,
    extractdir: &str,
) -> Result<()> {
    let dict = match tree.as_dict() {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };

    // Check Names array (leaf node)
    if let Some(names_arr) = dict.get("Names")
        && let Ok(arr) = doc.resolve(names_arr)?.as_array()
    {
        // Names array is pairs: [name1, filespec1, name2, filespec2, ...]
        let mut i = 0;
        while i + 1 < arr.len() {
            let name = &arr[i];
            let filespec_ref = &arr[i + 1];

            if let Ok(name_bytes) = name.as_string() {
                let filename = String::from_utf8_lossy(name_bytes).to_string();
                if let Ok(filespec) = doc.resolve(filespec_ref)
                    && let Ok(fs_dict) = filespec.as_dict()
                {
                    extract_single_embedded(doc, &filename, fs_dict, extractdir)?;
                }
            }
            i += 2;
        }
    }

    // Check Kids array (intermediate node)
    if let Some(kids) = dict.get("Kids")
        && let Ok(kids_arr) = doc.resolve(kids)?.as_array()
    {
        for kid in kids_arr {
            if let Ok(kid_tree) = doc.resolve(kid) {
                extract_embedded_files_from_tree(doc, &kid_tree, extractdir)?;
            }
        }
    }

    Ok(())
}

/// Extract a single embedded file.
fn extract_single_embedded(
    doc: &PDFDocument,
    filename: &str,
    filespec: &HashMap<String, PDFObject>,
    extractdir: &str,
) -> Result<()> {
    // Get the actual filename
    let basename = if let Some(PDFObject::String(uf)) = filespec.get("UF") {
        String::from_utf8_lossy(uf).to_string()
    } else if let Some(PDFObject::String(f)) = filespec.get("F") {
        String::from_utf8_lossy(f).to_string()
    } else {
        filename.to_owned()
    };

    let basename = std::path::Path::new(&basename)
        .file_name()
        .map_or_else(|| basename.clone(), |s| s.to_string_lossy().to_string());

    // Get EF dictionary
    let ef = match filespec.get("EF") {
        Some(obj) => doc.resolve(obj)?,
        None => return Ok(()),
    };

    let ef_dict = match ef.as_dict() {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };

    // Get file stream reference
    let file_ref = ef_dict.get("UF").or_else(|| ef_dict.get("F"));
    let file_ref = match file_ref {
        Some(r) => r,
        None => return Ok(()),
    };

    let file_obj = doc.resolve(file_ref)?;
    let stream = if let Ok(s) = file_obj.as_stream() {
        s
    } else {
        eprintln!("Warning: reference for {basename} is not a stream");
        return Ok(());
    };

    // Build output path
    let path = format!("{extractdir}/{basename}");

    if std::path::Path::new(&path).exists() {
        eprintln!("Warning: file exists: {path}");
        return Ok(());
    }

    eprintln!("extracting: {path}");

    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write file
    let data = stream.get_data();
    std::fs::write(&path, data)?;

    Ok(())
}

/// Main PDF dump function.
fn dumppdf<W: Write>(
    out: &mut W,
    doc: &PDFDocument,
    objids: &[u32],
    pagenos: &HashSet<usize>,
    codec: StreamCodec,
    show_fallback_xref: bool,
) -> Result<()> {
    // Dump specific objects
    if !objids.is_empty() {
        for &objid in objids {
            if let Ok(obj) = doc.getobj(objid) {
                dumpxml(out, &obj, codec)?;
            }
        }
    }

    // Dump specific pages
    if !pagenos.is_empty() {
        for (pageno, page_result) in PDFPage::create_pages(doc).enumerate() {
            if pagenos.contains(&pageno)
                && let Ok(page) = page_result
            {
                if codec == StreamCodec::None {
                    // Dump page attributes
                    dumpxml(out, &PDFObject::Dict(page.attrs.clone()), codec)?;
                } else {
                    // Dump page contents as stream
                    if let Some(contents) = page.attrs.get("Contents")
                        && let Ok(resolved) = doc.resolve(contents)
                    {
                        dumpxml(out, &resolved, codec)?;
                    }
                }
            }
        }
    }

    // If no specific objects or pages, dump trailers only
    if objids.is_empty() && pagenos.is_empty() {
        dumptrailers(out, doc, show_fallback_xref)?;
    }

    if codec != StreamCodec::Raw && codec != StreamCodec::Binary {
        writeln!(out)?;
    }

    Ok(())
}

/// A command line tool for dumping PDF internal structure as XML.
#[derive(Parser, Debug)]
#[command(name = "dumppdf")]
#[command(author, version, about = "Extract PDF structure in XML format", long_about = None)]
#[command(disable_version_flag = true)]
#[command(group(
    ArgGroup::new("procedure")
        .args(["extract_toc", "extract_embedded"])
))]
#[command(group(
    ArgGroup::new("stream_codec")
        .args(["raw_stream", "binary_stream", "text_stream"])
))]
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

    // === Procedure options (mutually exclusive) ===
    /// Extract structure of outline (table of contents)
    #[arg(short = 'T', long = "extract-toc", action = ArgAction::SetTrue)]
    extract_toc: bool,

    /// Extract embedded files to the specified directory
    #[arg(short = 'E', long = "extract-embedded")]
    extract_embedded: Option<String>,

    // === Parser options ===
    /// Page numbers to parse (1-indexed). Use --page-numbers=1,2,3 or multiple --page-numbers 1 --page-numbers 2
    #[arg(long = "page-numbers", value_delimiter = ',', action = clap::ArgAction::Append)]
    page_numbers: Option<Vec<usize>>,

    /// A comma-separated list of page numbers to parse (1-indexed, legacy)
    #[arg(short = 'p', long = "pagenos")]
    pagenos: Option<String>,

    /// Comma-separated list of object IDs to extract
    #[arg(short = 'i', long = "objects")]
    objects: Option<String>,

    /// Extract structure of all objects
    #[arg(short = 'a', long = "all", action = ArgAction::SetTrue)]
    all: bool,

    /// Show fallback xref if PDF has no valid xref
    #[arg(long = "show-fallback-xref", action = ArgAction::SetTrue)]
    show_fallback_xref: bool,

    /// The password to use for decrypting PDF file
    #[arg(short = 'P', long, default_value = "")]
    password: String,

    // === Output options ===
    /// Path to file where output is written, or "-" for stdout
    #[arg(short = 'o', long, default_value = "-")]
    outfile: String,

    /// Write stream objects without encoding (raw)
    #[arg(short = 'r', long = "raw-stream", action = ArgAction::SetTrue)]
    raw_stream: bool,

    /// Write stream objects with binary encoding
    #[arg(short = 'b', long = "binary-stream", action = ArgAction::SetTrue)]
    binary_stream: bool,

    /// Write stream objects as plain text
    #[arg(short = 't', long = "text-stream", action = ArgAction::SetTrue)]
    text_stream: bool,
}

fn main() -> core::result::Result<(), Box<dyn core::error::Error>> {
    let args = Args::parse();

    if args.debug {
        eprintln!("Debug mode enabled");
    }

    // Determine stream codec
    let codec = if args.raw_stream {
        StreamCodec::Raw
    } else if args.binary_stream {
        StreamCodec::Binary
    } else if args.text_stream {
        StreamCodec::Text
    } else {
        StreamCodec::None
    };

    // Parse object IDs
    let objids: Vec<u32> = if let Some(ref objs) = args.objects {
        objs.split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    } else {
        Vec::new()
    };

    // Parse page numbers
    let pagenos: HashSet<usize> = if let Some(ref nums) = args.page_numbers {
        nums.iter().map(|n| n.saturating_sub(1)).collect()
    } else if let Some(ref p) = args.pagenos {
        p.split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .map(|n| n.saturating_sub(1))
            .collect()
    } else {
        HashSet::new()
    };

    // Open output
    let mut output: Box<dyn Write> = if args.outfile == "-" {
        Box::new(BufWriter::new(io::stdout()))
    } else {
        let file = File::create(&args.outfile)?;
        Box::new(BufWriter::new(file))
    };

    // Process each input file
    for path in &args.files {
        if !path.exists() {
            eprintln!("Error: File not found: {}", path.display());
            std::process::exit(1);
        }

        // Read PDF
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file) }?;
        let doc = PDFDocument::new_from_mmap(mmap, &args.password)?;

        if args.extract_toc {
            dumpoutline(&mut output, &doc)?;
        } else if let Some(ref extractdir) = args.extract_embedded {
            extractembedded(&doc, extractdir)?;
        } else if args.all {
            dumpallobjs(&mut output, &doc, codec, args.show_fallback_xref)?;
        } else {
            dumppdf(
                &mut output,
                &doc,
                &objids,
                &pagenos,
                codec,
                args.show_fallback_xref,
            )?;
        }
    }

    output.flush()?;
    Ok(())
}
