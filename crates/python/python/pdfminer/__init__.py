# pdfminer.six compatibility shim
#
# This module provides a pdfminer.six-compatible API backed by bolivar (Rust).
# Users can replace pdfminer.six with bolivar using:
#   [tool.uv.sources]
#   pdfminer-six = { package = "bolivar" }

__version__ = "0.1.0"

# Re-export submodules for convenient access
import bolivar._shim_registry as _shim_registry
import pdfminer._bolivar_patch as _bolivar_patch
import pdfminer.converter as converter
import pdfminer.data_structures as data_structures
import pdfminer.high_level as high_level
import pdfminer.layout as layout
import pdfminer.pdfdocument as pdfdocument
import pdfminer.pdfinterp as pdfinterp
import pdfminer.pdfpage as pdfpage
import pdfminer.pdfparser as pdfparser
import pdfminer.pdftypes as pdftypes
import pdfminer.psparser as psparser
import pdfminer.utils as utils


def patch_pdfplumber() -> bool:
    return _bolivar_patch.patch_pdfplumber()


__all__ = [
    "converter",
    "data_structures",
    "high_level",
    "layout",
    "pdfdocument",
    "pdfinterp",
    "pdfpage",
    "pdfparser",
    "pdftypes",
    "psparser",
    "utils",
]

# Default-on pdfplumber monkeypatch
_shim_registry.apply_pdfplumber_patch()
