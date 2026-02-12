# bolivar - Fast PDF text extraction
#
# Python bindings for the bolivar Rust library.

from __future__ import annotations

from pkgutil import extend_path
from typing import TYPE_CHECKING

# Allow the source shim package and installed wheel package to co-exist on
# sys.path so native extension imports resolve in subprocess tests/CI.
__path__ = extend_path(__path__, __name__)

from bolivar import _native_api as _native_api

if TYPE_CHECKING:
    from bolivar._bolivar import (
        LAParams as LAParams,
    )
    from bolivar._bolivar import (
        LTChar as LTChar,
    )
    from bolivar._bolivar import (
        LTPage as LTPage,
    )
    from bolivar._bolivar import (
        PDFDocument as PDFDocument,
    )
    from bolivar._bolivar import (
        PDFPage as PDFPage,
    )
    from bolivar._bolivar import (
        __version__ as __version__,
    )
    from bolivar._bolivar import (
        _extract_tables_core as _extract_tables_core,
    )
    from bolivar._bolivar import (
        async_runtime_poc as async_runtime_poc,
    )
    from bolivar._bolivar import (
        extract_pages as extract_pages,
    )
    from bolivar._bolivar import (
        extract_pages_async as extract_pages_async,
    )
    from bolivar._bolivar import (
        extract_pages_from_path as extract_pages_from_path,
    )
    from bolivar._bolivar import (
        extract_pages_with_images as extract_pages_with_images,
    )
    from bolivar._bolivar import (
        extract_pages_with_images_from_path as extract_pages_with_images_from_path,
    )
    from bolivar._bolivar import (
        extract_tables_from_document as extract_tables_from_document,
    )
    from bolivar._bolivar import (
        extract_tables_stream_from_document as extract_tables_stream_from_document,
    )
    from bolivar._bolivar import (
        extract_text as extract_text,
    )
    from bolivar._bolivar import (
        extract_text_from_path as extract_text_from_path,
    )
    from bolivar._bolivar import (
        process_page as process_page,
    )
    from bolivar._bolivar import (
        process_pages as process_pages,
    )
    from bolivar._bolivar import (
        repair_pdf as repair_pdf,
    )

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
