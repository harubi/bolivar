//! Output Converters for PDF Content - port of pdfminer.six converter.py
//!
//! Provides converters for transforming PDF layout content into various output formats:
//! - PDFLayoutAnalyzer: Base device that creates layout objects from PDF content
//! - PDFPageAggregator: Collects analyzed pages for later retrieval
//! - TextConverter: Plain text output
//! - HTMLConverter: HTML output with positioning
//! - XMLConverter: XML output with full structure

use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use crate::image::ImageWriter;
use crate::layout::{
    LAParams, LTChar, LTCurve, LTFigure, LTImage, LTItem, LTLine, LTPage, LTRect, LTTextBox,
    LTTextGroup, LTTextLine, TextBoxType, TextGroupElement, TextLineElement, TextLineType,
};
use crate::pdfcolor::PDFColorSpace;
use crate::pdfdevice::{PDFDevice, PDFTextSeq, PDFTextSeqItem, PathSegment};
use crate::pdffont::{CharDisp, PDFFont};
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::pdftypes::PDFStream;
use crate::utils::{
    HasBBox, Matrix, Point, Rect, apply_matrix_pt, apply_matrix_rect, bbox2str, enc,
    make_compat_str, mult_matrix,
};

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
    cur_item: Option<LTContainer>,
    /// Current transformation matrix
    ctm: Matrix,
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
        let item = match shape.as_str() {
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
        use crate::pdffont::{CharDisp, PDFFont};
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
                            use crate::pdffont::PDFFont;
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
                        let ltchar = LTChar::with_colors_matrix(
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
pub struct PDFConverter<'a, W: Write> {
    /// Output writer
    outfp: &'a mut W,
    /// Output encoding
    codec: String,
    /// Current page number
    pageno: i32,
    /// Layout parameters
    laparams: Option<LAParams>,
    /// Whether output is binary
    outfp_binary: bool,
}

impl<'a, W: Write> PDFConverter<'a, W> {
    /// Create a new converter.
    pub fn new(outfp: &'a mut W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
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

// ============================================================================
// TextConverter
// ============================================================================

/// Text Converter - outputs plain text.
///
/// Port of TextConverter from pdfminer.six converter.py
pub struct TextConverter<'a, W: Write> {
    /// Output writer
    outfp: &'a mut W,
    /// Output encoding
    #[allow(dead_code)]
    codec: String,
    /// Current page number
    #[allow(dead_code)]
    pageno: i32,
    /// Layout parameters
    #[allow(dead_code)]
    laparams: Option<LAParams>,
    /// Whether to show page numbers
    showpageno: bool,
}

impl<'a, W: Write> TextConverter<'a, W> {
    /// Create a new text converter.
    pub fn new(
        outfp: &'a mut W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        showpageno: bool,
    ) -> Self {
        Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            showpageno,
        }
    }

    /// Check if page numbers are shown.
    pub fn show_pageno(&self) -> bool {
        self.showpageno
    }

    /// Write text to output.
    pub fn write_text(&mut self, text: &str) {
        let _ = self.outfp.write_all(text.as_bytes());
    }

    /// Receive and render a layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        if self.showpageno {
            let header = format!("Page {}\n", ltpage.pageid);
            self.write_text(&header);
        }

        // Render page content
        self.render_item(&LTItem::Page(Box::new(ltpage)));

        // Form feed at end of page
        self.write_text("\x0c");
    }

    /// Recursively render an item.
    fn render_item(&mut self, item: &LTItem) {
        match item {
            LTItem::Page(page) => {
                for child in page.iter() {
                    self.render_item(child);
                }
            }
            LTItem::TextBox(tb) => {
                let text = match tb {
                    TextBoxType::Horizontal(h) => h.get_text(),
                    TextBoxType::Vertical(v) => v.get_text(),
                };
                self.write_text(&text);
                self.write_text("\n");
            }
            LTItem::TextLine(tl) => {
                let text = match tl {
                    TextLineType::Horizontal(h) => h.get_text(),
                    TextLineType::Vertical(v) => v.get_text(),
                };
                self.write_text(&text);
            }
            LTItem::Char(c) => {
                self.write_text(c.get_text());
            }
            LTItem::Anno(a) => {
                self.write_text(a.get_text());
            }
            LTItem::Figure(fig) => {
                for child in fig.iter() {
                    self.render_item(child);
                }
            }
            _ => {}
        }
    }
}

// ============================================================================
// HTMLConverter
// ============================================================================

