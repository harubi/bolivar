//! Compact table-only PDF device.

use crate::arena::{ArenaContext, PageArena};
use crate::interp::{PDFTextSeq, PDFTextSeqItem, PathSegment};
use crate::layout::table::{
    BBox, CharObj, EdgeObj, Orientation, PageGeometry, bbox_overlap, clip_edge_to_bbox,
    rect_to_edges, to_top_left_bbox,
};
use crate::pdfcolor::PDFColorSpace;
use crate::pdffont::{CharDisp, PDFCIDFont, PDFFont};
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::utils::{Matrix, Point, Rect, apply_matrix_pt, apply_matrix_rect, mult_matrix};

use super::PDFDevice;

#[derive(Default)]
pub(crate) struct TableDeviceBuffers {
    chars: Vec<CharObj>,
    edges: Vec<EdgeObj>,
    path_ops: Vec<char>,
    path_points: Vec<Point>,
    cids: Vec<u32>,
}

impl TableDeviceBuffers {
    fn clear(&mut self) {
        self.chars.clear();
        self.edges.clear();
        self.path_ops.clear();
        self.path_points.clear();
        self.cids.clear();
    }

    #[cfg(test)]
    pub(crate) fn edge_capacity(&self) -> usize {
        self.edges.capacity()
    }
}

pub(crate) struct PDFTableDevice<'a> {
    arena: ArenaContext<'a>,
    geometry: Option<PageGeometry>,
    buffers: TableDeviceBuffers,
    ctm: Matrix,
    has_edges: bool,
}

impl<'a> PDFTableDevice<'a> {
    #[cfg(test)]
    pub(crate) fn new(arena: &'a mut PageArena, geometry: Option<PageGeometry>) -> Self {
        Self::with_buffers(arena, geometry, TableDeviceBuffers::default())
    }

    pub(crate) fn with_buffers(
        arena: &'a mut PageArena,
        geometry: Option<PageGeometry>,
        mut buffers: TableDeviceBuffers,
    ) -> Self {
        buffers.clear();
        Self {
            arena: arena.context(),
            geometry,
            buffers,
            ctm: (1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
            has_edges: false,
        }
    }

    pub(crate) fn objects(&self) -> (&[CharObj], &[EdgeObj]) {
        (&self.buffers.chars, &self.buffers.edges)
    }

    pub(crate) fn into_buffers(self) -> TableDeviceBuffers {
        self.buffers
    }

    #[cfg(test)]
    pub(crate) fn buffer_capacity(&self) -> (usize, usize) {
        (self.buffers.chars.capacity(), self.buffers.edges.capacity())
    }

    pub(crate) fn arena_lookup(&self) -> &ArenaContext<'a> {
        &self.arena
    }

    pub(crate) fn geometry(&self) -> &PageGeometry {
        self.geometry.as_ref().expect("table device page geometry")
    }

    pub(crate) const fn has_edges(&self) -> bool {
        self.has_edges
    }

    fn crop_bbox(&self) -> Option<BBox> {
        let geometry = self.geometry();
        if rects_equal(geometry.page_bbox, geometry.mediabox) {
            return None;
        }
        Some(BBox {
            x0: geometry.page_bbox.0,
            top: geometry.page_bbox.1,
            x1: geometry.page_bbox.2,
            bottom: geometry.page_bbox.3,
        })
    }

    fn push_edge(&mut self, edge: EdgeObj) {
        if let Some(crop) = self.crop_bbox() {
            if let Some(edge) = clip_edge_to_bbox(edge, crop) {
                self.buffers.edges.push(edge);
            }
            return;
        }
        self.buffers.edges.push(edge);
    }

