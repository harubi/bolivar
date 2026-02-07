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

use lasso::Spur;

use crate::arena::PageArena;
use crate::arena::page_arena::ArenaContext;
use crate::arena::types::{
    ArenaChar, ArenaCurve, ArenaFigure, ArenaImage, ArenaItem, ArenaLine, ArenaPage, ArenaRect,
};
use crate::image::ImageWriter;
use crate::layout::{LAParams, LTItem, LTPage};
use crate::pdfcolor::PDFColorSpace;
use crate::pdfdevice::{PDFDevice, PDFTextSeq, PDFTextSeqItem, PathSegment};
use crate::pdffont::{CharDisp, PDFFont};
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::pdftypes::PDFStream;
use crate::utils::{Matrix, Point, Rect, apply_matrix_pt, apply_matrix_rect, mult_matrix};
use bumpalo::collections::Vec as BumpVec;

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
    pub const fn new(bbox: Rect) -> Self {
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
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get items slice.
    pub fn items(&self) -> &[LTItem] {
        &self.items
    }

    /// Get bounding box.
    pub const fn bbox(&self) -> Rect {
        self.bbox
    }
}

#[derive(Debug, Clone)]
struct ArenaContainer<'a> {
    bbox: Rect,
    items: BumpVec<'a, ArenaItem<'a>>,
}

impl<'a> ArenaContainer<'a> {
    pub fn new_in(arena: &ArenaContext<'a>, bbox: Rect) -> Self {
        Self {
            bbox,
            items: BumpVec::new_in(arena.bump()),
        }
    }

    pub fn add(&mut self, item: ArenaItem<'a>) {
        self.items.push(item);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub const fn bbox(&self) -> Rect {
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
    tag: Spur,
    /// Marked Content ID from properties dict
    mcid: Option<i32>,
}

#[derive(Debug, Clone)]
struct FigureState {
    name: Spur,
    bbox: Rect,
    matrix: Matrix,
}

pub struct PDFLayoutAnalyzer<'a> {
    /// Current page number (1-indexed)
    pageno: i32,
    /// Layout analysis parameters
    laparams: Option<LAParams>,
    /// Stack of layout containers (for nested figures)
    stack: Vec<ArenaContainer<'a>>,
    /// Stack of figure metadata (for nested figures)
    figure_stack: Vec<FigureState>,
    /// Current layout container
    cur_item: Option<ArenaContainer<'a>>,
    /// Current transformation matrix
    pub(crate) ctm: Matrix,
    /// Optional image writer for exporting images
    image_writer: Option<Rc<RefCell<ImageWriter>>>,
    /// Page-scoped arena for typed layout items
    arena: ArenaContext<'a>,
    /// Stack of marked content states (for BMC/BDC/EMC operators)
    marked_content_stack: Vec<MarkedContentState>,
}

impl<'a> PDFLayoutAnalyzer<'a> {
    /// Create a new layout analyzer.
    ///
    /// # Arguments
    /// * `laparams` - Layout analysis parameters (None to disable analysis)
    /// * `pageno` - Starting page number (1-indexed)
    pub fn new(laparams: Option<LAParams>, pageno: i32, arena: ArenaContext<'a>) -> Self {
        Self::new_with_imagewriter(laparams, pageno, None, arena)
    }