/// HTML Converter - outputs HTML with positioning.
///
/// Port of HTMLConverter from pdfminer.six converter.py
pub struct HTMLConverter<'a, W: Write> {
    /// Output writer
    outfp: &'a mut W,
    /// Output encoding
    codec: String,
    /// Current page number
    pageno: i32,
    /// Layout parameters
    #[allow(dead_code)]
    laparams: Option<LAParams>,
    /// Scale factor
    scale: f64,
    /// Font scale factor
    #[allow(dead_code)]
    fontscale: f64,
    /// Layout mode
    #[allow(dead_code)]
    layoutmode: String,
    /// Show page numbers
    #[allow(dead_code)]
    showpageno: bool,
    /// Page margin
    #[allow(dead_code)]
    pagemargin: i32,
    /// Rectangle colors for debug rendering
    rect_colors: HashMap<String, String>,
    /// Text colors for debug rendering
    text_colors: HashMap<String, String>,
    /// Y offset for positioning
    #[allow(dead_code)]
    yoffset: f64,
}

impl<'a, W: Write> HTMLConverter<'a, W> {
    /// Default rectangle colors.
    pub fn default_rect_colors() -> HashMap<String, String> {
        let mut colors = HashMap::new();
        colors.insert("curve".to_string(), "black".to_string());
        colors.insert("page".to_string(), "gray".to_string());
        colors
    }

    /// Default text colors.
    pub fn default_text_colors() -> HashMap<String, String> {
        let mut colors = HashMap::new();
        colors.insert("char".to_string(), "black".to_string());
        colors
    }

    /// Full debug colors for rectangles.
    pub fn debug_rect_colors() -> HashMap<String, String> {
        let mut colors = Self::default_rect_colors();
        colors.insert("figure".to_string(), "yellow".to_string());
        colors.insert("textline".to_string(), "magenta".to_string());
        colors.insert("textbox".to_string(), "cyan".to_string());
        colors.insert("textgroup".to_string(), "red".to_string());
        colors
    }

    /// Full debug colors for text.
    pub fn debug_text_colors() -> HashMap<String, String> {
        let mut colors = Self::default_text_colors();
        colors.insert("textbox".to_string(), "blue".to_string());
        colors
    }

