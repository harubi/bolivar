# pdfminer.six compatibility shim
#
# This module provides a pdfminer.six-compatible API backed by bolivar (Rust).
# Users can replace pdfminer.six with bolivar using:
#   [tool.uv.sources]
#   pdfminer-six = { package = "bolivar" }

__version__ = "0.1.0"

# Re-export submodules for convenient access
from . import (
    converter,
    data_structures,
    layout,
    pdfdocument,
    pdfinterp,
    pdfpage,
    pdfparser,
    pdftypes,
    psparser,
    utils,
)

__all__ = [
    "converter",
    "data_structures",
    "layout",
    "pdfdocument",
    "pdfinterp",
    "pdfpage",
    "pdfparser",
    "pdftypes",
    "psparser",
    "utils",
]
