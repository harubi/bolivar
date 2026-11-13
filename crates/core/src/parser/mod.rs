//! PDF and PostScript parsing modules.
//!
//! - `lexer`: PostScript tokenizer (ported from psparser.py)
//! - `parser`: PDF object parser (ported from pdfparser.py)
//! - `recovery`: pdfminer.six's lenient stack reader, for damaged objects

pub mod lexer;
pub mod pdf_parser;
pub mod recovery;

// Re-export main types for convenience
pub use lexer::{Keyword, PSBaseParser, PSStackParser, PSToken};
pub use pdf_parser::PDFParser;
