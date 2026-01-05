//! Base types for PDF converters - port of pdfminer.six converter.py
//!
//! Provides common types for transforming PDF layout content into various output formats:
//! - PDFLayoutAnalyzer: Base device that creates layout objects from PDF content
//! - PDFPageAggregator: Collects analyzed pages for later retrieval
//! - PDFConverter: Base converter trait
//! - PathOp: Path operation type
//! - LTContainer: Container for layout items

use regex::Regex;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use crate::image::ImageWriter;
use crate::layout::{LAParams, LTChar, LTCurve, LTFigure, LTImage, LTItem, LTLine, LTPage, LTRect};
use crate::pdfcolor::PDFColorSpace;
use crate::pdfdevice::{PDFDevice, PDFTextSeq, PDFTextSeqItem, PathSegment};
use crate::pdffont::{CharDisp, PDFFont};
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::pdftypes::PDFStream;
use crate::utils::{Matrix, Point, Rect, apply_matrix_pt, apply_matrix_rect, mult_matrix};

/// Path operation with operator and operands.
pub type PathOp = (char, Vec<f64>);

/// Container for layout items during analysis.
#[derive(Debug, Clone)]
pub struct LTContainer {
    bbox: Rect,
    items: Vec<LTItem>,
}

impl LTContainer {
    /// Create a new container with the given bounding box.
    pub fn new(bbox: Rect) -> Self {
        Self {
            bbox,
            items: Vec::new(),
        }
    }

    /// Add an item to the container.
    pub fn add(&mut self, item: LTItem) {
        self.items.push(item);
    }

    /// Return the number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Return true if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get items slice.
    pub fn items(&self) -> &[LTItem] {
        &self.items
    }

    /// Get bounding box.
    pub fn bbox(&self) -> Rect {
        self.bbox
    }
}

/// PDF Layout Analyzer - creates layout objects from PDF page content.
///
/// Port of PDFLayoutAnalyzer from pdfminer.six converter.py
///
/// This device receives rendering commands from PDFPageInterpreter and
/// constructs layout objects (LTPage, LTFigure, LTChar, LTLine, etc.)
/// for downstream analysis or conversion.
/// State for tracking marked content.
#[derive(Debug, Clone)]
struct MarkedContentState {
    /// Tag name (e.g., "P", "Span", "H1")
    tag: String,
    /// Marked Content ID from properties dict
    mcid: Option<i32>,
}

#[derive(Debug, Clone)]
struct FigureState {
    name: String,
    bbox: Rect,
    matrix: Matrix,
}

pub struct PDFLayoutAnalyzer {
    /// Current page number (1-indexed)
    pageno: i32,
    /// Layout analysis parameters
    laparams: Option<LAParams>,
    /// Stack of layout containers (for nested figures)
    stack: Vec<LTContainer>,
    /// Stack of figure metadata (for nested figures)
    figure_stack: Vec<FigureState>,
    /// Current layout container
    pub(crate) cur_item: Option<LTContainer>,
    /// Current transformation matrix
    pub(crate) ctm: Matrix,
    /// Optional image writer for exporting images
    image_writer: Option<Rc<RefCell<ImageWriter>>>,
    /// Stack of marked content states (for BMC/BDC/EMC operators)
    marked_content_stack: Vec<MarkedContentState>,
}

impl PDFLayoutAnalyzer {
    /// Create a new layout analyzer.
    ///
    /// # Arguments
    /// * `laparams` - Layout analysis parameters (None to disable analysis)
    /// * `pageno` - Starting page number (1-indexed)
    pub fn new(laparams: Option<LAParams>, pageno: i32) -> Self {
        Self::new_with_imagewriter(laparams, pageno, None)
    }

