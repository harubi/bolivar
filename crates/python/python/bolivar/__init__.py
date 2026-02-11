# bolivar - Fast PDF text extraction
#
# Python bindings for the bolivar Rust library.

from pkgutil import extend_path
from typing import Any

# Allow the source shim package and installed wheel package to co-exist on
# sys.path so native extension imports resolve in subprocess tests/CI.
__path__ = extend_path(__path__, __name__)

from bolivar import _native_api as _native_api

LAParams: Any
PDFDocument: Any
PDFPage: Any
LTPage: Any
LTChar: Any
extract_text: Any
extract_text_from_path: Any
extract_pages: Any
extract_pages_with_images: Any
extract_pages_from_path: Any
extract_pages_with_images_from_path: Any
extract_pages_async: Any
async_runtime_poc: Any
process_page: Any
process_pages: Any
extract_tables_stream_from_document: Any
extract_tables_from_document: Any
repair_pdf: Any
__version__: Any
_extract_tables_core: Any

__all__ = [
    "LAParams",
    "LTChar",
    "LTPage",
    "PDFDocument",
    "PDFPage",
    "__version__",
    "async_runtime_poc",
    "extract_pages",
    "extract_pages_async",
    "extract_pages_from_path",
    "extract_pages_with_images",
    "extract_pages_with_images_from_path",
    "extract_tables_from_document",
    "extract_tables_stream_from_document",
    "extract_text",
    "extract_text_from_path",
    "process_page",
    "process_pages",
    "repair_pdf",
]

_LAZY_EXPORTS = set(__all__) | {"_extract_tables_core"}


def __getattr__(name: str) -> object:
    if name in _LAZY_EXPORTS:
        return getattr(_native_api, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
