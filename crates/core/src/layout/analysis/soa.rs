//! Struct-of-Arrays storage for layout analysis.
//!
//! Two distinct projections live here, each with its own iteration pattern:
//!
//! - [`RectSoA`] — SIMD rectangle indexing. Stores 4 columns of f64 in
//!   `LANES`-wide chunks, optimised for vectorised overlap queries against
//!   already-formed bounding boxes (used by `clustering.rs`).
//! - [`LayoutSoA`] — Layout element column store. Holds 12 scalar columns
//!   (x0/x1/top/bottom/w/h/cx/cy/text/font/size/flags) for `LTChar`, so
//!   `grouping.rs` can run sequential character-pair alignment without
//!   materialising intermediate structs.
//!
//! They share a name but solve different problems. Kept co-located because
//! both are SoA, both feed `analysis/`, and splitting them once cost a
//! confusing `soa.rs` vs `soa_layout.rs` pair.

use crate::layout::types::LTChar;
use crate::simd::F64_LANES as LANES;
use crate::utils::{HasBBox, Rect};
use std::simd::prelude::*;

// =====================================================================
// SIMD rectangle indexing (RectSoA)
// =====================================================================

pub struct RectSoA {
    pub x0: Vec<[f64; LANES]>,
    pub y0: Vec<[f64; LANES]>,
    pub x1: Vec<[f64; LANES]>,
    pub y1: Vec<[f64; LANES]>,
    len: usize,
}

impl RectSoA {
    pub fn from_bboxes(bboxes: &[Rect]) -> Self {
        let len = bboxes.len();
        let chunks = len.div_ceil(LANES);
        let mut x0 = Vec::with_capacity(chunks);
        let mut y0 = Vec::with_capacity(chunks);
        let mut x1 = Vec::with_capacity(chunks);
        let mut y1 = Vec::with_capacity(chunks);
        let mut idx = 0;
        for _ in 0..chunks {
            let mut cx0 = [0.0; LANES];
            let mut cy0 = [0.0; LANES];
            let mut cx1 = [0.0; LANES];
            let mut cy1 = [0.0; LANES];
            for lane in 0..LANES {
                if idx >= len {
                    break;
                }
                let (bx0, by0, bx1, by1) = bboxes[idx];
                cx0[lane] = bx0;
                cy0[lane] = by0;
                cx1[lane] = -bx1;
                cy1[lane] = -by1;
                idx += 1;
            }
            x0.push(cx0);
            y0.push(cy0);
            x1.push(cx1);
            y1.push(cy1);
        }
        Self {
            x0,
            y0,
            x1,
            y1,
            len,
        }
    }

