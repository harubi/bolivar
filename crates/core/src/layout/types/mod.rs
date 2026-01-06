//! Layout element types for PDF text extraction.
//!
//! Contains all LT* struct definitions:
//! - LTComponent: Base type for objects with bounding boxes
//! - LTAnno: Virtual characters (spaces, newlines) inserted during analysis
//! - LTChar: Actual characters with bounding boxes
//! - LTTextLine: A line of text (horizontal or vertical)
//! - LTTextBox: A group of text lines
//! - LTTextGroup: Hierarchical grouping of text boxes
//! - LTCurve, LTLine, LTRect: Graphical elements
//! - LTImage: Image container
//! - LTFigure: Figure container (embedded PDF forms)
//! - LTPage: Page container
//! - LTLayoutContainer: Container that performs layout analysis
//! - LTItem: Enum to represent any layout object

/// Implements HasBBox trait by delegating to a field.
///
/// # Field access mode
/// Use when the field has direct `.x0`, `.y0`, `.x1`, `.y1` fields:
/// ```ignore
/// impl_has_bbox_delegate!(LTCurve, component);
/// // expands to: self.component.x0
/// ```
///
/// # Method call mode
/// Use when the field has `.x0()`, `.y0()`, `.x1()`, `.y1()` methods:
/// ```ignore
/// impl_has_bbox_delegate!(LTLine, curve, method);
/// // expands to: self.curve.x0()
/// ```
///
/// # Tuple index mode
/// Use for newtype tuple structs that delegate through methods:
/// ```ignore
/// impl_has_bbox_delegate!(LTTextGroupLRTB, tuple_method);
/// // expands to: self.0.x0()
/// ```
macro_rules! impl_has_bbox_delegate {
    // Tuple index mode (for newtype tuple structs like Wrapper(Inner))
    // NOTE: Must come BEFORE $field:ident patterns to avoid matching as identifier
    ($type:ty, tuple_method) => {
        impl crate::utils::HasBBox for $type {
            fn x0(&self) -> f64 {
                self.0.x0()
            }
            fn y0(&self) -> f64 {
                self.0.y0()
            }
            fn x1(&self) -> f64 {
                self.0.x1()
            }
            fn y1(&self) -> f64 {
                self.0.y1()
            }
        }
    };
    // Method call mode (for types that delegate through trait methods)
    ($type:ty, $field:ident, method) => {
        impl crate::utils::HasBBox for $type {
            fn x0(&self) -> f64 {
                self.$field.x0()
            }
            fn y0(&self) -> f64 {
                self.$field.y0()
            }
            fn x1(&self) -> f64 {
                self.$field.x1()
            }
            fn y1(&self) -> f64 {
                self.$field.y1()
            }
        }
    };
    // Field access mode (for types with direct field access like component.x0)
    ($type:ty, $field:ident) => {
        impl crate::utils::HasBBox for $type {
            fn x0(&self) -> f64 {
                self.$field.x0
            }
            fn y0(&self) -> f64 {
                self.$field.y0
            }
            fn x1(&self) -> f64 {
                self.$field.x1
            }
            fn y1(&self) -> f64 {
                self.$field.y1
            }
        }
    };
}

// Export the macro for use in submodules
// Note: This appears unused but is needed for macro invocations in sibling modules
#[allow(unused_imports)]
pub(crate) use impl_has_bbox_delegate;

mod character;
mod component;
mod container;
mod graphics;
mod item;
mod textbox;
mod textline;

// Re-export all public types
pub use character::{Color, LTAnno, LTChar, LTCharBuilder};
pub use component::LTComponent;
pub use container::{LTFigure, LTLayoutContainer, LTPage};
pub use graphics::{LTCurve, LTImage, LTLine, LTRect};
pub use item::LTItem;
pub use textbox::{
    IndexAssigner, LTTextBox, LTTextBoxHorizontal, LTTextBoxVertical, LTTextGroup, LTTextGroupLRTB,
    LTTextGroupTBRL, TextBoxType, TextGroupElement,
};
pub use textline::{
    Axis, LTTextLine, LTTextLineHorizontal, LTTextLineVertical, TextLineElement, TextLineType,
};