    /// Create a new layout analyzer with an optional image writer.
    pub fn new_with_imagewriter(
        laparams: Option<LAParams>,
        pageno: i32,
        image_writer: Option<Rc<RefCell<ImageWriter>>>,
        arena: ArenaContext<'a>,
    ) -> Self {
        Self {
            pageno,
            laparams,
            stack: Vec::new(),
            figure_stack: Vec::new(),
            cur_item: None,
            ctm: (1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
            image_writer,
            arena,
            marked_content_stack: Vec::new(),
        }
    }

    /// Get the current page number.
    pub const fn pageno(&self) -> i32 {
        self.pageno
    }

    /// Set the current transformation matrix.
    pub const fn set_ctm(&mut self, ctm: Matrix) {
        self.ctm = ctm;
    }

    /// Set the current layout container bbox (for testing).
    pub fn set_cur_item(&mut self, bbox: Rect) {
        self.cur_item = Some(ArenaContainer::new_in(&self.arena, bbox));
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
            .map(|item| matches!(item, ArenaItem::Rect(_)))
            .unwrap_or(false)
    }

    /// Check if first item is a curve.
    pub fn cur_item_first_is_curve(&self) -> bool {
        self.cur_item
            .as_ref()
            .and_then(|c| c.items.first())
            .map(|item| matches!(item, ArenaItem::Curve(_)))
            .unwrap_or(false)
    }

    /// Check if all items are lines.
    pub fn all_cur_items_are_lines(&self) -> bool {
        self.cur_item
            .as_ref()
            .map(|c| {
                c.items
                    .iter()
                    .all(|item| matches!(item, ArenaItem::Line(_)))
            })
            .unwrap_or(false)
    }

    /// Get points of first curve/line item.
    pub fn cur_item_first_pts(&self) -> Vec<Point> {
        self.cur_item
            .as_ref()
            .and_then(|c| c.items.first())
            .and_then(|item| match item {
                ArenaItem::Curve(c) => Some(c.pts.clone()),
                ArenaItem::Line(l) => Some(vec![l.p0, l.p1]),
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
                ArenaItem::Curve(c) => c.dashing_style.clone(),
                ArenaItem::Line(l) => l.dashing_style.clone(),
                _ => None,
            })
    }

    /// Get original_path of first curve item (for testing).
    pub fn cur_item_first_original_path(&self) -> Option<Vec<(char, Vec<Point>)>> {
        self.cur_item
            .as_ref()
            .and_then(|c| c.items.first())
            .and_then(|item| match item {
                ArenaItem::Curve(c) => c.original_path.clone(),
                ArenaItem::Line(l) => l.original_path.clone(),
                _ => None,
            })
    }

    /// Check if currently in a figure.
    pub const fn in_figure(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn arena_lookup(&self) -> &ArenaContext<'a> {
        &self.arena
    }

    /// Begin processing a page.
    pub fn begin_page(&mut self, mediabox: Rect, ctm: Matrix) {
        let (x0, y0, x1, y1) = apply_matrix_rect(ctm, mediabox);
        let bbox = (0.0, 0.0, (x0 - x1).abs(), (y0 - y1).abs());
        self.marked_content_stack.clear();
        self.cur_item = Some(ArenaContainer::new_in(&self.arena, bbox));
    }

    /// End processing a page and return the arena page (no materialization).
    pub fn end_page_arena(&mut self) -> Option<ArenaPage<'a>> {
        assert!(self.stack.is_empty(), "stack not empty");
        let container = self.cur_item.take()?;
        let mut page = ArenaPage {
            pageid: self.pageno,
            bbox: container.bbox,
            rotate: 0.0,
            items: BumpVec::new_in(self.arena.bump()),
        };
        for item in container.items {
            page.add(item);
        }
        self.pageno += 1;
        Some(page)
    }

    /// End processing a page and return the analyzed page.
    pub fn end_page(&mut self) -> Option<LTPage> {
        let page = self.end_page_arena()?;
        let mut ltpage = page.materialize(&self.arena);
        if let Some(ref laparams) = self.laparams {
            ltpage.analyze(laparams);
        }
        Some(ltpage)
    }

