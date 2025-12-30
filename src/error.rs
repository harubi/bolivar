//! Error types for bolivar PDF parsing library.

use thiserror::Error;

/// Primary error type for PDF parsing operations.
#[derive(Error, Debug)]
pub enum PdfError {
    #[error("invalid token at position {pos}: {msg}")]
    TokenError { pos: usize, msg: String },

    #[error("unexpected end of input")]
    UnexpectedEof,

    #[error("unknown glyph name: {0}")]
    UnknownGlyph(String),

    #[error("invalid unicode codepoint: {0:#x}")]
    InvalidUnicode(u32),

    #[error("type error: expected {expected}, got {got}")]
    TypeError {
        expected: &'static str,
        got: &'static str,
    },

    #[error("key not found: {0}")]
    KeyError(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("PDF object not found: {0}")]
    ObjectNotFound(u32),

    #[error("no valid xref table found")]
    NoValidXRef,

    #[error("PDF syntax error: {0}")]
    SyntaxError(String),

    #[error("document has no page labels")]
    NoPageLabels,

    #[error("PDF document not initialized")]
    NotInitialized,

    #[error("decode error: {0}")]
    DecodeError(String),

    #[error("SASLprep: {0}")]
    SaslPrepError(String),

    #[error("CMap not found: {0}")]
    CMapNotFound(String),

    #[error("named destination not found: {0}")]
    DestinationNotFound(String),

    #[error("encryption error: {0}")]
    EncryptionError(String),
}

/// Convenience Result type alias for PdfError.
pub type Result<T> = std::result::Result<T, PdfError>;
