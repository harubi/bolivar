//! `PDFTableCollector` device - captures arena pages without materializing `LTPage`.

use crate::arena::PageArena;
use crate::arena::page_arena::ArenaContext;
use crate::arena::types::ArenaPage;
use crate::layout::LAParams;
use crate::pdfcolor::PDFColorSpace;
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::pdftypes::PDFStream;
use crate::utils::{Matrix, Rect};

use super::PDFDevice;
use super::helpers::{path_segments_to_path_ops, render_text_sequence};
use super::layout_analyzer::PDFLayoutAnalyzer;

/// Table collector device that captures arena pages (no `LTPage` materialization).
pub struct PDFTableCollector<'a> {
    analyzer: PDFLayoutAnalyzer<'a>,
    result: Option<ArenaPage<'a>>,
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
        path: &[crate::interp::types::PathSegment],
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
        seq: &crate::interp::types::PDFTextSeq,
        _ncs: &PDFColorSpace,
        graphicstate: &PDFGraphicState,
    ) {
        render_text_sequence(&mut self.analyzer, textstate, seq, graphicstate);
    }
}
