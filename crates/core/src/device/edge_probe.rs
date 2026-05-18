//! `PDFEdgeProbe` device - lightweight probe for vector edges without building layout.

use crate::interp::types::PathSegment;
use crate::pdfstate::PDFGraphicState;
use crate::utils::Matrix;

use super::PDFDevice;

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
