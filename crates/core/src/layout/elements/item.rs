//! LTItem - enum to represent any layout object.

use crate::utils::HasBBox;

use super::character::{LTAnno, LTChar};
use super::container::{LTFigure, LTPage};
use super::graphics::{LTCurve, LTImage, LTLine, LTRect};
use super::textbox::TextBoxType;
use super::textline::TextLineType;

/// Macro to dispatch a HasBBox method call to the appropriate enum variant.
/// Special-cases Anno which has no bounding box (returns 0.0).
macro_rules! dispatch_hasbbox {
    ($self:ident, $method:ident) => {
        match $self {
            LTItem::Char(c) => c.$method(),
            LTItem::Anno(_) => 0.0,
            LTItem::Curve(c) => c.$method(),
            LTItem::Line(l) => l.$method(),
            LTItem::Rect(r) => r.$method(),
            LTItem::Image(i) => i.$method(),
            LTItem::TextLine(l) => l.$method(),
            LTItem::TextBox(b) => b.$method(),
            LTItem::Figure(f) => f.$method(),
            LTItem::Page(p) => p.$method(),
        }
    };
}

/// Represents any item that can appear in a layout container.
#[derive(Debug, Clone)]
pub enum LTItem {
    Char(LTChar),
    Anno(LTAnno),
    Curve(LTCurve),
    Line(LTLine),
    Rect(LTRect),
    Image(LTImage),
    TextLine(TextLineType),
    TextBox(TextBoxType),
    Figure(Box<LTFigure>),
    Page(Box<LTPage>),
}

impl LTItem {
    pub fn is_char(&self) -> bool {
        matches!(self, LTItem::Char(_))
    }
}

impl HasBBox for LTItem {
    fn x0(&self) -> f64 {
        dispatch_hasbbox!(self, x0)
    }
    fn y0(&self) -> f64 {
        dispatch_hasbbox!(self, y0)
    }
    fn x1(&self) -> f64 {
        dispatch_hasbbox!(self, x1)
    }
    fn y1(&self) -> f64 {
        dispatch_hasbbox!(self, y1)
    }
}
