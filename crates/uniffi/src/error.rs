use bolivar_core::PdfError;
use std::io::ErrorKind;

#[derive(Debug, thiserror::Error)]
pub enum BolivarError {
    #[error("path is invalid")]
    InvalidPath,
    #[error("invalid argument")]
    InvalidArgument,
    #[error("io not found")]
    IoNotFound,
    #[error("io permission denied")]
    IoPermissionDenied,
    #[error("io error")]
    IoError,
    #[error("syntax error")]
    SyntaxError,
    #[error("encryption error")]
    EncryptionError,
    #[error("pdf error")]
    PdfError,
    #[error("decode error")]
    DecodeError,
    #[error("runtime error")]
    RuntimeError,
}

pub(crate) fn map_io_error_kind(kind: ErrorKind) -> BolivarError {
    match kind {
        ErrorKind::NotFound => BolivarError::IoNotFound,
        ErrorKind::PermissionDenied => BolivarError::IoPermissionDenied,
        _ => BolivarError::IoError,
    }
}

impl From<PdfError> for BolivarError {
    fn from(value: PdfError) -> Self {
        match value {
            PdfError::Io(err) => map_io_error_kind(err.kind()),
            PdfError::DecodeError(_) => Self::DecodeError,
            PdfError::SyntaxError(_) => Self::SyntaxError,
            PdfError::InvalidArgument(_) => Self::InvalidArgument,
            PdfError::EncryptionError(_) => Self::EncryptionError,
            _ => Self::PdfError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_pdf_error_kinds_without_collapsing_all_to_pdf_error() {
        assert!(matches!(
            BolivarError::from(PdfError::SyntaxError("bad".to_string())),
            BolivarError::SyntaxError
        ));
        assert!(matches!(
            BolivarError::from(PdfError::InvalidArgument("bad".to_string())),
            BolivarError::InvalidArgument
        ));
        assert!(matches!(
            BolivarError::from(PdfError::EncryptionError("bad".to_string())),
            BolivarError::EncryptionError
        ));
    }
}