    /// Create a new HTML converter.
    pub fn new(outfp: &'a mut W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
        let mut converter = Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            scale: 1.0,
            fontscale: 1.0,
            layoutmode: "normal".to_string(),
            showpageno: true,
            pagemargin: 50,
            rect_colors: Self::default_rect_colors(),
            text_colors: Self::default_text_colors(),
            yoffset: 50.0,
        };
        converter.write_header();
        converter
    }

    /// Create with debug mode.
    pub fn with_debug(
        outfp: &'a mut W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        debug: i32,
    ) -> Self {
        let mut converter = Self::new(outfp, codec, pageno, laparams);
        if debug > 0 {
            converter.rect_colors = Self::debug_rect_colors();
            converter.text_colors = Self::debug_text_colors();
        }
        converter
    }

    /// Create with custom options.
    pub fn with_options(
        outfp: &'a mut W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        scale: f64,
        fontscale: f64,
    ) -> Self {
        let mut converter = Self::new(outfp, codec, pageno, laparams);
        converter.scale = scale;
        converter.fontscale = fontscale;
        converter
    }

    /// Get scale factor.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Get rectangle colors.
    pub fn rect_colors(&self) -> &HashMap<String, String> {
        &self.rect_colors
    }

    /// Write output.
    fn write(&mut self, text: &str) {
        let _ = self.outfp.write_all(text.as_bytes());
    }

    /// Write header.
    fn write_header(&mut self) {
        self.write("<html><head>\n");
        if !self.codec.is_empty() {
            let meta = format!(
                "<meta http-equiv=\"Content-Type\" content=\"text/html; charset={}\">\n",
                self.codec
            );
            self.write(&meta);
        } else {
            self.write("<meta http-equiv=\"Content-Type\" content=\"text/html\">\n");
        }
        self.write("</head><body>\n");
    }

    /// Write footer.
    fn write_footer(&mut self) {
        let mut page_links = Vec::new();
        for i in 1..self.pageno {
            page_links.push(format!("<a href=\"#{}\">{}</a>", i, i));
        }
        let links = page_links.join(", ");
        let footer = format!(
            "<div style=\"position:absolute; top:0px;\">Page: {}</div>\n",
            links
        );
        self.write(&footer);
        self.write("</body></html>\n");
    }

    /// Write text with HTML encoding.
    pub fn write_text(&mut self, text: &str) {
        self.write(&enc(text));
    }

    /// Place a rectangle with the specified color and border.
    ///
    /// Port of HTMLConverter.place_rect from pdfminer.six
    pub fn place_rect(&mut self, color: &str, borderwidth: i32, x: f64, y: f64, w: f64, h: f64) {
        if let Some(color2) = self.rect_colors.get(color) {
            let s = format!(
                "<span style=\"position:absolute; border: {} {}px solid; \
                 left:{}px; top:{}px; width:{}px; height:{}px;\"></span>\n",
                color2,
                borderwidth,
                (x * self.scale) as i32,
                ((self.yoffset - y) * self.scale) as i32,
                (w * self.scale) as i32,
                (h * self.scale) as i32
            );
            self.write(&s);
        }
    }

    /// Place a border around a component.
    ///
    /// Port of HTMLConverter.place_border from pdfminer.six
    pub fn place_border<T: HasBBox>(&mut self, color: &str, borderwidth: i32, item: &T) {
        self.place_rect(
            color,
            borderwidth,
            item.x0(),
            item.y1(),
            item.width(),
            item.height(),
        );
    }

    /// Place an image.
    ///
    /// Port of HTMLConverter.place_image from pdfminer.six
    pub fn place_image(&mut self, name: &str, borderwidth: i32, x: f64, y: f64, w: f64, h: f64) {
        let s = format!(
            "<img src=\"{}\" border=\"{}\" style=\"position:absolute; \
             left:{}px; top:{}px;\" width=\"{}\" height=\"{}\" />\n",
            enc(name),
            borderwidth,
            (x * self.scale) as i32,
            ((self.yoffset - y) * self.scale) as i32,
            (w * self.scale) as i32,
            (h * self.scale) as i32
        );
        self.write(&s);
    }

    /// Place text at a specific position with specified size.
    ///
    /// Port of HTMLConverter.place_text from pdfminer.six
    pub fn place_text(&mut self, color: &str, text: &str, x: f64, y: f64, size: f64) {
        if let Some(color2) = self.text_colors.get(color) {
            let s = format!(
                "<span style=\"position:absolute; color:{}; left:{}px; \
                 top:{}px; font-size:{}px;\">",
                color2,
                (x * self.scale) as i32,
                ((self.yoffset - y) * self.scale) as i32,
                (size * self.scale * self.fontscale) as i32
            );
            self.write(&s);
            self.write_text(text);
            self.write("</span>\n");
        }
    }

    /// Begin a div element with positioning.
    ///
    /// Port of HTMLConverter.begin_div from pdfminer.six
    #[allow(clippy::too_many_arguments)]
    pub fn begin_div(
        &mut self,
        color: &str,
        borderwidth: i32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        writing_mode: &str,
    ) {
        let s = format!(
            "<div style=\"position:absolute; border: {} {}px solid; \
             writing-mode:{}; left:{}px; top:{}px; width:{}px; height:{}px;\">",
            color,
            borderwidth,
            writing_mode,
            (x * self.scale) as i32,
            ((self.yoffset - y) * self.scale) as i32,
            (w * self.scale) as i32,
            (h * self.scale) as i32
        );
        self.write(&s);
    }

    /// End a div element.
    ///
    /// Port of HTMLConverter.end_div from pdfminer.six
    pub fn end_div(&mut self, _color: &str) {
        self.write("</div>");
    }

    /// Write text with font styling.
    ///
    /// Port of HTMLConverter.put_text from pdfminer.six
    pub fn put_text(&mut self, text: &str, fontname: &str, fontsize: f64) {
        // Remove subset tag from fontname (e.g., ABCDEF+Times -> Times)
        let fontname_without_subset = fontname.split('+').next_back().unwrap_or(fontname);
        let s = format!(
            "<span style=\"font-family: {}; font-size:{}px\">",
            fontname_without_subset,
            (fontsize * self.scale * self.fontscale) as i32
        );
        self.write(&s);
        self.write_text(text);
        self.write("</span>");
    }

    /// Write a newline (line break).
    ///
    /// Port of HTMLConverter.put_newline from pdfminer.six
    pub fn put_newline(&mut self) {
        self.write("<br>");
    }

    /// Receive and render a layout page.
    ///
    /// Port of HTMLConverter.receive_layout from pdfminer.six
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        self.yoffset += ltpage.y1();
        self.place_border("page", 1, &ltpage);

        if self.showpageno {
            let header = format!(
                "<div style=\"position:absolute; top:{}px;\">\
                 <a name=\"{}\">Page {}</a></div>\n",
                ((self.yoffset - ltpage.y1()) * self.scale) as i32,
                ltpage.pageid,
                ltpage.pageid
            );
            self.write(&header);
        }

        // Render page content
        for item in ltpage.iter() {
            self.render_item(item);
        }

        // Render text groups if present
        if let Some(groups) = ltpage.groups() {
            for group in groups {
                self.show_group(group);
            }
        }

        self.yoffset += self.pagemargin as f64;
        self.pageno += 1;
    }

    /// Show text group borders (for debugging).
    fn show_group(&mut self, group: &LTTextGroup) {
        self.place_border("textgroup", 1, group);
        for elem in group.iter() {
            if let TextGroupElement::Group(subgroup) = elem {
                self.show_group(subgroup);
            }
        }
    }

    /// Render a layout item to HTML.
    fn render_item(&mut self, item: &LTItem) {
        match item {
            LTItem::Curve(_) => {
                if let LTItem::Curve(c) = item {
                    self.place_border("curve", 1, c);
                }
            }
            LTItem::Figure(fig) => {
                self.begin_div(
                    "figure",
                    1,
                    fig.x0(),
                    fig.y1(),
                    fig.width(),
                    fig.height(),
                    "lr-tb",
                );
                for child in fig.iter() {
                    self.render_item(child);
                }
                self.end_div("figure");
            }
            LTItem::Image(img) => {
                self.place_image(&img.name, 1, img.x0(), img.y1(), img.width(), img.height());
            }
            LTItem::TextLine(tl) => {
                self.place_border("textline", 1, tl);
                // Render children would go here if we stored them
            }
            LTItem::TextBox(tb) => {
                let (bbox, wmode, index) = match tb {
                    TextBoxType::Horizontal(h) => (h.bbox(), h.get_writing_mode(), h.index()),
                    TextBoxType::Vertical(v) => (v.bbox(), v.get_writing_mode(), v.index()),
                };
                self.place_border("textbox", 1, tb);
                self.place_text("textbox", &format!("{}", index + 1), bbox.0, bbox.3, 20.0);

                // In layoutmode "exact", render each character
                // In normal mode, render text boxes with divs
                if self.layoutmode == "exact" {
                    // Exact mode - render individual items
                } else {
                    self.begin_div(
                        "textbox",
                        1,
                        bbox.0,
                        bbox.3,
                        bbox.2 - bbox.0,
                        bbox.3 - bbox.1,
                        wmode,
                    );
                    // Render textbox text content in normal/loose modes
                    let text = match tb {
                        TextBoxType::Horizontal(h) => h.get_text(),
                        TextBoxType::Vertical(v) => v.get_text(),
                    };
                    self.write_text(&text);
                    self.end_div("textbox");
                }
            }
            LTItem::Char(c) => {
                if self.layoutmode == "exact" {
                    self.place_border("char", 1, c);
                    self.place_text("char", c.get_text(), c.x0(), c.y1(), c.size());
                } else {
                    let fontname = make_compat_str(c.fontname());
                    self.put_text(c.get_text(), &fontname, c.size());
                }
            }
            LTItem::Anno(a) => {
                self.write_text(a.get_text());
            }
            _ => {}
        }
    }

    /// Close the converter.
    pub fn close(&mut self) {
        self.write_footer();
    }
}

