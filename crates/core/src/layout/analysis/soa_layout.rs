use crate::layout::types::LTChar;
use crate::utils::HasBBox;

pub struct LayoutSoA {
    pub x0: Vec<f64>,
    pub x1: Vec<f64>,
    pub top: Vec<f64>,
    pub bottom: Vec<f64>,
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
            text: Vec::with_capacity(chars.len()),
            font: Vec::with_capacity(chars.len()),
            size: Vec::with_capacity(chars.len()),
            flags: Vec::with_capacity(chars.len()),
        };

        for ch in chars {
            soa.x0.push(ch.x0());
            soa.x1.push(ch.x1());
            soa.top.push(ch.y0());
            soa.bottom.push(ch.y1());
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
}
