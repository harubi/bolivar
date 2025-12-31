# bolivar - Fast PDF text extraction
#
# Python bindings for the bolivar Rust library.

from bolivar._bolivar import (
    LAParams,
    PDFDocument,
    PDFPage,
    LTPage,
    LTChar,
    process_page,
    __version__,
)

__all__ = [
    "LAParams",
    "PDFDocument",
    "PDFPage",
    "LTPage",
    "LTChar",
    "process_page",
    "__version__",
]