// ============================================================================
// XMLConverter
// ============================================================================

/// XML Converter - outputs XML with full structure.
///
/// Port of XMLConverter from pdfminer.six converter.py
pub struct XMLConverter<'a, W: Write> {
    /// Output writer
    outfp: &'a mut W,
    /// Output encoding
    codec: String,
    /// Current page number
    #[allow(dead_code)]
    pageno: i32,
    /// Layout parameters
    #[allow(dead_code)]
    laparams: Option<LAParams>,
    /// Strip control characters
    stripcontrol: bool,
    /// Regex for control characters
    control_re: Regex,
}

impl<'a, W: Write> XMLConverter<'a, W> {
    /// Create a new XML converter.
    pub fn new(outfp: &'a mut W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
        let mut converter = Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            stripcontrol: false,
            control_re: Regex::new(r"[\x00-\x08\x0b-\x0c\x0e-\x1f]").unwrap(),
        };
        converter.write_header();
        converter
    }

    /// Create with options.
    pub fn with_options(
        outfp: &'a mut W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        stripcontrol: bool,
    ) -> Self {
        let mut converter = Self::new(outfp, codec, pageno, laparams);
        converter.stripcontrol = stripcontrol;
        converter
    }

    /// Write output.
    fn write(&mut self, text: &str) {
        let _ = self.outfp.write_all(text.as_bytes());
    }

    /// Write header.
    fn write_header(&mut self) {
        if !self.codec.is_empty() {
            let decl = format!("<?xml version=\"1.0\" encoding=\"{}\" ?>\n", self.codec);
            self.write(&decl);
        } else {
            self.write("<?xml version=\"1.0\" ?>\n");
        }
        self.write("<pages>\n");
    }

    /// Write footer.
    fn write_footer(&mut self) {
        self.write("</pages>\n");
    }

    /// Write text with encoding and control character handling.
    pub fn write_text(&mut self, text: &str) {
        let text = if self.stripcontrol {
            self.control_re.replace_all(text, "").to_string()
        } else {
            text.to_string()
        };
        self.write(&enc(&text));
    }

    /// Receive and render a layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        let page_xml = format!(
            "<page id=\"{}\" bbox=\"{}\" rotate=\"{}\">\n",
            ltpage.pageid,
            bbox2str(ltpage.bbox()),
            ltpage.rotate as i32
        );
        self.write(&page_xml);

        // Render page content
        for item in ltpage.iter() {
            self.render_item(item);
        }

        // Render groups if present
        if let Some(groups) = ltpage.groups() {
            self.write("<layout>\n");
            for group in groups {
                self.show_group(group);
            }
            self.write("</layout>\n");
        }

        self.write("</page>\n");
    }

    /// Render a text group.
    fn show_group(&mut self, group: &LTTextGroup) {
        let group_xml = format!("<textgroup bbox=\"{}\">\n", bbox2str(group.bbox()));
        self.write(&group_xml);
        for elem in group.iter() {
            match elem {
                TextGroupElement::Box(tb) => {
                    let (id, bbox) = match tb {
                        TextBoxType::Horizontal(h) => (h.index(), h.bbox()),
                        TextBoxType::Vertical(v) => (v.index(), v.bbox()),
                    };
                    let tb_xml = format!("<textbox id=\"{}\" bbox=\"{}\" />\n", id, bbox2str(bbox));
                    self.write(&tb_xml);
                }
                TextGroupElement::Group(g) => {
                    self.show_group(g);
                }
            }
        }
        self.write("</textgroup>\n");
    }

    /// Render an item.
    fn render_item(&mut self, item: &LTItem) {
        match item {
            LTItem::Line(l) => {
                let xml = format!(
                    "<line linewidth=\"{}\" bbox=\"{}\" />\n",
                    l.linewidth as i32,
                    bbox2str(l.bbox())
                );
                self.write(&xml);
            }
            LTItem::Rect(r) => {
                let xml = format!(
                    "<rect linewidth=\"{}\" bbox=\"{}\" />\n",
                    r.linewidth as i32,
                    bbox2str(r.bbox())
                );
                self.write(&xml);
            }
            LTItem::Curve(c) => {
                let xml = format!(
                    "<curve linewidth=\"{}\" bbox=\"{}\" pts=\"{}\"/>\n",
                    c.linewidth as i32,
                    bbox2str(c.bbox()),
                    c.get_pts()
                );
                self.write(&xml);
            }
            LTItem::Figure(fig) => {
                let xml = format!(
                    "<figure name=\"{}\" bbox=\"{}\">\n",
                    fig.name,
                    bbox2str(fig.bbox())
                );
                self.write(&xml);
                for child in fig.iter() {
                    self.render_item(child);
                }
                self.write("</figure>\n");
            }
            LTItem::TextLine(tl) => {
                let bbox = match tl {
                    TextLineType::Horizontal(h) => h.bbox(),
                    TextLineType::Vertical(v) => v.bbox(),
                };
                let xml = format!("<textline bbox=\"{}\">\n", bbox2str(bbox));
                self.write(&xml);

                // Render children (characters and annotations)
                match tl {
                    TextLineType::Horizontal(h) => {
                        for elem in h.iter() {
                            self.render_textline_element(elem);
                        }
                    }
                    TextLineType::Vertical(v) => {
                        for elem in v.iter() {
                            self.render_textline_element(elem);
                        }
                    }
                }

                self.write("</textline>\n");
            }
            LTItem::TextBox(tb) => {
                let (id, bbox, wmode) = match tb {
                    TextBoxType::Horizontal(h) => (h.index(), h.bbox(), ""),
                    TextBoxType::Vertical(v) => (v.index(), v.bbox(), " wmode=\"vertical\""),
                };
                let xml = format!(
                    "<textbox id=\"{}\" bbox=\"{}\"{}>\n",
                    id,
                    bbox2str(bbox),
                    wmode
                );
                self.write(&xml);

                // Render children (text lines)
                match tb {
                    TextBoxType::Horizontal(h) => {
                        for line in h.iter() {
                            let line_xml =
                                format!("<textline bbox=\"{}\">\n", bbox2str(line.bbox()));
                            self.write(&line_xml);
                            for elem in line.iter() {
                                self.render_textline_element(elem);
                            }
                            self.write("</textline>\n");
                        }
                    }
                    TextBoxType::Vertical(v) => {
                        for line in v.iter() {
                            let line_xml =
                                format!("<textline bbox=\"{}\">\n", bbox2str(line.bbox()));
                            self.write(&line_xml);
                            for elem in line.iter() {
                                self.render_textline_element(elem);
                            }
                            self.write("</textline>\n");
                        }
                    }
                }

                self.write("</textbox>\n");
            }
            LTItem::Char(c) => {
                let xml = format!(
                    "<text font=\"{}\" bbox=\"{}\" size=\"{:.3}\">",
                    enc(c.fontname()),
                    bbox2str(c.bbox()),
                    c.size()
                );
                self.write(&xml);
                self.write_text(c.get_text());
                self.write("</text>\n");
            }
            LTItem::Anno(a) => {
                self.write("<text>");
                self.write_text(a.get_text());
                self.write("</text>\n");
            }
            LTItem::Image(img) => {
                let xml = format!(
                    "<image width=\"{}\" height=\"{}\" />\n",
                    img.width() as i32,
                    img.height() as i32
                );
                self.write(&xml);
            }
            _ => {}
        }
    }

    /// Render a text line element (char or annotation).
    fn render_textline_element(&mut self, elem: &TextLineElement) {
        match elem {
            TextLineElement::Char(c) => {
                let xml = format!(
                    "<text font=\"{}\" bbox=\"{}\" size=\"{:.3}\">",
                    enc(c.fontname()),
                    bbox2str(c.bbox()),
                    c.size()
                );
                self.write(&xml);
                self.write_text(c.get_text());
                self.write("</text>\n");
            }
            TextLineElement::Anno(a) => {
                self.write("<text>");
                self.write_text(a.get_text());
                self.write("</text>\n");
            }
        }
    }

    /// Close the converter.
    pub fn close(&mut self) {
        self.write_footer();
    }
}