    fn flush_path(&mut self) {
        if self.buffers.path_ops.len() < 2 || self.buffers.path_ops.first() != Some(&'m') {
            self.buffers.path_ops.clear();
            self.buffers.path_points.clear();
            return;
        }

        let len = self.buffers.path_ops.len();
        if len > 3
            && self.buffers.path_ops[len - 2..] == ['l', 'h']
            && self.buffers.path_points[len - 2] == self.buffers.path_points[0]
        {
            self.buffers.path_ops.remove(len - 2);
            self.buffers.path_points.remove(len - 2);
        }

        match self.buffers.path_ops.as_slice() {
            ['m', 'l'] | ['m', 'l', 'h'] => self.push_line(),
            ['m', 'l', 'l', 'l', 'h'] | ['m', 'l', 'l', 'l', 'l'] => {
                if !self.push_rect() {
                    self.push_curve();
                }
            }
            _ => self.push_curve(),
        }

        self.buffers.path_ops.clear();
        self.buffers.path_points.clear();
    }

    fn push_line(&mut self) {
        let p0 = self.buffers.path_points[0];
        let p1 = self.buffers.path_points[1];
        let raw = (
            p0.0.min(p1.0),
            p0.1.min(p1.1),
            p0.0.max(p1.0),
            p0.1.max(p1.1),
        );
        let bbox = to_top_left_bbox(raw.0, raw.1, raw.2, raw.3, self.geometry());
        self.push_edge(EdgeObj {
            x0: bbox.x0,
            x1: bbox.x1,
            top: bbox.top,
            bottom: bbox.bottom,
            width: bbox.width(),
            height: bbox.height(),
            orientation: if bbox.top == bbox.bottom {
                Some(Orientation::Horizontal)
            } else {
                Some(Orientation::Vertical)
            },
            object_type: "line",
        });
    }

    fn push_rect(&mut self) -> bool {
        if self.buffers.path_points.len() < 5 {
            return false;
        }
        let [p0, p1, p2, p3, p4] = self.buffers.path_points[..5] else {
            return false;
        };
        let square = (approx_eq(p0.0, p1.0)
            && approx_eq(p1.1, p2.1)
            && approx_eq(p2.0, p3.0)
            && approx_eq(p3.1, p0.1))
            || (approx_eq(p0.1, p1.1)
                && approx_eq(p1.0, p2.0)
                && approx_eq(p2.1, p3.1)
                && approx_eq(p3.0, p0.0));
        if p0 != p4 || !square {
            return false;
        }

        let bbox = to_top_left_bbox(
            p0.0.min(p2.0),
            p0.1.min(p2.1),
            p0.0.max(p2.0),
            p0.1.max(p2.1),
            self.geometry(),
        );
        for edge in rect_to_edges(bbox) {
            self.push_edge(edge);
        }
        true
    }

    fn push_curve(&mut self) {
        for index in 1..self.buffers.path_points.len() {
            let first = self.buffers.path_points[index - 1];
            let second = self.buffers.path_points[index];
            let first = to_top_left_bbox(first.0, first.1, first.0, first.1, self.geometry());
            let second = to_top_left_bbox(second.0, second.1, second.0, second.1, self.geometry());
            let x0 = first.x0.min(second.x0);
            let x1 = first.x0.max(second.x0);
            let top = first.top.min(second.top);
            let bottom = first.top.max(second.top);
            let orientation = if (first.x0 - second.x0).abs() < f64::EPSILON {
                Some(Orientation::Vertical)
            } else if (first.top - second.top).abs() < f64::EPSILON {
                Some(Orientation::Horizontal)
            } else {
                None
            };
            let edge = EdgeObj {
                x0,
                x1,
                top,
                bottom,
                width: (x1 - x0).abs(),
                height: (bottom - top).abs(),
                orientation,
                object_type: "curve_edge",
            };
            self.push_edge(edge);
        }
    }

