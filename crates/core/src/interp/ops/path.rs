//! Path construction and painting operators.
//!
//! Handles: m, l, c, v, y, h, re, S, s, f, F, f*, B, B*, b, b*, n, W, W*
//!
//! Path construction:
//! - m: Move to
//! - l: Line to
//! - c, v, y: Bezier curves (cubic variants)
//! - h: Close subpath
//! - re: Rectangle shorthand
//!
//! Path painting:
//! - S/s: Stroke (s closes first)
//! - f/F/f*: Fill (F is legacy, f* uses even-odd rule)
//! - B/B*/b/b*: Fill then stroke
//! - n: End path (no-op, often with clipping)
//!
//! Clipping:
//! - W/W*: Set clipping path (non-zero/even-odd)

use crate::interp::device::{PDFDevice, PathSegment};
use crate::interp::interpreter::PDFPageInterpreter;

#[allow(non_snake_case)]
impl<'a, D: PDFDevice> PDFPageInterpreter<'a, D> {
    // ========================================================================
    // Path Construction Operators
    // ========================================================================

    /// Begins a new subpath at the given point.
    ///
    /// PDF operator: `m`
    pub fn do_m(&mut self, x: f64, y: f64) {
        self.curpath.push(PathSegment::MoveTo(x, y));
        self.current_point = Some((x, y));
    }

    /// Appends a straight line segment from the current point.
    ///
    /// PDF operator: `l`
    pub fn do_l(&mut self, x: f64, y: f64) {
        self.curpath.push(PathSegment::LineTo(x, y));
        self.current_point = Some((x, y));
    }

    /// Appends a cubic Bezier curve to the path.
    ///
    /// The curve extends from the current point to (x3, y3),
    /// using (x1, y1) and (x2, y2) as control points.
    ///
    /// PDF operator: `c`
    pub fn do_c(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.curpath
            .push(PathSegment::CurveTo(x1, y1, x2, y2, x3, y3));
        self.current_point = Some((x3, y3));
    }

    /// Appends a cubic Bezier curve with the current point as first control point.
    ///
    /// The curve extends from the current point to (x3, y3),
    /// using the current point as the first control point and (x2, y2) as the second.
    ///
    /// PDF operator: `v`
    pub fn do_v(&mut self, x2: f64, y2: f64, x3: f64, y3: f64) {
        let (x1, y1) = self.current_point.unwrap_or((0.0, 0.0));
        self.curpath
            .push(PathSegment::CurveTo(x1, y1, x2, y2, x3, y3));
        self.current_point = Some((x3, y3));
    }

    /// Appends a cubic Bezier curve with the endpoint as second control point.
    ///
    /// The curve extends from the current point to (x3, y3),
    /// using (x1, y1) as the first control point and (x3, y3) as the second.
    ///
    /// PDF operator: `y`
    pub fn do_y(&mut self, x1: f64, y1: f64, x3: f64, y3: f64) {
        self.curpath
            .push(PathSegment::CurveTo(x1, y1, x3, y3, x3, y3));
        self.current_point = Some((x3, y3));
    }

    /// Closes the current subpath by appending a straight line from the
    /// current point to the starting point of the subpath.
    ///
    /// PDF operator: `h`
    pub fn do_h(&mut self) {
        self.curpath.push(PathSegment::ClosePath);
    }

    /// Appends a rectangle to the current path as a complete subpath.
    ///
    /// Equivalent to: m x y; l x+w y; l x+w y+h; l x y+h; h
    ///
    /// PDF operator: `re`
    pub fn do_re(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.curpath.push(PathSegment::MoveTo(x, y));
        self.curpath.push(PathSegment::LineTo(x + w, y));
        self.curpath.push(PathSegment::LineTo(x + w, y + h));
        self.curpath.push(PathSegment::LineTo(x, y + h));
        self.curpath.push(PathSegment::ClosePath);
        self.current_point = Some((x, y));
    }

    // ========================================================================
    // Path Painting Operators
    // ========================================================================

    /// Helper to paint the path and clear it.
    fn paint_path(&mut self, stroke: bool, fill: bool, evenodd: bool) {
        self.device
            .paint_path(&self.graphicstate, stroke, fill, evenodd, &self.curpath);
        self.curpath.clear();
        self.current_point = None;
    }

    /// Strokes the current path.
    ///
    /// PDF operator: `S`
    pub fn do_S(&mut self) {
        self.paint_path(true, false, false);
    }

    /// Closes and strokes the current path.
    ///
    /// Equivalent to: h S
    ///
    /// PDF operator: `s`
    pub fn do_s(&mut self) {
        self.do_h();
        self.do_S();
    }

    /// Fills the current path using the nonzero winding number rule.
    ///
    /// PDF operator: `f`
    pub fn do_f(&mut self) {
        self.paint_path(false, true, false);
    }

    /// Fills the current path using the nonzero winding number rule (obsolete).
    ///
    /// This is equivalent to `f` and exists for backward compatibility.
    ///
    /// PDF operator: `F`
    pub fn do_F(&mut self) {
        self.do_f();
    }

    /// Fills the current path using the even-odd rule.
    ///
    /// PDF operator: `f*`
    pub fn do_f_star(&mut self) {
        self.paint_path(false, true, true);
    }

    /// Fills and strokes the current path using the nonzero winding number rule.
    ///
    /// PDF operator: `B`
    pub fn do_B(&mut self) {
        self.paint_path(true, true, false);
    }

    /// Fills and strokes the current path using the even-odd rule.
    ///
    /// PDF operator: `B*`
    pub fn do_B_star(&mut self) {
        self.paint_path(true, true, true);
    }

    /// Closes, fills, and strokes the current path using the nonzero winding number rule.
    ///
    /// Equivalent to: h B
    ///
    /// PDF operator: `b`
    pub fn do_b(&mut self) {
        self.do_h();
        self.do_B();
    }

    /// Closes, fills, and strokes the current path using the even-odd rule.
    ///
    /// Equivalent to: h B*
    ///
    /// PDF operator: `b*`
    pub fn do_b_star(&mut self) {
        self.do_h();
        self.do_B_star();
    }

    /// Ends the path without filling or stroking it.
    ///
    /// This is primarily used in combination with clipping operators.
    ///
    /// PDF operator: `n`
    pub fn do_n(&mut self) {
        self.curpath.clear();
        self.current_point = None;
    }

    // ========================================================================
    // Clipping Path Operators
    // ========================================================================

    /// Sets the clipping path using the nonzero winding number rule.
    ///
    /// The clipping path is intersected with the current clipping path.
    /// Note: The path is not cleared and can still be painted afterward.
    ///
    /// PDF operator: `W`
    pub const fn do_W(&mut self) {
        // TODO: Implement actual clipping path handling
        // For now, this is a no-op that preserves the path
    }

    /// Sets the clipping path using the even-odd rule.
    ///
    /// The clipping path is intersected with the current clipping path.
    /// Note: The path is not cleared and can still be painted afterward.
    ///
    /// PDF operator: `W*`
    pub const fn do_W_star(&mut self) {
        // TODO: Implement actual clipping path handling
        // For now, this is a no-op that preserves the path
    }
}
