//! PDF device trait + concrete implementations.
//!
//! A "device" is a sink for callbacks emitted by `PDFPageInterpreter` while it
//! walks a page's content stream. Concrete devices live one-per-file in this
//! directory (added by Task C2).

mod helpers;
mod table;

pub mod aggregator;
pub mod collector;
pub mod edge_probe;
pub mod html;
pub mod layout_analyzer;
pub mod text;
pub mod xml;

pub use aggregator::PDFPageAggregator;
pub use collector::PDFTableCollector;
pub use edge_probe::PDFEdgeProbe;
pub use html::{HOCRConverter, HTMLConverter};
pub use layout_analyzer::{LTContainer, PDFLayoutAnalyzer, PathOp};
pub(crate) use table::{PDFTableDevice, TableDeviceBuffers};
pub use text::TextConverter;
pub use xml::XMLConverter;

use crate::interp::types::{PDFFontLike, PDFStackT, PDFTextSeq, PDFTextSeqItem, PathSegment};
use crate::pdfcolor::PDFColorSpace;
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::pdftypes::PDFStream;
use crate::psparser::PSLiteral;
use crate::utils::{Matrix, Point, Rect, mult_matrix, translate_matrix};

/// PDF Device trait - interface for rendering PDF page content.
///
/// Implementations translate the output of PDFPageInterpreter to the
/// desired output format (text extraction, rendering, etc.).
pub trait PDFDevice {
    /// Set the current transformation matrix.
    fn set_ctm(&mut self, ctm: Matrix);

    /// Get the current transformation matrix.
    fn ctm(&self) -> Option<Matrix>;

    /// Close the device and release resources.
    fn close(&mut self) {}

    /// Return true when the device has collected all data that it needs.
    fn is_complete(&self) -> bool {
        false
    }

    /// Begin a marked content tag.
    fn begin_tag(&mut self, _tag: &PSLiteral, _props: Option<&PDFStackT>) {}

    /// End a marked content tag.
    fn end_tag(&mut self) {}

    /// Handle an inline marked content tag (no content).
    fn do_tag(&mut self, _tag: &PSLiteral, _props: Option<&PDFStackT>) {}

    /// Begin processing a page.
    fn begin_page(&mut self, _pageid: u32, _mediabox: Rect, _ctm: Matrix) {}

    /// End processing a page.
    fn end_page(&mut self, _pageid: u32) {}

    /// Begin a Form XObject (figure).
    fn begin_figure(&mut self, _name: &str, _bbox: Rect, _matrix: Matrix) {}

    /// End a Form XObject (figure).
    fn end_figure(&mut self, _name: &str) {}

    /// Paint a graphics path.
    fn paint_path(
        &mut self,
        _graphicstate: &PDFGraphicState,
        _stroke: bool,
        _fill: bool,
        _evenodd: bool,
        _path: &[PathSegment],
    ) {
    }

    /// Render an inline or XObject image.
    fn render_image(&mut self, _name: &str, _stream: &PDFStream) {}

    /// Render a text string.
    ///
    /// # Arguments
    /// * `textstate` - Current text state (font, size, spacing, etc.)
    /// * `seq` - Text sequence containing positioning and character data
    /// * `ncs` - Non-stroking color space
    /// * `graphicstate` - Current graphics state
    fn render_string(
        &mut self,
        _textstate: &mut PDFTextState,
        _seq: &PDFTextSeq,
        _ncs: &PDFColorSpace,
        _graphicstate: &PDFGraphicState,
    ) {
    }
}

