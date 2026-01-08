# pdfminer.pdfinterp compatibility shim

from typing import Any, Dict, List, Tuple, Union

from bolivar import (
    LAParams as _RustLAParams,
    process_page as _rust_process_page,
)
from bolivar._bolivar import PDFResourceManager as _RustPDFResourceManager

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


# Rust-backed resource manager (drop-in compatible API).
PDFResourceManager = _RustPDFResourceManager


class PDFPageInterpreter:
    """PDF page interpreter - wraps bolivar's Rust page processing.

    Provides pdfminer.six-compatible API for page interpretation.
    """

    def __init__(self, rsrcmgr, device):
        """Create a page interpreter.

        Args:
            rsrcmgr: PDFResourceManager instance (for compatibility)
            device: PDFDevice instance (PDFPageAggregator or similar)
        """
        self.rsrcmgr = rsrcmgr
        self.device = device

    def process_page(self, page):
        """Process a PDF page and send results to device.

        Args:
            page: PDFPage instance to process
        """
        # Get the Rust document and page from the shim wrappers
        rust_doc = page.doc._rust_doc
        rust_page = page._rust_page

        # Get LAParams from the device if available
        laparams = getattr(self.device, "_laparams", None)
        rust_laparams = None
        if laparams is not None:
            # Convert to Rust LAParams if it's a shim LAParams
            if hasattr(laparams, "_to_rust"):
                rust_laparams = laparams._to_rust()
            else:
                # Already a Rust LAParams or compatible
                rust_laparams = laparams

        ltpage = _rust_process_page(rust_doc, rust_page, rust_laparams)

        # Send the result to the device
        self.device._receive_layout(ltpage)
