# pdfminer.pdfparser compatibility shim (Rust-backed)

from bolivar._bolivar import PDFParser as _RustPDFParser
import pdfminer.pdftypes as pdftypes

# Re-export PDFObjRef for compatibility (pdfplumber imports it from here)
PDFObjRef = pdftypes.PDFObjRef

PDFParser = _RustPDFParser

# pdfminer.six uses a PSKeyword for null; our objects resolve to None
PDFParser.KEYWORD_NULL = None
