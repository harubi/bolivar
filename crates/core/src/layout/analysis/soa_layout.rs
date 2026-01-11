use crate::layout::types::LTChar;
use crate::utils::HasBBox;

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
mod tests {
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
