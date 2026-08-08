//! `PDFPageAggregator` device - collects analyzed pages for later retrieval.
//!
//! Unlike streaming converters that output immediately, this aggregator stores
//! the most recent page for retrieval via `get_result()` / `result()`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::arena::PageArena;
use crate::image::ImageWriter;
use crate::layout::{LAParams, LTPage};
use crate::pdfcolor::PDFColorSpace;
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::pdftypes::PDFStream;
use crate::utils::{Matrix, Rect};

use super::PDFDevice;
use super::helpers::{path_segments_to_path_ops, render_text_sequence};
use super::layout_analyzer::PDFLayoutAnalyzer;

/// PDF Page Aggregator - collects analyzed pages for later retrieval.
pub struct PDFPageAggregator<'a> {
    #[allow(dead_code)]
    analyzer: PDFLayoutAnalyzer<'a>,
    result: Option<LTPage>,
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

    /// Take the analyzed page without cloning it.
    pub fn take_result(&mut self) -> Option<LTPage> {
        self.result.take()
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

    fn begin_tag(
        &mut self,
        tag: &crate::psparser::PSLiteral,
        props: Option<&crate::interp::types::PDFStackT>,
    ) {
        self.analyzer.begin_tag(tag, props);
    }

    fn end_tag(&mut self) {
        self.analyzer.end_tag();
    }
}