    pub fn overlap_simd(&self, q: Rect) -> Vec<usize> {
        let (qx0, qy0, qx1, qy1) = q;
        let mut out = Vec::new();
        let qx1v = Simd::<f64, LANES>::splat(qx1);
        let qy1v = Simd::<f64, LANES>::splat(qy1);
        let qnx0v = Simd::<f64, LANES>::splat(-qx0);
        let qny0v = Simd::<f64, LANES>::splat(-qy0);
        let mut idx = 0;
        for chunk in 0..self.x0.len() {
            let x0 = Simd::from_array(self.x0[chunk]);
            let y0 = Simd::from_array(self.y0[chunk]);
            let nx1 = Simd::from_array(self.x1[chunk]);
            let ny1 = Simd::from_array(self.y1[chunk]);
            let mask =
                x0.simd_lt(qx1v) & nx1.simd_lt(qnx0v) & y0.simd_lt(qy1v) & ny1.simd_lt(qny0v);
            let lanes = mask.to_array();
            for &hit in lanes.iter() {
                if idx >= self.len {
                    return out;
                }
                if hit {
                    out.push(idx);
                }
                idx += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soa_storage_preserves_bboxes() {
        let bboxes = vec![(0.0, 1.0, 2.0, 3.0), (-1.0, 0.0, 5.0, 7.0)];
        let soa = RectSoA::from_bboxes(&bboxes);
        assert_eq!(soa.x0.len(), 1);
        assert_eq!(soa.x0[0][0], 0.0);
        assert_eq!(soa.y0[0][0], 1.0);
        assert_eq!(soa.x1[0][0], -2.0);
        assert_eq!(soa.y1[0][0], -3.0);
        assert_eq!(soa.x0[0][1], -1.0);
        assert_eq!(soa.y0[0][1], 0.0);
        assert_eq!(soa.x1[0][1], -5.0);
        assert_eq!(soa.y1[0][1], -7.0);
    }

    #[test]
    fn soa_chunked_storage_rounds_up() {
        let bboxes = vec![
            (0.0, 0.0, 1.0, 1.0),
            (1.0, 0.0, 2.0, 1.0),
            (2.0, 0.0, 3.0, 1.0),
            (3.0, 0.0, 4.0, 1.0),
            (4.0, 0.0, 5.0, 1.0),
        ];
        let soa = RectSoA::from_bboxes(&bboxes);
        let expected_chunks = 5_usize.div_ceil(LANES);
        assert_eq!(soa.x0.len(), expected_chunks);
    }
}

#[cfg(test)]
mod overlap_tests {
    use super::*;

    #[test]
    fn simd_overlap_expected_indices() {
        let bboxes = vec![
            (0.0, 0.0, 2.0, 2.0),
            (3.0, 0.0, 5.0, 2.0),
            (1.0, 1.0, 4.0, 4.0),
            (-1.0, -1.0, 0.5, 0.5),
        ];
        let soa = RectSoA::from_bboxes(&bboxes);
        let q = (0.0, 0.0, 3.0, 3.0);
        let simd = soa.overlap_simd(q);
        assert_eq!(simd, vec![0, 2, 3]);
    }
}

// =====================================================================
// Layout element column store (LayoutSoA)
// =====================================================================

pub struct LayoutSoA {
    pub x0: Vec<f64>,
    pub x1: Vec<f64>,
    pub top: Vec<f64>,
    pub bottom: Vec<f64>,
    pub w: Vec<f64>,
    pub h: Vec<f64>,
    pub cx: Vec<f64>,
    pub cy: Vec<f64>,
    pub text: Vec<String>,
    pub font: Vec<String>,
    pub size: Vec<f64>,
    pub flags: Vec<u32>,
}

const FLAG_UPRIGHT: u32 = 1;

impl LayoutSoA {
    pub fn from_chars(chars: &[LTChar]) -> Self {
        let mut soa = Self {
            x0: Vec::with_capacity(chars.len()),
            x1: Vec::with_capacity(chars.len()),
            top: Vec::with_capacity(chars.len()),
            bottom: Vec::with_capacity(chars.len()),
            w: Vec::with_capacity(chars.len()),
            h: Vec::with_capacity(chars.len()),
            cx: Vec::with_capacity(chars.len()),
            cy: Vec::with_capacity(chars.len()),
            text: Vec::with_capacity(chars.len()),
            font: Vec::with_capacity(chars.len()),
            size: Vec::with_capacity(chars.len()),
            flags: Vec::with_capacity(chars.len()),
        };

        for ch in chars {
            let x0 = ch.x0();
            let x1 = ch.x1();
            let top = ch.y0();
            let bottom = ch.y1();
            soa.x0.push(x0);
            soa.x1.push(x1);
            soa.top.push(top);
            soa.bottom.push(bottom);
            soa.w.push(x1 - x0);
            soa.h.push(bottom - top);
            soa.cx.push((x0 + x1) * 0.5);
            soa.cy.push((top + bottom) * 0.5);
            soa.text.push(ch.get_text().to_string());
            soa.font.push(ch.fontname().to_string());
            soa.size.push(ch.size());
            soa.flags.push(if ch.upright() { FLAG_UPRIGHT } else { 0 });
        }

        soa
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }
}

#[cfg(test)]
mod layout_soa_tests {
    use super::LayoutSoA;
    use crate::layout::types::LTChar;

    #[test]
    fn layout_soa_from_chars_preserves_order() {
        let chars = vec![
            LTChar::builder((0.0, 0.0, 1.0, 1.0), "A", "F", 10.0).build(),
            LTChar::builder((1.0, 0.0, 2.0, 1.0), "B", "F", 10.0).build(),
        ];
        let soa = LayoutSoA::from_chars(&chars);
        assert_eq!(soa.len(), 2);
        assert_eq!(soa.text[0], "A");
        assert_eq!(soa.text[1], "B");
    }

    #[test]
    fn layout_soa_precomputes_metrics() {
        let chars = vec![LTChar::builder((0.0, 0.0, 4.0, 2.0), "A", "F", 10.0).build()];
        let soa = LayoutSoA::from_chars(&chars);
        assert_eq!(soa.w[0], 4.0);
        assert_eq!(soa.h[0], 2.0);
        assert_eq!(soa.cx[0], 2.0);
        assert_eq!(soa.cy[0], 1.0);
    }
}
