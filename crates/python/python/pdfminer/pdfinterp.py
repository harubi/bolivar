# pdfminer.pdfinterp compatibility shim

from typing import Any, Dict, List, Tuple, Union

# PDFStackT type alias (matches pdfminer.six)
PDFStackT = Union[
    bool,
    int,
    float,
    bytes,
    str,
    List[Any],
    Dict[str, Any],
    Tuple[Any, ...],
]


class PDFResourceManager:
    """PDF resource manager - stub for pdfplumber compatibility."""

    def __init__(self, caching=True):
        self.caching = caching
        self._cached_fonts = {}
        # TODO: Delegate to bolivar-core

    def get_font(self, objid, spec):
        """Get a font resource."""
        raise NotImplementedError("PDFResourceManager.get_font not yet implemented")


class PDFPageInterpreter:
    """PDF page interpreter - stub for pdfplumber compatibility."""

    def __init__(self, rsrcmgr, device):
        self.rsrcmgr = rsrcmgr
        self.device = device
        # TODO: Delegate to bolivar-core

    def process_page(self, page):
        """Process a PDF page."""
        raise NotImplementedError("PDFPageInterpreter.process_page not yet implemented via bolivar")