    fn render_font_char(
        &mut self,
        matrix: Matrix,
        font: &PDFCIDFont,
        fontsize: f64,
        scaling: f64,
        rise: f64,
        cid: u32,
    ) -> f64 {
        let textwidth = font.char_width(cid);
        let textdisp = font.char_disp(cid);
        let advance = textwidth * fontsize * scaling;
        let bbox_text = if font.is_vertical() {
            match textdisp {
                CharDisp::Vertical(vx, vy) => {
                    let vx = vx
                        .map(|value| value * fontsize * 0.001)
                        .unwrap_or(fontsize * 0.5);
                    let vy = (1000.0 - vy) * fontsize * 0.001;
                    (-vx, vy + rise + advance, -vx + fontsize, vy + rise)
                }
                CharDisp::Horizontal(_) => {
                    let descent = font.get_descent() * fontsize;
                    (0.0, descent + rise, advance, descent + rise + fontsize)
                }
            }
        } else {
            let descent = font.get_descent() * fontsize;
            (0.0, descent + rise, advance, descent + rise + fontsize)
        };
        if let Some(text) = font.unicode_cow(cid) {
            self.push_char(
                matrix,
                text.as_ref(),
                bbox_text,
                scaling,
                font.is_vertical(),
            );
        } else {
            let text = format!("(cid:{cid})");
            self.push_char(matrix, &text, bbox_text, scaling, font.is_vertical());
        }
        advance
    }

    fn render_fallback_char(
        &mut self,
        matrix: Matrix,
        fontsize: f64,
        scaling: f64,
        rise: f64,
        cid: u32,
    ) -> f64 {
        let advance = fontsize * scaling * 0.6;
        let descent = -fontsize * 0.25;
        let bbox = (0.0, descent + rise, advance, descent + rise + fontsize);
        if let Some(value) = char::from_u32(cid).filter(|_| (0x20..0x7f).contains(&cid)) {
            let mut buffer = [0; 4];
            self.push_char(matrix, value.encode_utf8(&mut buffer), bbox, scaling, false);
        } else {
            let text = format!("(cid:{cid})");
            self.push_char(matrix, &text, bbox, scaling, false);
        }
        advance
    }

    fn push_char(
        &mut self,
        matrix: Matrix,
        text: &str,
        local_bbox: Rect,
        scaling: f64,
        vertical: bool,
    ) {
        let (x0, y0, x1, y1) = apply_matrix_rect(matrix, local_bbox);
        let raw = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
        let size = if vertical {
            raw.2 - raw.0
        } else {
            raw.3 - raw.1
        };
        let mut bbox = to_top_left_bbox(raw.0, raw.1, raw.2, raw.3, self.geometry());
        if let Some(crop) = self.crop_bbox() {
            let Some(overlap) = bbox_overlap(bbox, crop) else {
                return;
            };
            bbox = overlap;
        }
        let (a, b, c, d, _, _) = matrix;
        let text = self.arena.intern(text);
        self.buffers.chars.push(CharObj {
            text,
            x0: bbox.x0,
            x1: bbox.x1,
            top: bbox.top,
            bottom: bbox.bottom,
            doctop: self.geometry().initial_doctop + bbox.top,
            width: bbox.width(),
            height: bbox.height(),
            size,
            upright: (a * d * scaling > 0.0) && (b * c <= 0.0),
        });
    }
}

