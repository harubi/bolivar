//! Graphics state operators.
//!
//! Handles: q, Q, cm, w, J, j, M, d, ri, i, gs
//!
//! These operators manage the graphics state stack and transformation matrix.
//! - q/Q: Push/pop graphics state
//! - cm: Concatenate transformation matrix
//! - w, J, j, M, d: Line styling (width, cap, join, miter limit, dash)
//! - ri, i: Rendering intent and flatness
//! - gs: Set parameters from graphics state dictionary

use crate::interp::device::PDFDevice;
use crate::interp::interpreter::PDFPageInterpreter;
use crate::utils::mult_matrix;

#[allow(non_snake_case)]
impl<'a, D: PDFDevice> PDFPageInterpreter<'a, D> {
    /// Saves the current graphics state to the stack.
    ///
    /// PDF operator: `q`
    pub fn do_q(&mut self) {
        self.gstack.push(self.get_current_state());
    }

    /// Restores the graphics state from the stack.
    ///
    /// PDF operator: `Q`
    pub fn do_Q(&mut self) {
        if let Some(state) = self.gstack.pop() {
            self.set_current_state(state);
        }
    }

    /// Concatenates a matrix to the current transformation matrix.
    ///
    /// PDF operator: `cm`
    pub fn do_cm(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        let matrix = (a, b, c, d, e, f);
        self.ctm = mult_matrix(matrix, self.ctm);
        self.device.set_ctm(self.ctm);
    }

    /// Sets the line width in the graphics state.
    ///
    /// PDF operator: `w`
    pub fn do_w(&mut self, linewidth: f64) {
        let scale = self.ctm.0.hypot(self.ctm.1);
        self.graphicstate.linewidth = linewidth * scale;
    }

    /// Sets the line cap style in the graphics state.
    ///
    /// PDF operator: `J`
    pub const fn do_J(&mut self, linecap: i32) {
        self.graphicstate.linecap = Some(linecap);
    }

    /// Sets the line join style in the graphics state.
    ///
    /// PDF operator: `j`
    pub const fn do_j(&mut self, linejoin: i32) {
        self.graphicstate.linejoin = Some(linejoin);
    }

    /// Sets the miter limit in the graphics state.
    ///
    /// PDF operator: `M`
    pub const fn do_M(&mut self, miterlimit: f64) {
        self.graphicstate.miterlimit = Some(miterlimit);
    }

    /// Sets the line dash pattern in the graphics state.
    ///
    /// PDF operator: `d`
    pub fn do_d(&mut self, dash_array: Vec<f64>, phase: f64) {
        self.graphicstate.dash = Some((dash_array, phase));
    }

    /// Sets the color rendering intent in the graphics state.
    ///
    /// PDF operator: `ri`
    pub fn do_ri(&mut self, intent: &str) {
        self.graphicstate.intent = Some(intent.to_string());
    }

    /// Sets the flatness tolerance in the graphics state.
    ///
    /// PDF operator: `i`
    pub const fn do_i(&mut self, flatness: f64) {
        self.graphicstate.flatness = Some(flatness);
    }

    /// Sets parameters from a graphics state parameter dictionary.
    ///
    /// PDF operator: `gs`
    ///
    /// TODO: Implement ExtGState parameter lookup from resources.
    pub const fn do_gs(&mut self, _name: &str) {
        // TODO: Look up ExtGState from resources and apply parameters
    }
}