/// PDF Text Device - base for text extraction devices.
///
/// Provides infrastructure for rendering text strings by iterating through
/// characters and tracking positions. Concrete implementations override
/// render_char to handle individual characters.
pub trait PDFTextDevice: PDFDevice {
    /// Render a text string by dispatching to horizontal or vertical rendering.
    ///
    /// This is the main entry point for text rendering. It computes the
    /// transformation matrix and delegates to render_string_horizontal or
    /// render_string_vertical based on font writing mode.
    ///
    /// Default implementation is a stub: the font is not yet pulled from
    /// `textstate` and dispatch by writing mode is not wired up. Concrete
    /// devices that need text override this; see `TagExtractor` for the
    /// Unicode-extraction path.
    fn render_string(
        &mut self,
        textstate: &mut PDFTextState,
        seq: &PDFTextSeq,
        ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) {
        let ctm = match self.ctm() {
            Some(ctm) => ctm,
            None => return,
        };
        let matrix = mult_matrix(textstate.matrix, ctm);

        // Stub: textstate.font is not yet wired to PDFCIDFont here. When it
        // is, dispatch to render_string_horizontal / render_string_vertical
        // based on font.is_vertical(). Left as scaffolding for now.

        let fontsize = textstate.fontsize;
        let scaling = textstate.scaling * 0.01;
        let charspace = textstate.charspace * scaling;
        let wordspace = textstate.wordspace * scaling;
        let rise = textstate.rise;
        let dxscale = 0.001 * fontsize * scaling;

        // Stub: Assume horizontal, no font available yet
        // When font is available, check font.is_vertical() and dispatch accordingly
        let _ = (
            matrix,
            charspace,
            wordspace,
            rise,
            dxscale,
            seq,
            ncs,
            graphicstate,
        );
    }

    /// Render a horizontal text string.
    ///
    /// Iterates through the text sequence, processing positioning adjustments
    /// and character data. Returns the final position after rendering.
    ///
    /// # Arguments
    /// * `seq` - Text sequence with numbers (positioning) and bytes (characters)
    /// * `matrix` - Transformation matrix for character rendering
    /// * `pos` - Starting position (x, y)
    /// * `font` - Font for decoding characters
    /// * `fontsize` - Font size in user units
    /// * `scaling` - Horizontal scaling factor
    /// * `charspace` - Character spacing
    /// * `wordspace` - Word spacing (applied to space character)
    /// * `rise` - Text rise (baseline offset)
    /// * `dxscale` - Positioning scale factor (0.001 * fontsize * scaling)
    /// * `ncs` - Non-stroking color space
    /// * `graphicstate` - Graphics state
    #[allow(clippy::too_many_arguments)]
    fn render_string_horizontal<F: PDFFontLike>(
        &mut self,
        seq: &PDFTextSeq,
        matrix: Matrix,
        pos: Point,
        font: &F,
        fontsize: f64,
        scaling: f64,
        charspace: f64,
        wordspace: f64,
        rise: f64,
        dxscale: f64,
        ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) -> Point {
        let (mut x, y) = pos;
        let mut needcharspace = false;

        for item in seq {
            match item {
                PDFTextSeqItem::Number(n) => {
                    x -= n * dxscale;
                    needcharspace = true;
                }
                PDFTextSeqItem::Bytes(data) => {
                    for cid in font.decode(data) {
                        if needcharspace {
                            x += charspace;
                        }
                        let char_matrix = translate_matrix(matrix, (x, y));
                        x += self.render_char(
                            char_matrix,
                            font,
                            fontsize,
                            scaling,
                            rise,
                            cid,
                            ncs,
                            graphicstate,
                        );
                        if cid == 32 && wordspace != 0.0 {
                            x += wordspace;
                        }
                        needcharspace = true;
                    }
                }
            }
        }
        (x, y)
    }

    /// Render a vertical text string.
    ///
    /// Similar to render_string_horizontal but advances in the Y direction.
    /// Used for vertical writing mode fonts (CJK vertical text).
    #[allow(clippy::too_many_arguments)]
    fn render_string_vertical<F: PDFFontLike>(
        &mut self,
        seq: &PDFTextSeq,
        matrix: Matrix,
        pos: Point,
        font: &F,
        fontsize: f64,
        scaling: f64,
        charspace: f64,
        wordspace: f64,
        rise: f64,
        dxscale: f64,
        ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) -> Point {
        let (x, mut y) = pos;
        let mut needcharspace = false;

        for item in seq {
            match item {
                PDFTextSeqItem::Number(n) => {
                    y -= n * dxscale;
                    needcharspace = true;
                }
                PDFTextSeqItem::Bytes(data) => {
                    for cid in font.decode(data) {
                        if needcharspace {
                            y += charspace;
                        }
                        let char_matrix = translate_matrix(matrix, (x, y));
                        y += self.render_char(
                            char_matrix,
                            font,
                            fontsize,
                            scaling,
                            rise,
                            cid,
                            ncs,
                            graphicstate,
                        );
                        if cid == 32 && wordspace != 0.0 {
                            y += wordspace;
                        }
                        needcharspace = true;
                    }
                }
            }
        }
        (x, y)
    }

