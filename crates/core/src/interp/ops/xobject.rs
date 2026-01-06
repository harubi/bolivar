//! XObject and marked content operators.
//!
//! Handles: Do, BI, ID, EI, BMC, BDC, EMC, MP, DP
//!
//! XObjects:
//! - Do: Invoke named XObject (Form or Image)
//!
//! Inline images:
//! - BI: Begin inline image dictionary
//! - ID: Begin inline image data
//! - EI: End inline image
//!
//! Marked content:
//! - BMC: Begin marked content sequence
//! - BDC: Begin marked content with property dict
//! - EMC: End marked content sequence
//! - MP: Marked content point
//! - DP: Marked content point with property dict

use std::collections::HashMap;

use crate::interp::device::{PDFDevice, PDFStackT, PathSegment};
use crate::interp::interpreter::PDFPageInterpreter;
use crate::pdfstate::{PDFGraphicState, PDFTextState};
use crate::pdftypes::PDFObject;
use crate::psparser::PSLiteral;
use crate::utils::{MATRIX_IDENTITY, Matrix, mult_matrix};

/// Saved interpreter state for XObject rendering.
pub(crate) struct InterpreterState {
    pub(crate) gstack: Vec<(Matrix, PDFTextState, PDFGraphicState)>,
    pub(crate) ctm: Matrix,
    pub(crate) textstate: PDFTextState,
    pub(crate) graphicstate: PDFGraphicState,
    pub(crate) curpath: Vec<PathSegment>,
    pub(crate) current_point: Option<(f64, f64)>,
    pub(crate) fontmap: HashMap<String, std::sync::Arc<crate::pdffont::PDFCIDFont>>,
    pub(crate) resources: HashMap<String, PDFObject>,
    pub(crate) xobjmap: HashMap<String, crate::pdftypes::PDFStream>,
}

#[allow(non_snake_case)]
impl<'a, D: PDFDevice> PDFPageInterpreter<'a, D> {
    // ========================================================================
    // XObject Operators
    // ========================================================================

    /// Do - Invoke named XObject (images or form XObjects).
    ///
    /// PDF operator: `Do`
    pub fn do_Do(&mut self, xobjid: String) {
        if std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1") {
            eprintln!("Do XObject: {}", xobjid);
        }
        let xobj = match self.xobjmap.get(&xobjid) {
            Some(xobj) => xobj,
            None => return,
        };

        let subtype = xobj
            .get("Subtype")
            .and_then(|obj| obj.as_name().ok())
            .unwrap_or("");