// ============================================================================
// HOCRConverter
// ============================================================================

/// HOCR Converter - outputs hOCR format for OCR-compatible output.
///
/// Port of HOCRConverter from pdfminer.six converter.py
///
/// HOCR is a standard format for OCR output that includes bounding box
/// coordinates for each word/line. This converter extracts explicit text
/// information from PDFs that have it and generates hOCR representation
/// that can be used in conjunction with page images.
pub struct HOCRConverter<'a, W: Write> {
    /// Output writer
    outfp: &'a mut W,
    /// Output encoding
    codec: String,
    /// Current page number
    #[allow(dead_code)]
    pageno: i32,
    /// Layout parameters
    #[allow(dead_code)]
    laparams: Option<LAParams>,
    /// Strip control characters
    stripcontrol: bool,
    /// Regex for control characters
    control_re: Regex,
    /// Whether currently accumulating characters within a word
    within_chars: bool,
    /// Current page bounding box (for y-coordinate inversion)
    page_bbox: Rect,
    /// Working text buffer for current word
    working_text: String,
    /// Working bounding box for current word
    working_bbox: Rect,
    /// Working font name for current word
    working_font: String,
    /// Working font size for current word
    working_size: f64,
}

impl<'a, W: Write> HOCRConverter<'a, W> {
    /// Create a new HOCR converter.
    pub fn new(outfp: &'a mut W, codec: &str, pageno: i32, laparams: Option<LAParams>) -> Self {
        let mut converter = Self {
            outfp,
            codec: codec.to_string(),
            pageno,
            laparams,
            stripcontrol: false,
            control_re: Regex::new(r"[\x00-\x08\x0b-\x0c\x0e-\x1f]").unwrap(),
            within_chars: false,
            page_bbox: (0.0, 0.0, 0.0, 0.0),
            working_text: String::new(),
            working_bbox: (0.0, 0.0, 0.0, 0.0),
            working_font: String::new(),
            working_size: 0.0,
        };
        converter.write_header();
        converter
    }

