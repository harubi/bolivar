# pdfminer.utils compatibility shim
#
# Utility functions for PDF processing (Rust-backed where available).

import io
import pathlib
from types import TracebackType
from typing import BinaryIO, Protocol, TextIO

from bolivar._native_api import (
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
Point = tuple[float, float]
Rect = tuple[float, float, float, float]
Matrix = tuple[float, float, float, float, float, float]

FileOrName = pathlib.PurePath | str | io.IOBase
AnyIO = TextIO | BinaryIO


class _ClosableIO(Protocol):
    def close(self) -> None:
        """Close stream."""


class open_filename:
    """Context manager that opens filenames and leaves file objects untouched."""

    closing: bool

    def __init__(
        self,
        filename: FileOrName,
        mode: str = "rb",
        buffering: int = -1,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> None:
        self.file_handler: _ClosableIO
        self.closing = False
        if isinstance(filename, pathlib.PurePath):
            filename = str(filename)
        if isinstance(filename, str):
            self.file_handler = open(  # noqa: SIM115
                filename,
                mode=mode,
                buffering=buffering,
                encoding=encoding,
                errors=errors,
                newline=newline,
            )
            self.closing = True
        elif isinstance(filename, io.IOBase):
            self.file_handler = filename
        else:
            raise PDFTypeError(f"Unsupported input type: {type(filename)}")

    def __enter__(self) -> _ClosableIO:
        return self.file_handler

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        if self.closing:
            self.file_handler.close()


__all__ = [
    "INF",
    "MATRIX_IDENTITY",
    "AnyIO",
    "Matrix",
    "PDFDocEncoding",
    "Plane",
    "Point",
    "Rect",
    "apply_matrix_pt",
    "apply_matrix_rect",
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
