# pdfminer.utils compatibility shim
#
# Utility functions for PDF processing (Rust-backed where available).

import io
import pathlib
from typing import Any, BinaryIO, TextIO, Tuple, Union

from bolivar._bolivar import (
    INF,
    MATRIX_IDENTITY,
    PDFDocEncoding,
    Plane,
    apply_matrix_pt,
    apply_matrix_rect,
    decode_text,
    format_int_alpha,
    format_int_roman,
    isnumber,
    mult_matrix,
    shorten_str,
    translate_matrix,
    unpad_aes,
)

from .pdfexceptions import PDFTypeError

# Type aliases
Point = Tuple[float, float]
Rect = Tuple[float, float, float, float]
Matrix = Tuple[float, float, float, float, float, float]

FileOrName = Union[pathlib.PurePath, str, io.IOBase]
AnyIO = Union[TextIO, BinaryIO]


class open_filename:
    """Context manager that opens filenames and leaves file objects untouched."""

    def __init__(self, filename: FileOrName, *args: Any, **kwargs: Any) -> None:
        if isinstance(filename, pathlib.PurePath):
            filename = str(filename)
        if isinstance(filename, str):
            self.file_handler = open(filename, *args, **kwargs)  # noqa: SIM115
            self.closing = True
        elif isinstance(filename, io.IOBase):
            self.file_handler = filename
            self.closing = False
        else:
            raise PDFTypeError(f"Unsupported input type: {type(filename)}")

    def __enter__(self) -> io.IOBase:
        return self.file_handler

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        if self.closing:
            self.file_handler.close()


__all__ = [
    "INF",
    "MATRIX_IDENTITY",
    "Matrix",
    "PDFDocEncoding",
    "Plane",
    "Point",
    "Rect",
    "apply_matrix_pt",
    "apply_matrix_rect",
    "AnyIO",
    "decode_text",
    "format_int_alpha",
    "format_int_roman",
    "isnumber",
    "mult_matrix",
    "open_filename",
    "shorten_str",
    "translate_matrix",
    "unpad_aes",
]