    /// Create with options.
    pub fn with_options(
        outfp: &'a mut W,
        codec: &str,
        pageno: i32,
        laparams: Option<LAParams>,
        stripcontrol: bool,
    ) -> Self {
        let mut converter = Self::new(outfp, codec, pageno, laparams);
        converter.stripcontrol = stripcontrol;
        converter
    }

    /// Write output.
    fn write(&mut self, text: &str) {
        let _ = self.outfp.write_all(text.as_bytes());
    }

    /// Convert PDF bbox to hOCR bbox string.
    ///
    /// PDF y-coordinates are inverted compared to hOCR coordinates.
    fn bbox_repr(&self, bbox: Rect) -> String {
        let (in_x0, in_y0, in_x1, in_y1) = bbox;
        // PDF y-coordinates are the other way round from hOCR coordinates
        let out_x0 = in_x0 as i32;
        let out_y0 = (self.page_bbox.3 - in_y1) as i32;
        let out_x1 = in_x1 as i32;
        let out_y1 = (self.page_bbox.3 - in_y0) as i32;
        format!("bbox {} {} {} {}", out_x0, out_y0, out_x1, out_y1)
    }

    /// Write header.
    fn write_header(&mut self) {
        if !self.codec.is_empty() {
            self.write(&format!(
                "<html xmlns='http://www.w3.org/1999/xhtml' xml:lang='en' lang='en' charset='{}'>\n",
                self.codec
            ));
        } else {
            self.write("<html xmlns='http://www.w3.org/1999/xhtml' xml:lang='en' lang='en'>\n");
        }
        self.write("<head>\n");
        self.write("<title></title>\n");
        self.write("<meta http-equiv='Content-Type' content='text/html;charset=utf-8' />\n");
        self.write("<meta name='ocr-system' content='bolivar' />\n");
        self.write(
            "  <meta name='ocr-capabilities' content='ocr_page ocr_block ocr_line ocrx_word'/>\n",
        );
        self.write("</head>\n");
        self.write("<body>\n");
    }

