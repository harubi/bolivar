# pdfminer.pdfparser compatibility shim (Rust-backed)

from bolivar._bolivar import PDFParser as _RustPDFParser

# Re-export PDFObjRef for compatibility (pdfplumber imports it from here)

PDFParser = _RustPDFParser

# pdfminer.six uses a PSKeyword for null; our objects resolve to None
PDFParser.KEYWORD_NULL = None
