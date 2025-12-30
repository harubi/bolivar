# pdfminer.pdfparser compatibility shim

# Re-export PDFObjRef for compatibility (pdfplumber imports it from here)
from .pdftypes import PDFObjRef


class PDFParser:
    """PDF parser - stub for pdfplumber compatibility."""

    def __init__(self, fp):
        self.fp = fp
        # TODO: Delegate to bolivar-core
        raise NotImplementedError("PDFParser not yet implemented via bolivar")