    /// Write footer.
    fn write_footer(&mut self) {
        self.write("<!-- comment in the following line to debug -->\n");
        self.write("<!--script src='https://unpkg.com/hocrjs'></script--></body></html>\n");
    }

    /// Write text with control character handling.
    pub fn write_text(&mut self, text: &str) {
        let text = if self.stripcontrol {
            self.control_re.replace_all(text, "").to_string()
        } else {
            text.to_string()
        };
        self.write(&text);
    }

    /// Write accumulated word span.
    fn write_word(&mut self) {
        if !self.working_text.is_empty() {
            let mut bold_and_italic_styles = String::new();
            if self.working_font.contains("Italic") {
                bold_and_italic_styles.push_str("font-style: italic; ");
            }
            if self.working_font.contains("Bold") {
                bold_and_italic_styles.push_str("font-weight: bold; ");
            }
            // Escape font name and text content to prevent XSS
            let escaped_font = enc(&self.working_font);
            let escaped_text = enc(self.working_text.trim());
            let output = format!(
                "<span style='font:\"{}\"; font-size:{}; {}' class='ocrx_word' title='{}; x_font {}; x_fsize {}'>{}</span>",
                escaped_font,
                self.working_size,
                bold_and_italic_styles,
                self.bbox_repr(self.working_bbox),
                escaped_font,
                self.working_size,
                escaped_text
            );
            self.write(&output);
        }
        self.within_chars = false;
    }

    /// Receive and render a layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        self.page_bbox = ltpage.bbox();

        // Write page div
        let page_output = format!(
            "<div class='ocr_page' id='{}' title='{}'>\n",
            ltpage.pageid,
            self.bbox_repr(ltpage.bbox())
        );
        self.write(&page_output);

        // Render page content
        for item in ltpage.iter() {
            self.render_item(item);
        }