    /// Create a new layout analyzer with an optional image writer.
    pub fn new_with_imagewriter(
        laparams: Option<LAParams>,
        pageno: i32,
        image_writer: Option<Rc<RefCell<ImageWriter>>>,
    ) -> Self {
        Self {
            pageno,
            laparams,
            stack: Vec::new(),
            figure_stack: Vec::new(),
            cur_item: None,
            ctm: (1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
            image_writer,
            marked_content_stack: Vec::new(),
        }
    }

    /// Get the current page number.
    pub fn pageno(&self) -> i32 {
        self.pageno
    }

    /// Set the current transformation matrix.
    pub fn set_ctm(&mut self, ctm: Matrix) {
        self.ctm = ctm;
    }

    /// Set the current layout container (for testing).
    pub fn set_cur_item(&mut self, container: LTContainer) {
        self.cur_item = Some(container);
    }

    /// Get the number of items in current container.
    pub fn cur_item_len(&self) -> usize {
        self.cur_item.as_ref().map(|c| c.len()).unwrap_or(0)
    }

    /// Check if first item is a rectangle.
    pub fn cur_item_first_is_rect(&self) -> bool {
        self.cur_item
            .as_ref()
            .and_then(|c| c.items.first())
            .map(|item| matches!(item, LTItem::Rect(_)))
            .unwrap_or(false)
    }

    /// Check if first item is a curve.
    pub fn cur_item_first_is_curve(&self) -> bool {
        self.cur_item
            .as_ref()
            .and_then(|c| c.items.first())
            .map(|item| matches!(item, LTItem::Curve(_)))
            .unwrap_or(false)
    }

    /// Check if all items are lines.
    pub fn all_cur_items_are_lines(&self) -> bool {
        self.cur_item
            .as_ref()
            .map(|c| c.items.iter().all(|item| matches!(item, LTItem::Line(_))))
            .unwrap_or(false)
    }

    /// Get points of first curve/line item.
    pub fn cur_item_first_pts(&self) -> Vec<Point> {
        self.cur_item
            .as_ref()
            .and_then(|c| c.items.first())
            .and_then(|item| match item {
                LTItem::Curve(c) => Some(c.pts.clone()),
                LTItem::Line(l) => Some(l.pts.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Get dashing style of first item.
    pub fn cur_item_first_dashing(&self) -> Option<(Vec<f64>, f64)> {
        self.cur_item
            .as_ref()
            .and_then(|c| c.items.first())
            .and_then(|item| match item {
                LTItem::Curve(c) => c.dashing_style.clone(),
                LTItem::Line(l) => l.dashing_style.clone(),
                _ => None,
            })
    }

    /// Get original_path of first curve item (for testing).
    pub fn cur_item_first_original_path(&self) -> Option<Vec<(char, Vec<Point>)>> {
        self.cur_item
            .as_ref()
            .and_then(|c| c.items.first())
            .and_then(|item| match item {
                LTItem::Curve(c) => c.original_path.clone(),
                LTItem::Line(l) => l.original_path.clone(),
                _ => None,
            })
    }

    /// Check if currently in a figure.
    pub fn in_figure(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Begin processing a page.
    pub fn begin_page(&mut self, mediabox: Rect, ctm: Matrix) {
        let (x0, y0, x1, y1) = apply_matrix_rect(ctm, mediabox);
        let bbox = (0.0, 0.0, (x0 - x1).abs(), (y0 - y1).abs());
        self.cur_item = Some(LTContainer::new(bbox));
    }

    /// End processing a page and return the analyzed page.
    pub fn end_page(&mut self) -> Option<LTPage> {
        assert!(self.stack.is_empty(), "stack not empty");
        if let Some(container) = self.cur_item.take() {
            let mut page = LTPage::new(self.pageno, container.bbox, 0.0);
            for item in container.items {
                page.add(item);
            }
            if let Some(ref laparams) = self.laparams {
                page.analyze(laparams);
            }
            self.pageno += 1;
            Some(page)
        } else {
            None
        }
    }

    /// Begin processing a figure (Form XObject).
    pub fn begin_figure(&mut self, name: &str, bbox: Rect, matrix: Matrix) {
        if let Some(cur) = self.cur_item.take() {
            self.stack.push(cur);
        }
        let combined_matrix = mult_matrix(matrix, self.ctm);
        let rect = (bbox.0, bbox.1, bbox.0 + bbox.2, bbox.1 + bbox.3);
        let fig_bbox = apply_matrix_rect(combined_matrix, rect);
        self.figure_stack.push(FigureState {
            name: name.to_string(),
            bbox,
            matrix: combined_matrix,
        });
        let fig_container = LTContainer::new(fig_bbox);
        self.cur_item = Some(fig_container);
    }

    /// End processing a figure.
    pub fn end_figure(&mut self, _name: &str) {
        let Some(fig_container) = self.cur_item.take() else {
            return;
        };
        let Some(fig_state) = self.figure_stack.pop() else {
            return;
        };
        let mut fig = LTFigure::new(&fig_state.name, fig_state.bbox, fig_state.matrix);
        for item in fig_container.items {
            fig.add(item);
        }
        if let Some(mut parent) = self.stack.pop() {
            parent.add(LTItem::Figure(Box::new(fig)));
            self.cur_item = Some(parent);
        }
    }

    /// Paint a graphics path.
    ///
    /// Paths are analyzed and converted to appropriate layout objects:
    /// - Single line segments become LTLine
    /// - Axis-aligned rectangles become LTRect
    /// - Other shapes become LTCurve
    pub fn paint_path(
        &mut self,
        gstate: &PDFGraphicState,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        path: &[PathOp],
    ) {
        // Extract shape string (just operators)
        let shape: String = path.iter().map(|(op, _)| *op).collect();

        // Path must start with 'm'
        if !shape.starts_with('m') {
            return;
        }

        // Handle multiple subpaths (multiple 'm' operators)
        if shape.matches('m').count() > 1 {
            // Split at each 'm' and recurse
            let re = Regex::new(r"m[^m]+").unwrap();
            let mut start = 0;
            for m in re.find_iter(&shape) {
                let subpath: Vec<PathOp> = path[m.start()..m.end()].to_vec();
                self.paint_path(gstate, stroke, fill, evenodd, &subpath);
                start = m.end();
            }
            let _ = start; // silence warning
            return;
        }

        // Calculate points from path operations
        // 'h' (closepath) uses the starting point
        let raw_pts: Vec<Point> = path
            .iter()
            .map(|(op, args)| {
                if *op == 'h' {
                    // Use starting point
                    let first = &path[0].1;
                    (first[first.len() - 2], first[first.len() - 1])
                } else {
                    // Last two args are the point
                    let n = args.len();
                    (args[n - 2], args[n - 1])
                }
            })
            .collect();

        // Apply CTM to points
        let pts: Vec<Point> = raw_pts
            .iter()
            .map(|&pt| apply_matrix_pt(self.ctm, pt))
            .collect();

        // Transform full path with control points
        let transformed_path: Vec<(char, Vec<Point>)> = path
            .iter()
            .map(|(op, args)| {
                let points: Vec<Point> = args
                    .chunks(2)
                    .map(|chunk| apply_matrix_pt(self.ctm, (chunk[0], chunk[1])))
                    .collect();
                (*op, points)
            })
            .collect();

        // Convert to original_path format
        let original_path: Option<Vec<(char, Vec<Point>)>> = Some(transformed_path);

        // Determine shape type and drop redundant trailing 'l' if closed
        let mut shape = shape;
        let mut pts = pts;
        if shape.len() > 3 && shape.ends_with("lh") && pts.len() >= 2 {
            let n = pts.len();
            if pts[n - 2] == pts[0] {
                shape = format!("{}h", &shape[..shape.len() - 2]);
                pts.pop();
            }
        }

        // Get dashing style
        let dashing_style = gstate.dash.clone();

        // Get colors from graphic state
        let scolor = Some(gstate.scolor.to_vec());
        let ncolor = Some(gstate.ncolor.to_vec());

        // Create appropriate layout object
        let mut item = match shape.as_str() {
            "mlh" | "ml" => {
                // Single line segment
                LTItem::Line(LTLine::new_with_dashing(
                    gstate.linewidth,
                    pts[0],
                    pts[1],
                    stroke,
                    fill,
                    evenodd,
                    scolor,
                    ncolor,
                    original_path.clone(),
                    dashing_style,
                ))
            }
            "mlllh" | "mllll" => {
                // Potential rectangle
                if pts.len() >= 5 {
                    let (x0, y0) = pts[0];
                    let (x1, y1) = pts[1];
                    let (x2, y2) = pts[2];
                    let (x3, y3) = pts[3];

                    let is_closed_loop = pts[0] == pts[4];
                    let has_square_coordinates = (approx_eq(x0, x1)
                        && approx_eq(y1, y2)
                        && approx_eq(x2, x3)
                        && approx_eq(y3, y0))
                        || (approx_eq(y0, y1)
                            && approx_eq(x1, x2)
                            && approx_eq(y2, y3)
                            && approx_eq(x3, x0));

                    if is_closed_loop && has_square_coordinates {
                        LTItem::Rect(LTRect::new_with_dashing(
                            gstate.linewidth,
                            (pts[0].0, pts[0].1, pts[2].0, pts[2].1),
                            stroke,
                            fill,
                            evenodd,
                            scolor,
                            ncolor,
                            original_path,
                            dashing_style,
                        ))
                    } else {
                        LTItem::Curve(LTCurve::new_with_dashing(
                            gstate.linewidth,
                            pts,
                            stroke,
                            fill,
                            evenodd,
                            scolor,
                            ncolor,
                            original_path,
                            dashing_style,
                        ))
                    }
                } else {
                    LTItem::Curve(LTCurve::new_with_dashing(
                        gstate.linewidth,
                        pts,
                        stroke,
                        fill,
                        evenodd,
                        scolor,
                        ncolor,
                        original_path,
                        dashing_style,
                    ))
                }
            }
            _ => {
                // Generic curve
                LTItem::Curve(LTCurve::new_with_dashing(
                    gstate.linewidth,
                    pts,
                    stroke,
                    fill,
                    evenodd,
                    scolor,
                    ncolor,
                    original_path,
                    dashing_style,
                ))
            }
        };

        let mcid = self.current_mcid();
        let tag = self.current_tag().map(|s| s.to_string());
        match &mut item {
            LTItem::Line(l) => l.set_marked_content(mcid, tag.clone()),
            LTItem::Rect(r) => r.set_marked_content(mcid, tag.clone()),
            LTItem::Curve(c) => c.set_marked_content(mcid, tag.clone()),
            _ => {}
        }

        if let Some(ref mut container) = self.cur_item {
            container.add(item);
        }
    }

    /// Render a character - creates LTChar from font/matrix/cid.
    ///
    /// This is the core method for text extraction. It computes the character's
    /// bounding box from the text matrix, font metrics, and character displacement.
    ///
    /// Port of PDFLayoutAnalyzer.render_char from pdfminer.six converter.py
    ///
    /// # Arguments
    /// * `matrix` - The text matrix (Tm * CTM combined)
    /// * `font` - The font object
    /// * `fontsize` - The font size
    /// * `scaling` - Horizontal scaling (as percentage, 100 = normal)
    /// * `rise` - Text rise for superscript/subscript
    /// * `cid` - Character ID to render
    /// * `ncs` - Non-stroking color space
    /// * `graphicstate` - Current graphics state
    ///
    /// # Returns
    /// The advance width of the character (for positioning next character)
    #[allow(clippy::too_many_arguments)]
    pub fn render_char(
        &mut self,
        matrix: Matrix,
        font: &dyn PDFFont,
        fontsize: f64,
        scaling: f64,
        rise: f64,
        cid: u32,
        _ncs: &PDFColorSpace,
        _graphicstate: &PDFGraphicState,
    ) -> f64 {
        // Try to get Unicode text
        let text = match font.to_unichr(cid) {
            Some(t) => t,
            None => self.handle_undefined_char(cid),
        };

        // Get character width and displacement
        let textwidth = font.char_width(cid);
        let textdisp = font.char_disp(cid);

        // Compute character advance (like Python's LTChar.adv)
        let adv = textwidth * fontsize * scaling;

        // Compute bounding box in text space, then transform to device space
        // This matches Python's LTChar.__init__ (layout.py lines 376-392)
        let bbox_text = if font.is_vertical() {
            // Vertical text layout (Python lines 376-385)
            match textdisp {
                CharDisp::Vertical(vx_opt, vy) => {
                    // vx: horizontal displacement (default fontsize * 0.5)
                    // vy: vertical displacement from DW2
                    let vx = vx_opt
                        .map(|v| v * fontsize * 0.001)
                        .unwrap_or(fontsize * 0.5);
                    let vy_scaled = (1000.0 - vy) * fontsize * 0.001;
                    // bbox = (-vx, vy + rise + adv, -vx + fontsize, vy + rise)
                    (
                        -vx,
                        vy_scaled + rise + adv,
                        -vx + fontsize,
                        vy_scaled + rise,
                    )
                }
                CharDisp::Horizontal(_) => {
                    // Shouldn't happen for vertical font, but handle gracefully
                    let descent = font.get_descent() * fontsize;
                    (0.0, descent + rise, adv, descent + rise + fontsize)
                }
            }
        } else {
            // Horizontal text layout (Python lines 387-389)
            let descent = font.get_descent() * fontsize;
            (0.0, descent + rise, adv, descent + rise + fontsize)
        };

        // Determine if text is upright (not rotated)
        let (a, b, c, d, _e, _f) = matrix;
        let upright = (a * d * scaling > 0.0) && (b * c <= 0.0);

        // Transform bbox to device space using apply_matrix_rect
        let (x0, y0, x1, y1) = apply_matrix_rect(matrix, bbox_text);

        // Normalize bbox (ensure x0 < x1, y0 < y1) - Python lines 393-396
        let bbox = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));

        // Size: width for vertical, height for horizontal (Python lines 398-401)
        let size = if font.is_vertical() {
            bbox.2 - bbox.0 // width
        } else {
            bbox.3 - bbox.1 // height
        };

        let fontname = font.fontname().unwrap_or("unknown");
        let item = LTChar::new_with_matrix(bbox, &text, fontname, size, upright, adv, matrix);

        // Add to current container
        if let Some(ref mut container) = self.cur_item {
            container.add(LTItem::Char(item));
        }

        adv
    }

    /// Render an image - creates LTImage from stream.
    ///
    /// Port of PDFLayoutAnalyzer.render_image from pdfminer.six converter.py
    ///
    /// # Arguments
    /// * `name` - Image identifier/name
    /// * `stream` - The image stream (contains image data and attributes)
    pub fn render_image(&mut self, name: &str, stream: &PDFStream) {
        // Image must be rendered inside a figure
        if !self.in_figure() {
            return;
        }

        // Get current figure's bounding box
        let bbox = self
            .cur_item
            .as_ref()
            .map(|c| c.bbox())
            .unwrap_or((0.0, 0.0, 0.0, 0.0));

        // Extract image metadata from stream
        let srcsize = self.get_image_srcsize(stream);
        let imagemask = self.get_image_mask(stream);
        let bits = self.get_image_bits(stream);
        let colorspace = self.get_image_colorspace(stream);

        // Create image layout item
        let item = LTImage::new(name, bbox, srcsize, imagemask, bits, colorspace.clone());

        // Export image if writer is configured
        if let Some(ref writer) = self.image_writer {
            let _ = writer
                .borrow_mut()
                .export_image(name, stream, srcsize, bits, &colorspace);
        }

        // Add to current container
        if let Some(ref mut container) = self.cur_item {
            container.add(LTItem::Image(item));
        }
    }

    /// Extract source dimensions from image stream.
    fn get_image_srcsize(&self, stream: &PDFStream) -> (Option<i32>, Option<i32>) {
        let width = stream
            .get_any(&["Width", "W"])
            .and_then(|obj| obj.as_int().ok())
            .map(|n| n as i32);
        let height = stream
            .get_any(&["Height", "H"])
            .and_then(|obj| obj.as_int().ok())
            .map(|n| n as i32);
        (width, height)
    }

    /// Check if image is an image mask.
    fn get_image_mask(&self, stream: &PDFStream) -> bool {
        stream
            .get_any(&["ImageMask", "IM"])
            .and_then(|obj| obj.as_bool().ok())
            .unwrap_or(false)
    }

    /// Get bits per component from image stream.
    fn get_image_bits(&self, stream: &PDFStream) -> i32 {
        stream
            .get_any(&["BitsPerComponent", "BPC"])
            .and_then(|obj| obj.as_int().ok())
            .map(|n| n as i32)
            .unwrap_or(1)
    }

    /// Get colorspace name(s) from image stream.
    fn get_image_colorspace(&self, stream: &PDFStream) -> Vec<String> {
        match stream.get_any(&["ColorSpace", "CS"]) {
            Some(crate::pdftypes::PDFObject::Name(name)) => vec![name.clone()],
            Some(crate::pdftypes::PDFObject::Array(arr)) => arr
                .iter()
                .filter_map(|obj| match obj {
                    crate::pdftypes::PDFObject::Name(name) => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    }

    /// Receive the analyzed layout page.
    ///
    /// Override this method to process the layout.
    pub fn receive_layout(&mut self, _ltpage: LTPage) {
        // Default implementation does nothing
    }

    /// Handle undefined character.
    pub fn handle_undefined_char(&self, _cid: u32) -> String {
        format!("(cid:{})", _cid)
    }

    // ========================================================================
    // Marked Content Support
    // ========================================================================

    /// Get the current MCID (Marked Content ID) if inside marked content.
    pub fn current_mcid(&self) -> Option<i32> {
        // Return the MCID from the topmost marked content with an MCID
        self.marked_content_stack
            .iter()
            .rev()
            .find_map(|mc| mc.mcid)
    }

    /// Get the current marked content tag if inside marked content.
    pub fn current_tag(&self) -> Option<&str> {
        self.marked_content_stack.last().map(|mc| mc.tag.as_str())
    }

    /// Begin a marked content section.
    pub fn begin_tag(
        &mut self,
        tag: &crate::psparser::PSLiteral,
        props: Option<&crate::pdfdevice::PDFStackT>,
    ) {
        // Extract MCID from properties if present
        let mcid = props.and_then(|p| {
            p.get("MCID").and_then(|v| match v {
                crate::pdfdevice::PDFStackValue::Int(n) => Some(*n as i32),
                _ => None,
            })
        });

        self.marked_content_stack.push(MarkedContentState {
            tag: tag.name().to_string(),
            mcid,
        });
    }

    /// End a marked content section.
    pub fn end_tag(&mut self) {
        self.marked_content_stack.pop();
    }
}

/// Approximate equality for floating point.
fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

// ============================================================================
// PDFPageAggregator
// ============================================================================

/// PDF Page Aggregator - collects analyzed pages for later retrieval.
///
/// Unlike other converters that output immediately, this aggregator stores
/// the most recent page for retrieval via get_result().
pub struct PDFPageAggregator {
    #[allow(dead_code)]
    analyzer: PDFLayoutAnalyzer,
    result: Option<LTPage>,
}

impl PDFPageAggregator {
    /// Create a new page aggregator.
    pub fn new(laparams: Option<LAParams>, pageno: i32) -> Self {
        Self::new_with_imagewriter(laparams, pageno, None)
    }

    /// Create a new page aggregator with an optional image writer.
    pub fn new_with_imagewriter(
        laparams: Option<LAParams>,
        pageno: i32,
        image_writer: Option<Rc<RefCell<ImageWriter>>>,
    ) -> Self {
        Self {
            analyzer: PDFLayoutAnalyzer::new_with_imagewriter(laparams, pageno, image_writer),
            result: None,
        }
    }

    /// Receive the analyzed layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        self.result = Some(ltpage);
    }

    /// Get the result (if any).
    pub fn result(&self) -> Option<&LTPage> {
        self.result.as_ref()
    }

    /// Get the result, panicking if none.
    pub fn get_result(&self) -> &LTPage {
        self.result.as_ref().expect("No result available")
    }

    /// Get the current MCID (Marked Content ID) if inside marked content.
    pub fn current_mcid(&self) -> Option<i32> {
        self.analyzer.current_mcid()
    }

    /// Get the current marked content tag if inside marked content.
    pub fn current_tag(&self) -> Option<&str> {
        self.analyzer.current_tag()
    }
}

impl PDFDevice for PDFPageAggregator {
    fn set_ctm(&mut self, ctm: Matrix) {
        self.analyzer.set_ctm(ctm);
    }

    fn ctm(&self) -> Option<Matrix> {
        Some(self.analyzer.ctm)
    }

    fn begin_page(&mut self, _pageid: u32, mediabox: Rect, ctm: Matrix) {
        self.analyzer.begin_page(mediabox, ctm);
    }

    fn end_page(&mut self, _pageid: u32) {
        if let Some(page) = self.analyzer.end_page() {
            self.result = Some(page);
        }
    }

    fn begin_figure(&mut self, name: &str, bbox: Rect, matrix: Matrix) {
        self.analyzer.begin_figure(name, bbox, matrix);
    }

    fn end_figure(&mut self, name: &str) {
        self.analyzer.end_figure(name);
    }

    fn paint_path(
        &mut self,
        graphicstate: &PDFGraphicState,
        stroke: bool,
        fill: bool,
        evenodd: bool,
        path: &[PathSegment],
    ) {
        // Convert PathSegment to PathOp format used by analyzer
        let path_ops: Vec<PathOp> = path
            .iter()
            .map(|seg| match seg {
                PathSegment::MoveTo(x, y) => ('m', vec![*x, *y]),
                PathSegment::LineTo(x, y) => ('l', vec![*x, *y]),
                PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) => {
                    ('c', vec![*x1, *y1, *x2, *y2, *x3, *y3])
                }
                PathSegment::ClosePath => ('h', vec![]),
            })
            .collect();
        self.analyzer
            .paint_path(graphicstate, stroke, fill, evenodd, &path_ops);
    }

    fn render_image(&mut self, name: &str, stream: &PDFStream) {
        self.analyzer.render_image(name, stream);
    }

    fn render_string(
        &mut self,
        textstate: &mut PDFTextState,
        seq: &PDFTextSeq,
        _ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) {
        // Skip invisible text (render mode 3 or 7 which includes clipping)
        // Modes: 0=fill, 1=stroke, 2=fill+stroke, 3=invisible,
        // 4-7 add clipping to modes 0-3
        if textstate.render == 3 || textstate.render == 7 {
            return;
        }

        // Process text sequence and render characters
        let ctm = self.analyzer.ctm;
        let matrix = mult_matrix(textstate.matrix, ctm);
        let fontsize = textstate.fontsize;
        let scaling = textstate.scaling * 0.01;
        let charspace = textstate.charspace * scaling;
        let wordspace = textstate.wordspace * scaling;
        let rise = textstate.rise;
        let dxscale = 0.001 * fontsize * scaling;

        let (mut x, mut y) = textstate.linematrix;
        let mut needcharspace = false;

        // Get font if available for proper CID decoding
        let font = textstate.font.clone();

        // Check if font is vertical writing mode
        // Python: dispatches to render_string_horizontal or render_string_vertical
        let is_vertical = font.as_ref().map(|f| f.is_vertical()).unwrap_or(false);

        for item in seq {
            match item {
                PDFTextSeqItem::Number(n) => {
                    // Adjustment in text space
                    // Python: x -= obj * dxscale (horizontal) or y -= obj * dxscale (vertical)
                    if is_vertical {
                        y -= n * dxscale;
                    } else {
                        x -= n * dxscale;
                    }
                    needcharspace = true;
                }
                PDFTextSeqItem::Bytes(data) => {
                    // Use font to decode bytes to CIDs, then convert CIDs to Unicode
                    let cids: Vec<u32> = if let Some(ref font) = font {
                        font.decode(data)
                    } else {
                        // Fallback: treat each byte as a CID
                        data.iter().map(|&b| b as u32).collect()
                    };

                    for cid in cids {
                        if needcharspace {
                            // Python: x += charspace (horizontal) or y += charspace (vertical)
                            if is_vertical {
                                y += charspace;
                            } else {
                                x += charspace;
                            }
                        }

                        // Get character text using font's to_unichr
                        let text = if let Some(ref font) = font {
                            font.to_unichr(cid)
                                .unwrap_or_else(|| self.analyzer.handle_undefined_char(cid))
                        } else {
                            // Fallback for no font: try ASCII, else (cid:X)
                            if (0x20..0x7f).contains(&cid) {
                                char::from_u32(cid)
                                    .map(|c| c.to_string())
                                    .unwrap_or_else(|| format!("(cid:{})", cid))
                            } else {
                                format!("(cid:{})", cid)
                            }
                        };
                        // Get char_disp and compute advancement
                        // Python: textdisp = font.char_disp(cid), adv = textwidth * fontsize * scaling
                        let char_disp = if let Some(ref font) = font {
                            font.char_disp(cid)
                        } else {
                            CharDisp::Horizontal(0.6) // Fallback
                        };

                        let char_width = if let Some(ref font) = font {
                            font.char_width(cid) * fontsize * scaling
                        } else {
                            fontsize * scaling * 0.6 // Approximate width
                        };

                        // Compute character matrix using translate_matrix pattern
                        // Python: utils.translate_matrix(matrix, (x, y))
                        let char_matrix = (
                            matrix.0,
                            matrix.1,
                            matrix.2,
                            matrix.3,
                            matrix.0 * x + matrix.2 * y + matrix.4,
                            matrix.1 * x + matrix.3 * y + matrix.5,
                        );

                        // Compute bounding box - match Python's formula exactly
                        // Python distinguishes vertical vs horizontal fonts
                        let local_bbox = match char_disp {
                            CharDisp::Vertical(vx_opt, vy) => {
                                // Python: if vx is None: vx = fontsize * 0.5
                                //         else: vx = vx * fontsize * 0.001
                                //         vy = (1000 - vy) * fontsize * 0.001
                                //         bbox = (-vx, vy + rise + self.adv, -vx + fontsize, vy + rise)
                                let vx = match vx_opt {
                                    Some(v) => v * fontsize * 0.001,
                                    None => fontsize * 0.5,
                                };
                                let vy_scaled = (1000.0 - vy) * fontsize * 0.001;
                                (
                                    -vx,
                                    vy_scaled + rise + char_width,
                                    -vx + fontsize,
                                    vy_scaled + rise,
                                )
                            }
                            CharDisp::Horizontal(_) => {
                                // Python: descent = font.get_descent() * fontsize
                                //         bbox = (0, descent + rise, self.adv, descent + rise + fontsize)
                                let descent = if let Some(ref font) = font {
                                    font.get_descent() * fontsize
                                } else {
                                    -fontsize * 0.25 // Fallback
                                };
                                (0.0, descent + rise, char_width, descent + rise + fontsize)
                            }
                        };

                        // Apply matrix transformation to get final bbox
                        // Must transform all 4 corners for rotated/scaled matrices
                        let bbox = apply_matrix_rect(char_matrix, local_bbox);

                        // Compute upright flag
                        let (a, b, c, d, _, _) = char_matrix;
                        let upright = (a * d * scaling > 0.0) && (b * c <= 0.0);

                        // Compute size: width for vertical, height for horizontal
                        let size = if is_vertical {
                            bbox.2 - bbox.0 // width
                        } else {
                            bbox.3 - bbox.1 // height
                        };

                        // Create LTChar and add to container
                        // Pass current MCID from marked content stack and colors from graphic state
                        let mcid = self.analyzer.current_mcid();
                        let tag = self.analyzer.current_tag().map(|s| s.to_string());
                        let ncolor = Some(graphicstate.ncolor.to_vec());
                        let scolor = Some(graphicstate.scolor.to_vec());
                        let fontname = font
                            .as_ref()
                            .and_then(|f| f.fontname())
                            .or_else(|| textstate.fontname.as_deref())
                            .unwrap_or("unknown");
                        let mut ltchar = LTChar::with_colors_matrix(
                            bbox,
                            &text,
                            fontname,
                            size,
                            upright,
                            char_width,
                            char_matrix,
                            mcid,
                            tag,
                            ncolor,
                            scolor,
                        );
                        ltchar.set_ncs(Some(graphicstate.ncs.name.clone()));
                        ltchar.set_scs(Some(graphicstate.scs.name.clone()));

                        if let Some(ref mut container) = self.analyzer.cur_item {
                            container.add(LTItem::Char(ltchar));
                        }

                        // Advance position
                        // Python: x += render_char(...) (horizontal) or y += render_char(...) (vertical)
                        if is_vertical {
                            y += char_width;
                        } else {
                            x += char_width;
                        }

                        // Word spacing for space character (CID 32)
                        if cid == 32 && wordspace != 0.0 {
                            if is_vertical {
                                y += wordspace;
                            } else {
                                x += wordspace;
                            }
                        }
                        needcharspace = true;
                    }
                }
            }
        }

        // Update text state line matrix
        textstate.linematrix = (x, y);
    }

    fn begin_tag(
        &mut self,
        tag: &crate::psparser::PSLiteral,
        props: Option<&crate::pdfdevice::PDFStackT>,
    ) {
        self.analyzer.begin_tag(tag, props);
    }

    fn end_tag(&mut self) {
        self.analyzer.end_tag();
    }
}

// ============================================================================
// PDFConverter
// ============================================================================

/// Base PDF Converter - common functionality for output converters.
///
/// Port of PDFConverter from pdfminer.six converter.py
#[allow(dead_code)]
pub struct PDFConverter<W: Write> {
    /// Output writer
    outfp: W,
    /// Output encoding
    codec: String,
    /// Current page number
    pageno: i32,
    /// Layout parameters
    laparams: Option<LAParams>,
    /// Whether output is binary
    outfp_binary: bool,
}

impl<W: Write> PDFConverter<W> {
    /// Create a new converter.
    pub fn new(outfp: W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
        Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            outfp_binary: true, // Default to binary
        }
    }

    /// Check if a stream is binary.
    ///
    /// In Rust, we use type-based detection rather than runtime checks.
    /// This is a simplified version that always returns true for byte writers.
    pub fn is_binary_stream<T>(_stream: &T) -> bool {
        true
    }

    /// Check if output is text (not binary).
    pub fn is_text_stream<T>(_stream: &T) -> bool {
        false
    }
}
