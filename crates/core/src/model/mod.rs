//! PDF model types - objects, state, and color definitions.
//!
//! This module contains the core PDF data model types:
//! - `objects` - PDF object types (PDFObject, PDFStream, PDFObjRef)
//! - `state` - Graphics and text state (PDFGraphicState, PDFTextState, Color)
//! - `color` - Color space definitions (PDFColorSpace)

pub mod color;
pub mod objects;
pub mod state;

// Re-export main types for convenience
pub use color::PDFColorSpace;
pub use objects::{PDFObjRef, PDFObject, PDFStream};
pub use state::{Color, PDFGraphicState, PDFTextState};