        self.write("</div>\n");
        self.pageno += 1;
    }

    /// Render an item.
    fn render_item(&mut self, item: &LTItem) {
        // Check if we need to write accumulated word before processing annotations
        if self.within_chars {
            if let LTItem::Anno(_) = item {
                self.write_word();
            }
        }

        match item {
            LTItem::Page(page) => {
                for child in page.iter() {
                    self.render_item(child);
                }
            }
            LTItem::TextLine(tl) => {
                let bbox = match tl {
                    TextLineType::Horizontal(h) => h.bbox(),
                    TextLineType::Vertical(v) => v.bbox(),
                };
                let line_output =
                    format!("<span class='ocr_line' title='{}'>", self.bbox_repr(bbox));
                self.write(&line_output);

                // Render children
                match tl {
                    TextLineType::Horizontal(h) => {
                        for elem in h.iter() {
                            self.render_textline_element(elem);
                        }
                    }
                    TextLineType::Vertical(v) => {
                        for elem in v.iter() {
                            self.render_textline_element(elem);
                        }
                    }
                }

                self.write("</span>\n");
            }
            LTItem::TextBox(tb) => {
                let (id, bbox) = match tb {
                    TextBoxType::Horizontal(h) => (h.index(), h.bbox()),
                    TextBoxType::Vertical(v) => (v.index(), v.bbox()),
                };
                let block_output = format!(
                    "<div class='ocr_block' id='{}' title='{}'>\n",
                    id,
                    self.bbox_repr(bbox)
                );
                self.write(&block_output);

                // Render children (text lines)
                match tb {
                    TextBoxType::Horizontal(h) => {
                        for line in h.iter() {
                            let line_bbox = line.bbox();
                            let line_output = format!(
                                "<span class='ocr_line' title='{}'>",
                                self.bbox_repr(line_bbox)
                            );
                            self.write(&line_output);
                            for elem in line.iter() {
                                self.render_textline_element(elem);
                            }
                            self.write("</span>\n");
                        }
                    }
                    TextBoxType::Vertical(v) => {
                        for line in v.iter() {
                            let line_bbox = line.bbox();
                            let line_output = format!(
                                "<span class='ocr_line' title='{}'>",
                                self.bbox_repr(line_bbox)
                            );
                            self.write(&line_output);
                            for elem in line.iter() {
                                self.render_textline_element(elem);
                            }
                            self.write("</span>\n");
                        }
                    }
                }

                self.write("</div>\n");
            }
            LTItem::Char(c) => {
                self.process_char(c);
            }
            LTItem::Anno(a) => {
                // Annotations (whitespace) - write accumulated word if any
                if self.within_chars {
                    self.write_word();
                }
                self.write_text(a.get_text());
            }
            LTItem::Figure(fig) => {
                for child in fig.iter() {
                    self.render_item(child);
                }
            }
            _ => {}
        }
    }

    /// Render a text line element (char or annotation).
    fn render_textline_element(&mut self, elem: &TextLineElement) {
        match elem {
            TextLineElement::Char(c) => {
                self.process_char(c);
            }
            TextLineElement::Anno(a) => {
                // Annotation ends current word
                if self.within_chars {
                    self.write_word();
                }
                self.write_text(a.get_text());
            }
        }
    }

    /// Process a character, accumulating into words.
    fn process_char(&mut self, c: &LTChar) {
        if !self.within_chars {
            // Start new word
            self.within_chars = true;
            self.working_text = c.get_text().to_string();
            self.working_bbox = c.bbox();
            self.working_font = c.fontname().to_string();
            self.working_size = c.size();
        } else if c.get_text().trim().is_empty() {
            // Whitespace character ends word
            self.write_word();
            self.write_text(c.get_text());
        } else {
            // Check if font/size/baseline changed - start new word if so
            let baseline_changed = (self.working_bbox.1 - c.bbox().1).abs() > 0.001;
            let font_changed = self.working_font != c.fontname();
            let size_changed = (self.working_size - c.size()).abs() > 0.001;

            if baseline_changed || font_changed || size_changed {
                self.write_word();
                self.within_chars = true;
                self.working_text = c.get_text().to_string();
                self.working_bbox = c.bbox();
                self.working_font = c.fontname().to_string();
                self.working_size = c.size();
            } else {
                // Continue accumulating word
                self.working_text.push_str(c.get_text());
                // Extend bbox to include this character
                self.working_bbox = (
                    self.working_bbox.0,
                    self.working_bbox.1,
                    c.bbox().2,
                    self.working_bbox.3,
                );
            }
        }
    }

    /// Close the converter.
    pub fn close(&mut self) {
        // Write any remaining word
        if self.within_chars {
            self.write_word();
        }
        self.write_footer();
    }
}