    /// Begin processing a figure (Form XObject).
    pub fn begin_figure(&mut self, name: &str, bbox: Rect, matrix: Matrix) {
        if let Some(cur) = self.cur_item.take() {
            self.stack.push(cur);
        }
        let combined_matrix = mult_matrix(matrix, self.ctm);
        let rect = (bbox.0, bbox.1, bbox.0 + bbox.2, bbox.1 + bbox.3);
        let fig_bbox = apply_matrix_rect(combined_matrix, rect);
        let name_key = self.arena.intern(name);
        self.figure_stack.push(FigureState {
            name: name_key,
            bbox,
            matrix: combined_matrix,
        });
        let fig_container = ArenaContainer::new_in(&self.arena, fig_bbox);
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
        let fig = ArenaFigure {
            name: fig_state.name,
            bbox: fig_state.bbox,
            matrix: fig_state.matrix,
            items: fig_container.items,
        };
        if let Some(mut parent) = self.stack.pop() {
            parent.add(ArenaItem::Figure(fig));
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
        let scolor = self.arena.intern_color(&gstate.scolor.to_vec());
        let ncolor = self.arena.intern_color(&gstate.ncolor.to_vec());

        // Create appropriate layout object
        let mut item = match shape.as_str() {
            "mlh" | "ml" => {
                // Single line segment
                ArenaItem::Line(ArenaLine {
                    linewidth: gstate.linewidth,
                    p0: pts[0],
                    p1: pts[1],
                    stroke,
                    fill,
                    evenodd,
                    stroking_color: scolor,
                    non_stroking_color: ncolor,
                    original_path,
                    dashing_style,
                    mcid: None,
                    tag: None,
                })
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
                        ArenaItem::Rect(ArenaRect {
                            linewidth: gstate.linewidth,
                            bbox: (pts[0].0, pts[0].1, pts[2].0, pts[2].1),
                            stroke,
                            fill,
                            evenodd,
                            stroking_color: scolor,
                            non_stroking_color: ncolor,
                            original_path,
                            dashing_style,
                            mcid: None,
                            tag: None,
                        })
                    } else {
                        ArenaItem::Curve(ArenaCurve {
                            linewidth: gstate.linewidth,
                            pts,
                            stroke,
                            fill,
                            evenodd,
                            stroking_color: scolor,
                            non_stroking_color: ncolor,
                            original_path,
                            dashing_style,
                            mcid: None,
                            tag: None,
                        })
                    }
                } else {
                    ArenaItem::Curve(ArenaCurve {
                        linewidth: gstate.linewidth,
                        pts,
                        stroke,
                        fill,
                        evenodd,
                        stroking_color: scolor,
                        non_stroking_color: ncolor,
                        original_path,
                        dashing_style,
                        mcid: None,
                        tag: None,
                    })
                }
            }
            _ => {
                // Generic curve
                ArenaItem::Curve(ArenaCurve {
                    linewidth: gstate.linewidth,
                    pts,
                    stroke,
                    fill,
                    evenodd,
                    stroking_color: scolor,
                    non_stroking_color: ncolor,
                    original_path,
                    dashing_style,
                    mcid: None,
                    tag: None,
                })
            }
        };

