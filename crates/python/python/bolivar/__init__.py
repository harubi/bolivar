# bolivar - Fast PDF text extraction
#
# Python bindings for the bolivar Rust library.

from bolivar._bolivar import (
    LAParams,
    PDFDocument,
    PDFPage,
    LTPage,
    LTChar,
    extract_text,
    extract_text_from_path,
    extract_pages,
    extract_pages_from_path,
    process_page,
    process_pages,
    __version__,
)

__all__ = [
    "LAParams",
    "PDFDocument",
    "PDFPage",
    "LTPage",
    "LTChar",
    "extract_text",
    "extract_text_from_path",
    "extract_pages",
    "extract_pages_from_path",
    "process_page",
    "process_pages",
    "__version__",
]