        if subtype == "Form" && xobj.get("BBox").is_some() {
            if self.xobj_stack.contains(&xobjid) {
                if std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1") {
                    eprintln!("Skip recursive Form XObject: {}", xobjid);
                }
                return;
            }
            if std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1") {
                eprintln!("Do Form XObject: {}", xobjid);
            }
            let bbox = match Self::parse_bbox(xobj.get("BBox")) {
                Some(b) => b,
                None => return,
            };
            let matrix = Self::parse_matrix(xobj.get("Matrix"));

            let resources = xobj
                .get("Resources")
                .and_then(|r| self.resolve_resources(r))
                .unwrap_or_else(|| self.resources.clone());

            let data = if let Some(doc) = self.doc {
                doc.decode_stream(xobj).ok()
            } else {
                Some(xobj.get_data().to_vec())
            };
            if data.is_none() && std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1") {
                eprintln!("Do Form XObject: {} decode failed", xobjid);
            }
            let Some(data) = data else {
                return;
            };
            if std::env::var("BOLIVAR_DEBUG_XOBJ").ok().as_deref() == Some("1") {
                eprintln!("Do Form XObject: {} data len {}", xobjid, data.len());
            }

            let saved = self.snapshot_state();
            self.xobj_stack.push(xobjid.clone());
            self.device.begin_figure(&xobjid, bbox, matrix);

            let form_ctm = mult_matrix(matrix, self.ctm);
            self.render_contents(resources, vec![data], form_ctm);

            self.device.end_figure(&xobjid);
            let _ = self.xobj_stack.pop();
            self.restore_state(saved);
        } else if subtype == "Image" && xobj.get("Width").is_some() && xobj.get("Height").is_some()
        {
            self.device
                .begin_figure(&xobjid, (0.0, 0.0, 1.0, 1.0), MATRIX_IDENTITY);
            self.device.render_image(&xobjid, xobj);
            self.device.end_figure(&xobjid);
        }
    }

    /// Render content streams with specific resources and CTM.
    pub(crate) fn render_contents(
        &mut self,
        resources: HashMap<String, PDFObject>,
        streams: Vec<Vec<u8>>,
        ctm: Matrix,
    ) {
        self.init_resources(&resources, self.doc);
        self.init_state(ctm);
        self.execute(&streams);
    }

    /// Snapshot the current interpreter state.
    pub(crate) fn snapshot_state(&mut self) -> InterpreterState {
        InterpreterState {
            gstack: std::mem::take(&mut self.gstack),
            ctm: self.ctm,
            textstate: self.textstate.clone(),
            graphicstate: self.graphicstate.clone(),
            curpath: std::mem::take(&mut self.curpath),
            current_point: self.current_point,
            fontmap: std::mem::take(&mut self.fontmap),
            resources: std::mem::take(&mut self.resources),
            xobjmap: std::mem::take(&mut self.xobjmap),
        }
    }

    /// Restore the interpreter state from a snapshot.
    pub(crate) fn restore_state(&mut self, state: InterpreterState) {
        self.gstack = state.gstack;
        self.ctm = state.ctm;
        self.device.set_ctm(self.ctm);
        self.textstate = state.textstate;
        self.graphicstate = state.graphicstate;
        self.curpath = state.curpath;
        self.current_point = state.current_point;
        self.fontmap = state.fontmap;
        self.resources = state.resources;
        self.xobjmap = state.xobjmap;
    }

    /// Resolve a resources object to a dictionary.
    pub(crate) fn resolve_resources(&self, obj: &PDFObject) -> Option<HashMap<String, PDFObject>> {
        match obj {
            PDFObject::Dict(d) => Some(d.clone()),
            PDFObject::Ref(r) => {
                if let Some(doc) = self.doc {
                    match doc.resolve_shared(&PDFObject::Ref(r.clone())) {
                        Ok(resolved) => match resolved.as_ref() {
                            PDFObject::Dict(d) => Some(d.clone()),
                            _ => None,
                        },
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Parse a BBox array from a PDFObject.
    pub(crate) fn parse_bbox(obj: Option<&PDFObject>) -> Option<(f64, f64, f64, f64)> {
        let arr = obj?.as_array().ok()?;
        if arr.len() < 4 {
            return None;
        }
        let x0 = arr[0].as_num().ok()?;
        let y0 = arr[1].as_num().ok()?;
        let x1 = arr[2].as_num().ok()?;
        let y1 = arr[3].as_num().ok()?;
        Some((x0, y0, x1, y1))
    }

    /// Parse a transformation matrix from a PDFObject.
    pub(crate) fn parse_matrix(obj: Option<&PDFObject>) -> Matrix {
        let arr = match obj.and_then(|o| o.as_array().ok()) {
            Some(arr) if arr.len() >= 6 => arr,
            _ => return MATRIX_IDENTITY,
        };
        let a = arr[0].as_num().ok();
        let b = arr[1].as_num().ok();
        let c = arr[2].as_num().ok();
        let d = arr[3].as_num().ok();
        let e = arr[4].as_num().ok();
        let f = arr[5].as_num().ok();
        match (a, b, c, d, e, f) {
            (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) => (a, b, c, d, e, f),
            _ => MATRIX_IDENTITY,
        }
    }

    // ========================================================================
    // Marked Content Operators
    // ========================================================================

    /// BMC - Begin Marked Content
    ///
    /// PDF operator: `BMC`
    pub fn do_BMC(&mut self, tag: &PSLiteral) {
        self.device.begin_tag(tag, None);
    }

    /// BDC - Begin Marked Content with property dictionary
    ///
    /// PDF operator: `BDC`
    pub fn do_BDC(&mut self, tag: &PSLiteral, props: &PDFStackT) {
        self.device.begin_tag(tag, Some(props));
    }

    /// EMC - End Marked Content
    ///
    /// PDF operator: `EMC`
    pub fn do_EMC(&mut self) {
        self.device.end_tag();
    }
}
