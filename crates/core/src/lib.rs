#![feature(portable_simd)]
//! bolivar - A Rust port of pdfminer.six for PDF text extraction.

pub mod api;
pub mod arena;
pub mod codec;
pub mod converter;
pub mod document;
pub mod error;
pub mod font;

// Re-export high_level for backwards compatibility
pub use api::high_level;
pub mod image;
pub mod interp;
pub mod layout;
pub mod model;
pub mod parser;
pub mod simd;
pub mod utils;

// Re-export table module for backwards compatibility
pub use layout::table;

// Re-export codec modules for backwards compatibility
pub use codec::aes;
pub use codec::arcfour;
pub use codec::ascii85;
pub use codec::ccitt;
pub use codec::jbig2;
pub use codec::lzw;
pub use codec::runlength;

// Re-export utils modules for backwards compatibility
pub use utils::casting;
pub use utils::data_structures;

// Re-export parser modules for backwards compatibility
pub use parser::lexer as psparser;
pub use parser::parser as pdfparser;

// Re-export model modules for backwards compatibility
pub use model::color as pdfcolor;
pub use model::objects as pdftypes;
pub use model::state as pdfstate;

// Re-export font modules for backwards compatibility
pub use font::cmap as cmapdb;
pub use font::encoding as encodingdb;
pub use font::latin_enc;
pub use font::metrics as fontmetrics;
pub use font::pdffont;
pub use font::truetype;

// Re-export document modules for backwards compatibility
pub use document::catalog as pdfdocument;
pub use document::page as pdfpage;
pub use document::saslprep;
pub use document::security;

// Re-export interp modules for backwards compatibility
pub use interp::device as pdfdevice;
pub use interp::interpreter as pdfinterp;

pub use error::{PdfError, Result};
