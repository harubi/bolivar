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
from bolivar._export_manifest import TOP_LEVEL_EXPORTS

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

__all__ = list(TOP_LEVEL_EXPORTS)
_LAZY_EXPORTS = frozenset(TOP_LEVEL_EXPORTS)


def __getattr__(name: str) -> object:
    if name in _LAZY_EXPORTS:
        return getattr(_native_api, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(set(globals()) | _LAZY_EXPORTS)