impl PDFDevice for PDFTableDevice<'_> {
    fn set_ctm(&mut self, ctm: Matrix) {
        self.ctm = ctm;
    }

    fn ctm(&self) -> Option<Matrix> {
        Some(self.ctm)
    }

    fn begin_page(&mut self, _pageid: u32, mediabox: Rect, ctm: Matrix) {
        self.ctm = ctm;
        if self.geometry.is_some() {
            return;
        }
        let (x0, y0, x1, y1) = apply_matrix_rect(ctm, mediabox);
        let bbox = (0.0, 0.0, (x0 - x1).abs(), (y0 - y1).abs());
        self.geometry = Some(PageGeometry {
            page_bbox: bbox,
            mediabox: bbox,
            initial_doctop: 0.0,
            force_crop: false,
        });
    }

    fn paint_path(
        &mut self,
        _graphicstate: &PDFGraphicState,
        stroke: bool,
        fill: bool,
        _evenodd: bool,
        path: &[PathSegment],
    ) {
        if !self.has_edges && (stroke || fill) {
            self.has_edges = path.iter().any(|segment| {
                matches!(segment, PathSegment::LineTo(..) | PathSegment::CurveTo(..))
            });
        }

        for segment in path {
            match *segment {
                PathSegment::MoveTo(x, y) => {
                    self.flush_path();
                    self.buffers.path_ops.push('m');
                    self.buffers
                        .path_points
                        .push(apply_matrix_pt(self.ctm, (x, y)));
                }
                PathSegment::LineTo(x, y) => {
                    self.buffers.path_ops.push('l');
                    self.buffers
                        .path_points
                        .push(apply_matrix_pt(self.ctm, (x, y)));
                }
                PathSegment::CurveTo(_, _, _, _, x, y) => {
                    self.buffers.path_ops.push('c');
                    self.buffers
                        .path_points
                        .push(apply_matrix_pt(self.ctm, (x, y)));
                }
                PathSegment::ClosePath => {
                    let Some(first) = self.buffers.path_points.first().copied() else {
                        continue;
                    };
                    self.buffers.path_ops.push('h');
                    self.buffers.path_points.push(first);
                }
            }
        }
        self.flush_path();
    }

    fn render_string(
        &mut self,
        textstate: &mut PDFTextState,
        seq: &PDFTextSeq,
        _ncs: &PDFColorSpace,
        _graphicstate: &PDFGraphicState,
    ) {
        if textstate.render == 3 || textstate.render == 7 {
            return;
        }

        let matrix = mult_matrix(textstate.matrix, self.ctm);
        let fontsize = textstate.fontsize;
        let scaling = textstate.scaling * 0.01;
        let charspace = textstate.charspace * scaling;
        let wordspace = textstate.wordspace * scaling;
        let rise = textstate.rise;
        let dxscale = 0.001 * fontsize * scaling;
        let (mut x, mut y) = textstate.linematrix;
        let mut need_charspace = false;
        let font = textstate.font.clone();
        let vertical = font.as_ref().is_some_and(|font| font.is_vertical());

        for item in seq {
            match item {
                PDFTextSeqItem::Number(value) => {
                    if vertical {
                        y -= value * dxscale;
                    } else {
                        x -= value * dxscale;
                    }
                    need_charspace = true;
                }
                PDFTextSeqItem::Bytes(data) => {
                    self.buffers.cids.clear();
                    if let Some(font) = &font {
                        font.decode_into(data, &mut self.buffers.cids);
                    } else {
                        self.buffers
                            .cids
                            .extend(data.iter().map(|byte| *byte as u32));
                    }
                    for index in 0..self.buffers.cids.len() {
                        let cid = self.buffers.cids[index];
                        if need_charspace {
                            if vertical {
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
                        let advance = if let Some(font) = &font {
                            self.render_font_char(
                                char_matrix,
                                font.as_ref(),
                                fontsize,
                                scaling,
                                rise,
                                cid,
                            )
                        } else {
                            self.render_fallback_char(char_matrix, fontsize, scaling, rise, cid)
                        };
                        if vertical {
                            y += advance;
                        } else {
                            x += advance;
                        }
                        if cid == 32 && wordspace != 0.0 {
                            if vertical {
                                y += wordspace;
                            } else {
                                x += wordspace;
                            }
                        }
                        need_charspace = true;
                    }
                }
            }
        }
        textstate.linematrix = (x, y);
    }
}

fn rects_equal(a: Rect, b: Rect) -> bool {
    const EPSILON: f64 = 1e-6;
    (a.0 - b.0).abs() < EPSILON
        && (a.1 - b.1).abs() < EPSILON
        && (a.2 - b.2).abs() < EPSILON
        && (a.3 - b.3).abs() < EPSILON
}

fn approx_eq(a: f64, b: f64) -> bool {
    const EPSILON: f64 = 1e-6;
    (a - b).abs() < EPSILON
}
