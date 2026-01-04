//! Base component with bounding box.

use std::hash::Hash;

use crate::utils::{HasBBox, Rect};

/// Base component with a bounding box.
#[derive(Debug, Clone, PartialEq)]
pub struct LTComponent {
    pub(crate) x0: f64,
    pub(crate) y0: f64,
    pub(crate) x1: f64,
    pub(crate) y1: f64,
}

impl LTComponent {
    pub fn new(bbox: Rect) -> Self {
        let (x0, y0, x1, y1) = bbox;
        Self { x0, y0, x1, y1 }
    }

    pub fn set_bbox(&mut self, bbox: Rect) {
        let (x0, y0, x1, y1) = bbox;
        self.x0 = x0;
        self.y0 = y0;
        self.x1 = x1;
        self.y1 = y1;
    }

    pub fn x0(&self) -> f64 {
        self.x0
    }

    pub fn y0(&self) -> f64 {
        self.y0
    }

    pub fn x1(&self) -> f64 {
        self.x1
    }

    pub fn y1(&self) -> f64 {
        self.y1
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn bbox(&self) -> Rect {
        (self.x0, self.y0, self.x1, self.y1)
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    /// Returns true if there is horizontal overlap with another component.
    pub fn is_hoverlap(&self, other: &LTComponent) -> bool {
        other.x0 <= self.x1 && self.x0 <= other.x1
    }

    /// Returns the horizontal distance to another component.
    /// Returns 0 if they overlap.
    pub fn hdistance(&self, other: &LTComponent) -> f64 {
        if self.is_hoverlap(other) {
            0.0
        } else {
            (self.x0 - other.x1).abs().min((self.x1 - other.x0).abs())
        }
    }

    /// Returns the amount of horizontal overlap with another component.
    pub fn hoverlap(&self, other: &LTComponent) -> f64 {
        if self.is_hoverlap(other) {
            (self.x0 - other.x1).abs().min((self.x1 - other.x0).abs())
        } else {
            0.0
        }
    }

    /// Returns true if there is vertical overlap with another component.
    pub fn is_voverlap(&self, other: &LTComponent) -> bool {
        other.y0 <= self.y1 && self.y0 <= other.y1
    }

    /// Returns the vertical distance to another component.
    /// Returns 0 if they overlap.
    pub fn vdistance(&self, other: &LTComponent) -> f64 {
        if self.is_voverlap(other) {
            0.0
        } else {
            (self.y0 - other.y1).abs().min((self.y1 - other.y0).abs())
        }
    }

    /// Returns the amount of vertical overlap with another component.
    pub fn voverlap(&self, other: &LTComponent) -> f64 {
        if self.is_voverlap(other) {
            (self.y0 - other.y1).abs().min((self.y1 - other.y0).abs())
        } else {
            0.0
        }
    }
}

impl HasBBox for LTComponent {
    fn x0(&self) -> f64 {
        self.x0
    }
    fn y0(&self) -> f64 {
        self.y0
    }
    fn x1(&self) -> f64 {
        self.x1
    }
    fn y1(&self) -> f64 {
        self.y1
    }
}

impl Eq for LTComponent {}

impl Hash for LTComponent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x0.to_bits().hash(state);
        self.y0.to_bits().hash(state);
        self.x1.to_bits().hash(state);
        self.y1.to_bits().hash(state);
    }
}
