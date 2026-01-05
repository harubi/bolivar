# pdfminer.pdfinterp compatibility shim

from typing import Any, Dict, List, Tuple, Union

import os

from bolivar import (
    LAParams as _RustLAParams,
    process_page as _rust_process_page,
    process_pages as _rust_process_pages,
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

        # Prefer parallel precomputation and cache per LAParams.
        cache = getattr(page.doc, "_layout_cache", None)
        if cache is None:
            cache = {}
            page.doc._layout_cache = cache

        key = None
        if laparams is None:
            key = ("default",)
        else:
            try:
                key = (
                    laparams.line_overlap,
                    laparams.char_margin,
                    laparams.line_margin,
                    laparams.word_margin,
                    laparams.boxes_flow,
                    laparams.detect_vertical,
                    laparams.all_texts,
                )
            except Exception:
                key = ("default",)

        cached_pages = cache.get(key)
        if cached_pages is None:
            threads = os.cpu_count() or 1
            cached_pages = _rust_process_pages(rust_doc, rust_laparams, threads)
            cache[key] = cached_pages

        page_index = getattr(page, "_page_index", None)
        if page_index is None:
            page_index = page.pageid - 1

        if 0 <= page_index < len(cached_pages):
            ltpage = cached_pages[page_index]
        else:
            # Fallback: process single page
            ltpage = _rust_process_page(rust_doc, rust_page, rust_laparams)

        # Send the result to the device
        self.device._receive_layout(ltpage)