    /// Render a single character.
    ///
    /// Returns the character advancement (width for horizontal, height for vertical).
    ///
    /// # Arguments
    /// * `matrix` - Transformation matrix for character placement
    /// * `font` - Font for glyph lookup
    /// * `fontsize` - Font size
    /// * `scaling` - Horizontal scaling factor
    /// * `rise` - Text rise
    /// * `cid` - Character ID
    /// * `ncs` - Non-stroking color space
    /// * `graphicstate` - Graphics state
    #[allow(clippy::too_many_arguments)]
    fn render_char<F: PDFFontLike>(
        &mut self,
        _matrix: Matrix,
        _font: &F,
        _fontsize: f64,
        _scaling: f64,
        _rise: f64,
        _cid: u32,
        _ncs: &PDFColorSpace,
        _graphicstate: &PDFGraphicState,
    ) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use crate::arena::PageArena;
    use crate::layout::table::{Orientation, PageGeometry};
    use crate::pdfstate::PDFGraphicState;

    use super::{PDFDevice, PDFTableDevice};

    #[test]
    fn table_device_collects_rectangle_edges_without_layout_objects() {
        let mut arena = PageArena::new();
        let geometry = PageGeometry {
            page_bbox: (0.0, 0.0, 100.0, 100.0),
            mediabox: (0.0, 0.0, 100.0, 100.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let mut device = PDFTableDevice::new(&mut arena, Some(geometry));
        device.begin_page(1, (0.0, 0.0, 100.0, 100.0), (1.0, 0.0, 0.0, 1.0, 0.0, 0.0));
        device.paint_path(
            &PDFGraphicState::new(),
            true,
            false,
            false,
            &[
                crate::interp::PathSegment::MoveTo(10.0, 20.0),
                crate::interp::PathSegment::LineTo(30.0, 20.0),
                crate::interp::PathSegment::LineTo(30.0, 40.0),
                crate::interp::PathSegment::LineTo(10.0, 40.0),
                crate::interp::PathSegment::ClosePath,
            ],
        );

        let (chars, edges) = device.objects();
        assert!(chars.is_empty());
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0].orientation, Some(Orientation::Horizontal));
        assert_eq!((edges[0].x0, edges[0].top, edges[0].x1), (10.0, 60.0, 30.0));
        assert_eq!(edges[1].top, 80.0);
    }

    #[test]
    fn table_device_reuses_edge_storage() {
        let mut arena = PageArena::new();
        let geometry = PageGeometry {
            page_bbox: (0.0, 0.0, 100.0, 100.0),
            mediabox: (0.0, 0.0, 100.0, 100.0),
            initial_doctop: 0.0,
            force_crop: false,
        };
        let mut device = PDFTableDevice::new(&mut arena, Some(geometry.clone()));
        device.paint_path(
            &PDFGraphicState::new(),
            true,
            false,
            false,
            &[
                crate::interp::PathSegment::MoveTo(10.0, 20.0),
                crate::interp::PathSegment::LineTo(30.0, 20.0),
            ],
        );
        let buffers = device.into_buffers();
        let edge_capacity = buffers.edge_capacity();

        let device = PDFTableDevice::with_buffers(&mut arena, Some(geometry), buffers);

        assert!(device.objects().1.is_empty());
        assert_eq!(device.buffer_capacity().1, edge_capacity);
    }
}
