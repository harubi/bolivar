use crate::utils::Rect;
use std::simd::prelude::*;

const LANES: usize = 4;

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
        let chunks = (len + LANES - 1) / LANES;
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
                cx1[lane] = bx1;
                cy1[lane] = by1;
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
        let qx0v = Simd::<f64, LANES>::splat(qx0);
        let qy0v = Simd::<f64, LANES>::splat(qy0);
        let qx1v = Simd::<f64, LANES>::splat(qx1);
        let qy1v = Simd::<f64, LANES>::splat(qy1);
        let mut idx = 0;
        for chunk in 0..self.x0.len() {
            let x0 = Simd::from_array(self.x0[chunk]);
            let y0 = Simd::from_array(self.y0[chunk]);
            let x1 = Simd::from_array(self.x1[chunk]);
            let y1 = Simd::from_array(self.y1[chunk]);
            let mask = x0.simd_lt(qx1v) & x1.simd_gt(qx0v) & y0.simd_lt(qy1v) & y1.simd_gt(qy0v);
            let lanes = mask.to_array();
            for lane in 0..LANES {
                if idx >= self.len {
                    return out;
                }
                if lanes[lane] {
                    out.push(idx);
                }
                idx += 1;
            }
        }
        out
    }
}

#[cfg(test)]
impl RectSoA {
    pub fn to_bboxes(&self) -> Vec<Rect> {
        let mut out = Vec::with_capacity(self.len);
        let mut idx = 0;
        for chunk in 0..self.x0.len() {
            for lane in 0..LANES {
                if idx >= self.len {
                    return out;
                }
                out.push((
                    self.x0[chunk][lane],
                    self.y0[chunk][lane],
                    self.x1[chunk][lane],
                    self.y1[chunk][lane],
                ));
                idx += 1;
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn overlap_scalar(&self, q: Rect) -> Vec<usize> {
        let (qx0, qy0, qx1, qy1) = q;
        let mut out = Vec::new();
        let mut idx = 0;
        for chunk in 0..self.x0.len() {
            for lane in 0..LANES {
                if idx >= self.len {
                    return out;
                }
                if self.x0[chunk][lane] < qx1
                    && self.x1[chunk][lane] > qx0
                    && self.y0[chunk][lane] < qy1
                    && self.y1[chunk][lane] > qy0
                {
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
    fn soa_roundtrip_preserves_bboxes() {
        let bboxes = vec![(0.0, 1.0, 2.0, 3.0), (-1.0, 0.0, 5.0, 7.0)];
        let soa = RectSoA::from_bboxes(&bboxes);
        let roundtrip = soa.to_bboxes();
        assert_eq!(roundtrip, bboxes);
        assert_eq!(soa.len(), bboxes.len());
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
        assert_eq!(soa.x0.len(), 2);
    }
}

#[cfg(test)]
mod overlap_tests {
    use super::*;

    #[test]
    fn simd_overlap_matches_scalar() {
        let bboxes = vec![
            (0.0, 0.0, 2.0, 2.0),
            (3.0, 0.0, 5.0, 2.0),
            (1.0, 1.0, 4.0, 4.0),
            (-1.0, -1.0, 0.5, 0.5),
        ];
        let soa = RectSoA::from_bboxes(&bboxes);
        let q = (0.0, 0.0, 3.0, 3.0);
        let scalar = soa.overlap_scalar(q);
        let simd = soa.overlap_simd(q);
        assert_eq!(scalar, simd);
    }
}
