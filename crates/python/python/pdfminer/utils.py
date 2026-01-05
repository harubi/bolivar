# pdfminer.utils compatibility shim
#
# Utility functions for PDF processing (Rust-backed).

from typing import Tuple

from bolivar._bolivar import INF, MATRIX_IDENTITY, PDFDocEncoding, decode_text, isnumber

# Type aliases
Point = Tuple[float, float]
Rect = Tuple[float, float, float, float]
Matrix = Tuple[float, float, float, float, float, float]

__all__ = [
    "INF",
    "MATRIX_IDENTITY",
    "Matrix",
    "PDFDocEncoding",
    "Point",
    "Rect",
    "decode_text",
    "isnumber",
]
