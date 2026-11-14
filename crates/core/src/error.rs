//! Error types for bolivar PDF parsing library.

use thiserror::Error;

/// Primary error type for PDF parsing operations.
#[derive(Error, Debug)]
pub enum PdfError {
    #[error("operation cancelled")]
    Cancelled,

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

    #[error("runtime error: {0}")]
    RuntimeError(String),

    #[error("{0}")]
    NotImplemented(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// A setting that failed validation, naming the setting.
    #[error("invalid argument: {0}")]
    InvalidSetting(#[from] InvalidSetting),

    #[error("SASLprep: {0}")]
    SaslPrepError(String),

    #[error("CMap not found: {0}")]
    CMapNotFound(String),

    #[error("named destination not found: {0}")]
    DestinationNotFound(String),

    #[error("encryption error: {0}")]
    EncryptionError(String),
}

/// A setting that failed validation: the field, and why.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct InvalidSetting {
    pub field: &'static str,
    pub message: String,
}

impl InvalidSetting {
    pub fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

/// A stable category for a [`PdfError`], for bindings that map failures to
/// their own types. Every error has exactly one kind; adding a `PdfError`
/// variant does not compile until it is given one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// A file or named resource does not exist.
    NotFound,
    PermissionDenied,
    /// Any other input or output failure.
    Io,
    /// The PDF syntax is invalid.
    Syntax,
    /// A stream, font or character mapping could not be decoded.
    Decode,
    /// The document is encrypted and the password is missing or wrong.
    Encryption,
    /// A caller-supplied value is invalid.
    InvalidArgument,
    /// The document uses a feature that is not implemented.
    Unsupported,
    /// The caller cancelled the operation.
    Cancelled,
    /// An internal failure that is neither the input's nor the caller's fault.
    Runtime,
    /// The document is damaged beyond recovery.
    Malformed,
}

impl PdfError {
    /// This error's category.
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Io(error) => match error.kind() {
                std::io::ErrorKind::NotFound => ErrorKind::NotFound,
                std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
                _ => ErrorKind::Io,
            },
            Self::Cancelled => ErrorKind::Cancelled,
            Self::SyntaxError(_) => ErrorKind::Syntax,
            Self::DecodeError(_) => ErrorKind::Decode,
            Self::EncryptionError(_) | Self::SaslPrepError(_) => ErrorKind::Encryption,
            Self::InvalidArgument(_) | Self::InvalidSetting(_) => ErrorKind::InvalidArgument,
            Self::NotImplemented(_) => ErrorKind::Unsupported,
            Self::RuntimeError(_) | Self::NotInitialized => ErrorKind::Runtime,
            Self::TokenError { .. }
            | Self::UnexpectedEof
            | Self::UnknownGlyph(_)
            | Self::InvalidUnicode(_)
            | Self::TypeError { .. }
            | Self::KeyError(_)
            | Self::ObjectNotFound(_)
            | Self::NoValidXRef
            | Self::NoPageLabels
            | Self::CMapNotFound(_)
            | Self::DestinationNotFound(_) => ErrorKind::Malformed,
        }
    }

    /// The setting at fault, for an invalid setting.
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::InvalidSetting(setting) => Some(setting.field),
            _ => None,
        }
    }
}

/// Convenience Result type alias for PdfError.
pub type Result<T> = std::result::Result<T, PdfError>;