        let mcid = self.current_mcid();
        let tag = self.current_tag_key();
        match &mut item {
            ArenaItem::Line(l) => {
                l.mcid = mcid;
                l.tag = tag;
            }
            ArenaItem::Rect(r) => {
                r.mcid = mcid;
                r.tag = tag;
            }
            ArenaItem::Curve(c) => {
                c.mcid = mcid;
                c.tag = tag;
            }
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
        graphicstate: &PDFGraphicState,
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
        let text_key = self.arena.intern(&text);
        let fontname_key = self.arena.intern(fontname);
        let ncolor = self.arena.intern_color(&graphicstate.ncolor.to_vec());
        let scolor = self.arena.intern_color(&graphicstate.scolor.to_vec());
        let tag = self.current_tag_key();
        let ncs_name = Some(self.arena.intern(&graphicstate.ncs.name));
        let scs_name = Some(self.arena.intern(&graphicstate.scs.name));
        let item = ArenaChar {
            bbox,
            text: text_key,
            fontname: fontname_key,
            size,
            upright,
            adv,
            matrix,
            mcid: self.current_mcid(),
            tag,
            ncs_name,
            scs_name,
            ncolor,
            scolor,
        };

        if let Some(ref mut container) = self.cur_item {
            container.add(ArenaItem::Char(item));
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

        let name_key = self.arena.intern(name);
        let mut colorspace_keys = BumpVec::new_in(self.arena.bump());
        for cs in &colorspace {
            colorspace_keys.push(self.arena.intern(cs));
        }
        let item = ArenaImage {
            name: name_key,
            bbox,
            srcsize,
            imagemask,
            bits,
            colorspace: colorspace_keys,
        };

        // Export image if writer is configured
        if let Some(ref writer) = self.image_writer {
            let _ = writer
                .borrow_mut()
                .export_image(name, stream, srcsize, bits, &colorspace);
        }

        // Add to current container
        if let Some(ref mut container) = self.cur_item {
            container.add(ArenaItem::Image(item));
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
        self.marked_content_stack
            .last()
            .map(|mc| self.arena.resolve(mc.tag))
    }

    fn current_tag_key(&self) -> Option<Spur> {
        self.marked_content_stack.last().map(|mc| mc.tag)
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

        let tag_key = self.arena.intern(tag.name());
        self.marked_content_stack
            .push(MarkedContentState { tag: tag_key, mcid });
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

fn path_segments_to_path_ops(path: &[PathSegment]) -> Vec<PathOp> {
    path.iter()
        .map(|seg| match seg {
            PathSegment::MoveTo(x, y) => ('m', vec![*x, *y]),
            PathSegment::LineTo(x, y) => ('l', vec![*x, *y]),
            PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) => {
                ('c', vec![*x1, *y1, *x2, *y2, *x3, *y3])
            }
            PathSegment::ClosePath => ('h', vec![]),
        })
        .collect()
}

struct FallbackCharRender<'a> {
    char_matrix: Matrix,
    fontsize: f64,
    scaling: f64,
    rise: f64,
    cid: u32,
    fallback_fontname: Option<&'a str>,
}

fn render_char_without_font(
    analyzer: &mut PDFLayoutAnalyzer<'_>,
    graphicstate: &PDFGraphicState,
    render: FallbackCharRender<'_>,
) -> f64 {
    let FallbackCharRender {
        char_matrix,
        fontsize,
        scaling,
        rise,
        cid,
        fallback_fontname,
    } = render;
    let text = if (0x20..0x7f).contains(&cid) {
        char::from_u32(cid)
            .map(|c| c.to_string())
            .unwrap_or_else(|| format!("(cid:{})", cid))
    } else {
        format!("(cid:{})", cid)
    };

    let char_width = fontsize * scaling * 0.6;
    let descent = -fontsize * 0.25;
    let local_bbox = (0.0, descent + rise, char_width, descent + rise + fontsize);
    let bbox = apply_matrix_rect(char_matrix, local_bbox);

    let (a, b, c, d, _, _) = char_matrix;
    let upright = (a * d * scaling > 0.0) && (b * c <= 0.0);

    let mcid = analyzer.current_mcid();
    let tag = analyzer.current_tag_key();
    let ncolor = analyzer.arena.intern_color(&graphicstate.ncolor.to_vec());
    let scolor = analyzer.arena.intern_color(&graphicstate.scolor.to_vec());
    let fontname = fallback_fontname.unwrap_or("unknown");
    let text_key = analyzer.arena.intern(&text);
    let fontname_key = analyzer.arena.intern(fontname);
    let ncs_name = Some(analyzer.arena.intern(&graphicstate.ncs.name));
    let scs_name = Some(analyzer.arena.intern(&graphicstate.scs.name));
    let item = ArenaChar {
        bbox,
        text: text_key,
        fontname: fontname_key,
        size: bbox.3 - bbox.1,
        upright,
        adv: char_width,
        matrix: char_matrix,
        mcid,
        tag,
        ncs_name,
        scs_name,
        ncolor,
        scolor,
    };

    if let Some(ref mut container) = analyzer.cur_item {
        container.add(ArenaItem::Char(item));
    }

    char_width
}

fn render_text_sequence(
    analyzer: &mut PDFLayoutAnalyzer<'_>,
    textstate: &mut PDFTextState,
    seq: &PDFTextSeq,
    graphicstate: &PDFGraphicState,
) {
    if textstate.render == 3 || textstate.render == 7 {
        return;
    }

    let ctm = analyzer.ctm;
    let matrix = mult_matrix(textstate.matrix, ctm);
    let fontsize = textstate.fontsize;
    let scaling = textstate.scaling * 0.01;
    let charspace = textstate.charspace * scaling;
    let wordspace = textstate.wordspace * scaling;
    let rise = textstate.rise;
    let dxscale = 0.001 * fontsize * scaling;

    let (mut x, mut y) = textstate.linematrix;
    let mut needcharspace = false;

    let font = textstate.font.clone();
    let fallback_fontname = textstate.fontname.clone();
    let is_vertical = font.as_ref().map(|f| f.is_vertical()).unwrap_or(false);

    for item in seq {
        match item {
            PDFTextSeqItem::Number(n) => {
                if is_vertical {
                    y -= n * dxscale;
                } else {
                    x -= n * dxscale;
                }
                needcharspace = true;
            }
            PDFTextSeqItem::Bytes(data) => {
                let cids: Vec<u32> = if let Some(ref font) = font {
                    font.decode(data)
                } else {
                    data.iter().map(|&b| b as u32).collect()
                };

                for cid in cids {
                    if needcharspace {
                        if is_vertical {
                            y += charspace;
                        } else {
                            x += charspace;
                        }
                    }

                    let char_matrix = (
                        matrix.0,
                        matrix.1,
                        matrix.2,
                        matrix.3,
                        matrix.0.mul_add(x, matrix.2 * y) + matrix.4,
                        matrix.1.mul_add(x, matrix.3 * y) + matrix.5,
                    );

                    let adv = if let Some(ref font) = font {
                        analyzer.render_char(
                            char_matrix,
                            font.as_ref(),
                            fontsize,
                            scaling,
                            rise,
                            cid,
                            &graphicstate.ncs,
                            graphicstate,
                        )
                    } else {
                        render_char_without_font(
                            analyzer,
                            graphicstate,
                            FallbackCharRender {
                                char_matrix,
                                fontsize,
                                scaling,
                                rise,
                                cid,
                                fallback_fontname: fallback_fontname.as_deref(),
                            },
                        )
                    };

                    if is_vertical {
                        y += adv;
                    } else {
                        x += adv;
                    }

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

    textstate.linematrix = (x, y);
}

// ============================================================================
// PDFPageAggregator
// ============================================================================

/// PDF Page Aggregator - collects analyzed pages for later retrieval.
///
/// Unlike other converters that output immediately, this aggregator stores
/// the most recent page for retrieval via get_result().
pub struct PDFPageAggregator<'a> {
    #[allow(dead_code)]
    analyzer: PDFLayoutAnalyzer<'a>,
    result: Option<LTPage>,
}

/// Table collector device that captures arena pages (no LTPage materialization).
pub struct PDFTableCollector<'a> {
    analyzer: PDFLayoutAnalyzer<'a>,
    result: Option<ArenaPage<'a>>,
}

/// Lightweight device to probe for vector edges without building layout.
pub struct PDFEdgeProbe {
    has_edges: bool,
    ctm: Option<Matrix>,
}

impl PDFEdgeProbe {
    pub const fn new() -> Self {
        Self {
            has_edges: false,
            ctm: None,
        }
    }

    pub const fn has_edges(&self) -> bool {
        self.has_edges
    }
}

impl Default for PDFEdgeProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> PDFPageAggregator<'a> {
    /// Create a new page aggregator.
    pub fn new(laparams: Option<LAParams>, pageno: i32, arena: &'a mut PageArena) -> Self {
        Self::new_with_imagewriter(laparams, pageno, None, arena)
    }

    /// Create a new page aggregator with an optional image writer.
    pub fn new_with_imagewriter(
        laparams: Option<LAParams>,
        pageno: i32,
        image_writer: Option<Rc<RefCell<ImageWriter>>>,
        arena: &'a mut PageArena,
    ) -> Self {
        Self {
            analyzer: PDFLayoutAnalyzer::new_with_imagewriter(
                laparams,
                pageno,
                image_writer,
                arena.context(),
            ),
            result: None,
        }
    }

    /// Receive the analyzed layout page.
    pub fn receive_layout(&mut self, ltpage: LTPage) {
        self.result = Some(ltpage);
    }

    /// Get the result (if any).
    pub const fn result(&self) -> Option<&LTPage> {
        self.result.as_ref()
    }

    /// Get the result, panicking if none.
    pub const fn get_result(&self) -> &LTPage {
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

impl<'a> PDFTableCollector<'a> {
    pub fn new(laparams: Option<LAParams>, pageno: i32, arena: &'a mut PageArena) -> Self {
        Self {
            analyzer: PDFLayoutAnalyzer::new(laparams, pageno, arena.context()),
            result: None,
        }
    }

    pub fn take_result(&mut self) -> Option<ArenaPage<'a>> {
        self.result.take()
    }

    pub fn arena_lookup(&self) -> &ArenaContext<'a> {
        self.analyzer.arena_lookup()
    }
}

impl<'a> PDFDevice for PDFPageAggregator<'a> {
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
        let path_ops = path_segments_to_path_ops(path);
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
        render_text_sequence(&mut self.analyzer, textstate, seq, graphicstate);
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

impl<'a> PDFDevice for PDFTableCollector<'a> {
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
        if let Some(page) = self.analyzer.end_page_arena() {
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
        let path_ops = path_segments_to_path_ops(path);
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
        render_text_sequence(&mut self.analyzer, textstate, seq, graphicstate);
    }
}

impl PDFDevice for PDFEdgeProbe {
    fn set_ctm(&mut self, ctm: Matrix) {
        self.ctm = Some(ctm);
    }

    fn ctm(&self) -> Option<Matrix> {
        self.ctm
    }

    fn paint_path(
        &mut self,
        _graphicstate: &PDFGraphicState,
        stroke: bool,
        fill: bool,
        _evenodd: bool,
        path: &[PathSegment],
    ) {
        if self.has_edges || (!stroke && !fill) {
            return;
        }
        for seg in path {
            match seg {
                PathSegment::LineTo(..) | PathSegment::CurveTo(..) => {
                    self.has_edges = true;
                    break;
                }
                PathSegment::MoveTo(..) | PathSegment::ClosePath => {}
            }
        }
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
    pub const fn is_binary_stream<T>(_stream: &T) -> bool {
        true
    }

    /// Check if output is text (not binary).
    pub const fn is_text_stream<T>(_stream: &T) -> bool {
        false
    }
}

#[cfg(test)]
mod shared_converter_helper_tests {
    use super::*;
    use crate::arena::PageArena;
    use crate::pdfstate::PDFTextState;
    use crate::utils::MATRIX_IDENTITY;

    #[test]
    fn path_segments_to_path_ops_converts_variants() {
        let path = vec![
            PathSegment::MoveTo(1.0, 2.0),
            PathSegment::LineTo(3.0, 4.0),
            PathSegment::CurveTo(5.0, 6.0, 7.0, 8.0, 9.0, 10.0),
            PathSegment::ClosePath,
        ];

        let got = path_segments_to_path_ops(&path);
        let expected: Vec<PathOp> = vec![
            ('m', vec![1.0, 2.0]),
            ('l', vec![3.0, 4.0]),
            ('c', vec![5.0, 6.0, 7.0, 8.0, 9.0, 10.0]),
            ('h', vec![]),
        ];

        assert_eq!(got, expected);
    }

    #[test]
    fn render_text_sequence_without_font_uses_fallback_and_advances_cursor() {
        let mut arena = PageArena::new();
        let mut analyzer = PDFLayoutAnalyzer::new(None, 1, arena.context());
        analyzer.set_ctm(MATRIX_IDENTITY);
        analyzer.set_cur_item((0.0, 0.0, 100.0, 100.0));

        let mut textstate = PDFTextState {
            fontsize: 10.0,
            matrix: MATRIX_IDENTITY,
            linematrix: (5.0, 7.0),
            ..Default::default()
        };
        let seq = vec![PDFTextSeqItem::Bytes(vec![65])];
        let graphicstate = PDFGraphicState::default();

        render_text_sequence(&mut analyzer, &mut textstate, &seq, &graphicstate);

        assert_eq!(analyzer.cur_item_len(), 1);
        assert!((textstate.linematrix.0 - 11.0).abs() < 1e-9);
        assert!((textstate.linematrix.1 - 7.0).abs() < 1e-9);
    }
}
