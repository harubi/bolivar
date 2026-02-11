# pdfminer.pdfparser compatibility shim (Rust-backed)

import pdfminer.pdftypes as pdftypes
from bolivar._native_api import PDFParser as _RustPDFParser

# Re-export PDFObjRef for compatibility (pdfplumber imports it from here)
PDFObjRef = pdftypes.PDFObjRef

PDFParser = _RustPDFParser

# pdfminer.six uses a PSKeyword for null; our objects resolve to None
PDFParser.KEYWORD_NULL = None
